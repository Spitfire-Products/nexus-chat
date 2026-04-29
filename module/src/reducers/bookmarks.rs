//! Bookmark reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::bookmarks::bookmarks;
use crate::tables::messages::messages;

const MAX_BOOKMARK_NOTE_LEN: usize = 500;
const MAX_BOOKMARKS_PER_USER: usize = 200;

/// Add a bookmark.
#[spacetimedb::reducer]
pub fn add_bookmark(
    ctx: &ReducerContext,
    id: String,
    message_id: String,
    room_id: String,
    note: Option<String>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[add_bookmark] Unauthorized");
        return;
    };

    // Validate message exists
    if ctx.db.messages().id().find(&message_id).is_none() {
        log::warn!("[add_bookmark] Message {} not found", message_id);
        return;
    }

    if let Some(ref n) = note {
        if n.len() > MAX_BOOKMARK_NOTE_LEN {
            return;
        }
    }

    // Check bookmark limit
    let count = ctx.db.bookmarks().iter()
        .filter(|b| b.user_id == user_id)
        .count();
    if count >= MAX_BOOKMARKS_PER_USER {
        log::warn!("[add_bookmark] User {} has too many bookmarks", user_id);
        return;
    }

    // Check not already bookmarked
    let already = ctx.db.bookmarks().iter()
        .any(|b| b.user_id == user_id && b.message_id == message_id);
    if already {
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.bookmarks().insert(Bookmark {
        id,
        user_id,
        message_id,
        room_id,
        note,
        created_at: now,
    });
}

/// Remove a bookmark (own bookmarks only).
#[spacetimedb::reducer]
pub fn remove_bookmark(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[remove_bookmark] Unauthorized");
        return;
    };

    let Some(bm) = ctx.db.bookmarks().id().find(&id) else {
        return;
    };

    if bm.user_id != user_id {
        log::warn!("[remove_bookmark] User {} cannot delete bookmark {}", user_id, id);
        return;
    }

    ctx.db.bookmarks().id().delete(&id);
}

/// Update a bookmark note (own bookmarks only).
#[spacetimedb::reducer]
pub fn update_bookmark_note(ctx: &ReducerContext, id: String, note: Option<String>) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[update_bookmark_note] Unauthorized");
        return;
    };

    let Some(bm) = ctx.db.bookmarks().id().find(&id) else {
        return;
    };

    if bm.user_id != user_id {
        return;
    }

    if let Some(ref n) = note {
        if n.len() > MAX_BOOKMARK_NOTE_LEN {
            return;
        }
    }

    ctx.db.bookmarks().id().delete(&id);
    ctx.db.bookmarks().insert(Bookmark {
        note,
        ..bm
    });
}
