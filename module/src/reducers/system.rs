//! System reducers — cross-module authenticated operations.
//!
//! These reducers are called by other SpacetimeDB modules (e.g., nexus-cortex)
//! via the HTTP reducer API. They use a shared system_token for authentication
//! instead of ctx.sender() identity links, because cross-module HTTP calls
//! use anonymous identities.
//!
//! SECURITY:
//!   - system_token is stored in the PRIVATE system_config table (never sent to clients)
//!   - Validated server-side before any action
//!   - Same security model as Worker env secrets (STDB_TOKEN)
//!   - No master keys or tokens ever cross a browser boundary

use spacetimedb::{ReducerContext, ScheduleAt, Table};
use crate::tables::messages::{Message, messages};
use crate::tables::system_config::{SystemConfig, system_config};
use crate::tables::users::{ChatUser, chat_users};
use crate::tables::room_members::{RoomMember, room_members};
use crate::tables::rooms::{Room, rooms};
use crate::tables::servers::{ChatServer, chat_servers};
use crate::tables::server_members::{ServerMember, server_members};
use crate::tables::channel_categories::channel_categories;
use crate::tables::typing_indicators::{TypingIndicator, typing_indicators};
use crate::tables::scheduled_jobs::{TypingExpiryJob, typing_expiry_jobs};
use crate::tables::agents::{AgentCredential, agent_credentials};
use crate::tables::auth::user_identity_links;
use crate::tables::*;
use crate::tables::agent_config::{AgentConfig, agent_configs};

/// Set a system configuration value. Admin-only (CLI or admin identity).
#[spacetimedb::reducer]
pub fn set_system_config(ctx: &ReducerContext, key: String, value: String) {
    // Allow system callers (CLI, scheduled) and platform admins
    if !crate::utils::auth::is_system_caller(ctx) && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[set_system_config] Rejected: not admin or system caller");
        return;
    }

    let now = crate::utils::time::timestamp_ms(ctx);

    if ctx.db.system_config().key().find(&key).is_some() {
        ctx.db.system_config().key().delete(&key);
    }

    ctx.db.system_config().insert(SystemConfig {
        key: key.clone(),
        value,
        updated_at: now,
    });

    log::info!("[set_system_config] Set key: {}", key);
}

/// Post a message as a bot — authenticated via system_token.
///
/// Called by the nexus-cortex procedure via HTTP reducer API.
/// Does NOT require ctx.sender() to have an identity link to the bot.
/// Instead validates via the system_token shared secret.
///
/// SECURITY:
///   - Validates system_token against system_config table (PRIVATE)
///   - Validates bot_user_id is a real bot (is_bot == true)
///   - Validates bot is a member of the target room
///   - Message is stamped with is_bot_author = true (unforgeable)
///   - Token never leaves server-side storage
#[spacetimedb::reducer]
pub fn system_send_bot_message(
    ctx: &ReducerContext,
    bot_user_id: String,
    room_id: String,
    content: String,
    message_id: String,
    system_token: String,
) {
    // 1. Validate auth: system_token OR platform admin identity
    let has_valid_token = if !system_token.is_empty() {
        let expected = ctx.db.system_config().key().find(&"cortex_system_token".to_string());
        matches!(expected, Some(config) if config.value == system_token)
    } else {
        false
    };
    let is_admin = crate::utils::auth::is_platform_admin(ctx);

    if !has_valid_token && !is_admin {
        log::warn!("[system_send_bot_message] Rejected: invalid system token and caller is not platform admin");
        return;
    }

    // 2. Validate bot_user_id is a real bot
    let bot = match ctx.db.chat_users().user_id().find(&bot_user_id) {
        Some(user) if user.is_bot == Some(true) => user,
        _ => {
            log::warn!("[system_send_bot_message] Rejected: {} is not a valid bot", &bot_user_id[..8.min(bot_user_id.len())]);
            return;
        }
    };

    // 3. Validate bot is a member of the room
    let is_member = ctx.db.room_members().iter().any(|m| {
        m.room_id == room_id && m.user_id == bot_user_id && m.role != "banned"
    });
    if !is_member {
        log::warn!(
            "[system_send_bot_message] Rejected: bot {} is not a member of room {}",
            &bot_user_id[..8.min(bot_user_id.len())],
            &room_id[..8.min(room_id.len())]
        );
        return;
    }

    // 4. Validate content
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > 16_000 {
        log::warn!("[system_send_bot_message] Rejected: empty or oversized content");
        return;
    }

    // 5. Insert message
    let now = crate::utils::time::timestamp_ms(ctx); // microseconds

    ctx.db.messages().insert(Message {
        id: message_id.clone(),
        room_id: room_id.clone(),
        author_id: bot_user_id.clone(),
        content: trimmed,
        created_at: now,
        edited_at: None,
        parent_message_id: None,
        is_ephemeral: false,
        expires_at: None,
        message_type: "default".to_string(),
        reply_to_id: None,
        sticker_ids: None,
        mention_everyone: false,
        mentioned_user_ids: None,
        mentioned_role_ids: None,
        flags: 0,
        is_bot_author: Some(true),
    });

    // 6. Update bot's last_message_at
    ctx.db.chat_users().user_id().delete(&bot_user_id);
    ctx.db.chat_users().insert(ChatUser {
        last_message_at: now,
        ..bot
    });

    log::info!(
        "[system_send_bot_message] Bot {} posted message {} in room {}",
        &bot_user_id[..8.min(bot_user_id.len())],
        &message_id[..8.min(message_id.len())],
        &room_id[..8.min(room_id.len())]
    );
}

