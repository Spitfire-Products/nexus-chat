//! Agent lifecycle reducers: provision, deprovision, identity registration, cleanup.

use spacetimedb::{ReducerContext, ScheduleAt, Table};
use std::time::Duration;
use crate::tables::*;
use crate::tables::agents::{agent_credentials, agent_spawn_limits, agent_cleanup_jobs};
use crate::tables::auth::user_identity_links;
use crate::tables::users::chat_users;
use crate::tables::rooms::rooms;
use crate::tables::room_members::room_members;
use crate::tables::messages::messages;
use crate::utils::auth::{get_caller_user_id, is_platform_admin, sender_hex, register_identity_link};

/// One hour in milliseconds — idle threshold for sub-agent cleanup.
const IDLE_THRESHOLD_MS: u64 = 60 * 60 * 1000;

/// Validate a bot display name: 2-32 chars, plain ASCII alphanumeric + hyphens + single spaces.
///
/// Allowed: ASCII letters/digits, hyphens, and single spaces between other characters
/// (e.g. "Katharina Novikov", "R2-D2", "Agent-01", "Dr House").
/// Disallowed: leading/trailing whitespace or hyphens, consecutive spaces, punctuation,
/// accented letters, non-Latin scripts, emoji, and control characters.
fn validate_bot_display_name(name: &str) -> Result<(), &'static str> {
    if name.len() < 2 || name.len() > 32 {
        return Err("Display name must be 2-32 characters");
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err("Display name cannot start or end with a hyphen");
    }
    if name.starts_with(' ') || name.ends_with(' ') {
        return Err("Display name cannot start or end with a space");
    }
    if name.contains("  ") {
        return Err("Display name cannot contain consecutive spaces");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ' ') {
        return Err("Display name must contain only plain letters, numbers, hyphens, and spaces");
    }
    Ok(())
}

#[cfg(test)]
mod display_name_tests {
    use super::validate_bot_display_name;

    #[test] fn accepts_simple_alphanumeric() { assert!(validate_bot_display_name("Agent01").is_ok()); }
    #[test] fn accepts_hyphen() { assert!(validate_bot_display_name("R2-D2").is_ok()); }
    #[test] fn accepts_first_last_name() { assert!(validate_bot_display_name("Katharina Novikov").is_ok()); }
    #[test] fn accepts_three_word_name() { assert!(validate_bot_display_name("Dr John Smith").is_ok()); }
    #[test] fn accepts_hyphen_and_space() { assert!(validate_bot_display_name("Agent-01 Prime").is_ok()); }

    #[test] fn rejects_too_short() { assert!(validate_bot_display_name("A").is_err()); }
    #[test] fn rejects_too_long() { assert!(validate_bot_display_name(&"A".repeat(33)).is_err()); }
    #[test] fn rejects_leading_hyphen() { assert!(validate_bot_display_name("-Alice").is_err()); }
    #[test] fn rejects_trailing_hyphen() { assert!(validate_bot_display_name("Alice-").is_err()); }
    #[test] fn rejects_leading_space() { assert!(validate_bot_display_name(" Alice").is_err()); }
    #[test] fn rejects_trailing_space() { assert!(validate_bot_display_name("Alice ").is_err()); }
    #[test] fn rejects_double_space() { assert!(validate_bot_display_name("Alice  Smith").is_err()); }
    #[test] fn rejects_punctuation() { assert!(validate_bot_display_name("Alice.Smith").is_err()); }
    #[test] fn rejects_underscore() { assert!(validate_bot_display_name("Alice_Smith").is_err()); }
    #[test] fn rejects_accented_letters() { assert!(validate_bot_display_name("Renée Dupont").is_err()); }
    #[test] fn rejects_cjk() { assert!(validate_bot_display_name("田中 太郎").is_err()); }
    #[test] fn rejects_cyrillic() { assert!(validate_bot_display_name("Иван Петров").is_err()); }
    #[test] fn rejects_emoji() { assert!(validate_bot_display_name("Alice 🦀").is_err()); }
    #[test] fn rejects_non_ascii_hyphen() {
        // en-dash (–) is NOT an ASCII hyphen (-)
        assert!(validate_bot_display_name("Agent–01").is_err());
    }
}

