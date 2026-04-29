//! Messaging reducers: send_message, send_ephemeral_message, edit_message, delete_message.

use spacetimedb::{ReducerContext, ScheduleAt, Table};
use crate::tables::*;
use crate::tables::messages::messages;
use crate::tables::users::chat_users;
use crate::tables::message_edits::message_edits;
use crate::tables::message_edit_room_index::message_edit_room_index;
use crate::tables::reaction_room_index::reaction_room_index;
use crate::tables::scheduled_jobs::ephemeral_cleanup_jobs;
use crate::tables::rooms::rooms;
use crate::tables::server_members::server_members;
use crate::tables::reactions::reactions;
use crate::tables::pinned_messages::pinned_messages;
use crate::tables::message_attachments::message_attachments;
use crate::tables::user_blocks::user_blocks;
use crate::tables::room_members::room_members;
use crate::utils::crypto::hash_blocked_id;
use crate::utils::validation::{require_membership, require_admin, check_message_rate, MAX_MESSAGE_LEN, get_effective_room_tier, meets_tier};
use crate::utils::mentions::{parse_mentions, user_ids_to_json, role_ids_to_json};
use crate::utils::auto_mod::{check_auto_mod, AutoModAction};
use crate::utils::permissions::*;

/// Send a message to a room.
#[spacetimedb::reducer]
pub fn send_message(
    ctx: &ReducerContext,
    id: String,
    room_id: String,
    content: String,
    parent_message_id: Option<String>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[send_message] Unauthorized: no identity link");
        return;
    };

    // Membership check
    if require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        log::warn!("[send_message] Room {} not found", room_id);
        return;
    };

    // Locked thread check
    if room.locked {
        log::warn!("[send_message] Room {} is locked", room_id);
        return;
    }

    // Archived thread check
    if room.archived {
        log::warn!("[send_message] Room {} is archived", room_id);
        return;
    }

    // DM block guard: prevent messages in DM rooms between blocked users
    if room.is_dm {
        let other_user_id: Option<String> = ctx.db.room_members().iter()
            .filter(|m| m.room_id == room_id && m.user_id != user_id)
            .map(|m| m.user_id.clone())
            .next();
        if let Some(ref other_id) = other_user_id {
            let hash_fwd = hash_blocked_id(&user_id, other_id);
            let block_key_fwd = format!("{}:{}", user_id, hash_fwd);
            let hash_rev = hash_blocked_id(other_id, &user_id);
            let block_key_rev = format!("{}:{}", other_id, hash_rev);
            if ctx.db.user_blocks().id().find(&block_key_fwd).is_some()
                || ctx.db.user_blocks().id().find(&block_key_rev).is_some()
            {
                log::warn!("[send_message] Blocked: cannot send DM between {} and {}", &user_id[..8.min(user_id.len())], &other_id[..8.min(other_id.len())]);
                return;
            }
        }
    }

    // Tier check
    let effective_tier = get_effective_room_tier(ctx, &room);
    if !effective_tier.is_empty() {
        let user_tier = ctx.db.chat_users().user_id().find(&user_id)
            .and_then(|u| u.tier.clone());
        if !meets_tier(user_tier.as_deref(), &effective_tier) {
            log::warn!("[send_message] User {} tier insufficient for room {}", user_id, room_id);
            return;
        }
    }

    // Timeout check (server members)
    if let Some(ref server_id) = room.server_id {
        let member_id = format!("{}-{}", server_id, user_id);
        if let Some(member) = ctx.db.server_members().id().find(&member_id) {
            if let Some(timeout_until) = member.timeout_until {
                let now_check = crate::timestamp_ms(ctx);
                if now_check < timeout_until {
                    log::warn!("[send_message] User {} is timed out in server {}", user_id, server_id);
                    return;
                }
            }
        }
    }

    // SEND_MESSAGES permission check
    if let Some(ref server_id) = room.server_id {
        if !has_permission(ctx, server_id, &room_id, &user_id, PERM_SEND_MESSAGES) {
            log::warn!("[send_message] User {} lacks SEND_MESSAGES in {}", user_id, room_id);
            return;
        }
    }

    // Content validation
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_MESSAGE_LEN {
        log::warn!("[send_message] Invalid content length: {}", trimmed.len());
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Slowmode check
    if let Some(slowmode) = room.slowmode_seconds {
        if slowmode > 0 {
            if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
                let cooldown_ms = (slowmode as u64) * 1000;
                if now.saturating_sub(user.last_message_at) < cooldown_ms {
                    log::warn!("[send_message] Slowmode: user {} must wait", user_id);
                    return;
                }
            }
        }
    }

    // Auto-mod check
    if let Some(ref server_id) = room.server_id {
        match check_auto_mod(ctx, server_id, &room_id, &user_id, &trimmed) {
            AutoModAction::Allow => {}
            AutoModAction::Block(reason) => {
                log::warn!("[send_message] Auto-mod blocked: {}", reason);
                return;
            }
            AutoModAction::Flag(reason) => {
                log::info!("[send_message] Auto-mod flagged: {}", reason);
                // Still allow the message through, but log for review
            }
            AutoModAction::Timeout(duration, reason) => {
                log::warn!("[send_message] Auto-mod timeout: {} ({}s)", reason, duration);
                // Apply timeout and block message
                crate::reducers::timeouts::timeout_member(
                    ctx,
                    server_id.clone(),
                    user_id.clone(),
                    duration,
                    Some(format!("Auto-mod: {}", reason)),
                );
                return;
            }
        }
    }

    // Rate limit (stricter for bots: 500ms cooldown vs standard rate)
    let is_bot = ctx.db.chat_users().user_id().find(&user_id)
        .map(|u| u.is_bot == Some(true))
        .unwrap_or(false);

    if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
        if is_bot {
            // Bot rate limit: configurable per tier (default 5000ms = 5s)
            let bot_rate_ms = crate::reducers::agents::get_bot_rate_limit_ms(ctx, &user_id);
            if now.saturating_sub(user.last_message_at) < bot_rate_ms {
                log::warn!("[send_message] Bot rate limited: {}", &user_id[..8.min(user_id.len())]);
                return;
            }
        } else if !check_message_rate(&user, now) {
            log::warn!("[send_message] Rate limited: user {}", &user_id[..8.min(user_id.len())]);
            return;
        }
        // Update rate limit timestamp
        ctx.db.chat_users().user_id().delete(&user_id);
        ctx.db.chat_users().insert(ChatUser {
            last_message_at: now,
            last_seen_at: now,
            ..user
        });
    }

    // Validate parent_message_id if provided
    if let Some(ref parent_id) = parent_message_id {
        if ctx.db.messages().id().find(parent_id).is_none() {
            log::warn!("[send_message] Parent message {} not found", parent_id);
            return;
        }
    }

    // Parse mentions
    let mentions = parse_mentions(&trimmed);

    // Check MENTION_EVERYONE permission
    let mention_everyone = if mentions.everyone || mentions.here {
        if let Some(ref server_id) = room.server_id {
            has_permission(ctx, server_id, &room_id, &user_id, PERM_MENTION_EVERYONE)
        } else {
            true // Non-server rooms allow @everyone
        }
    } else {
        false
    };

    let mentioned_user_ids = if mentions.user_ids.is_empty() {
        None
    } else {
        Some(user_ids_to_json(&mentions.user_ids))
    };

    let mentioned_role_ids = if mentions.role_ids.is_empty() {
        None
    } else {
        Some(role_ids_to_json(&mentions.role_ids))
    };

    let message_type = if parent_message_id.is_some() {
        "reply".to_string()
    } else {
        "default".to_string()
    };

    ctx.db.messages().insert(Message {
        id,
        room_id,
        author_id: user_id,
        content: trimmed,
        created_at: now,
        edited_at: None,
        parent_message_id,
        is_ephemeral: false,
        expires_at: None,
        message_type,
        reply_to_id: None,
        sticker_ids: None,
        mention_everyone,
        mentioned_user_ids,
        mentioned_role_ids,
        flags: 0,
        is_bot_author: if is_bot { Some(true) } else { None },
    });
}

