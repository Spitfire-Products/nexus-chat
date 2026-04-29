//! User management reducers: set_display_name, set_status.

use spacetimedb::{ReducerContext, Table};
use crate::tables::ChatUser;
use crate::tables::users::chat_users;
use crate::utils::validation::{MAX_DISPLAY_NAME_LEN, ALLOWED_STATUSES};

/// Set the caller's display name.
/// Enforces: length 1-32, no control/zero-width chars, unique (case-insensitive).
#[spacetimedb::reducer]
pub fn set_display_name(ctx: &ReducerContext, name: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[set_display_name] Unauthorized: no identity link");
        return;
    };

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_DISPLAY_NAME_LEN {
        log::warn!("[set_display_name] Invalid name length: {}", trimmed.len());
        return;
    }

    // Block control characters and zero-width chars used for spoofing
    if trimmed.chars().any(|c| c.is_control() || c == '\u{200B}' || c == '\u{200C}' || c == '\u{200D}' || c == '\u{FEFF}') {
        log::warn!("[set_display_name] Rejected: contains control/zero-width chars for user {}", user_id);
        return;
    }

    // Uniqueness check (case-insensitive) — reject if another user has this name
    let lower_name = trimmed.to_lowercase();
    let conflict = ctx.db.chat_users().iter().find(|u| {
        u.user_id != user_id && u.display_name.to_lowercase() == lower_name
    });
    if let Some(conflicting) = conflict {
        log::warn!("[set_display_name] Rejected: name '{}' already taken by user {}", trimmed, conflicting.user_id);
        return;
    }

    if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
        let now = crate::timestamp_ms(ctx);
        ctx.db.chat_users().user_id().delete(&user_id);
        ctx.db.chat_users().insert(ChatUser {
            display_name: trimmed,
            last_seen_at: now,
            ..user
        });
    }
}

/// Sync the caller's platform tier to the chat module.
/// Called by frontend after login to propagate subscription tier.
#[spacetimedb::reducer]
pub fn sync_user_tier(ctx: &ReducerContext, tier: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[sync_user_tier] Unauthorized: no identity link");
        return;
    };

    // Validate tier value
    let valid_tiers = ["free", "pro", "creator", "team", "admin"];
    let effective_tier = if valid_tiers.contains(&tier.as_str()) {
        tier
    } else {
        "free".to_string()
    };

    if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
        let now = crate::timestamp_ms(ctx);
        ctx.db.chat_users().user_id().delete(&user_id);
        ctx.db.chat_users().insert(ChatUser {
            tier: Some(effective_tier),
            last_seen_at: now,
            ..user
        });
    }
}

/// Sync the caller's platform role from AuthBridge.
/// Called automatically on login and role changes.
#[spacetimedb::reducer]
pub fn sync_platform_role(ctx: &ReducerContext, role: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[sync_platform_role] Unauthorized: no identity link");
        return;
    };

    let valid = ["admin", "moderator", "developer", "user"];
    let effective = if valid.contains(&role.as_str()) { role } else { "user".to_string() };

    if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
        let now = crate::timestamp_ms(ctx);
        ctx.db.chat_users().user_id().delete(&user_id);
        ctx.db.chat_users().insert(ChatUser {
            platform_role: Some(effective),
            last_seen_at: now,
            ..user
        });
    }
}

/// Set the caller's presence status.
#[spacetimedb::reducer]
pub fn set_status(ctx: &ReducerContext, status: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[set_status] Unauthorized: no identity link");
        return;
    };

    if !ALLOWED_STATUSES.contains(&status.as_str()) {
        log::warn!("[set_status] Invalid status: {}", status);
        return;
    }

    if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
        let now = crate::timestamp_ms(ctx);
        ctx.db.chat_users().user_id().delete(&user_id);
        ctx.db.chat_users().insert(ChatUser {
            status,
            online: user.online,
            last_seen_at: now,
            ..user
        });
    }
}