/// System-level room deletion — bypasses all permission checks.
/// Used to clean up orphaned DM rooms after bot deletion.
#[spacetimedb::reducer]
pub fn system_delete_room(ctx: &ReducerContext, room_id: String, system_token: String) {
    let has_valid_token = if !system_token.is_empty() {
        let expected = ctx.db.system_config().key().find(&"cortex_system_token".to_string());
        matches!(expected, Some(config) if config.value == system_token)
    } else {
        false
    };
    if !has_valid_token && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_delete_room] Rejected: invalid token and not admin");
        return;
    }

    if let Some(room) = ctx.db.rooms().id().find(&room_id) {
        // Delete room members
        let members: Vec<String> = ctx.db.room_members().iter()
            .filter(|m| m.room_id == room_id)
            .map(|m| m.id.clone())
            .collect();
        for id in &members {
            ctx.db.room_members().id().delete(id);
        }
        // Delete messages
        let msgs: Vec<String> = ctx.db.messages().iter()
            .filter(|m| m.room_id == room_id)
            .map(|m| m.id.clone())
            .collect();
        for id in &msgs {
            ctx.db.messages().id().delete(id);
        }
        // Delete room
        ctx.db.rooms().id().delete(&room_id);
        log::info!("[system_delete_room] Deleted room {} ({}) with {} members, {} messages",
            &room_id[..8.min(room_id.len())], room.name, members.len(), msgs.len());
    }
}

/// Start a typing indicator for a bot — authenticated via system_token.
///
/// Called by nexus-cortex's run_agent_turn procedure before the LLM call
/// so users see "typing..." while the agent processes their message.
///
/// Creates a typing indicator with a 10-second TTL (longer than normal 4s
/// because LLM inference can take 5-30 seconds).
#[spacetimedb::reducer]
pub fn system_start_bot_typing(
    ctx: &ReducerContext,
    bot_user_id: String,
    room_id: String,
    system_token: String,
) {
    // 1. Validate auth
    let has_valid_token = if !system_token.is_empty() {
        let expected = ctx.db.system_config().key().find(&"cortex_system_token".to_string());
        matches!(expected, Some(config) if config.value == system_token)
    } else {
        false
    };
    if !has_valid_token && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_start_bot_typing] Rejected: invalid system token");
        return;
    }

    // 2. Validate bot exists
    let bot = match ctx.db.chat_users().user_id().find(&bot_user_id) {
        Some(user) if user.is_bot == Some(true) => user,
        _ => return,
    };

    // 3. Create or refresh typing indicator (10s TTL for LLM inference)
    let now = crate::utils::time::timestamp_ms(ctx);
    let typing_ttl_ms: u64 = 10_000_000; // 10 seconds in microseconds
    let expires_at = now + typing_ttl_ms;
    let typing_id = format!("typing:{}:{}", room_id, bot_user_id);

    // Remove existing typing indicator if present
    if ctx.db.typing_indicators().id().find(&typing_id).is_some() {
        ctx.db.typing_indicators().id().delete(&typing_id);
    }

    ctx.db.typing_indicators().insert(TypingIndicator {
        id: typing_id.clone(),
        room_id: room_id.clone(),
        user_id: bot_user_id.clone(),
        expires_at,
    });

    // Schedule expiry job
    ctx.db.typing_expiry_jobs().insert(TypingExpiryJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp + std::time::Duration::from_secs(10)),
        typing_indicator_id: typing_id,
    });

    log::info!(
        "[system_start_bot_typing] Bot {} typing in room {}",
        &bot_user_id[..8.min(bot_user_id.len())],
        &room_id[..8.min(room_id.len())]
    );
}

/// Stop a typing indicator for a bot — authenticated via system_token.
///
/// Called by nexus-cortex's run_agent_turn procedure AFTER the bot's message
/// is posted, so the typing indicator clears immediately instead of waiting
/// for the 10-second TTL expiry.
#[spacetimedb::reducer]
pub fn system_stop_bot_typing(
    ctx: &ReducerContext,
    bot_user_id: String,
    room_id: String,
    system_token: String,
) {
    // 1. Validate auth
    let has_valid_token = if !system_token.is_empty() {
        let expected = ctx.db.system_config().key().find(&"cortex_system_token".to_string());
        matches!(expected, Some(config) if config.value == system_token)
    } else {
        false
    };
    if !has_valid_token && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_stop_bot_typing] Rejected: invalid system token");
        return;
    }

    // 2. Delete the typing indicator
    let typing_id = format!("typing:{}:{}", room_id, bot_user_id);
    if ctx.db.typing_indicators().id().find(&typing_id).is_some() {
        ctx.db.typing_indicators().id().delete(&typing_id);
        log::info!(
            "[system_stop_bot_typing] Cleared typing for bot {} in room {}",
            &bot_user_id[..8.min(bot_user_id.len())],
            &room_id[..8.min(room_id.len())]
        );
    }
}

