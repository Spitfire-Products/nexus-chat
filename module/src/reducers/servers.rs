//! Server management reducers: create, update, delete, join, leave, kick, ban, set role, admin bot management.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::servers::chat_servers;
use crate::tables::server_members::server_members;
use crate::tables::rooms::rooms;
use crate::tables::room_members::room_members;
use crate::tables::channel_categories::channel_categories;
use crate::tables::users::chat_users;
use crate::utils::validation::{
    ensure_chat_user, find_membership, find_server_membership, require_server_admin, require_server_owner,
    MAX_ROOM_NAME_LEN,
};

/// Maximum server name length (reuse room name limit)
const MAX_SERVER_NAME_LEN: usize = MAX_ROOM_NAME_LEN;

/// Create a new chat server. Creator auto-joins as owner.
#[spacetimedb::reducer]
pub fn create_server(
    ctx: &ReducerContext,
    id: String,
    name: String,
    audience_id: String,
    is_public: bool,
    default_tier: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_server] Unauthorized: no identity link");
        return;
    };

    if !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[create_server] Rejected: user {} is not platform admin", &user_id[..8.min(user_id.len())]);
        return;
    }

    let sender_hex = crate::utils::auth::sender_hex(ctx);
    ensure_chat_user(ctx, &user_id, &sender_hex);

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_SERVER_NAME_LEN {
        log::warn!("[create_server] Invalid server name length: {}", trimmed.len());
        return;
    }

    if ctx.db.chat_servers().id().find(&id).is_some() {
        log::warn!("[create_server] Server {} already exists", id);
        return;
    }

    let now = crate::timestamp_ms(ctx);

    ctx.db.chat_servers().insert(ChatServer {
        id: id.clone(),
        name: trimmed,
        description: String::new(),
        audience_id,
        owner_user_id: user_id.clone(),
        is_public,
        default_tier,
        icon_url: String::new(),
        created_at: now,
        updated_at: now,
        template: None,
    });

    // Auto-join creator as owner
    let member_id = format!("{}-{}", id, user_id);
    ctx.db.server_members().insert(ServerMember {
        id: member_id,
        server_id: id.clone(),
        user_id: user_id.clone(),
        role: "owner".to_string(),
        joined_at: now,
        nickname: None,
        timeout_until: None,
        deaf: false,
        mute: false,
    });

    // Auto-create @everyone default role
    crate::reducers::roles::create_default_role(ctx, &id);

    // Auto-create "General" category
    let cat_id = format!("{}-general-cat", id);
    ctx.db.channel_categories().insert(crate::tables::ChannelCategory {
        id: cat_id,
        server_id: id.clone(),
        name: "Text Channels".to_string(),
        sort_order: 0,
        created_at: now,
    });

    log::info!("[create_server] User {} created server {}", &user_id[..8.min(user_id.len())], &id[..8.min(id.len())]);
}

/// Update server settings. Requires server admin or owner.
#[spacetimedb::reducer]
pub fn update_server(
    ctx: &ReducerContext,
    server_id: String,
    name: String,
    description: String,
    audience_id: String,
    is_public: bool,
    default_tier: String,
    icon_url: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[update_server] Unauthorized");
        return;
    };

    if require_server_admin(ctx, &server_id, &user_id).is_none() {
        return;
    }

    let Some(server) = ctx.db.chat_servers().id().find(&server_id) else {
        log::warn!("[update_server] Server {} not found", server_id);
        return;
    };

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_SERVER_NAME_LEN {
        log::warn!("[update_server] Invalid server name length: {}", trimmed.len());
        return;
    }

    let now = crate::timestamp_ms(ctx);

    ctx.db.chat_servers().id().delete(&server_id);
    ctx.db.chat_servers().insert(ChatServer {
        name: trimmed,
        description,
        audience_id,
        is_public,
        default_tier,
        icon_url,
        updated_at: now,
        ..server
    });
}

/// Delete a server. Owner only. Unsets rooms' server_id and removes all server_members.
#[spacetimedb::reducer]
pub fn delete_server(ctx: &ReducerContext, server_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_server] Unauthorized");
        return;
    };

    if require_server_owner(ctx, &server_id, &user_id).is_none()
        && !crate::utils::auth::is_platform_admin(ctx)
    {
        return;
    }

    // Unset server_id on all rooms in this server
    let server_rooms: Vec<Room> = ctx.db.rooms().iter()
        .filter(|r| r.server_id.as_deref() == Some(&server_id))
        .collect();
    for room in server_rooms {
        let room_id = room.id.clone();
        ctx.db.rooms().id().delete(&room_id);
        ctx.db.rooms().insert(Room {
            server_id: None,
            ..room
        });
    }

    // Remove all server members
    let members: Vec<ServerMember> = ctx.db.server_members().iter()
        .filter(|m| m.server_id == server_id)
        .collect();
    for m in members {
        ctx.db.server_members().id().delete(&m.id);
    }

    // Delete the server
    ctx.db.chat_servers().id().delete(&server_id);
    log::info!("[delete_server] Server {} deleted by {}", &server_id[..8.min(server_id.len())], &user_id[..8.min(user_id.len())]);
}

