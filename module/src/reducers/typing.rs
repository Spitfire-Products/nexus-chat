//! Typing indicator reducers.

use spacetimedb::{ReducerContext, ScheduleAt, Table};
use crate::tables::*;
use crate::tables::typing_indicators::typing_indicators;
use crate::tables::users::chat_users;
use crate::tables::scheduled_jobs::typing_expiry_jobs;
use crate::utils::validation::{require_membership, check_typing_rate, TYPING_TTL_MS};

/// Signal that the caller is typing in a room.
/// Creates or refreshes a typing indicator with a 4-second TTL.
#[spacetimedb::reducer]
pub fn start_typing(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    if require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Rate limit
    if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
        if !check_typing_rate(&user, now) {
            return;
        }
        ctx.db.chat_users().user_id().delete(&user_id);
        ctx.db.chat_users().insert(ChatUser {
            last_typing_at: now,
            last_seen_at: now,
            ..user
        });
    }

    let expires_at = now + TYPING_TTL_MS;

    // Check for existing typing indicator and refresh it
    let existing: Option<TypingIndicator> = ctx.db.typing_indicators().iter()
        .find(|t| t.room_id == room_id && t.user_id == user_id);

    let indicator_id = if let Some(existing) = existing {
        let id = existing.id.clone();
        ctx.db.typing_indicators().id().delete(&id);
        ctx.db.typing_indicators().insert(TypingIndicator {
            id: id.clone(),
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            expires_at,
        });
        id
    } else {
        let id = format!("typing-{}-{}", room_id, user_id);
        ctx.db.typing_indicators().insert(TypingIndicator {
            id: id.clone(),
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            expires_at,
        });
        id
    };

    // Schedule expiry job
    // expires_at is already in microseconds (from timestamp_ms which returns microseconds)
    ctx.db.typing_expiry_jobs().insert(TypingExpiryJob {
        scheduled_id: 0, // auto-inc
        scheduled_at: ScheduleAt::Time(spacetimedb::Timestamp::from_micros_since_unix_epoch(expires_at as i64)),
        typing_indicator_id: indicator_id,
    });
}

/// Scheduled reducer: expire a typing indicator.
#[spacetimedb::reducer]
pub fn expire_typing_indicator(ctx: &ReducerContext, arg: TypingExpiryJob) {
    let now = crate::timestamp_ms(ctx);

    if let Some(indicator) = ctx.db.typing_indicators().id().find(&arg.typing_indicator_id) {
        // Only delete if actually expired (a newer job may have extended it)
        if indicator.expires_at <= now {
            ctx.db.typing_indicators().id().delete(&arg.typing_indicator_id);
        }
    }
}