/// Send an ephemeral message that auto-deletes after ttl_seconds.
#[spacetimedb::reducer]
pub fn send_ephemeral_message(
    ctx: &ReducerContext,
    id: String,
    room_id: String,
    content: String,
    ttl_seconds: u64,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[send_ephemeral_message] Unauthorized");
        return;
    };

    if require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    // Tier check
    if let Some(room) = ctx.db.rooms().id().find(&room_id) {
        let effective_tier = get_effective_room_tier(ctx, &room);
        if !effective_tier.is_empty() {
            let user_tier = ctx.db.chat_users().user_id().find(&user_id)
                .and_then(|u| u.tier.clone());
            if !meets_tier(user_tier.as_deref(), &effective_tier) {
                log::warn!("[send_ephemeral_message] User {} tier insufficient for room {}", user_id, room_id);
                return;
            }
        }
    }

    let trimmed = content.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_MESSAGE_LEN {
        return;
    }

    // Clamp TTL between 10 seconds and 1 hour
    let clamped_ttl = ttl_seconds.clamp(10, 3600);

    let now = crate::timestamp_ms(ctx);
    let expires_at = now + (clamped_ttl * 1000);

    // Rate limit
    if let Some(user) = ctx.db.chat_users().user_id().find(&user_id) {
        if !check_message_rate(&user, now) {
            return;
        }
        ctx.db.chat_users().user_id().delete(&user_id);
        ctx.db.chat_users().insert(ChatUser {
            last_message_at: now,
            last_seen_at: now,
            ..user
        });
    }

    // Check if author is a bot
    let is_bot = ctx.db.chat_users().user_id().find(&user_id)
        .map(|u| u.is_bot == Some(true))
        .unwrap_or(false);

    // Insert ephemeral message
    ctx.db.messages().insert(Message {
        id: id.clone(),
        room_id,
        author_id: user_id,
        content: trimmed,
        created_at: now,
        edited_at: None,
        parent_message_id: None,
        is_ephemeral: true,
        expires_at: Some(expires_at),
        message_type: "default".to_string(),
        reply_to_id: None,
        sticker_ids: None,
        mention_everyone: false,
        mentioned_user_ids: None,
        mentioned_role_ids: None,
        flags: 0,
        is_bot_author: if is_bot { Some(true) } else { None },
    });

    // Schedule cleanup job
    let expires_micros = (expires_at as i64) * 1000;
    ctx.db.ephemeral_cleanup_jobs().insert(EphemeralCleanupJob {
        scheduled_id: 0, // auto-inc
        scheduled_at: ScheduleAt::Time(spacetimedb::Timestamp::from_micros_since_unix_epoch(expires_micros)),
        message_id: id,
    });
}