/// Check if a bot display name is already taken (case-insensitive).
fn is_bot_name_taken(ctx: &ReducerContext, display_name: &str) -> bool {
    let lower = display_name.to_ascii_lowercase();
    ctx.db.chat_users().iter()
        .any(|u| u.is_bot == Some(true) && u.display_name.to_ascii_lowercase() == lower)
}

/// Provision a new agent — callable by platform admins or agents with can_spawn=true.
///
/// Creates a ChatUser with is_bot=true and stores credentials in the private
/// agent_credentials table. The owner_secret is caller-generated (like webhook tokens)
/// and stored server-side for the agent's standalone connection to prove ownership.
#[spacetimedb::reducer]
pub fn provision_agent(
    ctx: &ReducerContext,
    agent_user_id: String,
    display_name: String,
    owner_secret: String,
    can_spawn: bool,
    room_ids: Vec<String>,
    is_platform_agent: bool,
) {
    let Some(caller_user_id) = get_caller_user_id(ctx) else {
        log::warn!("[provision_agent] Unauthorized: no identity link");
        return;
    };

    // Determine if caller is human or agent, and resolve owner_user_id + parent_agent_id
    let (owner_user_id, parent_agent_id) = if let Some(caller_cred) = ctx.db.agent_credentials().agent_user_id().find(&caller_user_id) {
        // Caller is an agent — must have can_spawn
        if !caller_cred.can_spawn {
            log::warn!("[provision_agent] Agent {} does not have spawn permission", &caller_user_id[..8.min(caller_user_id.len())]);
            return;
        }
        // Depth check: if parent agent itself has a parent, reject (max depth = 1)
        if caller_cred.parent_agent_id.is_some() {
            log::warn!("[provision_agent] Rejected: sub-agents cannot spawn sub-agents (max depth = 1)");
            return;
        }
        // Owner chains to human
        (caller_cred.owner_user_id.clone(), Some(caller_user_id.clone()))
    } else {
        // Caller is human — must be platform admin
        if !is_platform_admin(ctx) {
            log::warn!("[provision_agent] Rejected: user {} is not platform admin", &caller_user_id[..8.min(caller_user_id.len())]);
            return;
        }
        (caller_user_id.clone(), None)
    };

    // Check spawn limit for this owner (platform admins bypass)
    if !is_platform_admin(ctx) {
        let active_count = ctx.db.agent_credentials().iter()
            .filter(|c| c.owner_user_id == owner_user_id)
            .count() as u32;

        let owner_tier = ctx.db.chat_users().user_id().find(&owner_user_id)
            .and_then(|u| u.tier.clone())
            .unwrap_or_else(|| "free".to_string());

        let max_agents = ctx.db.agent_spawn_limits().tier().find(&owner_tier)
            .map(|l| l.max_concurrent_agents)
            .unwrap_or(0);

        if active_count >= max_agents {
            log::warn!("[provision_agent] Spawn limit reached: {}/{} for tier {}", active_count, max_agents, owner_tier);
            return;
        }
    }

    // Check agent_user_id doesn't already exist
    if ctx.db.chat_users().user_id().find(&agent_user_id).is_some() {
        log::warn!("[provision_agent] User {} already exists", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    // Validate and enforce unique bot display name
    if let Err(reason) = validate_bot_display_name(&display_name) {
        log::warn!("[provision_agent] Invalid display name '{}': {}", display_name, reason);
        return;
    }
    if is_bot_name_taken(ctx, &display_name) {
        log::warn!("[provision_agent] Bot display name '{}' is already taken", display_name);
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Create ChatUser for the agent
    ctx.db.chat_users().insert(ChatUser {
        user_id: agent_user_id.clone(),
        stdb_identity: String::new(), // Will be set by register_agent_identity
        display_name,
        status: "online".to_string(),
        online: false, // Not connected yet
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
        is_platform_agent: if is_platform_agent { Some(true) } else { None },
        bot_owner_user_id: Some(owner_user_id.clone()),
        is_swarm_member: Some(false), // provision_agent = admin-created, not swarm
        is_steward_projection: None,
    });

    // Store credentials (private table — never sent to clients)
    ctx.db.agent_credentials().insert(AgentCredential {
        agent_user_id: agent_user_id.clone(),
        owner_user_id: owner_user_id.clone(),
        parent_agent_id,
        owner_secret,
        can_spawn,
        created_at: now,
        last_active_at: now,
        is_platform_agent: if is_platform_agent { Some(true) } else { None },
    });

    // Auto-join rooms
    for room_id in &room_ids {
        if ctx.db.rooms().id().find(room_id).is_none() {
            log::warn!("[provision_agent] Room {} not found, skipping", room_id);
            continue;
        }
        let member_id = format!("{}-{}", room_id, agent_user_id);
        if ctx.db.room_members().id().find(&member_id).is_some() {
            continue; // Already a member
        }
        ctx.db.room_members().insert(RoomMember {
            id: member_id,
            room_id: room_id.clone(),
            user_id: agent_user_id.clone(),
            role: "member".to_string(),
            joined_at: now,
        });
    }

    log::info!("[provision_agent] Agent {} provisioned by {} (owner: {})",
        &agent_user_id[..8.min(agent_user_id.len())],
        &caller_user_id[..8.min(caller_user_id.len())],
        &owner_user_id[..8.min(owner_user_id.len())]);
}

/// Register an agent's standalone SpacetimeDB identity.
///
/// Called by the agent's NEW connection (separate from the human's) to link
/// its SpacetimeDB identity to the bot user_id. The owner_secret proves
/// the connection is authorized without needing the human's identity.
#[spacetimedb::reducer]
pub fn register_agent_identity(ctx: &ReducerContext, agent_user_id: String, owner_secret: String) {
    // Verify agent exists and is a bot
    let Some(user) = ctx.db.chat_users().user_id().find(&agent_user_id) else {
        log::warn!("[register_agent_identity] Agent user {} not found", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    };
    if user.is_bot != Some(true) {
        log::warn!("[register_agent_identity] User {} is not a bot", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    // Verify owner_secret matches
    let Some(cred) = ctx.db.agent_credentials().agent_user_id().find(&agent_user_id) else {
        log::warn!("[register_agent_identity] No credentials for agent {}", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    };
    if cred.owner_secret != owner_secret {
        log::warn!("[register_agent_identity] Invalid secret for agent {}", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    // Link this connection's identity to the agent user_id
    let stdb_identity = sender_hex(ctx);
    register_identity_link(ctx, &stdb_identity, &agent_user_id);

    // Update the ChatUser's stdb_identity and set online
    let now = crate::timestamp_ms(ctx);
    ctx.db.chat_users().user_id().delete(&agent_user_id);
    ctx.db.chat_users().insert(ChatUser {
        stdb_identity: stdb_identity.clone(),
        online: true,
        last_seen_at: now,
        ..user
    });

    // Update last_active_at on credentials
    ctx.db.agent_credentials().agent_user_id().delete(&agent_user_id);
    ctx.db.agent_credentials().insert(AgentCredential {
        last_active_at: now,
        ..cred
    });

    log::info!("[register_agent_identity] Agent {} linked to identity {}...",
        &agent_user_id[..8.min(agent_user_id.len())],
        &stdb_identity[..16.min(stdb_identity.len())]);
}

/// Deprovision an agent — removes ChatUser, credentials, memberships, identity links.
/// Cascades to sub-agents first.
#[spacetimedb::reducer]
pub fn deprovision_agent(ctx: &ReducerContext, agent_user_id: String) {
    let Some(caller_user_id) = get_caller_user_id(ctx) else {
        log::warn!("[deprovision_agent] Unauthorized: no identity link");
        return;
    };

    let Some(cred) = ctx.db.agent_credentials().agent_user_id().find(&agent_user_id) else {
        log::warn!("[deprovision_agent] Agent {} not found", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    };

    // Verify caller is owner or platform admin
    if cred.owner_user_id != caller_user_id && !is_platform_admin(ctx) {
        log::warn!("[deprovision_agent] User {} is not owner of agent {}", &caller_user_id[..8.min(caller_user_id.len())], &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    deprovision_agent_internal(ctx, &agent_user_id);
}

/// Internal deprovision logic (called by reducer and cleanup job).
fn deprovision_agent_internal(ctx: &ReducerContext, agent_user_id: &str) {
    // Cascade: deprovision any sub-agents first
    let sub_agents: Vec<String> = ctx.db.agent_credentials().iter()
        .filter(|c| c.parent_agent_id.as_deref() == Some(agent_user_id))
        .map(|c| c.agent_user_id.clone())
        .collect();
    for sub_id in sub_agents {
        deprovision_agent_internal(ctx, &sub_id);
    }

    // Remove credentials
    ctx.db.agent_credentials().agent_user_id().delete(&agent_user_id.to_string());

    // Remove agent config
    ctx.db.agent_configs().agent_user_id().delete(&agent_user_id.to_string());

    // Remove room memberships
    let memberships: Vec<String> = ctx.db.room_members().iter()
        .filter(|m| m.user_id == agent_user_id)
        .map(|m| m.id.clone())
        .collect();
    for mid in memberships {
        ctx.db.room_members().id().delete(&mid);
    }

    // Remove identity links
    let links: Vec<String> = ctx.db.user_identity_links().iter()
        .filter(|l| l.user_id == agent_user_id)
        .map(|l| l.stdb_identity.clone())
        .collect();
    for link_id in links {
        ctx.db.user_identity_links().stdb_identity().delete(&link_id);
    }

    // Remove ChatUser
    ctx.db.chat_users().user_id().delete(&agent_user_id.to_string());

    // Cascade: delete all DM rooms involving this bot
    let dm_rooms: Vec<String> = ctx.db.rooms().iter()
        .filter(|r| r.is_dm && r.name.contains(agent_user_id))
        .map(|r| r.id.clone())
        .collect();

    for room_id in &dm_rooms {
        // Delete room members
        let members: Vec<String> = ctx.db.room_members().iter()
            .filter(|m| m.room_id == *room_id)
            .map(|m| m.id.clone())
            .collect();
        for mid in &members {
            ctx.db.room_members().id().delete(mid);
        }
        // Delete messages
        let msgs: Vec<String> = ctx.db.messages().iter()
            .filter(|m| m.room_id == *room_id)
            .map(|m| m.id.clone())
            .collect();
        for mid in &msgs {
            ctx.db.messages().id().delete(mid);
        }
        // Delete room
        ctx.db.rooms().id().delete(room_id);
    }

    log::info!("[deprovision_agent] Agent {} deprovisioned, cleaned up {} DM rooms", &agent_user_id[..8.min(agent_user_id.len())], dm_rooms.len());
}

/// Scheduled: cleanup agents idle for >1 hour (sub-agents only, main agents exempt).
/// Re-schedules itself every 5 minutes.
#[spacetimedb::reducer]
pub fn cleanup_idle_agents(ctx: &ReducerContext, _job: AgentCleanupJob) {
    if !ctx.sender_auth().is_internal() { return; }

    let now = crate::timestamp_ms(ctx);
    let threshold = now.saturating_sub(IDLE_THRESHOLD_MS);

    // Only clean up sub-agents (those with parent_agent_id)
    let idle_agents: Vec<String> = ctx.db.agent_credentials().iter()
        .filter(|c| c.parent_agent_id.is_some() && c.last_active_at < threshold)
        .map(|c| c.agent_user_id.clone())
        .collect();

    for agent_id in &idle_agents {
        log::info!("[cleanup_idle_agents] Deprovisioning idle sub-agent {}", &agent_id[..8.min(agent_id.len())]);
        deprovision_agent_internal(ctx, agent_id);
    }

    if !idle_agents.is_empty() {
        log::info!("[cleanup_idle_agents] Cleaned up {} idle sub-agents", idle_agents.len());
    }

    // Re-schedule next cleanup in 5 minutes
    ctx.db.agent_cleanup_jobs().insert(AgentCleanupJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_secs(300).into()),
    });
}

/// Seed default agent spawn limits — admin only.
/// Idempotent: skips if tier row already exists.
#[spacetimedb::reducer]
pub fn seed_agent_spawn_limits(ctx: &ReducerContext) {
    if !is_platform_admin(ctx) {
        log::warn!("[seed_agent_spawn_limits] Rejected: not platform admin");
        return;
    }

    // (tier, max_agents, max_servers)
    // admin/developer bumped to 1000 to accommodate platform-scale swarm
    // simulations (NPC swarms can run 100+ bots per simulation and dev
    // testing stacks many runs before cleanup).
    let defaults: [(&str, u32, u32); 6] = [
        ("free", 0, 0),
        ("pro", 3, 3),
        ("creator", 10, 10),
        ("team", 25, 25),
        ("admin", 1000, 1000),
        ("developer", 1000, 1000),
    ];

    for (tier, max_agents, max_svr) in defaults {
        if ctx.db.agent_spawn_limits().tier().find(&tier.to_string()).is_some() {
            continue;
        }
        ctx.db.agent_spawn_limits().insert(AgentSpawnLimit {
            tier: tier.to_string(),
            max_concurrent_agents: max_agents,
            bot_rate_limit_ms: Some(5000), // 5s default
            max_servers: Some(max_svr),
        });
    }

    log::info!("[seed_agent_spawn_limits] Spawn limits seeded");
}

/// Update agent spawn limits for a tier — admin only.
#[spacetimedb::reducer]
pub fn update_agent_spawn_limits(
    ctx: &ReducerContext,
    tier: String,
    max_concurrent_agents: u32,
    bot_rate_limit_ms: Option<u32>,
    max_servers: Option<u32>,
) {
    if !is_platform_admin(ctx) {
        log::warn!("[update_agent_spawn_limits] Rejected: not platform admin");
        return;
    }

    let Some(existing) = ctx.db.agent_spawn_limits().tier().find(&tier) else {
        log::warn!("[update_agent_spawn_limits] Tier {} not found", tier);
        return;
    };

    ctx.db.agent_spawn_limits().tier().delete(&tier);
    ctx.db.agent_spawn_limits().insert(AgentSpawnLimit {
        tier,
        max_concurrent_agents,
        bot_rate_limit_ms: if bot_rate_limit_ms.is_some() { bot_rate_limit_ms } else { existing.bot_rate_limit_ms },
        max_servers: if max_servers.is_some() { max_servers } else { existing.max_servers },
    });
}

/// Provision a user agent — callable by any registered human user (not admin-only).
///
/// Creates a personal agent ChatUser with is_bot=true. Tier-gated: free=0 agents.
/// Bot display names must be globally unique (case-insensitive) and serve as @mention handles.
#[spacetimedb::reducer]
pub fn provision_user_agent(
    ctx: &ReducerContext,
    agent_user_id: String,
    display_name: String,
    owner_secret: String,
) {
    let Some(caller_user_id) = get_caller_user_id(ctx) else {
        log::warn!("[provision_user_agent] Unauthorized: no identity link");
        return;
    };

    // Must be a human (not a bot)
    let Some(caller_user) = ctx.db.chat_users().user_id().find(&caller_user_id) else {
        log::warn!("[provision_user_agent] Caller user {} not found", &caller_user_id[..8.min(caller_user_id.len())]);
        return;
    };
    if caller_user.is_bot == Some(true) {
        log::warn!("[provision_user_agent] Bots cannot self-provision user agents");
        return;
    }

    // Check spawn limit for this user's tier
    let owner_tier = caller_user.tier.clone().unwrap_or_else(|| "free".to_string());
    let active_count = ctx.db.agent_credentials().iter()
        .filter(|c| c.owner_user_id == caller_user_id)
        .count() as u32;
    let max_agents = ctx.db.agent_spawn_limits().tier().find(&owner_tier)
        .map(|l| l.max_concurrent_agents)
        .unwrap_or(0);
    if active_count >= max_agents {
        log::warn!("[provision_user_agent] Spawn limit reached: {}/{} for tier {}", active_count, max_agents, owner_tier);
        return;
    }

    // Validate display name
    if let Err(reason) = validate_bot_display_name(&display_name) {
        log::warn!("[provision_user_agent] Invalid display name '{}': {}", display_name, reason);
        return;
    }
    if is_bot_name_taken(ctx, &display_name) {
        log::warn!("[provision_user_agent] Bot display name '{}' is already taken", display_name);
        return;
    }

    // Check agent_user_id doesn't already exist
    if ctx.db.chat_users().user_id().find(&agent_user_id).is_some() {
        log::warn!("[provision_user_agent] User {} already exists", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Create ChatUser for the agent (personal — NOT platform agent, NOT swarm)
    ctx.db.chat_users().insert(ChatUser {
        user_id: agent_user_id.clone(),
        stdb_identity: String::new(),
        display_name,
        status: "online".to_string(),
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
        is_platform_agent: None, // Personal user agent, not platform
        bot_owner_user_id: Some(caller_user_id.clone()),
        is_swarm_member: Some(false), // Personal — appears in My Agents
        is_steward_projection: None,
    });

    // Store credentials (private table)
    ctx.db.agent_credentials().insert(AgentCredential {
        agent_user_id: agent_user_id.clone(),
        owner_user_id: caller_user_id.clone(),
        parent_agent_id: None,
        owner_secret,
        can_spawn: false,
        created_at: now,
        last_active_at: now,
        is_platform_agent: None,
    });

    log::info!("[provision_user_agent] User agent {} created by {}",
        &agent_user_id[..8.min(agent_user_id.len())],
        &caller_user_id[..8.min(caller_user_id.len())]);
}

/// Provision a personal bot with a pre-defined persona — used by the
/// starred-NPC "Promote to Personal" flow.
///
/// Same structure as provision_user_agent (caller becomes owner, bot starts
/// as is_swarm_member=false, counts against tier spawn limit), PLUS creates
/// the agent_config row populated with the SOUL/domain/temperature/persona
/// prompt so CORTEX can actually use the bot as a persona. If an agent_config
/// already exists for this user_id (e.g. a stale row from a prior deletion),
/// it's replaced with the new persona.
#[spacetimedb::reducer]
pub fn provision_personal_bot_with_persona(
    ctx: &ReducerContext,
    agent_user_id: String,
    display_name: String,
    owner_secret: String,
    soul_document: Option<String>,
    domain: Option<String>,
    temperature: Option<f64>,
    persona_prompt: Option<String>,
    personality_vector_json: Option<String>,
    demographics_json: Option<String>,
) {
    let Some(caller_user_id) = get_caller_user_id(ctx) else {
        log::warn!("[provision_personal_bot_with_persona] Unauthorized");
        return;
    };

    let Some(caller_user) = ctx.db.chat_users().user_id().find(&caller_user_id) else {
        log::warn!("[provision_personal_bot_with_persona] Caller {} not found",
            &caller_user_id[..8.min(caller_user_id.len())]);
        return;
    };
    if caller_user.is_bot == Some(true) {
        log::warn!("[provision_personal_bot_with_persona] Bots cannot provision personal agents");
        return;
    }

    // Spawn limit check (same as provision_user_agent)
    let owner_tier = caller_user.tier.clone().unwrap_or_else(|| "free".to_string());
    let active_count = ctx.db.agent_credentials().iter()
        .filter(|c| c.owner_user_id == caller_user_id)
        .count() as u32;
    let max_agents = ctx.db.agent_spawn_limits().tier().find(&owner_tier)
        .map(|l| l.max_concurrent_agents)
        .unwrap_or(0);
    if active_count >= max_agents {
        log::warn!("[provision_personal_bot_with_persona] Spawn limit reached: {}/{} for tier {}",
            active_count, max_agents, owner_tier);
        return;
    }

    if let Err(reason) = validate_bot_display_name(&display_name) {
        log::warn!("[provision_personal_bot_with_persona] Invalid display name '{}': {}", display_name, reason);
        return;
    }
    if is_bot_name_taken(ctx, &display_name) {
        log::warn!("[provision_personal_bot_with_persona] Display name '{}' taken", display_name);
        return;
    }
    if ctx.db.chat_users().user_id().find(&agent_user_id).is_some() {
        log::warn!("[provision_personal_bot_with_persona] User {} already exists",
            &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // 1. chat_users (personal bot)
    ctx.db.chat_users().insert(ChatUser {
        user_id: agent_user_id.clone(),
        stdb_identity: String::new(),
        display_name: display_name.clone(),
        status: "online".to_string(),
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
        bot_owner_user_id: Some(caller_user_id.clone()),
        is_swarm_member: Some(false),
        is_steward_projection: None,
    });

    // 2. agent_credentials
    ctx.db.agent_credentials().insert(AgentCredential {
        agent_user_id: agent_user_id.clone(),
        owner_user_id: caller_user_id.clone(),
        parent_agent_id: None,
        owner_secret,
        can_spawn: false,
        created_at: now,
        last_active_at: now,
        is_platform_agent: None,
    });

    // 3. agent_configs with persona fields populated
    ctx.db.agent_configs().agent_user_id().delete(&agent_user_id); // clear stale
    ctx.db.agent_configs().insert(crate::tables::agent_config::AgentConfig {
        agent_user_id: agent_user_id.clone(),
        owner_user_id: caller_user_id.clone(),
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

    log::info!("[provision_personal_bot_with_persona] Created personal bot '{}' ({}) for {}",
        display_name, &agent_user_id[..8.min(agent_user_id.len())],
        &caller_user_id[..8.min(caller_user_id.len())]);
}

/// Rename an agent — change its display name (which serves as its @mention handle).
/// Callable by agent owner or platform admin.
#[spacetimedb::reducer]
pub fn rename_agent(ctx: &ReducerContext, agent_user_id: String, new_display_name: String) {
    let Some(caller_user_id) = get_caller_user_id(ctx) else {
        log::warn!("[rename_agent] Unauthorized: no identity link");
        return;
    };

    let Some(cred) = ctx.db.agent_credentials().agent_user_id().find(&agent_user_id) else {
        log::warn!("[rename_agent] Agent {} not found", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    };

    // Verify caller is owner or platform admin
    if cred.owner_user_id != caller_user_id && !is_platform_admin(ctx) {
        log::warn!("[rename_agent] User {} is not owner of agent {}", &caller_user_id[..8.min(caller_user_id.len())], &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    // Validate new name
    if let Err(reason) = validate_bot_display_name(&new_display_name) {
        log::warn!("[rename_agent] Invalid display name '{}': {}", new_display_name, reason);
        return;
    }

    // Check uniqueness (excluding this agent's current name)
    let lower = new_display_name.to_ascii_lowercase();
    let name_taken = ctx.db.chat_users().iter()
        .any(|u| u.is_bot == Some(true) && u.user_id != agent_user_id && u.display_name.to_ascii_lowercase() == lower);
    if name_taken {
        log::warn!("[rename_agent] Bot display name '{}' is already taken", new_display_name);
        return;
    }

    // Update ChatUser display name
    let Some(user) = ctx.db.chat_users().user_id().find(&agent_user_id) else {
        log::warn!("[rename_agent] ChatUser {} not found", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    };
    ctx.db.chat_users().user_id().delete(&agent_user_id);
    ctx.db.chat_users().insert(ChatUser {
        display_name: new_display_name.clone(),
        ..user
    });

    log::info!("[rename_agent] Agent {} renamed to '{}'",
        &agent_user_id[..8.min(agent_user_id.len())], new_display_name);
}

/// Promote a swarm-origin bot to a personal bot.
///
/// Flips chat_users.is_swarm_member from true to false, which moves the
/// bot from the NPC Roster into CORTEX My Agents. The bot keeps its full
/// identity (personality, demographics, SOUL, memories) — only the
/// categorization flag changes. Caller must be the bot's owner (or an
/// admin). Idempotent — already-personal bots are a no-op.
#[spacetimedb::reducer]
pub fn promote_bot_to_personal(ctx: &ReducerContext, bot_user_id: String) {
    let caller_user_id = match get_caller_user_id(ctx) {
        Some(id) => id,
        None => { log::warn!("[promote_bot_to_personal] Caller not registered"); return; }
    };

    let user = match ctx.db.chat_users().user_id().find(&bot_user_id) {
        Some(u) => u,
        None => {
            log::warn!("[promote_bot_to_personal] Bot {} not found",
                &bot_user_id[..8.min(bot_user_id.len())]);
            return;
        }
    };

    if user.is_bot != Some(true) {
        log::warn!("[promote_bot_to_personal] {} is not a bot", &bot_user_id[..8.min(bot_user_id.len())]);
        return;
    }

    // Owner check: caller must own the bot OR be an admin
    let is_owner = user.bot_owner_user_id.as_deref() == Some(caller_user_id.as_str());
    let is_admin = crate::utils::auth::is_platform_admin(ctx);
    if !is_owner && !is_admin {
        log::warn!("[promote_bot_to_personal] Caller {} not owner of {} and not admin",
            &caller_user_id[..8.min(caller_user_id.len())],
            &bot_user_id[..8.min(bot_user_id.len())]);
        return;
    }

    if user.is_swarm_member != Some(true) {
        log::info!("[promote_bot_to_personal] {} already personal (no-op)",
            &bot_user_id[..8.min(bot_user_id.len())]);
        return;
    }

    ctx.db.chat_users().user_id().delete(&bot_user_id);
    ctx.db.chat_users().insert(ChatUser {
        is_swarm_member: Some(false),
        ..user
    });

    log::info!("[promote_bot_to_personal] Promoted {} to personal bot",
        &bot_user_id[..8.min(bot_user_id.len())]);
}

/// Admin: bulk-mark all `swarm-bot-*` prefixed bots as swarm members (backfill).
/// Use once after adding the is_swarm_member field to existing data.
#[spacetimedb::reducer]
pub fn admin_backfill_swarm_member_flag(ctx: &ReducerContext) {
    if !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[admin_backfill_swarm_member_flag] Admin only");
        return;
    }
    let to_update: Vec<_> = ctx.db.chat_users().iter()
        .filter(|u| u.is_bot == Some(true)
            && u.is_swarm_member.is_none()
            && u.user_id.starts_with("swarm-bot-"))
        .collect();
    let count = to_update.len();
    for user in to_update {
        let uid = user.user_id.clone();
        ctx.db.chat_users().user_id().delete(&uid);
        ctx.db.chat_users().insert(ChatUser {
            is_swarm_member: Some(true),
            ..user
        });
    }
    log::info!("[admin_backfill_swarm_member_flag] Flagged {} swarm bots", count);
}

/// Helper: get the bot rate limit for a user's tier (in milliseconds).
/// Returns the configured per-tier limit, or 5000ms as fallback.
pub fn get_bot_rate_limit_ms(ctx: &ReducerContext, user_id: &str) -> u64 {
    let tier = ctx.db.chat_users().user_id().find(&user_id.to_string())
        .and_then(|u| u.tier.clone())
        .unwrap_or_else(|| "free".to_string());

    ctx.db.agent_spawn_limits().tier().find(&tier)
        .and_then(|l| l.bot_rate_limit_ms)
        .map(|ms| ms as u64)
        .unwrap_or(5000)
}

/// Force-delete a bot from chat_users and all related tables.
/// Unlike deprovision_agent, this does NOT require agent_credentials
/// to exist — it directly removes the chat_user row and cascades
/// room memberships, messages authored by the bot, identity links,
/// agent_configs, and credentials (if present). Admin only.
///
/// Use case: cleaning up ill-formed swarm bots from early test runs
/// that were provisioned without credentials or with broken configs.
#[spacetimedb::reducer]
pub fn admin_force_delete_bot(ctx: &ReducerContext, bot_user_id: String) {
    if !is_platform_admin(ctx) {
        log::warn!("[admin_force_delete_bot] Rejected: not platform admin");
        return;
    }

    let user = ctx.db.chat_users().user_id().find(&bot_user_id);
    if user.is_none() {
        log::warn!("[admin_force_delete_bot] User '{}' not found", &bot_user_id[..16.min(bot_user_id.len())]);
        return;
    }

    // Credentials (may not exist for old swarm bots)
    ctx.db.agent_credentials().agent_user_id().delete(&bot_user_id);

    // Agent config
    ctx.db.agent_configs().agent_user_id().delete(&bot_user_id);

    // Room memberships
    let memberships: Vec<String> = ctx.db.room_members().iter()
        .filter(|m| m.user_id == bot_user_id)
        .map(|m| m.id.clone())
        .collect();
    for mid in memberships {
        ctx.db.room_members().id().delete(&mid);
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

    log::info!("[admin_force_delete_bot] Force-deleted bot '{}'", &bot_user_id[..16.min(bot_user_id.len())]);
}