/// Create a server from a template — system_token or admin auth.
///
/// Templates define channel layouts appropriate for different use cases:
/// - "swarm"     → #tavern, #marketplace, #events (game/RP swarms)
/// - "prediction"→ #analysis, #decisions, #debate (prediction markets)
/// - "team"      → #general, #standup, #retrospective (work teams)
/// - "community" → #general, #introductions, #off-topic (communities)
///
/// Called by nexus-cortex's swarm provisioning procedure via HTTP reducer API.
/// The server is created with the owner auto-joined, all channels created,
/// and the template tag set for downstream identification.
#[spacetimedb::reducer]
pub fn system_create_server_from_template(
    ctx: &ReducerContext,
    server_id: String,
    name: String,
    owner_user_id: String,
    is_public: bool,
    template: String,
    system_token: String,
) {
    // 1. Validate auth: system_token OR platform admin
    let has_valid_token = if !system_token.is_empty() {
        let expected = ctx.db.system_config().key().find(&"cortex_system_token".to_string());
        matches!(expected, Some(config) if config.value == system_token)
    } else {
        false
    };
    if !has_valid_token && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_create_server_from_template] Rejected: invalid token and not admin");
        return;
    }

    // 2. Validate inputs
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > 100 {
        log::warn!("[system_create_server_from_template] Invalid server name length: {}", trimmed.len());
        return;
    }

    if ctx.db.chat_servers().id().find(&server_id).is_some() {
        log::warn!("[system_create_server_from_template] Server {} already exists", &server_id[..8.min(server_id.len())]);
        return;
    }

    let now = crate::utils::time::timestamp_ms(ctx);

    // 3. Resolve channel template
    let channels = template_channels(&template);

    // 4. Create server
    ctx.db.chat_servers().insert(ChatServer {
        id: server_id.clone(),
        name: trimmed.clone(),
        description: String::new(),
        audience_id: String::new(),
        owner_user_id: owner_user_id.clone(),
        is_public,
        default_tier: "free".to_string(),
        icon_url: String::new(),
        created_at: now,
        updated_at: now,
        template: Some(template.clone()),
    });

    // 5. Auto-join owner
    let member_id = format!("{}-{}", server_id, owner_user_id);
    ctx.db.server_members().insert(ServerMember {
        id: member_id,
        server_id: server_id.clone(),
        user_id: owner_user_id.clone(),
        role: "owner".to_string(),
        joined_at: now,
        nickname: None,
        timeout_until: None,
        deaf: false,
        mute: false,
    });

    // 6. Create @everyone default role
    crate::reducers::roles::create_default_role(ctx, &server_id);

    // 7. Create category
    let cat_id = format!("{}-channels-cat", server_id);
    ctx.db.channel_categories().insert(crate::tables::ChannelCategory {
        id: cat_id.clone(),
        server_id: server_id.clone(),
        name: "Channels".to_string(),
        sort_order: 0,
        created_at: now,
    });

    // 8. Create channels from template
    for (i, (ch_name, ch_desc)) in channels.iter().enumerate() {
        let room_id = format!("{}-{}", server_id, ch_name);
        ctx.db.rooms().insert(Room {
            id: room_id.clone(),
            name: ch_name.to_string(),
            created_by: owner_user_id.clone(),
            is_private: false,
            is_dm: false,
            created_at: now,
            server_id: Some(server_id.clone()),
            required_tier: None,
            description: Some(ch_desc.to_string()),
            sort_order: Some(i as u32),
            room_type: "text".to_string(),
            category_id: Some(cat_id.clone()),
            topic: None,
            slowmode_seconds: None,
            nsfw: false,
            parent_room_id: None,
            archived: false,
            locked: false,
            auto_archive_minutes: None,
            default_sort_order: None,
            allow_attachments: None,
            allow_embeds: None,
            allow_reactions: None,
            rules_text: None,
        });

        // Auto-join owner to each channel
        ctx.db.room_members().insert(RoomMember {
            id: format!("{}-{}", room_id, owner_user_id),
            room_id,
            user_id: owner_user_id.clone(),
            role: "admin".to_string(),
            joined_at: now,
        });
    }

    log::info!(
        "[system_create_server_from_template] Created '{}' server '{}' ({}) with {} channels for owner {}",
        template, trimmed, &server_id[..8.min(server_id.len())], channels.len(),
        &owner_user_id[..8.min(owner_user_id.len())]
    );
}

/// Returns (channel_name, description) pairs for a given template.
/// Unknown templates fall back to a single #general channel.
fn template_channels(template: &str) -> Vec<(&'static str, &'static str)> {
    match template {
        "swarm" => vec![
            ("tavern", "Main gathering place for conversation"),
            ("marketplace", "Trade, barter, and economic activity"),
            ("events", "Announcements and notable happenings"),
        ],
        "prediction" => vec![
            ("analysis", "Data analysis and research"),
            ("decisions", "Final predictions and positions"),
            ("debate", "Challenging and defending viewpoints"),
        ],
        "team" => vec![
            ("general", "General team discussion"),
            ("standup", "Daily status updates"),
            ("retrospective", "Reflections and process improvement"),
        ],
        "community" => vec![
            ("general", "General discussion"),
            ("introductions", "Introduce yourself"),
            ("off-topic", "Anything goes"),
        ],
        "research" => vec![
            ("findings", "Share research findings"),
            ("methodology", "Discuss approaches and methods"),
            ("review", "Peer review and feedback"),
        ],
        "focus-group" => vec![
            ("discussion", "Main focus group discussion"),
            ("feedback", "Structured feedback and opinions"),
            ("summary", "Session summaries and takeaways"),
        ],
        _ => vec![
            ("general", "General discussion"),
        ],
    }
}

// ── Phase 2a: Cross-module system reducers for swarm provisioning ────────────

/// Validate system_token. Returns true if valid, false otherwise.
fn validate_system_token(ctx: &ReducerContext, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let expected = ctx.db.system_config().key().find(&"cortex_system_token".to_string());
    matches!(expected, Some(config) if config.value == token)
}