/// Delete a message.
/// - Author can delete their own messages
/// - Server owner can delete any message in their server
/// - Server members with MANAGE_MESSAGES permission can delete any message in that server
/// - Room admins can delete any message in non-server rooms
#[spacetimedb::reducer]
pub fn delete_message(ctx: &ReducerContext, message_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_message] Unauthorized");
        return;
    };

    let Some(msg) = ctx.db.messages().id().find(&message_id) else {
        log::warn!("[delete_message] Message {} not found", message_id);
        return;
    };

    let is_author = msg.author_id == user_id;

    // Check permission if not the author
    if !is_author {
        let room = ctx.db.rooms().id().find(&msg.room_id);
        let has_perm = if let Some(ref room) = room {
            if let Some(ref server_id) = room.server_id {
                // Server room: server owner bypasses, or check MANAGE_MESSAGES permission
                is_server_owner(ctx, server_id, &user_id)
                || has_permission(ctx, server_id, &msg.room_id, &user_id, PERM_MANAGE_MESSAGES)
            } else {
                // Non-server room (DMs etc): check if user is room admin
                require_admin(ctx, &msg.room_id, &user_id).is_some()
            }
        } else {
            false
        };

        if !has_perm {
            log::warn!("[delete_message] User {} cannot delete message {}", user_id, message_id);
            return;
        }

        // Log audit event for moderation deletions (not self-deletes)
        if let Some(ref room) = room {
            if let Some(ref server_id) = room.server_id {
                let audit_id = format!("audit-del-{}-{}", message_id, crate::timestamp_ms(ctx));
                crate::reducers::audit_log::log_audit_event(
                    ctx, &audit_id, server_id, "message_delete",
                    &user_id, "message", &message_id,
                    Some(format!("Deleted message by {} in room {}", msg.author_id, msg.room_id)),
                );
            }
        }
    }

    // Clean up related data: reactions, edits, attachments, pins.
    // shadowing-stork: cascade-delete the room-scoped shadow indexes
    // alongside the source rows (same primary key on both).
    let reactions_to_delete: Vec<String> = ctx.db.reactions().iter()
        .filter(|r| r.message_id == message_id)
        .map(|r| r.id.clone())
        .collect();
    for rid in reactions_to_delete {
        ctx.db.reactions().id().delete(&rid);
        ctx.db.reaction_room_index().reaction_id().delete(&rid);
    }

    let edits_to_delete: Vec<String> = ctx.db.message_edits().iter()
        .filter(|e| e.message_id == message_id)
        .map(|e| e.id.clone())
        .collect();
    for eid in edits_to_delete {
        ctx.db.message_edits().id().delete(&eid);
        ctx.db.message_edit_room_index().edit_id().delete(&eid);
    }

    let pins_to_delete: Vec<String> = ctx.db.pinned_messages().iter()
        .filter(|p| p.message_id == message_id)
        .map(|p| p.id.clone())
        .collect();
    for pid in pins_to_delete {
        ctx.db.pinned_messages().id().delete(&pid);
    }

    let attachments_to_delete: Vec<String> = ctx.db.message_attachments().iter()
        .filter(|a| a.message_id == message_id)
        .map(|a| a.id.clone())
        .collect();
    for aid in attachments_to_delete {
        ctx.db.message_attachments().id().delete(&aid);
    }

    // Delete the message itself
    ctx.db.messages().id().delete(&message_id);
    log::info!("[delete_message] Message {} deleted by {}", message_id, user_id);
}

