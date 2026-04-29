//! Pin/unpin message reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::pinned_messages::pinned_messages;
use crate::tables::messages::messages;
use crate::tables::rooms::rooms;
use crate::utils::permissions::*;

const MAX_PINS_PER_ROOM: usize = 50;

/// Pin a message in a room.
#[spacetimedb::reducer]
pub fn pin_message(
    ctx: &ReducerContext,
    id: String,
    room_id: String,
    message_id: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[pin_message] Unauthorized");
        return;
    };

    // Verify message exists and belongs to room
    let Some(msg) = ctx.db.messages().id().find(&message_id) else {
        log::warn!("[pin_message] Message {} not found", message_id);
        return;
    };
    if msg.room_id != room_id {
        log::warn!("[pin_message] Message {} is not in room {}", message_id, room_id);
        return;
    }

    // Check PIN_MESSAGES permission if in server
    if let Some(room) = ctx.db.rooms().id().find(&room_id) {
        if let Some(ref server_id) = room.server_id {
            if require_permission(ctx, server_id, &room_id, &user_id, PERM_PIN_MESSAGES, "pin_message").is_none() {
                return;
            }
        } else {
            // Non-server room: must be member
            if crate::utils::validation::require_membership(ctx, &room_id, &user_id).is_none() {
                return;
            }
        }
    }

    // Check not already pinned
    let already_pinned = ctx.db.pinned_messages().iter()
        .any(|p| p.room_id == room_id && p.message_id == message_id);
    if already_pinned {
        return; // Already pinned, no-op
    }

    // Check pin limit
    let pin_count = ctx.db.pinned_messages().iter()
        .filter(|p| p.room_id == room_id)
        .count();
    if pin_count >= MAX_PINS_PER_ROOM {
        log::warn!("[pin_message] Room {} has reached pin limit ({})", room_id, MAX_PINS_PER_ROOM);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.pinned_messages().insert(PinnedMessage {
        id,
        room_id,
        message_id,
        pinned_by: user_id,
        pinned_at: now,
    });
}

/// Unpin a message.
#[spacetimedb::reducer]
pub fn unpin_message(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[unpin_message] Unauthorized");
        return;
    };

    let Some(pin) = ctx.db.pinned_messages().id().find(&id) else {
        log::warn!("[unpin_message] Pin {} not found", id);
        return;
    };

    // Check PIN_MESSAGES permission if in server
    if let Some(room) = ctx.db.rooms().id().find(&pin.room_id) {
        if let Some(ref server_id) = room.server_id {
            if require_permission(ctx, server_id, &pin.room_id, &user_id, PERM_PIN_MESSAGES, "unpin_message").is_none() {
                return;
            }
        }
    }

    ctx.db.pinned_messages().id().delete(&id);
}
