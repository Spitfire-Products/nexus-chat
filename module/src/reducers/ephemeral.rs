//! Ephemeral message cleanup scheduled reducer.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::messages::messages;
use crate::tables::reactions::reactions;
use crate::tables::message_edits::message_edits;
use crate::tables::reaction_room_index::reaction_room_index;
use crate::tables::message_edit_room_index::message_edit_room_index;

/// Scheduled reducer: delete an ephemeral message and its reactions.
#[spacetimedb::reducer]
pub fn delete_ephemeral_message(ctx: &ReducerContext, arg: EphemeralCleanupJob) {
    if let Some(msg) = ctx.db.messages().id().find(&arg.message_id) {
        if msg.is_ephemeral {
            // Delete reactions on this message — cascade index too.
            let msg_reactions: Vec<crate::tables::Reaction> = ctx.db.reactions().iter()
                .filter(|r| r.message_id == arg.message_id)
                .collect();
            for r in msg_reactions {
                ctx.db.reactions().id().delete(&r.id);
                ctx.db.reaction_room_index().reaction_id().delete(&r.id);
            }

            // Delete message edits — cascade index too.
            let edits: Vec<crate::tables::MessageEdit> = ctx.db.message_edits().iter()
                .filter(|e| e.message_id == arg.message_id)
                .collect();
            for e in edits {
                ctx.db.message_edits().id().delete(&e.id);
                ctx.db.message_edit_room_index().edit_id().delete(&e.id);
            }

            // Delete the message
            ctx.db.messages().id().delete(&arg.message_id);
            log::info!("[delete_ephemeral_message] Deleted ephemeral message {}", arg.message_id);
        }
    }
}
