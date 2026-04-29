//! Reaction reducers: toggle_reaction.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::messages::messages;
use crate::tables::reactions::reactions;
use crate::tables::reaction_room_index::reaction_room_index;
use crate::tables::server_emojis::server_emojis;
use crate::tables::rooms::rooms;
use crate::utils::validation::{require_membership, is_valid_emoji};
use crate::utils::permissions::*;

/// Toggle an emoji reaction on a message.
/// If the user already reacted with this emoji, remove it. Otherwise, add it.
///
/// Supports both built-in emojis and custom server emojis.
/// - Built-in: "thumbsup", "heart", etc.
/// - Custom: "custom:{emoji_id}" format
#[spacetimedb::reducer]
pub fn toggle_reaction(ctx: &ReducerContext, message_id: String, emoji: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[toggle_reaction] Unauthorized");
        return;
    };

    // Validate emoji format (Unicode characters, :pepe-name:, or custom:id)
    if !is_valid_emoji(&emoji) {
        log::warn!("[toggle_reaction] Invalid emoji: {}", emoji);
        return;
    }

    // If custom server emoji, validate it exists in the server
    if emoji.starts_with("custom:") {
        let emoji_id = &emoji[7..]; // strip "custom:"
        if ctx.db.server_emojis().id().find(&emoji_id.to_string()).is_none() {
            log::warn!("[toggle_reaction] Custom emoji {} not found", emoji_id);
            return;
        }
    }

    // Verify message exists and get its room
    let Some(msg) = ctx.db.messages().id().find(&message_id) else {
        log::warn!("[toggle_reaction] Message {} not found", message_id);
        return;
    };

    // Must be a member of the room
    if require_membership(ctx, &msg.room_id, &user_id).is_none() {
        return;
    }

    // Check ADD_REACTIONS permission if in a server
    if let Some(room) = ctx.db.rooms().id().find(&msg.room_id) {
        if let Some(ref server_id) = room.server_id {
            if !has_permission(ctx, server_id, &msg.room_id, &user_id, PERM_ADD_REACTIONS) {
                log::warn!("[toggle_reaction] User {} lacks ADD_REACTIONS", user_id);
                return;
            }
        }
    }

    // Check for existing reaction
    let existing: Option<Reaction> = ctx.db.reactions().iter()
        .find(|r| r.message_id == message_id && r.user_id == user_id && r.emoji == emoji);

    if let Some(existing) = existing {
        // Remove reaction (toggle off). Dual-delete from the shadow index.
        // shadowing-stork plan — index PK (reaction_id) matches source.id.
        ctx.db.reactions().id().delete(&existing.id);
        ctx.db.reaction_room_index().reaction_id().delete(&existing.id);
    } else {
        // Add reaction (toggle on)
        let now = crate::timestamp_ms(ctx);
        let reaction_id = format!("{}-{}-{}", message_id, user_id, emoji);
        let room_id_for_index = msg.room_id.clone();
        let message_id_for_index = message_id.clone();
        let user_id_for_index = user_id.clone();
        let emoji_for_index = emoji.clone();
        ctx.db.reactions().insert(Reaction {
            id: reaction_id.clone(),
            message_id,
            user_id,
            emoji,
            created_at: now,
            room_id: Some(msg.room_id.clone()),
        });
        // Dual-write into the room-scoped index for filterable subscriptions.
        // Same transaction — STDB rolls back both on any error.
        ctx.db.reaction_room_index().insert(ReactionRoomIndex {
            reaction_id,
            room_id: room_id_for_index,
            message_id: message_id_for_index,
            user_id: user_id_for_index,
            emoji: emoji_for_index,
            created_at: now,
        });
    }
}
