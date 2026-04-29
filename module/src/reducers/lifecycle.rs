//! Lifecycle reducers: init, client_connected, client_disconnected.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::rooms::rooms;
use crate::tables::users::chat_users;
use crate::tables::typing_indicators::typing_indicators;
use crate::utils::validation::ensure_chat_user;

/// Module initialization — create the default #general room.
#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    let now = crate::timestamp_ms(ctx);

    // Create #general room if it doesn't exist
    let general_id = "00000000-0000-0000-0000-000000000001".to_string();
    if ctx.db.rooms().id().find(&general_id).is_none() {
        ctx.db.rooms().insert(Room {
            id: general_id,
            name: "general".to_string(),
            created_by: "system".to_string(),
            is_private: false,
            is_dm: false,
            created_at: now,
            server_id: None,
            required_tier: None,
            description: None,
            sort_order: None,
            room_type: "text".to_string(),
            category_id: None,
            topic: None,
            slowmode_seconds: None,
            nsfw: false,
            parent_room_id: None,
            archived: false,
            locked: false,
            auto_archive_minutes: None,
            default_sort_order: None,
            allow_attachments: None,
            allow_embeds: None,
            allow_reactions: None,
            rules_text: None,
        });
        log::info!("[init] Created default #general room");
    }
}

/// Called when a client connects.
/// Sets user online and updates identity mapping.
#[spacetimedb::reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    let sender_hex = crate::utils::auth::sender_hex(ctx);
    let now = crate::timestamp_ms(ctx);

    // Look up the user_id from identity link
    if let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) {
        // Ensure chat user exists and set online
        let user = ensure_chat_user(ctx, &user_id, &sender_hex);

        // Update to online
        ctx.db.chat_users().user_id().delete(&user_id);
        ctx.db.chat_users().insert(ChatUser {
            stdb_identity: sender_hex.clone(),
            status: if user.status == "invisible" { "invisible".to_string() } else { "online".to_string() },
            online: true,
            last_seen_at: now,
            ..user
        });

        log::info!("[client_connected] User {} online", &user_id[..8.min(user_id.len())]);
    } else {
        log::info!("[client_connected] Unknown identity {}... (not yet registered)", &sender_hex[..16.min(sender_hex.len())]);
    }
}

/// Called when a client disconnects.
/// Sets user offline and cleans up typing indicators.
#[spacetimedb::reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    let now = crate::timestamp_ms(ctx);

    if let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) {
        // Set offline
        if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
            ctx.db.chat_users().user_id().delete(&user_id);
            ctx.db.chat_users().insert(ChatUser {
                status: "offline".to_string(),
                online: false,
                last_seen_at: now,
                ..user
            });
        }

        // Clean up typing indicators for this user
        let user_typing: Vec<TypingIndicator> = ctx.db.typing_indicators()
            .iter()
            .filter(|t| t.user_id == user_id)
            .collect();
        for indicator in user_typing {
            ctx.db.typing_indicators().id().delete(&indicator.id);
        }

        log::info!("[client_disconnected] User {} offline", &user_id[..8.min(user_id.len())]);
    }
}
