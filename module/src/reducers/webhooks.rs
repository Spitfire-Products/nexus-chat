//! Webhook management and message sending reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::webhooks::webhooks;
use crate::tables::rooms::rooms;
use crate::utils::permissions::*;

const MAX_WEBHOOK_NAME_LEN: usize = 80;
const MAX_WEBHOOKS_PER_ROOM: usize = 10;

/// Create a webhook for a room.
#[spacetimedb::reducer]
pub fn create_webhook(
    ctx: &ReducerContext,
    id: String,
    room_id: String,
    name: String,
    avatar_url: Option<String>,
    token: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_webhook] Unauthorized");
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        log::warn!("[create_webhook] Room {} not found", room_id);
        return;
    };

    if let Some(ref server_id) = room.server_id {
        if require_server_permission(ctx, server_id, &user_id, PERM_MANAGE_WEBHOOKS, "create_webhook").is_none() {
            return;
        }
    }

    let name_trimmed = name.trim().to_string();
    if name_trimmed.is_empty() || name_trimmed.len() > MAX_WEBHOOK_NAME_LEN {
        return;
    }

    if token.is_empty() || token.len() > 256 {
        return;
    }

    let count = ctx.db.webhooks().iter()
        .filter(|w| w.room_id == room_id)
        .count();
    if count >= MAX_WEBHOOKS_PER_ROOM {
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.webhooks().insert(Webhook {
        id,
        room_id,
        name: name_trimmed,
        avatar_url,
        token,
        created_by: user_id,
        created_at: now,
    });
}

/// Delete a webhook.
#[spacetimedb::reducer]
pub fn delete_webhook(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_webhook] Unauthorized");
        return;
    };

    let Some(webhook) = ctx.db.webhooks().id().find(&id) else {
        log::warn!("[delete_webhook] Webhook {} not found", id);
        return;
    };

    if let Some(room) = ctx.db.rooms().id().find(&webhook.room_id) {
        if let Some(ref server_id) = room.server_id {
            if require_server_permission(ctx, server_id, &user_id, PERM_MANAGE_WEBHOOKS, "delete_webhook").is_none() {
                return;
            }
        }
    }

    ctx.db.webhooks().id().delete(&id);
}

/// Send a message via webhook (token auth, no identity link needed).
///
/// `sender_name` and `sender_avatar` are optional per-message overrides
/// used by bridges (e.g. Discord) to show the original sender's identity
/// instead of the static webhook name/avatar.
#[spacetimedb::reducer]
pub fn send_webhook_message(
    ctx: &ReducerContext,
    webhook_id: String,
    token: String,
    content: String,
    sender_name: Option<String>,
    sender_avatar: Option<String>,
) {
    let Some(webhook) = ctx.db.webhooks().id().find(&webhook_id) else {
        log::warn!("[send_webhook_message] Webhook {} not found", webhook_id);
        return;
    };

    // Token authentication
    if webhook.token != token {
        log::warn!("[send_webhook_message] Invalid token for webhook {}", webhook_id);
        return;
    }

    let content_trimmed = content.trim().to_string();
    if content_trimmed.is_empty() || content_trimmed.len() > crate::utils::validation::MAX_MESSAGE_LEN {
        return;
    }

    let now = crate::timestamp_ms(ctx);
    let msg_id = format!("wh-{}-{}", webhook_id, now);

    ctx.db.webhook_messages().insert(WebhookMessage {
        id: msg_id,
        room_id: webhook.room_id,
        webhook_id,
        webhook_name: webhook.name,
        webhook_avatar: webhook.avatar_url,
        content: content_trimmed,
        created_at: now,
        sender_name,
        sender_avatar,
    });
}