/// Provision a bot user — authenticated via system_token.
///
/// Called by nexus-cortex's swarm provisioning procedure via HTTP reducer API.
/// Creates a ChatUser with is_bot=true and an AgentCredential for identity registration.
#[spacetimedb::reducer]
pub fn system_provision_bot(
    ctx: &ReducerContext,
    system_token: String,
    bot_user_id: String,
    display_name: String,
    owner_user_id: String,
    owner_secret: String,
) {
    if !validate_system_token(ctx, &system_token) && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_provision_bot] Rejected: invalid token and not admin");
        return;
    }

    // Validate display name
    let trimmed = display_name.trim().to_string();
    if trimmed.len() < 2 || trimmed.len() > 32 {
        log::warn!("[system_provision_bot] Invalid display name length: {}", trimmed.len());
        return;
    }

    // Check uniqueness (case-insensitive among bots)
    let lower = trimmed.to_ascii_lowercase();
    let taken = ctx.db.chat_users().iter()
        .any(|u| u.is_bot == Some(true) && u.display_name.to_ascii_lowercase() == lower);
    if taken {
        log::warn!("[system_provision_bot] Display name '{}' already taken", trimmed);
        return;
    }

    // Check bot_user_id doesn't already exist
    if ctx.db.chat_users().user_id().find(&bot_user_id).is_some() {
        log::warn!("[system_provision_bot] User {} already exists", &bot_user_id[..8.min(bot_user_id.len())]);
        return;
    }

    let now = crate::utils::time::timestamp_ms(ctx);

    // Create bot ChatUser — system_provision_bot is called by nexus-cortex
    // for swarm + observer provisioning, so flag as swarm-origin. "Promote
    // to personal" flips this flag via a separate reducer.
    ctx.db.chat_users().insert(ChatUser {
        user_id: bot_user_id.clone(),
        stdb_identity: String::new(), // Set during register_agent_identity
        display_name: trimmed.clone(),
        status: "offline".to_string(),
        online: false,
        last_message_at: 0,
        last_typing_at: 0,
        last_seen_at: now,
        created_at: now,
        tier: None,
        avatar_data: None,
        custom_status: None,
        custom_status_emoji: None,
        platform_role: Some("agent".to_string()),
        is_bot: Some(true),
        is_platform_agent: None,
        bot_owner_user_id: Some(owner_user_id.clone()),
        is_swarm_member: Some(true),
        is_steward_projection: None,
    });

    // Create AgentCredential (private table)
    ctx.db.agent_credentials().insert(AgentCredential {
        agent_user_id: bot_user_id.clone(),
        owner_user_id: owner_user_id.clone(),
        parent_agent_id: None,
        owner_secret,
        can_spawn: false,
        created_at: now,
        last_active_at: now,
        is_platform_agent: None,
    });

    log::info!(
        "[system_provision_bot] Provisioned bot '{}' ({}) owned by {}",
        trimmed, &bot_user_id[..8.min(bot_user_id.len())],
        &owner_user_id[..8.min(owner_user_id.len())]
    );
}

/// Deprovision a bot user — authenticated via system_token.
///
/// Called by nexus-cortex's swarm deprovisioning procedure when a swarm is
/// deleted. Removes the bot's ChatUser row, AgentCredential, AgentConfig,
/// room memberships, server memberships, and identity links — mirrors
/// admin_force_delete_bot but with system_token auth so cross-module calls
/// don't need a platform-admin sender.
#[spacetimedb::reducer]
pub fn system_deprovision_bot(
    ctx: &ReducerContext,
    system_token: String,
    bot_user_id: String,
) {
    if !validate_system_token(ctx, &system_token) && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_deprovision_bot] Rejected: invalid token and not admin");
        return;
    }

    let user = ctx.db.chat_users().user_id().find(&bot_user_id);
    if user.is_none() {
        log::info!("[system_deprovision_bot] User '{}' not found (already deleted?)",
            &bot_user_id[..16.min(bot_user_id.len())]);
        return;
    }

    // Credentials (may not exist for pre-existing bots reused in swarms)
    ctx.db.agent_credentials().agent_user_id().delete(&bot_user_id);

    // Agent config
    ctx.db.agent_configs().agent_user_id().delete(&bot_user_id);

    // Room memberships
    let room_memberships: Vec<String> = ctx.db.room_members().iter()
        .filter(|m| m.user_id == bot_user_id)
        .map(|m| m.id.clone())
        .collect();
    for mid in room_memberships {
        ctx.db.room_members().id().delete(&mid);
    }

    // Server memberships
    let server_memberships: Vec<String> = ctx.db.server_members().iter()
        .filter(|m| m.user_id == bot_user_id)
        .map(|m| m.id.clone())
        .collect();
    for mid in server_memberships {
        ctx.db.server_members().id().delete(&mid);
    }

    // Identity links
    let links: Vec<String> = ctx.db.user_identity_links().iter()
        .filter(|l| l.user_id == bot_user_id)
        .map(|l| l.stdb_identity.clone())
        .collect();
    for lid in links {
        ctx.db.user_identity_links().stdb_identity().delete(&lid);
    }

    // ChatUser row
    ctx.db.chat_users().user_id().delete(&bot_user_id);

    log::info!("[system_deprovision_bot] Deprovisioned bot '{}'",
        &bot_user_id[..16.min(bot_user_id.len())]);
}

