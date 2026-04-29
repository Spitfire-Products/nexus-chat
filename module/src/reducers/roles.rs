//! Role management reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::server_roles::server_roles;
use crate::tables::member_roles::member_roles;
use crate::tables::channel_overrides::channel_overrides;
use crate::utils::permissions::*;

const MAX_ROLE_NAME_LEN: usize = 64;

/// Create a role in a server.
#[spacetimedb::reducer]
pub fn create_role(
    ctx: &ReducerContext,
    id: String,
    server_id: String,
    name: String,
    color: String,
    permissions: u64,
    mentionable: bool,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_role] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MANAGE_ROLES, "create_role").is_none() {
        return;
    }

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_ROLE_NAME_LEN {
        return;
    }

    if ctx.db.server_roles().id().find(&id).is_some() {
        log::warn!("[create_role] Role {} already exists", id);
        return;
    }

    // Determine sort order (append at end)
    let max_sort = ctx.db.server_roles().iter()
        .filter(|r| r.server_id == server_id)
        .map(|r| r.sort_order)
        .max()
        .unwrap_or(0);

    let now = crate::timestamp_ms(ctx);
    ctx.db.server_roles().insert(ServerRole {
        id,
        server_id,
        name: trimmed,
        color,
        permissions,
        sort_order: max_sort + 1,
        is_default: false,
        mentionable,
        created_at: now,
    });
}

/// Update a role's properties.
#[spacetimedb::reducer]
pub fn update_role(
    ctx: &ReducerContext,
    id: String,
    name: String,
    color: String,
    permissions: u64,
    sort_order: u32,
    mentionable: bool,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[update_role] Unauthorized");
        return;
    };

    let Some(role) = ctx.db.server_roles().id().find(&id) else {
        log::warn!("[update_role] Role {} not found", id);
        return;
    };

    if require_server_permission(ctx, &role.server_id, &user_id, PERM_MANAGE_ROLES, "update_role").is_none() {
        return;
    }

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_ROLE_NAME_LEN {
        return;
    }

    ctx.db.server_roles().id().delete(&id);
    ctx.db.server_roles().insert(ServerRole {
        name: trimmed,
        color,
        permissions,
        sort_order,
        mentionable,
        ..role
    });
}

/// Delete a role. Cascades to member_roles. Cannot delete @everyone.
#[spacetimedb::reducer]
pub fn delete_role(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_role] Unauthorized");
        return;
    };

    let Some(role) = ctx.db.server_roles().id().find(&id) else {
        log::warn!("[delete_role] Role {} not found", id);
        return;
    };

    if role.is_default {
        log::warn!("[delete_role] Cannot delete @everyone role");
        return;
    }

    if require_server_permission(ctx, &role.server_id, &user_id, PERM_MANAGE_ROLES, "delete_role").is_none() {
        return;
    }

    // Cascade: remove all member_roles referencing this role
    let assignments: Vec<MemberRole> = ctx.db.member_roles().iter()
        .filter(|mr| mr.role_id == id)
        .collect();
    for mr in assignments {
        ctx.db.member_roles().id().delete(&mr.id);
    }

    // Also remove channel overrides for this role
    let overrides: Vec<ChannelOverride> = ctx.db.channel_overrides().iter()
        .filter(|o| o.target_type == "role" && o.target_id == id)
        .collect();
    for o in overrides {
        ctx.db.channel_overrides().id().delete(&o.id);
    }

    ctx.db.server_roles().id().delete(&id);
}

/// Assign a role to a server member.
#[spacetimedb::reducer]
pub fn assign_role(
    ctx: &ReducerContext,
    server_id: String,
    target_user_id: String,
    role_id: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[assign_role] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MANAGE_ROLES, "assign_role").is_none() {
        return;
    }

    // Validate role exists and belongs to server
    let Some(role) = ctx.db.server_roles().id().find(&role_id) else {
        log::warn!("[assign_role] Role {} not found", role_id);
        return;
    };
    if role.server_id != server_id {
        log::warn!("[assign_role] Role {} does not belong to server {}", role_id, server_id);
        return;
    }

    // Check member exists
    if crate::utils::validation::find_server_membership(ctx, &server_id, &target_user_id).is_none() {
        log::warn!("[assign_role] User {} is not a member of server {}", target_user_id, server_id);
        return;
    }

    // Check not already assigned
    let assignment_id = format!("{}-{}-{}", server_id, target_user_id, role_id);
    if ctx.db.member_roles().id().find(&assignment_id).is_some() {
        return; // Already assigned, no-op
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.member_roles().insert(MemberRole {
        id: assignment_id,
        server_id,
        user_id: target_user_id,
        role_id,
        assigned_at: now,
    });
}

/// Remove a role from a server member.
#[spacetimedb::reducer]
pub fn remove_role(
    ctx: &ReducerContext,
    server_id: String,
    target_user_id: String,
    role_id: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[remove_role] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MANAGE_ROLES, "remove_role").is_none() {
        return;
    }

    let assignment_id = format!("{}-{}-{}", server_id, target_user_id, role_id);
    if ctx.db.member_roles().id().find(&assignment_id).is_some() {
        ctx.db.member_roles().id().delete(&assignment_id);
    }
}

/// Internal helper: create the @everyone default role for a new server.
pub fn create_default_role(ctx: &ReducerContext, server_id: &str) {
    let role_id = format!("{}-everyone", server_id);
    let now = crate::timestamp_ms(ctx);
    ctx.db.server_roles().insert(ServerRole {
        id: role_id,
        server_id: server_id.to_string(),
        name: "@everyone".to_string(),
        color: String::new(),
        permissions: DEFAULT_EVERYONE_PERMS,
        sort_order: 0,
        is_default: true,
        mentionable: false,
        created_at: now,
    });
}
