//! Read tracking reducers: mark_message_read, mark_room_read.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::messages::messages;
use crate::tables::read_positions::read_positions;
use crate::utils::validation::require_membership;

/// Mark a specific message as read, updating the room read position.
#[spacetimedb::reducer]
pub fn mark_message_read(ctx: &ReducerContext, room_id: String, message_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    if require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    // Verify message exists
    if ctx.db.messages().id().find(&message_id).is_none() {
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Find existing read position for this user+room
    let existing: Option<ReadPosition> = ctx.db.read_positions().iter()
        .find(|rp| rp.room_id == room_id && rp.user_id == user_id);

    if let Some(existing) = existing {
        ctx.db.read_positions().id().delete(&existing.id);
        ctx.db.read_positions().insert(ReadPosition {
            last_read_message_id: message_id,
            updated_at: now,
            ..existing
        });
    } else {
        let pos_id = format!("rp-{}-{}", room_id, user_id);
        ctx.db.read_positions().insert(ReadPosition {
            id: pos_id,
            room_id,
            user_id,
            last_read_message_id: message_id,
            updated_at: now,
        });
    }
}

/// Mark all messages in a room as read (using the latest message).
#[spacetimedb::reducer]
pub fn mark_room_read(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    if require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    // Find the latest message in the room
    let latest_msg = ctx.db.messages().iter()
        .filter(|m| m.room_id == room_id)
        .max_by_key(|m| m.created_at);

    let Some(latest) = latest_msg else {
        return; // No messages to mark
    };

    let now = crate::timestamp_ms(ctx);
    let latest_id = latest.id.clone();

    let existing: Option<ReadPosition> = ctx.db.read_positions().iter()
        .find(|rp| rp.room_id == room_id && rp.user_id == user_id);

    if let Some(existing) = existing {
        ctx.db.read_positions().id().delete(&existing.id);
        ctx.db.read_positions().insert(ReadPosition {
            last_read_message_id: latest_id,
            updated_at: now,
            ..existing
        });
    } else {
        let pos_id = format!("rp-{}-{}", room_id, user_id);
        ctx.db.read_positions().insert(ReadPosition {
            id: pos_id,
            room_id,
            user_id,
            last_read_message_id: latest_id,
            updated_at: now,
        });
    }
}