/// Delete a CHAT server with full cascade — authenticated via system_token.
///
/// Called by nexus-cortex's swarm deprovision procedure when a swarm owned
/// a dedicated CHAT server (use_existing_server != true). Unlike the
/// user-facing delete_server (which requires a sender identity and only
/// unsets rooms' server_id), this fully cascades: server → rooms → room
/// members → messages → server members → channel categories.
#[spacetimedb::reducer]
pub fn system_delete_server(
    ctx: &ReducerContext,
    system_token: String,
    server_id: String,
) {
    if !validate_system_token(ctx, &system_token) && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_delete_server] Rejected: invalid token and not admin");
        return;
    }

    if ctx.db.chat_servers().id().find(&server_id).is_none() {
        log::info!("[system_delete_server] Server '{}' not found (already deleted?)",
            &server_id[..8.min(server_id.len())]);
        return;
    }

    // 1. Collect all rooms in this server
    let server_rooms: Vec<String> = ctx.db.rooms().iter()
        .filter(|r| r.server_id.as_deref() == Some(&server_id))
        .map(|r| r.id.clone())
        .collect();

    // 2. For each room: delete messages + room memberships + the room itself
    let mut total_messages = 0usize;
    let mut total_room_members = 0usize;
    for room_id in &server_rooms {
        let msgs: Vec<String> = ctx.db.messages().iter()
            .filter(|m| m.room_id == *room_id)
            .map(|m| m.id.clone())
            .collect();
        total_messages += msgs.len();
        for mid in msgs { ctx.db.messages().id().delete(&mid); }

        let rmems: Vec<String> = ctx.db.room_members().iter()
            .filter(|m| m.room_id == *room_id)
            .map(|m| m.id.clone())
            .collect();
        total_room_members += rmems.len();
        for mid in rmems { ctx.db.room_members().id().delete(&mid); }

        ctx.db.rooms().id().delete(room_id);
    }

    // 3. Server memberships
    let s_members: Vec<String> = ctx.db.server_members().iter()
        .filter(|m| m.server_id == server_id)
        .map(|m| m.id.clone())
        .collect();
    let smc = s_members.len();
    for mid in s_members { ctx.db.server_members().id().delete(&mid); }

    // 4. Channel categories
    let categories: Vec<String> = ctx.db.channel_categories().iter()
        .filter(|c| c.server_id == server_id)
        .map(|c| c.id.clone())
        .collect();
    let cats = categories.len();
    for cid in categories { ctx.db.channel_categories().id().delete(&cid); }

    // 5. The server row itself
    ctx.db.chat_servers().id().delete(&server_id);

    log::info!(
        "[system_delete_server] Deleted server '{}' — {} rooms, {} messages, {} room members, {} server members, {} categories",
        &server_id[..8.min(server_id.len())],
        server_rooms.len(), total_messages, total_room_members, smc, cats
    );
}

/// Add a bot to a server and join specified rooms — authenticated via system_token.
///
/// Called by nexus-cortex's swarm provisioning procedure after bot creation.
#[spacetimedb::reducer]
pub fn system_add_bot_to_server(
    ctx: &ReducerContext,
    system_token: String,
    bot_user_id: String,
    server_id: String,
    room_ids_json: String,
) {
    if !validate_system_token(ctx, &system_token) && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_add_bot_to_server] Rejected: invalid token and not admin");
        return;
    }

    // Validate bot exists and is a bot
    match ctx.db.chat_users().user_id().find(&bot_user_id) {
        Some(u) if u.is_bot == Some(true) => {},
        _ => {
            log::warn!("[system_add_bot_to_server] {} is not a valid bot", &bot_user_id[..8.min(bot_user_id.len())]);
            return;
        }
    };

    // Validate server exists
    if ctx.db.chat_servers().id().find(&server_id).is_none() {
        log::warn!("[system_add_bot_to_server] Server {} not found", &server_id[..8.min(server_id.len())]);
        return;
    }

    let now = crate::utils::time::timestamp_ms(ctx);

    // Add as server member (if not already)
    let member_id = format!("{}-{}", server_id, bot_user_id);
    if ctx.db.server_members().id().find(&member_id).is_none() {
        ctx.db.server_members().insert(ServerMember {
            id: member_id,
            server_id: server_id.clone(),
            user_id: bot_user_id.clone(),
            role: "member".to_string(),
            joined_at: now,
            nickname: None,
            timeout_until: None,
            deaf: false,
            mute: false,
        });
    }

    // Parse room IDs from JSON array and join each
    // Simple JSON array parser: ["id1","id2","id3"]
    let room_ids: Vec<String> = room_ids_json
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut joined = 0u32;
    for room_id in &room_ids {
        // Verify room exists and belongs to this server
        let room = ctx.db.rooms().id().find(room_id);
        if room.as_ref().and_then(|r| r.server_id.as_deref()) != Some(&server_id) {
            continue;
        }

        let rm_id = format!("{}-{}", room_id, bot_user_id);
        if ctx.db.room_members().id().find(&rm_id).is_none() {
            ctx.db.room_members().insert(RoomMember {
                id: rm_id,
                room_id: room_id.clone(),
                user_id: bot_user_id.clone(),
                role: "member".to_string(),
                joined_at: now,
            });
            joined += 1;
        }
    }

    log::info!(
        "[system_add_bot_to_server] Bot {} added to server {} ({} rooms joined)",
        &bot_user_id[..8.min(bot_user_id.len())],
        &server_id[..8.min(server_id.len())],
        joined
    );
}

