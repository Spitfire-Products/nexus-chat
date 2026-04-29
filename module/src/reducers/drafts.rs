//! Draft sync reducers: update_draft, clear_draft.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::drafts::drafts;
use crate::utils::validation::require_membership;

/// Update or create a draft for the caller in a specific room.
#[spacetimedb::reducer]
pub fn update_draft(ctx: &ReducerContext, room_id: String, content: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    if require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    // Cap draft length
    if content.len() > 4000 {
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Find existing draft for this user+room
    let existing: Option<Draft> = ctx.db.drafts().iter()
        .find(|d| d.room_id == room_id && d.user_id == user_id);

    if let Some(existing) = existing {
        ctx.db.drafts().id().delete(&existing.id);
        ctx.db.drafts().insert(Draft {
            content,
            updated_at: now,
            ..existing
        });
    } else {
        let draft_id = format!("draft-{}-{}", room_id, user_id);
        ctx.db.drafts().insert(Draft {
            id: draft_id,
            room_id,
            user_id,
            content,
            updated_at: now,
        });
    }
}

/// Clear a draft for the caller in a specific room.
#[spacetimedb::reducer]
pub fn clear_draft(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let draft: Option<Draft> = ctx.db.drafts().iter()
        .find(|d| d.room_id == room_id && d.user_id == user_id);

    if let Some(draft) = draft {
        ctx.db.drafts().id().delete(&draft.id);
    }
}
