//! Custom emoji reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::server_emojis::server_emojis;
use crate::utils::permissions::*;

/// Max emoji shortcode length
const MAX_EMOJI_NAME_LEN: usize = 32;
/// Max base64 image data size (~256KB encoded = ~192KB raw)
const MAX_EMOJI_DATA_LEN: usize = 350_000;
/// Max emojis per server
const MAX_EMOJIS_PER_SERVER: usize = 50;

/// Create a custom emoji for a server.
#[spacetimedb::reducer]
pub fn create_server_emoji(
    ctx: &ReducerContext,
    id: String,
    server_id: String,
    name: String,
    image_data: String,
    animated: bool,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_server_emoji] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MANAGE_EMOJIS, "create_server_emoji").is_none() {
        return;
    }

    let trimmed = name.trim().to_lowercase();
    if trimmed.is_empty() || trimmed.len() > MAX_EMOJI_NAME_LEN {
        log::warn!("[create_server_emoji] Invalid name length: {}", trimmed.len());
        return;
    }

    // Validate name is alphanumeric + underscores only
    if !trimmed.chars().all(|c| c.is_alphanumeric() || c == '_') {
        log::warn!("[create_server_emoji] Invalid name characters: {}", trimmed);
        return;
    }

    if image_data.len() > MAX_EMOJI_DATA_LEN {
        log::warn!("[create_server_emoji] Image data too large: {} bytes", image_data.len());
        return;
    }

    // Check emoji limit per server
    let count = ctx.db.server_emojis().iter()
        .filter(|e| e.server_id == server_id)
        .count();
    if count >= MAX_EMOJIS_PER_SERVER {
        log::warn!("[create_server_emoji] Server {} has reached emoji limit ({})", server_id, MAX_EMOJIS_PER_SERVER);
        return;
    }

    // Check duplicate name in server
    let name_exists = ctx.db.server_emojis().iter()
        .any(|e| e.server_id == server_id && e.name == trimmed);
    if name_exists {
        log::warn!("[create_server_emoji] Emoji name '{}' already exists in server {}", trimmed, server_id);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.server_emojis().insert(ServerEmoji {
        id,
        server_id,
        name: trimmed,
        image_data,
        animated,
        uploaded_by: user_id,
        created_at: now,
    });
}

/// Delete a custom emoji.
#[spacetimedb::reducer]
pub fn delete_server_emoji(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_server_emoji] Unauthorized");
        return;
    };

    let Some(emoji) = ctx.db.server_emojis().id().find(&id) else {
        log::warn!("[delete_server_emoji] Emoji {} not found", id);
        return;
    };

    if require_server_permission(ctx, &emoji.server_id, &user_id, PERM_MANAGE_EMOJIS, "delete_server_emoji").is_none() {
        return;
    }

    ctx.db.server_emojis().id().delete(&id);
}

/// Rename a custom emoji.
#[spacetimedb::reducer]
pub fn rename_server_emoji(ctx: &ReducerContext, id: String, name: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[rename_server_emoji] Unauthorized");
        return;
    };

    let Some(emoji) = ctx.db.server_emojis().id().find(&id) else {
        log::warn!("[rename_server_emoji] Emoji {} not found", id);
        return;
    };

    if require_server_permission(ctx, &emoji.server_id, &user_id, PERM_MANAGE_EMOJIS, "rename_server_emoji").is_none() {
        return;
    }

    let trimmed = name.trim().to_lowercase();
    if trimmed.is_empty() || trimmed.len() > MAX_EMOJI_NAME_LEN {
        return;
    }
    if !trimmed.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return;
    }

    ctx.db.server_emojis().id().delete(&id);
    ctx.db.server_emojis().insert(ServerEmoji {
        name: trimmed,
        ..emoji
    });
}