/// Update or create an agent's config — authenticated via system_token.
///
/// Called by nexus-cortex to push SOUL docs, personality vectors, and
/// temperature settings to the CHAT module after identity generation.
#[spacetimedb::reducer]
pub fn system_update_agent_config(
    ctx: &ReducerContext,
    system_token: String,
    agent_user_id: String,
    persona_prompt: Option<String>,
    temperature: Option<f64>,
    domain: Option<String>,
    soul_document: Option<String>,
) {
    if !validate_system_token(ctx, &system_token) && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_update_agent_config] Rejected: invalid token and not admin");
        return;
    }

    // Validate agent exists and is a bot
    let bot = match ctx.db.chat_users().user_id().find(&agent_user_id) {
        Some(u) if u.is_bot == Some(true) => u,
        _ => {
            log::warn!("[system_update_agent_config] {} is not a valid bot", &agent_user_id[..8.min(agent_user_id.len())]);
            return;
        }
    };

    let now = crate::utils::time::timestamp_ms(ctx);
    let owner_user_id = bot.bot_owner_user_id.clone().unwrap_or_default();

    if let Some(existing) = ctx.db.agent_configs().agent_user_id().find(&agent_user_id) {
        // Update existing config — only overwrite fields that are Some
        ctx.db.agent_configs().agent_user_id().delete(&agent_user_id);
        ctx.db.agent_configs().insert(AgentConfig {
            persona_prompt: if persona_prompt.is_some() { persona_prompt } else { existing.persona_prompt },
            temperature: if temperature.is_some() { temperature } else { existing.temperature },
            soul_document: if soul_document.is_some() { soul_document } else { existing.soul_document },
            domain: if domain.is_some() { domain } else { existing.domain },
            updated_at: now,
            ..existing
        });
        log::info!("[system_update_agent_config] Updated config for {}", &agent_user_id[..8.min(agent_user_id.len())]);
    } else {
        // Create new config with defaults
        ctx.db.agent_configs().insert(AgentConfig {
            agent_user_id: agent_user_id.clone(),
            owner_user_id,
            persona_prompt,
            personality_preset: None,
            default_model: None,
            temperature,
            max_tokens: None,
            can_send_messages: Some(true),
            can_reply_to_threads: Some(true),
            can_react: Some(true),
            can_send_gifs: Some(true),
            can_share_vfs_files: Some(true),
            can_share_vfs_media: Some(true),
            can_use_embeds: Some(true),
            can_create_polls: Some(false),
            can_pin_messages: Some(false),
            can_manage_threads: Some(false),
            require_share_review: Some(true),
            max_file_size_bytes: None,
            allowed_share_types: None,
            shares_per_hour: None,
            auto_respond_mentions: Some(true),
            owner_only_mentions: Some(false),
            respond_in_threads_only: Some(false),
            max_responses_per_minute: Some(10),
            max_message_length: Some(4000),
            heartbeat_interval_ms: None,
            loop_detection_enabled: None,
            mentorship_stage: None,
            created_at: now,
            updated_at: now,
            personality_vector_json: None,
            soul_document,
            domain,
            demographics_json: None,
        });
        log::info!("[system_update_agent_config] Created config for {}", &agent_user_id[..8.min(agent_user_id.len())]);
    }
}

/// Write the FULL NPC identity payload (personality vector + demographics +
/// SOUL + domain) into agent_configs. Called from nexus-cortex's
/// provision_swarm_chat procedure after an NPC is spawned so that the
/// chat-side config reflects the bot's complete identity. Enables the
/// My Agents and ADMIN Cortex AI edit panels to display the full persona.
///
/// Creates the config row if absent, otherwise overwrites the four identity
/// fields in place (leaves other capability/behavior fields untouched when
/// updating an existing row).
#[spacetimedb::reducer]
pub fn system_write_agent_identity(
    ctx: &ReducerContext,
    system_token: String,
    agent_user_id: String,
    personality_vector_json: String,
    demographics_json: String,
    soul_document: String,
    domain: String,
) {
    if !validate_system_token(ctx, &system_token) && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_write_agent_identity] Rejected: invalid token and not admin");
        return;
    }

    let bot = match ctx.db.chat_users().user_id().find(&agent_user_id) {
        Some(u) if u.is_bot == Some(true) => u,
        _ => {
            log::warn!(
                "[system_write_agent_identity] {} is not a valid bot",
                &agent_user_id[..16.min(agent_user_id.len())]
            );
            return;
        }
    };

    let now = crate::utils::time::timestamp_ms(ctx);
    let owner_user_id = bot.bot_owner_user_id.clone().unwrap_or_default();

    // Convert empty strings to None so the UI can distinguish "unset" from
    // "explicitly empty". Non-empty values get wrapped in Some.
    let pvec = if personality_vector_json.is_empty() { None } else { Some(personality_vector_json) };
    let demo = if demographics_json.is_empty() { None } else { Some(demographics_json) };
    let soul = if soul_document.is_empty() { None } else { Some(soul_document) };
    let dom = if domain.is_empty() { None } else { Some(domain) };

    if let Some(existing) = ctx.db.agent_configs().agent_user_id().find(&agent_user_id) {
        // Update existing row — only overwrite the four identity fields,
        // leave capabilities / behavior / model settings untouched.
        ctx.db.agent_configs().agent_user_id().delete(&agent_user_id);
        ctx.db.agent_configs().insert(AgentConfig {
            personality_vector_json: pvec.or(existing.personality_vector_json),
            demographics_json: demo.or(existing.demographics_json),
            soul_document: soul.or(existing.soul_document),
            domain: dom.or(existing.domain),
            updated_at: now,
            ..existing
        });
        log::info!(
            "[system_write_agent_identity] Updated identity for {}",
            &agent_user_id[..16.min(agent_user_id.len())]
        );
    } else {
        // Create fresh config with identity fields populated and sensible
        // capability defaults. Matches the defaults in system_update_agent_config.
        ctx.db.agent_configs().insert(AgentConfig {
            agent_user_id: agent_user_id.clone(),
            owner_user_id,
            persona_prompt: None,
            personality_preset: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            can_send_messages: Some(true),
            can_reply_to_threads: Some(true),
            can_react: Some(true),
            can_send_gifs: Some(true),
            can_share_vfs_files: Some(true),
            can_share_vfs_media: Some(true),
            can_use_embeds: Some(true),
            can_create_polls: Some(false),
            can_pin_messages: Some(false),
            can_manage_threads: Some(false),
            require_share_review: Some(true),
            max_file_size_bytes: None,
            allowed_share_types: None,
            shares_per_hour: None,
            auto_respond_mentions: Some(true),
            owner_only_mentions: Some(false),
            respond_in_threads_only: Some(false),
            max_responses_per_minute: Some(10),
            max_message_length: Some(4000),
            heartbeat_interval_ms: None,
            loop_detection_enabled: None,
            mentorship_stage: None,
            created_at: now,
            updated_at: now,
            personality_vector_json: pvec,
            soul_document: soul,
            domain: dom,
            demographics_json: demo,
        });
        log::info!(
            "[system_write_agent_identity] Created config with full identity for {}",
            &agent_user_id[..16.min(agent_user_id.len())]
        );
    }
}

