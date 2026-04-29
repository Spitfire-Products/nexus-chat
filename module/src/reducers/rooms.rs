//! Room management reducers: create_room, join_room, leave_room, delete_room.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::rooms::rooms;
use crate::tables::room_members::room_members;
use crate::tables::room_invitations::room_invitations;
use crate::tables::typing_indicators::typing_indicators;
use crate::tables::drafts::drafts;
use crate::tables::users::chat_users;
use crate::tables::messages::messages;
use crate::tables::reactions::reactions;
use crate::tables::message_edits::message_edits;
use crate::tables::reaction_room_index::reaction_room_index;
use crate::tables::message_edit_room_index::message_edit_room_index;
use crate::tables::pinned_messages::pinned_messages;
use crate::tables::message_attachments::message_attachments;
use crate::tables::bookmarks::bookmarks;
use crate::tables::read_positions::read_positions;
use crate::tables::polls::polls;
use crate::tables::polls::poll_votes;
use crate::tables::scheduled_messages::scheduled_messages;
use crate::tables::channel_overrides::channel_overrides;
use crate::tables::webhooks::webhooks;
use crate::utils::validation::{
    MAX_ROOM_NAME_LEN, find_membership, ensure_chat_user,
    require_server_admin, require_admin, get_effective_room_tier, meets_tier,
};
use crate::utils::permissions::*;

/// Create a new room. The creator is automatically joined as admin.
/// When server_id is set, caller must be admin/owner of that server.
#[spacetimedb::reducer]
pub fn create_room(
    ctx: &ReducerContext,
    id: String,
    name: String,
    is_private: bool,
    server_id: Option<String>,
    required_tier: Option<String>,
    description: Option<String>,
    sort_order: Option<u32>,
    category_id: Option<String>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_room] Unauthorized: no identity link");
        return;
    };

    let sender_hex = crate::utils::auth::sender_hex(ctx);
    ensure_chat_user(ctx, &user_id, &sender_hex);

    let trimmed = name.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > MAX_ROOM_NAME_LEN {
        log::warn!("[create_room] Invalid room name length: {}", trimmed.len());
        return;
    }

    // Check duplicate ID
    if ctx.db.rooms().id().find(&id).is_some() {
        log::warn!("[create_room] Room {} already exists", id);
        return;
    }

    // If assigning to a server, verify the server exists and caller is admin/owner
    // If no server (serverless room), require platform admin
    if let Some(ref sid) = server_id {
        if !sid.is_empty() {
            if ctx.db.chat_servers().id().find(sid).is_none() {
                log::warn!("[create_room] Server {} not found", sid);
                return;
            }
            if require_server_admin(ctx, sid, &user_id).is_none() {
                return;
            }
        } else {
            // Empty string server_id = serverless room = admin only
            if !crate::utils::auth::is_platform_admin(ctx) {
                log::warn!("[create_room] Rejected: serverless room creation requires platform admin");
                return;
            }
        }
    } else {
        // No server_id = serverless room = admin only
        if !crate::utils::auth::is_platform_admin(ctx) {
            log::warn!("[create_room] Rejected: serverless room creation requires platform admin");
            return;
        }
    }

    let now = crate::timestamp_ms(ctx);

    ctx.db.rooms().insert(Room {
        id: id.clone(),
        name: trimmed,
        created_by: user_id.clone(),
        is_private,
        is_dm: false,
        created_at: now,
        server_id,
        required_tier,
        description,
        sort_order,
        // New fields — defaults for standard text room
        room_type: "text".to_string(),
        category_id,
        topic: None,
        slowmode_seconds: None,
        nsfw: false,
        parent_room_id: None,
        archived: false,
        locked: false,
        auto_archive_minutes: None,
        default_sort_order: None,
        // Channel content rules — all allowed by default
        allow_attachments: None,
        allow_embeds: None,
        allow_reactions: None,
        rules_text: None,
    });

    // Auto-join creator as admin
    let member_id = format!("{}-{}", id, user_id);
    ctx.db.room_members().insert(RoomMember {
        id: member_id,
        room_id: id.clone(),
        user_id: user_id.clone(),
        role: "admin".to_string(),
        joined_at: now,
    });

    log::info!("[create_room] User {} created room {}", &user_id[..8.min(user_id.len())], &id[..8.min(id.len())]);
}