/// Join a server as "member". Checks not banned.
#[spacetimedb::reducer]
pub fn join_server(ctx: &ReducerContext, server_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[join_server] Unauthorized");
        return;
    };

    if ctx.db.chat_servers().id().find(&server_id).is_none() {
        log::warn!("[join_server] Server {} not found", server_id);
        return;
    }

    // Check existing membership
    if let Some(existing) = find_server_membership(ctx, &server_id, &user_id) {
        if existing.role == "banned" {
            log::warn!("[join_server] User {} is banned from server {}", user_id, server_id);
            return;
        }
        // Already a member
        return;
    }

    let now = crate::timestamp_ms(ctx);
    let member_id = format!("{}-{}", server_id, user_id);
    ctx.db.server_members().insert(ServerMember {
        id: member_id,
        server_id,
        user_id,
        role: "member".to_string(),
        joined_at: now,
        nickname: None,
        timeout_until: None,
        deaf: false,
        mute: false,
    });
}

/// Leave a server. Owner cannot leave (must delete or transfer).
#[spacetimedb::reducer]
pub fn leave_server(ctx: &ReducerContext, server_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[leave_server] Unauthorized");
        return;
    };

    if let Some(membership) = find_server_membership(ctx, &server_id, &user_id) {
        if membership.role == "owner" {
            log::warn!("[leave_server] Owner cannot leave server — delete it instead");
            return;
        }
        ctx.db.server_members().id().delete(&membership.id);
    }
}

/// Kick a user from a server. Requires admin or owner.
#[spacetimedb::reducer]
pub fn kick_from_server(ctx: &ReducerContext, server_id: String, target_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[kick_from_server] Unauthorized");
        return;
    };

    if !crate::utils::auth::is_platform_admin(ctx) && require_server_admin(ctx, &server_id, &user_id).is_none() {
        return;
    }

    let Some(target) = find_server_membership(ctx, &server_id, &target_user_id) else {
        log::warn!("[kick_from_server] Target user {} not in server {}", target_user_id, server_id);
        return;
    };

    // Can't kick owner or other admins (only owner can demote admins)
    if target.role == "owner" || target.role == "admin" {
        log::warn!("[kick_from_server] Cannot kick {} with role {}", target_user_id, target.role);
        return;
    }

    ctx.db.server_members().id().delete(&target.id);

    // Cascade: remove room memberships across this server's rooms so we
    // don't leave orphan room_members rows that keep the bot/user in
    // channel member lists after they're kicked.
    let server_room_ids: Vec<String> = ctx.db.rooms().iter()
        .filter(|r| r.server_id.as_deref() == Some(&server_id))
        .map(|r| r.id.clone())
        .collect();
    let mut rooms_removed = 0u32;
    for room_id in &server_room_ids {
        let to_remove: Vec<String> = ctx.db.room_members().iter()
            .filter(|m| m.room_id == *room_id && m.user_id == target_user_id)
            .map(|m| m.id.clone())
            .collect();
        for id in to_remove {
            ctx.db.room_members().id().delete(&id);
            rooms_removed += 1;
        }
    }

    log::info!(
        "[kick_from_server] Kicked {} from server {} by {} ({} room memberships cascaded)",
        &target_user_id[..8.min(target_user_id.len())],
        &server_id[..8.min(server_id.len())],
        &user_id[..8.min(user_id.len())],
        rooms_removed
    );
}