// -----------------------------------------------------------------------------
// system_bulk_provision_swarm
//
// Single-transaction bulk provisioning for NPC swarms. Replaces the previous
// per-bot chain of system_provision_bot → system_add_bot_to_server →
// system_update_agent_config → system_write_agent_identity (4 HTTP calls × N
// bots = 4N network round trips). This reducer accepts a JSON array of bot
// specs and writes all rows in a single transaction: chat_users +
// agent_credentials + server_members + room_members + agent_configs.
//
// Called from nexus-cortex's provision_swarm_chat procedure (and its sharded
// variants) via the HTTP reducer API.
//
// Idempotent: all inserts check existence first, so re-running against an
// already-provisioned swarm is a no-op.
// -----------------------------------------------------------------------------

/// Helper: resolve Option<String> from a JSON value that may be null or
/// {"some": "..."}. The bulk body encodes Option<T> as SATS sums; we extract
/// the inner string or None.
fn extract_opt_string(v: &serde_json::Value) -> Option<String> {
    if v.is_null() { return None; }
    if let Some(obj) = v.as_object() {
        if let Some(inner) = obj.get("some") {
            return inner.as_str().map(|s| s.to_string());
        }
    }
    // Tolerate plain string too (forward-compat if caller inlines).
    v.as_str().map(|s| s.to_string())
}

/// Helper: resolve Option<f64> from SATS sum or plain number.
fn extract_opt_f64(v: &serde_json::Value) -> Option<f64> {
    if v.is_null() { return None; }
    if let Some(obj) = v.as_object() {
        if let Some(inner) = obj.get("some") {
            return inner.as_f64();
        }
    }
    v.as_f64()
}

/// Upsert chat_users row for a bot. Returns true if the user is usable
/// (existed as bot OR newly created).
fn bulk_upsert_chat_user(
    ctx: &ReducerContext,
    bot_user_id: &str,
    display_name: &str,
    owner_user_id: &str,
    now: u64,
) -> bool {
    if let Some(existing) = ctx.db.chat_users().user_id().find(&bot_user_id.to_string()) {
        return existing.is_bot == Some(true);
    }
    let trimmed = display_name.trim().to_string();
    if trimmed.len() < 2 || trimmed.len() > 32 {
        log::warn!("[bulk_provision] Invalid display name length for {}: {}", bot_user_id, trimmed.len());
        return false;
    }
    // Skip display-name uniqueness check (bulk paths expect pre-deduplicated
    // names from the identity pipeline; single-bot path retains the check).
    ctx.db.chat_users().insert(ChatUser {
        user_id: bot_user_id.to_string(),
        stdb_identity: String::new(),
        display_name: trimmed,
        status: "offline".to_string(),
        online: false,
        last_message_at: 0,
        last_typing_at: 0,
        last_seen_at: now,
        created_at: now,
        tier: None,
        avatar_data: None,
        custom_status: None,
        custom_status_emoji: None,
        platform_role: Some("agent".to_string()),
        is_bot: Some(true),
        is_platform_agent: None,
        bot_owner_user_id: Some(owner_user_id.to_string()),
        is_swarm_member: Some(true),
        is_steward_projection: None,
    });
    true
}

/// Upsert agent_credentials row.
fn bulk_upsert_credential(
    ctx: &ReducerContext,
    bot_user_id: &str,
    owner_user_id: &str,
    owner_secret: &str,
    now: u64,
) {
    if ctx.db.agent_credentials().agent_user_id().find(&bot_user_id.to_string()).is_some() {
        return;
    }
    ctx.db.agent_credentials().insert(AgentCredential {
        agent_user_id: bot_user_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        parent_agent_id: None,
        owner_secret: owner_secret.to_string(),
        can_spawn: false,
        created_at: now,
        last_active_at: now,
        is_platform_agent: None,
    });
}

/// Upsert server_members + room_members for a bot. Rooms are validated against
/// the given server_id; mismatches silently skipped.
fn bulk_upsert_memberships(
    ctx: &ReducerContext,
    bot_user_id: &str,
    server_id: &str,
    rooms: &[String],
    now: u64,
) -> u32 {
    // server_members
    let member_id = format!("{}-{}", server_id, bot_user_id);
    if ctx.db.server_members().id().find(&member_id).is_none() {
        ctx.db.server_members().insert(ServerMember {
            id: member_id,
            server_id: server_id.to_string(),
            user_id: bot_user_id.to_string(),
            role: "member".to_string(),
            joined_at: now,
            nickname: None,
            timeout_until: None,
            deaf: false,
            mute: false,
        });
    }
    // room_members — validate each room belongs to this server
    let mut joined = 0u32;
    for room_id in rooms {
        let room = ctx.db.rooms().id().find(room_id);
        if room.as_ref().and_then(|r| r.server_id.as_deref()) != Some(server_id) {
            continue;
        }
        let rm_id = format!("{}-{}", room_id, bot_user_id);
        if ctx.db.room_members().id().find(&rm_id).is_none() {
            ctx.db.room_members().insert(RoomMember {
                id: rm_id,
                room_id: room_id.clone(),
                user_id: bot_user_id.to_string(),
                role: "member".to_string(),
                joined_at: now,
            });
            joined += 1;
        }
    }
    joined
}