/// Join an existing public room. Checks tier requirements.
#[spacetimedb::reducer]
pub fn join_room(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[join_room] Unauthorized: no identity link");
        return;
    };

    let sender_hex = crate::utils::auth::sender_hex(ctx);
    ensure_chat_user(ctx, &user_id, &sender_hex);

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        log::warn!("[join_room] Room {} not found", room_id);
        return;
    };

    // Can't join archived or locked threads directly
    if room.archived || room.locked {
        log::warn!("[join_room] Room {} is archived/locked", room_id);
        return;
    }

    // Can't join private rooms without invitation
    if room.is_private {
        let has_invite = ctx.db.room_invitations().iter()
            .any(|inv| inv.room_id == room_id && inv.invitee_id == user_id && inv.status == "accepted");
        if !has_invite {
            log::warn!("[join_room] Room {} is private and user {} has no accepted invitation", room_id, user_id);
            return;
        }
    }

    // Tier check
    let effective_tier = get_effective_room_tier(ctx, &room);
    if !effective_tier.is_empty() {
        let user_tier = ctx.db.chat_users().user_id().find(&user_id)
            .and_then(|u| u.tier.clone());
        if !meets_tier(user_tier.as_deref(), &effective_tier) {
            log::warn!("[join_room] User {} tier {:?} does not meet room requirement {}", user_id, user_tier, effective_tier);
            return;
        }
    }

    // Check if already a member
    if let Some(existing) = find_membership(ctx, &room_id, &user_id) {
        if existing.role == "banned" {
            log::warn!("[join_room] User {} is banned from room {}", user_id, room_id);
            return;
        }
        return;
    }

    let now = crate::timestamp_ms(ctx);
    let member_id = format!("{}-{}", room_id, user_id);
    ctx.db.room_members().insert(RoomMember {
        id: member_id,
        room_id,
        user_id,
        role: "member".to_string(),
        joined_at: now,
    });
}

/// Leave a room.
#[spacetimedb::reducer]
pub fn leave_room(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[leave_room] Unauthorized: no identity link");
        return;
    };

    if let Some(membership) = find_membership(ctx, &room_id, &user_id) {
        ctx.db.room_members().id().delete(&membership.id);

        // Clean up typing indicators
        let typing: Vec<TypingIndicator> = ctx.db.typing_indicators().iter()
            .filter(|t| t.room_id == room_id && t.user_id == user_id)
            .collect();
        for t in typing {
            ctx.db.typing_indicators().id().delete(&t.id);
        }

        // Clean up drafts
        let user_drafts: Vec<Draft> = ctx.db.drafts().iter()
            .filter(|d| d.room_id == room_id && d.user_id == user_id)
            .collect();
        for d in user_drafts {
            ctx.db.drafts().id().delete(&d.id);
        }
    }
}

