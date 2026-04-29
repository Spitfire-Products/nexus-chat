//! Block/unblock user reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::user_blocks::user_blocks;
use crate::tables::UserBlock;
use crate::utils::crypto::hash_blocked_id;

const MAX_BLOCKS_PER_USER: usize = 200;

/// Block a user. Prevents DMs and hides their messages client-side.
/// The blocked_id is stored as a hash to prevent casual plaintext exposure.
#[spacetimedb::reducer]
pub fn block_user(ctx: &ReducerContext, blocked_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[block_user] Unauthorized");
        return;
    };

    if user_id == blocked_user_id {
        log::warn!("[block_user] Cannot block yourself");
        return;
    }

    let blocked_hash = hash_blocked_id(&user_id, &blocked_user_id);
    let id = format!("{}:{}", user_id, blocked_hash);

    // Idempotent — no-op if already blocked
    if ctx.db.user_blocks().id().find(&id).is_some() {
        return;
    }

    // Rate limit: max blocks per user
    let current_blocks = ctx.db.user_blocks().iter()
        .filter(|b| b.blocker_id == user_id)
        .count();
    if current_blocks >= MAX_BLOCKS_PER_USER {
        log::warn!("[block_user] Block limit reached ({}) for {}", MAX_BLOCKS_PER_USER, &user_id[..8.min(user_id.len())]);
        return;
    }

    ctx.db.user_blocks().insert(UserBlock {
        id,
        blocker_id: user_id,
        blocked_id: blocked_hash,
        created_at: crate::timestamp_ms(ctx),
    });
}

/// Unblock a user.
#[spacetimedb::reducer]
pub fn unblock_user(ctx: &ReducerContext, blocked_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[unblock_user] Unauthorized");
        return;
    };

    let blocked_hash = hash_blocked_id(&user_id, &blocked_user_id);
    let id = format!("{}:{}", user_id, blocked_hash);
    if ctx.db.user_blocks().id().find(&id).is_some() {
        ctx.db.user_blocks().id().delete(&id);
    }
}
