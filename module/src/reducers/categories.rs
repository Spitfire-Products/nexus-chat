//! Category management reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::channel_categories::channel_categories;
use crate::tables::rooms::rooms;
use crate::utils::permissions::*;

const MAX_CATEGORY_NAME_LEN: usize = 64;

/// Check if caller has MANAGE_CHANNELS permission or is platform admin.
fn can_manage_channels(ctx: &ReducerContext, server_id: &str, user_id: &str, reducer: &str) -> bool {
    crate::utils::auth::is_platform_admin(ctx)
        || require_server_permission(ctx, server_id, user_id, PERM_MANAGE_CHANNELS, reducer).is_some()
}

/// Create a channel category in a server.
#[spacetimedb::reducer]
pub fn create_category(
    ctx: &ReducerContext,
    id: String,
    server_id: String,
    name: String,
    sort_order: u32,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_category] Unauthorized");
        return;
    };

    if !can_manage_channels(ctx, &server_id, &user_id, "create_category") {
        return;
    }

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_CATEGORY_NAME_LEN {
        log::warn!("[create_category] Invalid name length: {}", trimmed.len());
        return;
    }

    if ctx.db.channel_categories().id().find(&id).is_some() {
        log::warn!("[create_category] Category {} already exists", id);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.channel_categories().insert(ChannelCategory {
        id,
        server_id,
        name: trimmed,
        sort_order,
        created_at: now,
    });
}

/// Update a category's name or sort order.
#[spacetimedb::reducer]
pub fn update_category(
    ctx: &ReducerContext,
    id: String,
    name: String,
    sort_order: u32,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[update_category] Unauthorized");
        return;
    };

    let Some(cat) = ctx.db.channel_categories().id().find(&id) else {
        log::warn!("[update_category] Category {} not found", id);
        return;
    };

    if !can_manage_channels(ctx, &cat.server_id, &user_id, "update_category") {
        return;
    }

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_CATEGORY_NAME_LEN {
        return;
    }

    ctx.db.channel_categories().id().delete(&id);
    ctx.db.channel_categories().insert(ChannelCategory {
        name: trimmed,
        sort_order,
        ..cat
    });
}

/// Delete a category. Rooms in this category become uncategorized.
#[spacetimedb::reducer]
pub fn delete_category(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_category] Unauthorized");
        return;
    };

    let Some(cat) = ctx.db.channel_categories().id().find(&id) else {
        log::warn!("[delete_category] Category {} not found", id);
        return;
    };

    if !can_manage_channels(ctx, &cat.server_id, &user_id, "delete_category") {
        return;
    }

    // Unset category_id on all rooms in this category
    let affected_rooms: Vec<Room> = ctx.db.rooms().iter()
        .filter(|r| r.category_id.as_deref() == Some(&id))
        .collect();
    for room in affected_rooms {
        let room_id = room.id.clone();
        ctx.db.rooms().id().delete(&room_id);
        ctx.db.rooms().insert(Room {
            category_id: None,
            ..room
        });
    }

    ctx.db.channel_categories().id().delete(&id);
}

/// Move a room to a different category (or uncategorized with empty category_id).
#[spacetimedb::reducer]
pub fn move_room_to_category(ctx: &ReducerContext, room_id: String, category_id: Option<String>) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[move_room_to_category] Unauthorized");
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        log::warn!("[move_room_to_category] Room {} not found", room_id);
        return;
    };

    let server_id = match &room.server_id {
        Some(sid) => sid.clone(),
        None => {
            log::warn!("[move_room_to_category] Room {} is not in a server", room_id);
            return;
        }
    };

    if !can_manage_channels(ctx, &server_id, &user_id, "move_room_to_category") {
        return;
    }

    // Validate category exists if provided
    if let Some(ref cat_id) = category_id {
        if ctx.db.channel_categories().id().find(cat_id).is_none() {
            log::warn!("[move_room_to_category] Category {} not found", cat_id);
            return;
        }
    }

    ctx.db.rooms().id().delete(&room_id);
    ctx.db.rooms().insert(Room {
        category_id,
        ..room
    });
}
