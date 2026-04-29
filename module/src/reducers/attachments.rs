//! Attachment management reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::message_attachments::message_attachments;
use crate::tables::messages::messages;
use crate::tables::rooms::rooms;
use crate::utils::permissions::*;

const MAX_FILE_NAME_LEN: usize = 256;
const MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;

/// Add an attachment to a message.
#[spacetimedb::reducer]
pub fn add_attachment(
    ctx: &ReducerContext,
    id: String,
    message_id: String,
    room_id: String,
    file_name: String,
    file_url: String,
    file_size: u64,
    content_type: String,
    width: Option<u32>,
    height: Option<u32>,
    is_spoiler: bool,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[add_attachment] Unauthorized");
        return;
    };

    // Verify room membership
    if crate::utils::validation::require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    // Check ATTACH_FILES permission if in a server
    if let Some(room) = ctx.db.rooms().id().find(&room_id) {
        if let Some(ref server_id) = room.server_id {
            if !has_permission(ctx, server_id, &room_id, &user_id, PERM_ATTACH_FILES) {
                log::warn!("[add_attachment] User {} lacks ATTACH_FILES in {}", user_id, room_id);
                return;
            }
        }
    }

    if file_name.len() > MAX_FILE_NAME_LEN {
        return;
    }

    // Check attachment count per message
    let count = ctx.db.message_attachments().iter()
        .filter(|a| a.message_id == message_id)
        .count();
    if count >= MAX_ATTACHMENTS_PER_MESSAGE {
        log::warn!("[add_attachment] Message {} has reached attachment limit", message_id);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.message_attachments().insert(MessageAttachment {
        id,
        message_id,
        room_id,
        file_name,
        file_url,
        file_size,
        content_type,
        width,
        height,
        is_spoiler,
        created_at: now,
    });
}

/// Delete an attachment. Owner or MANAGE_MESSAGES.
#[spacetimedb::reducer]
pub fn delete_attachment(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_attachment] Unauthorized");
        return;
    };

    let Some(attachment) = ctx.db.message_attachments().id().find(&id) else {
        log::warn!("[delete_attachment] Attachment {} not found", id);
        return;
    };

    // Check if caller is the message author
    let is_author = ctx.db.messages().id().find(&attachment.message_id)
        .map(|m| m.author_id == user_id)
        .unwrap_or(false);

    if !is_author {
        // Check MANAGE_MESSAGES if in a server
        if let Some(room) = ctx.db.rooms().id().find(&attachment.room_id) {
            if let Some(ref server_id) = room.server_id {
                if !has_permission(ctx, server_id, &attachment.room_id, &user_id, PERM_MANAGE_MESSAGES) {
                    log::warn!("[delete_attachment] User {} cannot delete attachment {}", user_id, id);
                    return;
                }
            } else {
                return; // Not author and not in server
            }
        }
    }

    ctx.db.message_attachments().id().delete(&id);
}