/// Delete a room and all associated data.
/// - Room admin can delete the room in non-server rooms
/// - Server admin/owner can delete any room in their server
/// - Platform admin (checked via MANAGE_CHANNELS perm) can delete server rooms
#[spacetimedb::reducer]
pub fn delete_room(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_room] Unauthorized: no identity link");
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        log::warn!("[delete_room] Room {} not found", room_id);
        return;
    };

    // Permission check — platform admins can always delete any room
    let has_perm = crate::utils::auth::is_platform_admin(ctx)
        || if let Some(ref server_id) = room.server_id {
            // Server room: need MANAGE_CHANNELS or server admin
            has_permission(ctx, server_id, &room_id, &user_id, PERM_MANAGE_CHANNELS)
                || require_server_admin(ctx, server_id, &user_id).is_some()
        } else {
            // Non-server room: need to be room admin or room creator
            require_admin(ctx, &room_id, &user_id).is_some() || room.created_by == user_id
        };

    if !has_perm {
        log::warn!("[delete_room] User {} lacks permission to delete room {}", user_id, room_id);
        return;
    }

    // Log audit event for server rooms
    if let Some(ref server_id) = room.server_id {
        let audit_id = format!("audit-delroom-{}-{}", room_id, crate::timestamp_ms(ctx));
        crate::reducers::audit_log::log_audit_event(
            ctx, &audit_id, server_id, "room_delete",
            &user_id, "room", &room_id,
            Some(format!("Deleted room '{}'", room.name)),
        );
    }

    // Clean up all related data
    // Messages
    let msg_ids: Vec<String> = ctx.db.messages().iter()
        .filter(|m| m.room_id == room_id)
        .map(|m| m.id.clone())
        .collect();
    for mid in &msg_ids {
        ctx.db.messages().id().delete(mid);
    }

    // Reactions (have denormalized room_id) — cascade-delete shadow index too.
    let reaction_ids: Vec<String> = ctx.db.reactions().iter()
        .filter(|r| r.room_id.as_deref() == Some(room_id.as_str()))
        .map(|r| r.id.clone())
        .collect();
    for rid in reaction_ids {
        ctx.db.reactions().id().delete(&rid);
        ctx.db.reaction_room_index().reaction_id().delete(&rid);
    }

    // Message edits (have denormalized room_id) — cascade-delete shadow index too.
    let edit_ids: Vec<String> = ctx.db.message_edits().iter()
        .filter(|e| e.room_id.as_deref() == Some(room_id.as_str()))
        .map(|e| e.id.clone())
        .collect();
    for eid in edit_ids {
        ctx.db.message_edits().id().delete(&eid);
        ctx.db.message_edit_room_index().edit_id().delete(&eid);
    }

    // Pinned messages
    let pin_ids: Vec<String> = ctx.db.pinned_messages().iter()
        .filter(|p| p.room_id == room_id)
        .map(|p| p.id.clone())
        .collect();
    for pid in pin_ids {
        ctx.db.pinned_messages().id().delete(&pid);
    }

    // Attachments
    let att_ids: Vec<String> = ctx.db.message_attachments().iter()
        .filter(|a| a.room_id == room_id)
        .map(|a| a.id.clone())
        .collect();
    for aid in att_ids {
        ctx.db.message_attachments().id().delete(&aid);
    }

    // Bookmarks
    let bm_ids: Vec<String> = ctx.db.bookmarks().iter()
        .filter(|b| b.room_id == room_id)
        .map(|b| b.id.clone())
        .collect();
    for bid in bm_ids {
        ctx.db.bookmarks().id().delete(&bid);
    }

    // Read positions
    let rp_ids: Vec<String> = ctx.db.read_positions().iter()
        .filter(|r| r.room_id == room_id)
        .map(|r| r.id.clone())
        .collect();
    for rid in rp_ids {
        ctx.db.read_positions().id().delete(&rid);
    }

    // Polls and votes
    let poll_ids: Vec<String> = ctx.db.polls().iter()
        .filter(|p| p.room_id == room_id)
        .map(|p| p.id.clone())
        .collect();
    for pid in &poll_ids {
        let vote_ids: Vec<String> = ctx.db.poll_votes().iter()
            .filter(|v| v.poll_id == *pid)
            .map(|v| v.id.clone())
            .collect();
        for vid in vote_ids {
            ctx.db.poll_votes().id().delete(&vid);
        }
        ctx.db.polls().id().delete(pid);
    }

    // Scheduled messages
    let sched_ids: Vec<String> = ctx.db.scheduled_messages().iter()
        .filter(|s| s.room_id == room_id)
        .map(|s| s.id.clone())
        .collect();
    for sid in sched_ids {
        ctx.db.scheduled_messages().id().delete(&sid);
    }

    // Channel overrides
    let co_ids: Vec<String> = ctx.db.channel_overrides().iter()
        .filter(|c| c.room_id == room_id)
        .map(|c| c.id.clone())
        .collect();
    for cid in co_ids {
        ctx.db.channel_overrides().id().delete(&cid);
    }

    // Room members
    let member_ids: Vec<String> = ctx.db.room_members().iter()
        .filter(|m| m.room_id == room_id)
        .map(|m| m.id.clone())
        .collect();
    for mid in member_ids {
        ctx.db.room_members().id().delete(&mid);
    }

    // Typing indicators
    let typing_ids: Vec<String> = ctx.db.typing_indicators().iter()
        .filter(|t| t.room_id == room_id)
        .map(|t| t.id.clone())
        .collect();
    for tid in typing_ids {
        ctx.db.typing_indicators().id().delete(&tid);
    }

    // Drafts
    let draft_ids: Vec<String> = ctx.db.drafts().iter()
        .filter(|d| d.room_id == room_id)
        .map(|d| d.id.clone())
        .collect();
    for did in draft_ids {
        ctx.db.drafts().id().delete(&did);
    }

    // Room invitations
    let inv_ids: Vec<String> = ctx.db.room_invitations().iter()
        .filter(|i| i.room_id == room_id)
        .map(|i| i.id.clone())
        .collect();
    for iid in inv_ids {
        ctx.db.room_invitations().id().delete(&iid);
    }

    // Webhooks
    let wh_ids: Vec<String> = ctx.db.webhooks().iter()
        .filter(|w| w.room_id == room_id)
        .map(|w| w.id.clone())
        .collect();
    for wid in wh_ids {
        ctx.db.webhooks().id().delete(&wid);
    }

    // Delete the room itself
    ctx.db.rooms().id().delete(&room_id);
    log::info!("[delete_room] Room {} ('{}') deleted by {}", room_id, room.name, user_id);
}

