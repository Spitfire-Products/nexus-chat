//! Authentication reducers.

use spacetimedb::ReducerContext;
use crate::utils::auth::{sender_hex, register_identity_link};

/// Register this device's SpacetimeDB identity with an application user_id.
/// Called by clients after authenticating with the main Nexus Terminal module.
#[spacetimedb::reducer]
pub fn register_identity(ctx: &ReducerContext, user_id: String) {
    if user_id.trim().is_empty() {
        log::warn!("[register_identity] Rejected: empty user_id");
        return;
    }
    let stdb_identity = sender_hex(ctx);
    register_identity_link(ctx, &stdb_identity, &user_id);
    log::info!("[register_identity] Registered identity for user {}", user_id);
}
