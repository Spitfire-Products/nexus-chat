//! Permission bitfield system for role-based access control.
//!
//! Discord-style permission model:
//! 1. Start with @everyone role permissions
//! 2. OR in all assigned role permissions
//! 3. If ADMINISTRATOR, return ALL permissions
//! 4. Apply channel overrides (role-based, then member-based)

use spacetimedb::{ReducerContext, Table};
use crate::tables::server_roles::server_roles;
use crate::tables::member_roles::member_roles;
use crate::tables::channel_overrides::channel_overrides;
use crate::tables::{ServerRole, ChannelOverride};

// =============================================================================
// Permission bit constants
// =============================================================================

pub const PERM_SEND_MESSAGES: u64       = 1 << 0;
pub const PERM_MANAGE_MESSAGES: u64     = 1 << 1;
pub const PERM_MANAGE_CHANNELS: u64     = 1 << 2;
pub const PERM_MANAGE_ROLES: u64        = 1 << 3;
pub const PERM_KICK_MEMBERS: u64        = 1 << 4;
pub const PERM_BAN_MEMBERS: u64         = 1 << 5;
pub const PERM_MANAGE_SERVER: u64       = 1 << 6;
pub const PERM_CREATE_INVITES: u64      = 1 << 7;
pub const PERM_ATTACH_FILES: u64        = 1 << 8;
pub const PERM_ADD_REACTIONS: u64       = 1 << 9;
pub const PERM_USE_EXTERNAL_EMOJIS: u64 = 1 << 10;
pub const PERM_MENTION_EVERYONE: u64    = 1 << 11;
pub const PERM_PIN_MESSAGES: u64        = 1 << 12;
pub const PERM_MANAGE_WEBHOOKS: u64     = 1 << 13;
pub const PERM_VIEW_AUDIT_LOG: u64      = 1 << 14;
pub const PERM_MANAGE_THREADS: u64      = 1 << 15;
pub const PERM_CREATE_THREADS: u64      = 1 << 16;
pub const PERM_MODERATE_MEMBERS: u64    = 1 << 17;
pub const PERM_MANAGE_EMOJIS: u64       = 1 << 18;
pub const PERM_CREATE_POLLS: u64        = 1 << 19;
pub const PERM_ADMINISTRATOR: u64       = 1 << 31;

/// Default permissions for @everyone role (new servers).
pub const DEFAULT_EVERYONE_PERMS: u64 =
    PERM_SEND_MESSAGES
    | PERM_ADD_REACTIONS
    | PERM_CREATE_INVITES
    | PERM_ATTACH_FILES
    | PERM_USE_EXTERNAL_EMOJIS
    | PERM_CREATE_THREADS
    | PERM_CREATE_POLLS;

/// All permission bits OR'd together (for ADMINISTRATOR shortcut).
pub const ALL_PERMISSIONS: u64 = (1 << 20) - 1 | PERM_ADMINISTRATOR;

// =============================================================================
// Permission computation
// =============================================================================

/// Compute the effective permissions for a user in a specific channel/room.
///
/// Algorithm (mirrors Discord):
/// 1. Find the server's @everyone role → base permissions
/// 2. OR in permissions from all roles assigned to the user
/// 3. If any role has ADMINISTRATOR, return ALL_PERMISSIONS
/// 4. Apply channel overrides for each of the user's roles (allow/deny)
/// 5. Apply member-specific channel overrides (allow/deny)
pub fn compute_permissions(
    ctx: &ReducerContext,
    server_id: &str,
    room_id: &str,
    user_id: &str,
) -> u64 {
    // Step 1: @everyone base permissions
    let everyone_role: Option<ServerRole> = ctx.db.server_roles().iter()
        .find(|r| r.server_id == server_id && r.is_default);

    let mut perms = everyone_role.map(|r| r.permissions).unwrap_or(DEFAULT_EVERYONE_PERMS);

    // Step 2: OR in all assigned role permissions
    let user_role_ids: Vec<String> = ctx.db.member_roles().iter()
        .filter(|mr| mr.server_id == server_id && mr.user_id == user_id)
        .map(|mr| mr.role_id.clone())
        .collect();

    let mut role_perms_list: Vec<(String, u64)> = Vec::new();
    for role_id in &user_role_ids {
        if let Some(role) = ctx.db.server_roles().id().find(role_id) {
            perms |= role.permissions;
            role_perms_list.push((role_id.clone(), role.permissions));
        }
    }

    // Step 3: ADMINISTRATOR bypass
    if perms & PERM_ADMINISTRATOR != 0 {
        return ALL_PERMISSIONS;
    }

    // Step 4: Channel overrides for roles
    let overrides: Vec<ChannelOverride> = ctx.db.channel_overrides().iter()
        .filter(|o| o.room_id == room_id)
        .collect();

    // Apply @everyone role override first
    if let Some(ref everyone) = ctx.db.server_roles().iter()
        .find(|r| r.server_id == server_id && r.is_default)
    {
        for o in &overrides {
            if o.target_type == "role" && o.target_id == everyone.id {
                perms &= !o.deny;
                perms |= o.allow;
            }
        }
    }

    // Apply user's role overrides (union of allows, union of denies)
    let mut role_allow: u64 = 0;
    let mut role_deny: u64 = 0;
    for o in &overrides {
        if o.target_type == "role" && user_role_ids.contains(&o.target_id) {
            role_allow |= o.allow;
            role_deny |= o.deny;
        }
    }
    perms &= !role_deny;
    perms |= role_allow;

    // Step 5: Member-specific overrides (highest priority)
    for o in &overrides {
        if o.target_type == "member" && o.target_id == user_id {
            perms &= !o.deny;
            perms |= o.allow;
        }
    }

    perms
}

/// Check if a user has a specific permission in a channel.
pub fn has_permission(
    ctx: &ReducerContext,
    server_id: &str,
    room_id: &str,
    user_id: &str,
    permission: u64,
) -> bool {
    compute_permissions(ctx, server_id, room_id, user_id) & permission != 0
}

/// Require a specific permission, logging a warning if not met.
/// Returns Some(()) if the user has the permission, None otherwise.
pub fn require_permission(
    ctx: &ReducerContext,
    server_id: &str,
    room_id: &str,
    user_id: &str,
    permission: u64,
    action: &str,
) -> Option<()> {
    if has_permission(ctx, server_id, room_id, user_id, permission) {
        Some(())
    } else {
        log::warn!(
            "[{}] User {} lacks permission 0x{:x} in server {}/room {}",
            action,
            &user_id[..8.min(user_id.len())],
            permission,
            &server_id[..8.min(server_id.len())],
            &room_id[..8.min(room_id.len())],
        );
        None
    }
}

/// Check if the user is the server owner (bypasses all permission checks).
pub fn is_server_owner(ctx: &ReducerContext, server_id: &str, user_id: &str) -> bool {
    use crate::tables::servers::chat_servers;
    ctx.db.chat_servers().id().find(&server_id.to_string())
        .map(|s| s.owner_user_id == user_id)
        .unwrap_or(false)
}

/// Require a server-level permission (not channel-specific).
/// Uses an empty room_id which skips channel overrides.
pub fn require_server_permission(
    ctx: &ReducerContext,
    server_id: &str,
    user_id: &str,
    permission: u64,
    action: &str,
) -> Option<()> {
    // Server owner always passes
    if is_server_owner(ctx, server_id, user_id) {
        return Some(());
    }
    require_permission(ctx, server_id, "", user_id, permission, action)
}
