//! Thread lifecycle reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::rooms::rooms;
use crate::tables::messages::messages;
use crate::tables::room_members::room_members;
use crate::utils::permissions::*;

/// Create a thread from a message.
#[spacetimedb::reducer]
pub fn create_thread(
    ctx: &ReducerContext,
    thread_room_id: String,
    parent_room_id: String,
    parent_message_id: Option<String>,
    name: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_thread] Unauthorized");
        return;
    };

    let Some(parent) = ctx.db.rooms().id().find(&parent_room_id) else {
        log::warn!("[create_thread] Parent room {} not found", parent_room_id);
        return;
    };

    // Membership check
    if crate::utils::validation::require_membership(ctx, &parent_room_id, &user_id).is_none() {
        return;
    }

    // Permission check
    if let Some(ref server_id) = parent.server_id {
        if !has_permission(ctx, server_id, &parent_room_id, &user_id, PERM_CREATE_THREADS) {
            log::warn!("[create_thread] User {} lacks CREATE_THREADS", user_id);
            return;
        }
    }

    let name_trimmed = name.trim().to_string();
    if name_trimmed.is_empty() || name_trimmed.len() > 100 {
        return;
    }

    // Validate parent message if provided
    if let Some(ref msg_id) = parent_message_id {
        let Some(msg) = ctx.db.messages().id().find(msg_id) else {
            log::warn!("[create_thread] Parent message {} not found", msg_id);
            return;
        };
        if msg.room_id != parent_room_id {
            return;
        }
    }

    let now = crate::timestamp_ms(ctx);

    ctx.db.rooms().insert(Room {
        id: thread_room_id.clone(),
        name: name_trimmed,
        created_by: user_id.clone(),
        is_private: parent.is_private,
        is_dm: false,
        created_at: now,
        server_id: parent.server_id.clone(),
        required_tier: parent.required_tier.clone(),
        description: None,
        sort_order: None,
        room_type: "text".to_string(),
        category_id: parent.category_id.clone(),
        topic: None,
        slowmode_seconds: parent.slowmode_seconds,
        nsfw: parent.nsfw,
        parent_room_id: Some(parent_room_id),
        archived: false,
        locked: false,
        auto_archive_minutes: Some(4320), // 3 days default
        default_sort_order: None,
        allow_attachments: parent.allow_attachments,
        allow_embeds: parent.allow_embeds,
        allow_reactions: parent.allow_reactions,
        rules_text: None,
    });

    // Auto-join creator
    let member_id = format!("{}-{}", thread_room_id, user_id);
    ctx.db.room_members().insert(RoomMember {
        id: member_id,
        room_id: thread_room_id,
        user_id,
        role: "admin".to_string(),
        joined_at: now,
    });
}

/// Archive a thread.
#[spacetimedb::reducer]
pub fn archive_thread(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        return;
    };

    if room.parent_room_id.is_none() {
        log::warn!("[archive_thread] Room {} is not a thread", room_id);
        return;
    }

    // Thread creator or MANAGE_THREADS
    let is_creator = room.created_by == user_id;
    if !is_creator {
        if let Some(ref server_id) = room.server_id {
            if !has_permission(ctx, server_id, &room_id, &user_id, PERM_MANAGE_THREADS) {
                return;
            }
        } else {
            return;
        }
    }

    ctx.db.rooms().id().delete(&room_id);
    ctx.db.rooms().insert(Room {
        archived: true,
        ..room
    });
}

/// Unarchive a thread. Requires MANAGE_THREADS.
#[spacetimedb::reducer]
pub fn unarchive_thread(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        return;
    };

    if room.parent_room_id.is_none() {
        return;
    }

    if let Some(ref server_id) = room.server_id {
        if !has_permission(ctx, server_id, &room_id, &user_id, PERM_MANAGE_THREADS) {
            return;
        }
    }

    ctx.db.rooms().id().delete(&room_id);
    ctx.db.rooms().insert(Room {
        archived: false,
        ..room
    });
}

/// Lock a thread (no new messages). Requires MANAGE_THREADS.
#[spacetimedb::reducer]
pub fn lock_thread(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        return;
    };

    if room.parent_room_id.is_none() {
        return;
    }

    if let Some(ref server_id) = room.server_id {
        if !has_permission(ctx, server_id, &room_id, &user_id, PERM_MANAGE_THREADS) {
            return;
        }
    }

    ctx.db.rooms().id().delete(&room_id);
    ctx.db.rooms().insert(Room {
        locked: true,
        ..room
    });
}

/// Unlock a thread. Requires MANAGE_THREADS.
#[spacetimedb::reducer]
pub fn unlock_thread(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        return;
    };

    if room.parent_room_id.is_none() {
        return;
    }

    if let Some(ref server_id) = room.server_id {
        if !has_permission(ctx, server_id, &room_id, &user_id, PERM_MANAGE_THREADS) {
            return;
        }
    }

    ctx.db.rooms().id().delete(&room_id);
    ctx.db.rooms().insert(Room {
        locked: false,
        ..room
    });
}
