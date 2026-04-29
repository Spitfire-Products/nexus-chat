//! Backfill admin reducers for the shadow indexes added by the
//! shadowing-stork plan.
//!
//! After the table migration deploys, run these once to populate the
//! indexes from existing reactions/message_edits rows. Both reducers are
//! idempotent — they skip rows that already have an index entry, so
//! re-running is safe.
//!
//! Auth: platform admin only. The reducers walk the source tables and
//! insert into the index tables only — no source-table mutation.

use spacetimedb::{ReducerContext, Table};
use crate::tables::reactions::reactions;
use crate::tables::message_edits::message_edits;
use crate::tables::reaction_room_index::{reaction_room_index, ReactionRoomIndex};
use crate::tables::message_edit_room_index::{message_edit_room_index, MessageEditRoomIndex};

/// Backfill `reaction_room_index` from existing `reactions` rows.
/// Idempotent — skips rows already in the index. Skips reactions whose
/// `room_id` is None (legacy/orphaned data) and logs the count.
#[spacetimedb::reducer]
pub fn backfill_reaction_room_index(ctx: &ReducerContext) {
    if !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[backfill_reaction_room_index] Rejected: caller is not platform admin");
        return;
    }

    let mut indexed = 0u64;
    let mut skipped_no_room = 0u64;
    let mut already_indexed = 0u64;

    let rows: Vec<crate::tables::Reaction> = ctx.db.reactions().iter().collect();
    for r in rows {
        if ctx.db.reaction_room_index().reaction_id().find(&r.id).is_some() {
            already_indexed += 1;
            continue;
        }
        let Some(room_id) = r.room_id else {
            skipped_no_room += 1;
            continue;
        };
        ctx.db.reaction_room_index().insert(ReactionRoomIndex {
            reaction_id: r.id,
            room_id,
            message_id: r.message_id,
            user_id: r.user_id,
            emoji: r.emoji,
            created_at: r.created_at,
        });
        indexed += 1;
    }

    log::info!(
        "[backfill_reaction_room_index] indexed={} skipped_no_room={} already_indexed={}",
        indexed, skipped_no_room, already_indexed
    );
}

/// Backfill `message_edit_room_index` from existing `message_edits` rows.
/// Idempotent — skips rows already in the index. Skips edits whose
/// `room_id` is None and logs the count.
#[spacetimedb::reducer]
pub fn backfill_message_edit_room_index(ctx: &ReducerContext) {
    if !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[backfill_message_edit_room_index] Rejected: caller is not platform admin");
        return;
    }

    let mut indexed = 0u64;
    let mut skipped_no_room = 0u64;
    let mut already_indexed = 0u64;

    let rows: Vec<crate::tables::MessageEdit> = ctx.db.message_edits().iter().collect();
    for e in rows {
        if ctx.db.message_edit_room_index().edit_id().find(&e.id).is_some() {
            already_indexed += 1;
            continue;
        }
        let Some(room_id) = e.room_id else {
            skipped_no_room += 1;
            continue;
        };
        ctx.db.message_edit_room_index().insert(MessageEditRoomIndex {
            edit_id: e.id,
            room_id,
            message_id: e.message_id,
            editor_id: e.editor_id,
            edited_at: e.edited_at,
            new_content: e.new_content,
        });
        indexed += 1;
    }

    log::info!(
        "[backfill_message_edit_room_index] indexed={} skipped_no_room={} already_indexed={}",
        indexed, skipped_no_room, already_indexed
    );
}
