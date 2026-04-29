//! Scheduled message reducers: schedule_message, cancel_scheduled_message, send_scheduled_message.

use spacetimedb::{ReducerContext, ScheduleAt, Table};
use crate::tables::*;
use crate::tables::messages::messages;
use crate::tables::scheduled_messages::scheduled_messages;
use crate::tables::scheduled_jobs::scheduled_delivery_jobs;
use crate::tables::users::chat_users;
use crate::utils::validation::require_membership;

/// Schedule a message for future delivery.
#[spacetimedb::reducer]
pub fn schedule_message(
    ctx: &ReducerContext,
    id: String,
    room_id: String,
    content: String,
    send_at: u64,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[schedule_message] Unauthorized");
        return;
    };

    if require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    let trimmed = content.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > crate::utils::validation::MAX_MESSAGE_LEN {
        return;
    }

    let now = crate::timestamp_ms(ctx);
    if send_at <= now {
        log::warn!("[schedule_message] send_at must be in the future");
        return;
    }

    // Create the delivery job first to get the job_id
    let send_micros = (send_at as i64) * 1000;
    let job = ScheduledDeliveryJob {
        scheduled_id: 0, // auto-inc
        scheduled_at: ScheduleAt::Time(spacetimedb::Timestamp::from_micros_since_unix_epoch(send_micros)),
        scheduled_message_id: id.clone(),
    };
    let inserted_job = ctx.db.scheduled_delivery_jobs().insert(job);

    ctx.db.scheduled_messages().insert(ScheduledMessage {
        id,
        room_id,
        author_id: user_id,
        content: trimmed,
        send_at,
        job_id: inserted_job.scheduled_id,
        created_at: now,
    });
}

/// Cancel a pending scheduled message.
#[spacetimedb::reducer]
pub fn cancel_scheduled_message(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let Some(scheduled) = ctx.db.scheduled_messages().id().find(&id) else {
        log::warn!("[cancel_scheduled_message] Scheduled message {} not found", id);
        return;
    };

    if scheduled.author_id != user_id {
        log::warn!("[cancel_scheduled_message] User {} is not the author", user_id);
        return;
    }

    // Delete the delivery job
    ctx.db.scheduled_delivery_jobs().scheduled_id().delete(&scheduled.job_id);

    // Delete the scheduled message
    ctx.db.scheduled_messages().id().delete(&id);
}

/// Scheduled reducer: deliver a scheduled message.
#[spacetimedb::reducer]
pub fn send_scheduled_message(ctx: &ReducerContext, arg: ScheduledDeliveryJob) {
    let Some(scheduled) = ctx.db.scheduled_messages().id().find(&arg.scheduled_message_id) else {
        return; // Already cancelled
    };

    let now = crate::timestamp_ms(ctx);

    // Check if author is a bot
    let is_bot = ctx.db.chat_users().user_id().find(&scheduled.author_id)
        .map(|u| u.is_bot == Some(true))
        .unwrap_or(false);

    // Insert as a real message
    ctx.db.messages().insert(Message {
        id: scheduled.id.clone(),
        room_id: scheduled.room_id.clone(),
        author_id: scheduled.author_id.clone(),
        content: scheduled.content.clone(),
        created_at: now,
        edited_at: None,
        parent_message_id: None,
        is_ephemeral: false,
        expires_at: None,
        message_type: "default".to_string(),
        reply_to_id: None,
        sticker_ids: None,
        mention_everyone: false,
        mentioned_user_ids: None,
        mentioned_role_ids: None,
        flags: 0,
        is_bot_author: if is_bot { Some(true) } else { None },
    });

    // Remove the scheduled message entry
    ctx.db.scheduled_messages().id().delete(&scheduled.id);
}
