//! Poll reducers: create, vote, unvote, close.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::polls::polls;
use crate::tables::rooms::rooms;
use crate::utils::permissions::*;

const MAX_POLL_QUESTION_LEN: usize = 300;
const MAX_POLL_OPTIONS: usize = 10;

/// Create a poll attached to a message.
#[spacetimedb::reducer]
pub fn create_poll(
    ctx: &ReducerContext,
    id: String,
    message_id: String,
    room_id: String,
    question: String,
    options_json: String,
    allow_multiple: bool,
    anonymous: bool,
    expires_at: Option<u64>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_poll] Unauthorized");
        return;
    };

    if crate::utils::validation::require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    // Check CREATE_POLLS permission if in server
    if let Some(room) = ctx.db.rooms().id().find(&room_id) {
        if let Some(ref server_id) = room.server_id {
            if !has_permission(ctx, server_id, &room_id, &user_id, PERM_CREATE_POLLS) {
                log::warn!("[create_poll] User {} lacks CREATE_POLLS", user_id);
                return;
            }
        }
    }

    let question_trimmed = question.trim().to_string();
    if question_trimmed.is_empty() || question_trimmed.len() > MAX_POLL_QUESTION_LEN {
        return;
    }

    // Validate options_json is a simple JSON array with 2-10 items
    let option_count = options_json.matches('"').count() / 2; // rough count
    if option_count < 2 || option_count > MAX_POLL_OPTIONS {
        log::warn!("[create_poll] Invalid option count: ~{}", option_count);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.polls().insert(Poll {
        id,
        message_id,
        room_id,
        question: question_trimmed,
        options: options_json,
        allow_multiple,
        anonymous,
        expires_at,
        created_by: user_id,
        closed: false,
        created_at: now,
    });
}

/// Vote on a poll option.
#[spacetimedb::reducer]
pub fn vote_poll(ctx: &ReducerContext, poll_id: String, option_index: u32) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[vote_poll] Unauthorized");
        return;
    };

    let Some(poll) = ctx.db.polls().id().find(&poll_id) else {
        log::warn!("[vote_poll] Poll {} not found", poll_id);
        return;
    };

    if poll.closed {
        log::warn!("[vote_poll] Poll {} is closed", poll_id);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    if let Some(expires) = poll.expires_at {
        if now > expires {
            log::warn!("[vote_poll] Poll {} has expired", poll_id);
            return;
        }
    }

    // Must be room member
    if crate::utils::validation::require_membership(ctx, &poll.room_id, &user_id).is_none() {
        return;
    }

    let vote_id = format!("{}-{}-{}", poll_id, user_id, option_index);

    // Check already voted for this option
    if ctx.db.poll_votes().id().find(&vote_id).is_some() {
        return; // Already voted this option
    }

    // If not allow_multiple, remove any existing votes by this user
    if !poll.allow_multiple {
        let existing: Vec<PollVote> = ctx.db.poll_votes().iter()
            .filter(|v| v.poll_id == poll_id && v.user_id == user_id)
            .collect();
        for v in existing {
            ctx.db.poll_votes().id().delete(&v.id);
        }
    }

    ctx.db.poll_votes().insert(PollVote {
        id: vote_id,
        poll_id,
        room_id: poll.room_id,
        user_id,
        option_index,
        created_at: now,
    });
}

/// Remove a vote from a poll option.
#[spacetimedb::reducer]
pub fn unvote_poll(ctx: &ReducerContext, poll_id: String, option_index: u32) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[unvote_poll] Unauthorized");
        return;
    };

    let vote_id = format!("{}-{}-{}", poll_id, user_id, option_index);
    if ctx.db.poll_votes().id().find(&vote_id).is_some() {
        ctx.db.poll_votes().id().delete(&vote_id);
    }
}

/// Close a poll (creator or MANAGE_MESSAGES).
#[spacetimedb::reducer]
pub fn close_poll(ctx: &ReducerContext, poll_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[close_poll] Unauthorized");
        return;
    };

    let Some(poll) = ctx.db.polls().id().find(&poll_id) else {
        log::warn!("[close_poll] Poll {} not found", poll_id);
        return;
    };

    let is_creator = poll.created_by == user_id;
    if !is_creator {
        if let Some(room) = ctx.db.rooms().id().find(&poll.room_id) {
            if let Some(ref server_id) = room.server_id {
                if !has_permission(ctx, server_id, &poll.room_id, &user_id, PERM_MANAGE_MESSAGES) {
                    log::warn!("[close_poll] User {} cannot close poll {}", user_id, poll_id);
                    return;
                }
            } else {
                return;
            }
        }
    }

    ctx.db.polls().id().delete(&poll_id);
    ctx.db.polls().insert(Poll {
        closed: true,
        ..poll
    });
}
