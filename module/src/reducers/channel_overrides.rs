//! Channel permission override reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::channel_overrides::channel_overrides;
use crate::tables::rooms::rooms;
use crate::utils::permissions::*;

/// Set (create or update) a channel permission override.
#[spacetimedb::reducer]
pub fn set_channel_override(
    ctx: &ReducerContext,
    room_id: String,
    target_type: String,
    target_id: String,
    allow: u64,
    deny: u64,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[set_channel_override] Unauthorized");
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        log::warn!("[set_channel_override] Room {} not found", room_id);
        return;
    };

    let server_id = match &room.server_id {
        Some(sid) => sid.clone(),
        None => {
            log::warn!("[set_channel_override] Room {} is not in a server", room_id);
            return;
        }
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MANAGE_CHANNELS, "set_channel_override").is_none() {
        return;
    }

    if target_type != "role" && target_type != "member" {
        log::warn!("[set_channel_override] Invalid target_type: {}", target_type);
        return;
    }

    let override_id = format!("{}-{}-{}", room_id, target_type, target_id);

    // Upsert: delete existing if present
    if ctx.db.channel_overrides().id().find(&override_id).is_some() {
        ctx.db.channel_overrides().id().delete(&override_id);
    }

    ctx.db.channel_overrides().insert(ChannelOverride {
        id: override_id,
        room_id,
        target_type,
        target_id,
        allow,
        deny,
    });
}

/// Delete a channel permission override.
#[spacetimedb::reducer]
pub fn delete_channel_override(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_channel_override] Unauthorized");
        return;
    };

    let Some(over) = ctx.db.channel_overrides().id().find(&id) else {
        log::warn!("[delete_channel_override] Override {} not found", id);
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&over.room_id) else {
        return;
    };

    let server_id = match &room.server_id {
        Some(sid) => sid.clone(),
        None => return,
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MANAGE_CHANNELS, "delete_channel_override").is_none() {
        return;
    }

    ctx.db.channel_overrides().id().delete(&id);
}
