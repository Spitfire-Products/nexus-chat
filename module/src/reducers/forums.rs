//! Forum channel reducers: forum posts, tags.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::rooms::rooms;
use crate::tables::messages::messages;
use crate::tables::room_members::room_members;
use crate::tables::forum_tags::forum_tags;
use crate::tables::users::chat_users;
use crate::utils::permissions::*;

const MAX_TAG_NAME_LEN: usize = 20;
const MAX_TAGS_PER_FORUM: usize = 20;
const MAX_TAGS_PER_POST: usize = 5;

/// Create a forum post (creates a thread room + starter message).
#[spacetimedb::reducer]
pub fn create_forum_post(
    ctx: &ReducerContext,
    thread_room_id: String,
    parent_room_id: String,
    title: String,
    content: String,
    tag_ids_json: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_forum_post] Unauthorized");
        return;
    };

    let Some(parent) = ctx.db.rooms().id().find(&parent_room_id) else {
        log::warn!("[create_forum_post] Parent room {} not found", parent_room_id);
        return;
    };

    if parent.room_type != "forum" {
        log::warn!("[create_forum_post] Parent room {} is not a forum", parent_room_id);
        return;
    }

    // Membership check
    if crate::utils::validation::require_membership(ctx, &parent_room_id, &user_id).is_none() {
        return;
    }

    // Permission check
    if let Some(ref server_id) = parent.server_id {
        if !has_permission(ctx, server_id, &parent_room_id, &user_id, PERM_CREATE_THREADS) {
            log::warn!("[create_forum_post] User {} lacks CREATE_THREADS", user_id);
            return;
        }
    }

    let title_trimmed = title.trim().to_string();
    if title_trimmed.is_empty() || title_trimmed.len() > 100 {
        return;
    }

    let content_trimmed = content.trim().to_string();
    if content_trimmed.is_empty() || content_trimmed.len() > crate::utils::validation::MAX_MESSAGE_LEN {
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Create the thread room
    ctx.db.rooms().insert(Room {
        id: thread_room_id.clone(),
        name: title_trimmed,
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
        auto_archive_minutes: Some(10080), // 7 days default
        default_sort_order: None,
        allow_attachments: parent.allow_attachments,
        allow_embeds: parent.allow_embeds,
        allow_reactions: parent.allow_reactions,
        rules_text: None,
    });

    // Create starter message
    let is_bot = ctx.db.chat_users().user_id().find(&user_id)
        .map(|u| u.is_bot == Some(true))
        .unwrap_or(false);
    let msg_id = format!("{}-starter", thread_room_id);
    ctx.db.messages().insert(Message {
        id: msg_id,
        room_id: thread_room_id.clone(),
        author_id: user_id.clone(),
        content: content_trimmed,
        created_at: now,
        edited_at: None,
        parent_message_id: None,
        is_ephemeral: false,
        expires_at: None,
        message_type: "thread_starter".to_string(),
        reply_to_id: None,
        sticker_ids: None,
        mention_everyone: false,
        mentioned_user_ids: None,
        mentioned_role_ids: None,
        flags: 0,
        is_bot_author: if is_bot { Some(true) } else { None },
    });

    // Auto-join creator
    let member_id = format!("{}-{}", thread_room_id, user_id);
    ctx.db.room_members().insert(RoomMember {
        id: member_id,
        room_id: thread_room_id.clone(),
        user_id: user_id.clone(),
        role: "admin".to_string(),
        joined_at: now,
    });

    // Apply tags
    let tag_ids = crate::utils::auto_mod::extract_json_string_array(&tag_ids_json, "");
    for tag_id in tag_ids.iter().take(MAX_TAGS_PER_POST) {
        if ctx.db.forum_tags().id().find(tag_id).is_some() {
            let link_id = format!("{}-{}", thread_room_id, tag_id);
            ctx.db.forum_post_tags().insert(ForumPostTag {
                id: link_id,
                thread_room_id: thread_room_id.clone(),
                tag_id: tag_id.clone(),
            });
        }
    }
}