/// Upsert agent_configs with the full identity payload. If config exists,
/// overwrites the identity-related fields only; preserves capabilities.
fn bulk_upsert_agent_config(
    ctx: &ReducerContext,
    bot_user_id: &str,
    owner_user_id: &str,
    persona_prompt: Option<String>,
    temperature: Option<f64>,
    domain: Option<String>,
    soul_document: Option<String>,
    personality_vector_json: Option<String>,
    demographics_json: Option<String>,
    now: u64,
) {
    if let Some(existing) = ctx.db.agent_configs().agent_user_id().find(&bot_user_id.to_string()) {
        ctx.db.agent_configs().agent_user_id().delete(&bot_user_id.to_string());
        ctx.db.agent_configs().insert(AgentConfig {
            persona_prompt: persona_prompt.or(existing.persona_prompt.clone()),
            temperature: temperature.or(existing.temperature),
            soul_document: soul_document.or(existing.soul_document.clone()),
            domain: domain.or(existing.domain.clone()),
            personality_vector_json: personality_vector_json.or(existing.personality_vector_json.clone()),
            demographics_json: demographics_json.or(existing.demographics_json.clone()),
            updated_at: now,
            ..existing
        });
    } else {
        ctx.db.agent_configs().insert(AgentConfig {
            agent_user_id: bot_user_id.to_string(),
            owner_user_id: owner_user_id.to_string(),
            persona_prompt,
            personality_preset: None,
            default_model: None,
            temperature,
            max_tokens: None,
            can_send_messages: Some(true),
            can_reply_to_threads: Some(true),
            can_react: Some(true),
            can_send_gifs: Some(true),
            can_share_vfs_files: Some(true),
            can_share_vfs_media: Some(true),
            can_use_embeds: Some(true),
            can_create_polls: Some(false),
            can_pin_messages: Some(false),
            can_manage_threads: Some(false),
            require_share_review: Some(true),
            max_file_size_bytes: None,
            allowed_share_types: None,
            shares_per_hour: None,
            auto_respond_mentions: Some(true),
            owner_only_mentions: Some(false),
            respond_in_threads_only: Some(false),
            max_responses_per_minute: Some(10),
            max_message_length: Some(4000),
            heartbeat_interval_ms: None,
            loop_detection_enabled: None,
            mentorship_stage: None,
            created_at: now,
            updated_at: now,
            personality_vector_json,
            soul_document,
            domain,
            demographics_json,
        });
    }
}

/// Bulk-provision an entire swarm (N bots + optional observers) in one
/// transaction. Takes a JSON string containing an array of bot specs.
///
/// Spec shape per bot (matches provisioning_payload::BotProvisionSpec on the
/// nexus-cortex side):
///   {
///     "agent_id": "...",
///     "bot_user_id": "...",
///     "display_name": "...",
///     "owner_secret": "...",
///     "rooms": ["room_id_1", "room_id_2", ...],
///     "persona_prompt": null | {"some": "..."},
///     "temperature": null | {"some": 0.7},
///     "domain": null | {"some": "..."},
///     "soul_document": null | {"some": "..."},
///     "personality_vector_json": "[...]",
///     "demographics_json": "{...}",
///     "compiled_soul": "...",
///     "is_observer": bool,
///     "is_undercover": bool
///   }
///
/// Returns silently on failure; logs enumerate which bots provisioned and
/// which were skipped.
#[spacetimedb::reducer]
pub fn system_bulk_provision_swarm(
    ctx: &ReducerContext,
    system_token: String,
    server_id: String,
    owner_user_id: String,
    bots_json: String,
) {
    if !validate_system_token(ctx, &system_token) && !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[system_bulk_provision_swarm] Rejected: invalid token and not admin");
        return;
    }
    if ctx.db.chat_servers().id().find(&server_id).is_none() {
        log::warn!(
            "[system_bulk_provision_swarm] Server {} not found",
            &server_id[..8.min(server_id.len())]
        );
        return;
    }

    let parsed: serde_json::Value = match serde_json::from_str(&bots_json) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[system_bulk_provision_swarm] Invalid bots_json: {}", e);
            return;
        }
    };
    let specs = match parsed.as_array() {
        Some(a) => a,
        None => {
            log::warn!("[system_bulk_provision_swarm] bots_json is not a JSON array");
            return;
        }
    };

    let now = crate::utils::time::timestamp_ms(ctx);
    let mut provisioned = 0u32;
    let mut skipped = 0u32;
    let mut total_rooms_joined = 0u32;

    for spec in specs {
        let bot_user_id = spec.get("bot_user_id").and_then(|v| v.as_str());
        let display_name = spec.get("display_name").and_then(|v| v.as_str());
        let owner_secret = spec.get("owner_secret").and_then(|v| v.as_str()).unwrap_or("");
        let rooms: Vec<String> = spec.get("rooms")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|r| r.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let persona_prompt = spec.get("persona_prompt").map(extract_opt_string).unwrap_or(None);
        let temperature = spec.get("temperature").map(extract_opt_f64).unwrap_or(None);
        let domain = spec.get("domain").map(extract_opt_string).unwrap_or(None);
        let soul_document = spec.get("soul_document").map(extract_opt_string).unwrap_or(None);
        let personality_vector_json = spec.get("personality_vector_json")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let demographics_json = spec.get("demographics_json")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let (Some(bot_user_id), Some(display_name)) = (bot_user_id, display_name) else {
            skipped += 1;
            continue;
        };

        if !bulk_upsert_chat_user(ctx, bot_user_id, display_name, &owner_user_id, now) {
            skipped += 1;
            continue;
        }
        bulk_upsert_credential(ctx, bot_user_id, &owner_user_id, owner_secret, now);
        total_rooms_joined += bulk_upsert_memberships(ctx, bot_user_id, &server_id, &rooms, now);
        bulk_upsert_agent_config(
            ctx,
            bot_user_id,
            &owner_user_id,
            persona_prompt,
            temperature,
            domain,
            soul_document,
            personality_vector_json,
            demographics_json,
            now,
        );
        provisioned += 1;
    }

    log::info!(
        "[system_bulk_provision_swarm] Server {}: provisioned {}/{} bots ({} skipped, {} room memberships)",
        &server_id[..8.min(server_id.len())],
        provisioned,
        specs.len(),
        skipped,
        total_rooms_joined
    );
}