/// Ban a user from a server. Sets role to "banned". Requires admin or owner.
#[spacetimedb::reducer]
pub fn ban_from_server(ctx: &ReducerContext, server_id: String, target_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[ban_from_server] Unauthorized");
        return;
    };

    if !crate::utils::auth::is_platform_admin(ctx) && require_server_admin(ctx, &server_id, &user_id).is_none() {
        return;
    }

    let member_id = format!("{}-{}", server_id, target_user_id);

    if let Some(target) = ctx.db.server_members().id().find(&member_id) {
        if target.role == "owner" {
            log::warn!("[ban_from_server] Cannot ban server owner");
            return;
        }
        ctx.db.server_members().id().delete(&member_id);
        ctx.db.server_members().insert(ServerMember {
            role: "banned".to_string(),
            ..target
        });
    } else {
        // Ban even non-members (preemptive ban)
        let now = crate::timestamp_ms(ctx);
        ctx.db.server_members().insert(ServerMember {
            id: member_id,
            server_id,
            user_id: target_user_id,
            role: "banned".to_string(),
            joined_at: now,
            nickname: None,
            timeout_until: None,
            deaf: false,
            mute: false,
        });
    }
}

/// Set a server member's role. Owner only.
#[spacetimedb::reducer]
pub fn set_server_member_role(
    ctx: &ReducerContext,
    server_id: String,
    target_user_id: String,
    role: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[set_server_member_role] Unauthorized");
        return;
    };

    if !crate::utils::auth::is_platform_admin(ctx) && require_server_owner(ctx, &server_id, &user_id).is_none() {
        return;
    }

    // Validate role
    let valid_roles = ["admin", "moderator", "member", "banned"];
    if !valid_roles.contains(&role.as_str()) {
        log::warn!("[set_server_member_role] Invalid role: {}", role);
        return;
    }

    let member_id = format!("{}-{}", server_id, target_user_id);
    let Some(target) = ctx.db.server_members().id().find(&member_id) else {
        log::warn!("[set_server_member_role] Target user {} not in server {}", target_user_id, server_id);
        return;
    };

    if target.role == "owner" {
        log::warn!("[set_server_member_role] Cannot change owner's role");
        return;
    }

    ctx.db.server_members().id().delete(&member_id);
    ctx.db.server_members().insert(ServerMember {
        role,
        ..target
    });
}

