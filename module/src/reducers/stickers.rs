//! Sticker management reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::stickers::stickers;
use crate::utils::permissions::*;

const MAX_STICKER_NAME_LEN: usize = 32;
const MAX_STICKER_DESC_LEN: usize = 200;
const MAX_STICKER_DATA_LEN: usize = 700_000; // ~512KB encoded
const MAX_STICKERS_PER_SERVER: usize = 30;

/// Create a sticker for a server.
#[spacetimedb::reducer]
pub fn create_sticker(
    ctx: &ReducerContext,
    id: String,
    server_id: String,
    name: String,
    description: String,
    image_data: String,
    tags: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_sticker] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MANAGE_EMOJIS, "create_sticker").is_none() {
        return;
    }

    let name_trimmed = name.trim().to_string();
    if name_trimmed.is_empty() || name_trimmed.len() > MAX_STICKER_NAME_LEN {
        return;
    }

    if description.len() > MAX_STICKER_DESC_LEN {
        return;
    }

    if image_data.len() > MAX_STICKER_DATA_LEN {
        log::warn!("[create_sticker] Image data too large: {} bytes", image_data.len());
        return;
    }

    let count = ctx.db.stickers().iter()
        .filter(|s| s.server_id == server_id)
        .count();
    if count >= MAX_STICKERS_PER_SERVER {
        log::warn!("[create_sticker] Server {} has reached sticker limit", server_id);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.stickers().insert(Sticker {
        id,
        server_id,
        name: name_trimmed,
        description,
        image_data,
        tags,
        uploaded_by: user_id,
        created_at: now,
    });
}

/// Delete a sticker.
#[spacetimedb::reducer]
pub fn delete_sticker(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_sticker] Unauthorized");
        return;
    };

    let Some(sticker) = ctx.db.stickers().id().find(&id) else {
        log::warn!("[delete_sticker] Sticker {} not found", id);
        return;
    };

    if require_server_permission(ctx, &sticker.server_id, &user_id, PERM_MANAGE_EMOJIS, "delete_sticker").is_none() {
        return;
    }

    ctx.db.stickers().id().delete(&id);
}