/// Edit an existing message (own messages only). Creates edit history entry.
#[spacetimedb::reducer]
pub fn edit_message(ctx: &ReducerContext, message_id: String, new_content: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[edit_message] Unauthorized");
        return;
    };

    let trimmed = new_content.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_MESSAGE_LEN {
        return;
    }

    let Some(msg) = ctx.db.messages().id().find(&message_id) else {
        log::warn!("[edit_message] Message {} not found", message_id);
        return;
    };

    if msg.author_id != user_id {
        log::warn!("[edit_message] User {} is not the author of message {}", user_id, message_id);
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Re-parse mentions for edited content
    let mentions = parse_mentions(&trimmed);
    let mention_everyone = mentions.everyone || mentions.here;
    let mentioned_user_ids = if mentions.user_ids.is_empty() {
        None
    } else {
        Some(user_ids_to_json(&mentions.user_ids))
    };
    let mentioned_role_ids = if mentions.role_ids.is_empty() {
        None
    } else {
        Some(role_ids_to_json(&mentions.role_ids))
    };

    // Create edit history entry
    let edit_id = format!("{}-edit-{}", message_id, now);
    let edit_id_for_index = edit_id.clone();
    let editor_for_index = user_id.clone();
    let new_content_for_index = trimmed.clone();
    let room_for_index = msg.room_id.clone();
    let message_for_index = message_id.clone();
    ctx.db.message_edits().insert(MessageEdit {
        id: edit_id,
        message_id: message_id.clone(),
        editor_id: user_id,
        old_content: msg.content.clone(),
        new_content: trimmed.clone(),
        edited_at: now,
        room_id: Some(msg.room_id.clone()),
    });
    // shadowing-stork: dual-write into the room-scoped index.
    ctx.db.message_edit_room_index().insert(MessageEditRoomIndex {
        edit_id: edit_id_for_index,
        room_id: room_for_index,
        message_id: message_for_index,
        editor_id: editor_for_index,
        edited_at: now,
        new_content: new_content_for_index,
    });

    // Update the message
    ctx.db.messages().id().delete(&message_id);
    ctx.db.messages().insert(Message {
        content: trimmed,
        edited_at: Some(now),
        mention_everyone,
        mentioned_user_ids,
        mentioned_role_ids,
        ..msg
    });
}