/// Create a user-owned server. Available to pro+ tier users.
/// Auto-joins the creator as owner and creates a default #general channel.
#[spacetimedb::reducer]
pub fn create_user_server(
    ctx: &ReducerContext,
    id: String,
    name: String,
    is_public: bool,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_user_server] Unauthorized: no identity link");
        return;
    };

    // Tier gate: pro+ required (pro, creator, team, admin, developer)
    let user = ctx.db.chat_users().user_id().find(&user_id);
    let tier = user.as_ref().and_then(|u| u.tier.clone()).unwrap_or_else(|| "free".to_string());
    let platform_role = user.as_ref().and_then(|u| u.platform_role.clone()).unwrap_or_default();

    // Platform admins/developers bypass tier check
    let is_privileged = platform_role == "admin" || platform_role == "developer";
    if !is_privileged {
        let tier_ok = matches!(tier.as_str(), "pro" | "creator" | "team");
        if !tier_ok {
            log::warn!("[create_user_server] Rejected: user {} has tier '{}' (pro+ required)", &user_id[..8.min(user_id.len())], tier);
            return;
        }
    }

    // Server creation limit enforcement
    if !is_privileged {
        let owned_count = ctx.db.chat_servers().iter()
            .filter(|s| s.owner_user_id == user_id)
            .count() as u32;

        let max_servers = ctx.db.agent_spawn_limits()
            .tier().find(&tier)
            .and_then(|l| l.max_servers)
            .unwrap_or(0);

        if owned_count >= max_servers {
            log::warn!("[create_user_server] Server limit reached for user {}: {}/{}", &user_id[..8.min(user_id.len())], owned_count, max_servers);
            return;
        }
    }

    let sender_hex = crate::utils::auth::sender_hex(ctx);
    ensure_chat_user(ctx, &user_id, &sender_hex);

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_SERVER_NAME_LEN {
        log::warn!("[create_user_server] Invalid server name length: {}", trimmed.len());
        return;
    }

    if ctx.db.chat_servers().id().find(&id).is_some() {
        log::warn!("[create_user_server] Server {} already exists", id);
        return;
    }

    let now = crate::timestamp_ms(ctx);

    ctx.db.chat_servers().insert(ChatServer {
        id: id.clone(),
        name: trimmed.clone(),
        description: String::new(),
        audience_id: String::new(),
        owner_user_id: user_id.clone(),
        is_public,
        default_tier: "free".to_string(),
        icon_url: String::new(),
        created_at: now,
        updated_at: now,
        template: None,
    });

    // Auto-join creator as owner
    let member_id = format!("{}-{}", id, user_id);
    ctx.db.server_members().insert(ServerMember {
        id: member_id,
        server_id: id.clone(),
        user_id: user_id.clone(),
        role: "owner".to_string(),
        joined_at: now,
        nickname: None,
        timeout_until: None,
        deaf: false,
        mute: false,
    });

    // Auto-create @everyone default role
    crate::reducers::roles::create_default_role(ctx, &id);

    // Auto-create "General" category
    let cat_id = format!("{}-general-cat", id);
    ctx.db.channel_categories().insert(crate::tables::ChannelCategory {
        id: cat_id.clone(),
        server_id: id.clone(),
        name: "Text Channels".to_string(),
        sort_order: 0,
        created_at: now,
    });

    // Auto-create #general channel
    let general_room_id = format!("{}-general", id);
    ctx.db.rooms().insert(Room {
        id: general_room_id.clone(),
        name: "general".to_string(),
        created_by: user_id.clone(),
        is_private: false,
        is_dm: false,
        created_at: now,
        server_id: Some(id.clone()),
        required_tier: None,
        description: Some("General discussion".to_string()),
        sort_order: Some(0),
        room_type: "text".to_string(),
        category_id: Some(cat_id),
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

    // Auto-join creator to #general
    ctx.db.room_members().insert(RoomMember {
        id: format!("{}-{}", general_room_id, user_id),
        room_id: general_room_id,
        user_id: user_id.clone(),
        role: "admin".to_string(),
        joined_at: now,
    });

    log::info!("[create_user_server] User {} created server '{}' ({})", &user_id[..8.min(user_id.len())], trimmed, &id[..8.min(id.len())]);
}

/// Admin: add a bot (or any user) to a server. Platform admin only.
/// Unlike join_server (which adds the caller), this adds an arbitrary user_id.
///
/// Cascade: auto-joins the target to all public (non-private) rooms in the
/// server. This mirrors the personal-bot `add_own_agent_to_server` cascade so
/// admin-invited platform bots show up in channel member lists immediately
/// (otherwise only `server_members` gets a row and the bot is invisible at
/// the channel level until manually added per-room).
#[spacetimedb::reducer]
pub fn admin_add_to_server(ctx: &ReducerContext, server_id: String, target_user_id: String) {
    if !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[admin_add_to_server] Rejected: not platform admin");
        return;
    }

    if ctx.db.chat_servers().id().find(&server_id).is_none() {
        log::warn!("[admin_add_to_server] Server {} not found", server_id);
        return;
    }

    // Check target user exists
    if ctx.db.chat_users().user_id().find(&target_user_id).is_none() {
        log::warn!("[admin_add_to_server] User {} not found", &target_user_id[..8.min(target_user_id.len())]);
        return;
    }

    // Check existing membership
    if let Some(existing) = find_server_membership(ctx, &server_id, &target_user_id) {
        if existing.role == "banned" {
            // Unban by removing the ban membership, then re-add below
            ctx.db.server_members().id().delete(&existing.id);
        } else {
            // Already a server member — still cascade public rooms (covers
            // the case where a previous admin_add_to_server ran before this
            // cascade existed and left the bot orphaned).
        }
    }

    let now = crate::timestamp_ms(ctx);
    let member_id = format!("{}-{}", server_id, target_user_id);
    if find_server_membership(ctx, &server_id, &target_user_id).is_none() {
        ctx.db.server_members().insert(ServerMember {
            id: member_id,
            server_id: server_id.clone(),
            user_id: target_user_id.clone(),
            role: "member".to_string(),
            joined_at: now,
            nickname: None,
            timeout_until: None,
            deaf: false,
            mute: false,
        });
    }

    // Cascade: auto-join all public (non-private) rooms in this server.
    let server_rooms: Vec<String> = ctx.db.rooms().iter()
        .filter(|r| r.server_id.as_deref() == Some(&server_id) && !r.is_private)
        .map(|r| r.id.clone())
        .collect();

    let mut rooms_joined = 0u32;
    for room_id in &server_rooms {
        if find_membership(ctx, room_id, &target_user_id).is_some() {
            continue;
        }
        let rm_id = format!("{}-{}", room_id, target_user_id);
        ctx.db.room_members().insert(crate::tables::room_members::RoomMember {
            id: rm_id,
            room_id: room_id.clone(),
            user_id: target_user_id.clone(),
            role: "member".to_string(),
            joined_at: now,
        });
        rooms_joined += 1;
    }

    log::info!(
        "[admin_add_to_server] Target {} added to server {} ({} public rooms auto-joined)",
        &target_user_id[..8.min(target_user_id.len())],
        &server_id[..8.min(server_id.len())],
        rooms_joined
    );
}
