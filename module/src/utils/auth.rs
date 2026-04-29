//! Security helpers for reducer auth guards.
//!
//! IMPORTANT CONSTRAINTS (SpacetimeDB v2.0):
//! - Only `=` operator works in subscription WHERE clauses
//! - Security enforced via reducer auth guards + client-side subscription scoping

use spacetimedb::{ReducerContext, Table};
use crate::tables::auth::{UserIdentityLink, user_identity_links};
use crate::tables::users::chat_users;

/// Get the caller's SpacetimeDB identity as a hex string.
pub fn sender_hex(ctx: &ReducerContext) -> String {
    ctx.sender().to_hex().to_string()
}

/// Look up the caller's application user_id from their SpacetimeDB identity.
/// Returns None if no identity link exists (before register_identity is called).
pub fn get_caller_user_id(ctx: &ReducerContext) -> Option<String> {
    let caller_hex = ctx.sender().to_hex().to_string();
    ctx.db.user_identity_links().stdb_identity().find(&caller_hex)
        .map(|link| link.user_id.clone())
}

/// Check if the caller is a system/internal caller (scheduled reducers, module identity).
pub fn is_system_caller(ctx: &ReducerContext) -> bool {
    ctx.sender() == ctx.identity()
}

/// Check if the caller has platform admin/developer/moderator role.
/// Follows BlasterLab's is_admin_or_moderator pattern.
pub fn is_platform_admin(ctx: &ReducerContext) -> bool {
    let Some(user_id) = get_caller_user_id(ctx) else { return false };
    let Some(user) = ctx.db.chat_users().user_id().find(&user_id) else { return false };
    matches!(user.platform_role.as_deref(), Some("admin") | Some("moderator") | Some("developer"))
}

/// Register a SpacetimeDB identity link for multi-device support.
/// Auto-prunes to cap 10 links per user.
pub fn register_identity_link(ctx: &ReducerContext, stdb_identity: &str, user_id: &str) {
    let now = crate::timestamp_ms(ctx);

    if let Some(existing) = ctx.db.user_identity_links().stdb_identity().find(&stdb_identity.to_string()) {
        // SECURITY: Reject re-linking to a different user_id (first-link-wins).
        // Prevents identity hijacking where an attacker calls register_identity
        // with a victim's user_id to impersonate them.
        if existing.user_id != user_id {
            log::warn!(
                "[register_identity_link] REJECTED: identity {}... already linked to user {}, cannot re-link to {}",
                &stdb_identity[..16.min(stdb_identity.len())],
                existing.user_id,
                user_id
            );
            return;
        }
        // Same user_id: update last_seen_at (heartbeat)
        ctx.db.user_identity_links().stdb_identity().delete(&stdb_identity.to_string());
        ctx.db.user_identity_links().insert(UserIdentityLink {
            stdb_identity: stdb_identity.to_string(),
            user_id: user_id.to_string(),
            created_at: existing.created_at,
            last_seen_at: now,
        });
    } else {
        ctx.db.user_identity_links().insert(UserIdentityLink {
            stdb_identity: stdb_identity.to_string(),
            user_id: user_id.to_string(),
            created_at: now,
            last_seen_at: now,
        });
        log::info!("[register_identity_link] Linked identity {}... to user {}",
            &stdb_identity[..16.min(stdb_identity.len())], user_id);
    }

    // Auto-prune: keep only 10 most recent links per user
    let mut user_links: Vec<UserIdentityLink> = ctx.db.user_identity_links()
        .iter()
        .filter(|l| l.user_id == user_id)
        .collect();

    if user_links.len() > 10 {
        user_links.sort_by(|a, b| b.last_seen_at.cmp(&a.last_seen_at));
        for stale in &user_links[10..] {
            ctx.db.user_identity_links().stdb_identity().delete(&stale.stdb_identity);
        }
    }
}
