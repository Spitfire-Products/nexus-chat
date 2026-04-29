//! Star/unstar channels for the watchlist.

use spacetimedb::{ReducerContext, Table};
use crate::tables::starred_channels::starred_channels;
use crate::tables::StarredChannel;

/// Star a channel (add to watchlist).
#[spacetimedb::reducer]
pub fn star_channel(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[star_channel] Unauthorized");
        return;
    };

    let id = format!("{}:{}", user_id, room_id);

    // Check if already starred
    if ctx.db.starred_channels().id().find(&id).is_some() {
        return; // Already starred, idempotent
    }

    ctx.db.starred_channels().insert(StarredChannel {
        id,
        user_id,
        room_id,
        starred_at: crate::timestamp_ms(ctx),
    });
}

/// Unstar a channel (remove from watchlist).
#[spacetimedb::reducer]
pub fn unstar_channel(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[unstar_channel] Unauthorized");
        return;
    };

    let id = format!("{}:{}", user_id, room_id);
    if ctx.db.starred_channels().id().find(&id).is_some() {
        ctx.db.starred_channels().id().delete(&id);
    }
}