/// Update room settings: name, topic, rules, content restrictions, etc.
/// Only room admins or server admins can update.
#[spacetimedb::reducer]
pub fn update_room_settings(
    ctx: &ReducerContext,
    room_id: String,
    name: Option<String>,
    topic: Option<String>,
    description: Option<String>,
    slowmode_seconds: Option<u32>,
    nsfw: Option<bool>,
    allow_attachments: Option<bool>,
    allow_embeds: Option<bool>,
    allow_reactions: Option<bool>,
    rules_text: Option<String>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[update_room_settings] Unauthorized");
        return;
    };

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        log::warn!("[update_room_settings] Room {} not found", room_id);
        return;
    };

    // Permission check
    let has_perm = if let Some(ref server_id) = room.server_id {
        has_permission(ctx, server_id, &room_id, &user_id, PERM_MANAGE_CHANNELS)
            || require_server_admin(ctx, server_id, &user_id).is_some()
    } else {
        require_admin(ctx, &room_id, &user_id).is_some()
    };

    if !has_perm {
        log::warn!("[update_room_settings] User {} lacks permission for room {}", user_id, room_id);
        return;
    }

    // Delete and re-insert with updated fields (SpacetimeDB pattern)
    ctx.db.rooms().id().delete(&room_id);
    ctx.db.rooms().insert(Room {
        id: room.id,
        name: name.unwrap_or(room.name),
        created_by: room.created_by,
        is_private: room.is_private,
        is_dm: room.is_dm,
        created_at: room.created_at,
        server_id: room.server_id,
        required_tier: room.required_tier,
        description: if description.is_some() { description } else { room.description },
        sort_order: room.sort_order,
        room_type: room.room_type,
        category_id: room.category_id,
        topic: if topic.is_some() { topic } else { room.topic },
        slowmode_seconds: if slowmode_seconds.is_some() { slowmode_seconds } else { room.slowmode_seconds },
        nsfw: nsfw.unwrap_or(room.nsfw),
        parent_room_id: room.parent_room_id,
        archived: room.archived,
        locked: room.locked,
        auto_archive_minutes: room.auto_archive_minutes,
        default_sort_order: room.default_sort_order,
        allow_attachments: if allow_attachments.is_some() { allow_attachments } else { room.allow_attachments },
        allow_embeds: if allow_embeds.is_some() { allow_embeds } else { room.allow_embeds },
        allow_reactions: if allow_reactions.is_some() { allow_reactions } else { room.allow_reactions },
        rules_text: if rules_text.is_some() { rules_text } else { room.rules_text },
    });

    log::info!("[update_room_settings] Room {} updated by {}", room_id, &user_id[..8.min(user_id.len())]);
}

/// Admin: add a bot (or any user) to a room. Platform admin only.
/// Bypasses tier checks, privacy checks, and archived/locked checks.
#[spacetimedb::reducer]
pub fn admin_add_to_room(ctx: &ReducerContext, room_id: String, target_user_id: String) {
    if !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[admin_add_to_room] Rejected: not platform admin");
        return;
    }

    if ctx.db.rooms().id().find(&room_id).is_none() {
        log::warn!("[admin_add_to_room] Room {} not found", room_id);
        return;
    }

    if ctx.db.chat_users().user_id().find(&target_user_id).is_none() {
        log::warn!("[admin_add_to_room] User {} not found", &target_user_id[..8.min(target_user_id.len())]);
        return;
    }

    // Check existing membership
    if let Some(existing) = find_membership(ctx, &room_id, &target_user_id) {
        if existing.role == "banned" {
            ctx.db.room_members().id().delete(&existing.id);
        } else {
            return; // Already a member
        }
    }

    let now = crate::timestamp_ms(ctx);
    let member_id = format!("{}-{}", room_id, target_user_id);
    ctx.db.room_members().insert(RoomMember {
        id: member_id,
        room_id,
        user_id: target_user_id,
        role: "member".to_string(),
        joined_at: now,
    });
}

/// Admin: remove a bot (or any user) from a room. Platform admin only.
/// The mirror of `admin_add_to_room` — used for platform-bot channel
/// toggles in the ADMIN > Cortex AI > Agents & Runtime placement UI.
#[spacetimedb::reducer]
pub fn admin_remove_from_room(ctx: &ReducerContext, room_id: String, target_user_id: String) {
    if !crate::utils::auth::is_platform_admin(ctx) {
        log::warn!("[admin_remove_from_room] Rejected: not platform admin");
        return;
    }

    let to_remove: Vec<String> = ctx.db.room_members().iter()
        .filter(|m| m.room_id == room_id && m.user_id == target_user_id)
        .map(|m| m.id.clone())
        .collect();

    for id in to_remove {
        ctx.db.room_members().id().delete(&id);
    }

    log::info!(
        "[admin_remove_from_room] Removed {} from room {}",
        &target_user_id[..8.min(target_user_id.len())],
        &room_id[..8.min(room_id.len())]
    );
}