/// Add a forum tag definition to a forum channel.
#[spacetimedb::reducer]
pub fn add_forum_tag(
    ctx: &ReducerContext,
    id: String,
    room_id: String,
    name: String,
    emoji: Option<String>,
    color: Option<String>,
    sort_order: u32,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[add_forum_tag] Unauthorized");
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        return;
    };

    if let Some(ref server_id) = room.server_id {
        if require_server_permission(ctx, server_id, &user_id, PERM_MANAGE_CHANNELS, "add_forum_tag").is_none() {
            return;
        }
    }

    let name_trimmed = name.trim().to_string();
    if name_trimmed.is_empty() || name_trimmed.len() > MAX_TAG_NAME_LEN {
        return;
    }

    let count = ctx.db.forum_tags().iter()
        .filter(|t| t.room_id == room_id)
        .count();
    if count >= MAX_TAGS_PER_FORUM {
        return;
    }

    ctx.db.forum_tags().insert(ForumTag {
        id,
        room_id,
        name: name_trimmed,
        emoji,
        color,
        sort_order,
    });
}

/// Remove a forum tag definition.
#[spacetimedb::reducer]
pub fn remove_forum_tag(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let Some(tag) = ctx.db.forum_tags().id().find(&id) else {
        return;
    };

    if let Some(room) = ctx.db.rooms().id().find(&tag.room_id) {
        if let Some(ref server_id) = room.server_id {
            if require_server_permission(ctx, server_id, &user_id, PERM_MANAGE_CHANNELS, "remove_forum_tag").is_none() {
                return;
            }
        }
    }

    // Cascade: remove post-tag links
    let links: Vec<ForumPostTag> = ctx.db.forum_post_tags().iter()
        .filter(|pt| pt.tag_id == id)
        .collect();
    for link in links {
        ctx.db.forum_post_tags().id().delete(&link.id);
    }

    ctx.db.forum_tags().id().delete(&id);
}

/// Tag a forum post.
#[spacetimedb::reducer]
pub fn tag_forum_post(ctx: &ReducerContext, thread_room_id: String, tag_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&thread_room_id) else {
        return;
    };

    // Must be post author or have MANAGE_THREADS
    let is_author = room.created_by == user_id;
    if !is_author {
        if let Some(ref server_id) = room.server_id {
            if !has_permission(ctx, server_id, &thread_room_id, &user_id, PERM_MANAGE_THREADS) {
                return;
            }
        } else {
            return;
        }
    }

    if ctx.db.forum_tags().id().find(&tag_id).is_none() {
        return;
    }

    let link_id = format!("{}-{}", thread_room_id, tag_id);
    if ctx.db.forum_post_tags().id().find(&link_id).is_some() {
        return; // Already tagged
    }

    // Check tag limit per post
    let count = ctx.db.forum_post_tags().iter()
        .filter(|pt| pt.thread_room_id == thread_room_id)
        .count();
    if count >= MAX_TAGS_PER_POST {
        return;
    }

    ctx.db.forum_post_tags().insert(ForumPostTag {
        id: link_id,
        thread_room_id,
        tag_id,
    });
}

/// Untag a forum post.
#[spacetimedb::reducer]
pub fn untag_forum_post(ctx: &ReducerContext, thread_room_id: String, tag_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&thread_room_id) else {
        return;
    };

    let is_author = room.created_by == user_id;
    if !is_author {
        if let Some(ref server_id) = room.server_id {
            if !has_permission(ctx, server_id, &thread_room_id, &user_id, PERM_MANAGE_THREADS) {
                return;
            }
        } else {
            return;
        }
    }

    let link_id = format!("{}-{}", thread_room_id, tag_id);
    if ctx.db.forum_post_tags().id().find(&link_id).is_some() {
        ctx.db.forum_post_tags().id().delete(&link_id);
    }
}
