//! Private room and DM reducers: invite_to_room, respond_to_invitation, start_dm.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::rooms::rooms;
use crate::tables::room_members::room_members;
use crate::tables::room_invitations::room_invitations;
use crate::tables::user_blocks::user_blocks;
use crate::tables::messages::messages;
use crate::utils::crypto::hash_blocked_id;
use crate::utils::validation::{require_membership, ensure_chat_user};

/// Invite a user to a private room.
#[spacetimedb::reducer]
pub fn invite_to_room(ctx: &ReducerContext, room_id: String, invitee_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[invite_to_room] Unauthorized");
        return;
    };

    // Must be a member of the room
    if require_membership(ctx, &room_id, &user_id).is_none() {
        return;
    }

    let Some(room) = ctx.db.rooms().id().find(&room_id) else {
        return;
    };

    if !room.is_private {
        log::warn!("[invite_to_room] Room {} is not private — anyone can join", room_id);
        return;
    }

    // Check invitee isn't already a member
    if ctx.db.room_members().iter()
        .any(|m| m.room_id == room_id && m.user_id == invitee_user_id && m.role != "banned")
    {
        log::warn!("[invite_to_room] User {} is already a member of room {}", invitee_user_id, room_id);
        return;
    }

    // Check for existing pending invitation
    if ctx.db.room_invitations().iter()
        .any(|inv| inv.room_id == room_id && inv.invitee_id == invitee_user_id && inv.status == "pending")
    {
        return; // Already invited
    }

    let now = crate::timestamp_ms(ctx);
    let invite_id = format!("inv-{}-{}-{}", room_id, invitee_user_id, now);

    ctx.db.room_invitations().insert(RoomInvitation {
        id: invite_id,
        room_id,
        inviter_id: user_id,
        invitee_id: invitee_user_id,
        status: "pending".to_string(),
        created_at: now,
    });
}

/// Accept or decline a room invitation.
#[spacetimedb::reducer]
pub fn respond_to_invitation(ctx: &ReducerContext, invitation_id: String, accept: bool) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[respond_to_invitation] Unauthorized");
        return;
    };

    let Some(invitation) = ctx.db.room_invitations().id().find(&invitation_id) else {
        log::warn!("[respond_to_invitation] Invitation {} not found", invitation_id);
        return;
    };

    if invitation.invitee_id != user_id {
        log::warn!("[respond_to_invitation] User {} is not the invitee", user_id);
        return;
    }

    if invitation.status != "pending" {
        log::warn!("[respond_to_invitation] Invitation already {}", invitation.status);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    let new_status = if accept { "accepted" } else { "declined" };

    // Update invitation status
    ctx.db.room_invitations().id().delete(&invitation_id);
    ctx.db.room_invitations().insert(RoomInvitation {
        status: new_status.to_string(),
        ..invitation.clone()
    });

    if accept {
        // Join the room
        let member_id = format!("{}-{}", invitation.room_id, user_id);
        if ctx.db.room_members().id().find(&member_id).is_none() {
            ctx.db.room_members().insert(RoomMember {
                id: member_id,
                room_id: invitation.room_id.clone(),
                user_id,
                role: "member".to_string(),
                joined_at: now,
            });
        }
    }
}

/// Start a direct message conversation with another user.
/// Creates a private DM room and auto-joins both users.
/// If a DM already exists, ensures the caller's room_member row is present
/// (triggers subscription update so the client sees the room).
#[spacetimedb::reducer]
pub fn start_dm(ctx: &ReducerContext, dm_room_id: String, target_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[start_dm] Unauthorized");
        return;
    };

    if user_id == target_user_id {
        log::warn!("[start_dm] Can't DM yourself");
        return;
    }

    // Check if either user has blocked the other (hash-based lookup)
    let hash_fwd = hash_blocked_id(&user_id, &target_user_id);
    let block_key_1 = format!("{}:{}", user_id, hash_fwd);
    let hash_rev = hash_blocked_id(&target_user_id, &user_id);
    let block_key_2 = format!("{}:{}", target_user_id, hash_rev);
    if ctx.db.user_blocks().id().find(&block_key_1).is_some()
        || ctx.db.user_blocks().id().find(&block_key_2).is_some()
    {
        log::warn!("[start_dm] Blocked: cannot create DM between {} and {}", user_id, target_user_id);
        return;
    }

    // Check DM restrictions for bots
    if let Some(target) = ctx.db.chat_users().user_id().find(&target_user_id) {
        // Platform bots cannot be DM'd
        if target.is_platform_agent == Some(true) {
            log::warn!("[start_dm] Rejected: cannot DM platform bot {}", &target_user_id[..8.min(target_user_id.len())]);
            return;
        }
        // Personal bots: only owner can DM
        if target.is_bot == Some(true) {
            match target.bot_owner_user_id {
                Some(ref owner) if owner == &user_id => { /* owner can DM their own bot */ }
                _ => {
                    log::warn!("[start_dm] Rejected: non-owner cannot DM personal bot");
                    return;
                }
            }
        }
    }

    let sender_hex = crate::utils::auth::sender_hex(ctx);
    ensure_chat_user(ctx, &user_id, &sender_hex);

    // Check if DM already exists between these two users
    let existing_dm = ctx.db.rooms().iter()
        .filter(|r| r.is_dm)
        .find(|r| {
            let members: Vec<String> = ctx.db.room_members().iter()
                .filter(|m| m.room_id == r.id)
                .map(|m| m.user_id.clone())
                .collect();
            members.len() == 2 && members.contains(&user_id) && members.contains(&target_user_id)
        });

    if let Some(existing) = existing_dm {
        log::info!("[start_dm] DM already exists between {} and {} (room {})", user_id, target_user_id, existing.id);

        // Migrate old name format ("dm-xxx-yyy") to new format ("dm:full_id1:full_id2")
        let mut sorted = [user_id.clone(), target_user_id.clone()];
        sorted.sort();
        let expected_name = format!("dm:{}:{}", sorted[0], sorted[1]);
        let existing_room_id = existing.id.clone();
        if existing.name != expected_name {
            ctx.db.rooms().id().delete(&existing_room_id);
            ctx.db.rooms().insert(Room {
                name: expected_name,
                ..existing
            });
            log::info!("[start_dm] Migrated DM room {} name to new format", existing_room_id);
        }

        // Delete+re-insert caller's room_member row to trigger a subscription refresh
        // so the client sees the room update.
        let member_id = format!("{}-{}", existing_room_id, user_id);
        if let Some(member) = ctx.db.room_members().id().find(&member_id) {
            ctx.db.room_members().id().delete(&member_id);
            ctx.db.room_members().insert(member);
        }
        return;
    }

    let now = crate::timestamp_ms(ctx);

    // Store full user IDs in the name so clients can identify DM partners
    // without needing access to the other user's room_members row.
    let mut sorted = [user_id.clone(), target_user_id.clone()];
    sorted.sort();
    let dm_name = format!("dm:{}:{}", sorted[0], sorted[1]);

    // Create DM room
    ctx.db.rooms().insert(Room {
        id: dm_room_id.clone(),
        name: dm_name,
        created_by: user_id.clone(),
        is_private: true,
        is_dm: true,
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

    // Join both users
    ctx.db.room_members().insert(RoomMember {
        id: format!("{}-{}", dm_room_id, user_id),
        room_id: dm_room_id.clone(),
        user_id: user_id.clone(),
        role: "admin".to_string(),
        joined_at: now,
    });

    ctx.db.room_members().insert(RoomMember {
        id: format!("{}-{}", dm_room_id, target_user_id),
        room_id: dm_room_id,
        user_id: target_user_id,
        role: "admin".to_string(),
        joined_at: now,
    });
}

/// Admin-only: migrate all DM room names to new format and clean up orphaned/duplicate DMs.
#[spacetimedb::reducer]
pub fn migrate_dm_rooms(ctx: &ReducerContext) {
    // Require authenticated caller (any user can trigger migration — it's idempotent)
    if crate::utils::auth::get_caller_user_id(ctx).is_none() && !crate::utils::auth::is_system_caller(ctx) {
        log::warn!("[migrate_dm_rooms] Unauthorized");
        return;
    }

    let dm_rooms: Vec<Room> = ctx.db.rooms().iter().filter(|r| r.is_dm).collect();
    let mut migrated = 0u32;
    let mut orphaned_deleted = 0u32;
    let mut duplicates_deleted = 0u32;
    let mut seen_pairs: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for room in dm_rooms {
        let members: Vec<String> = ctx.db.room_members().iter()
            .filter(|m| m.room_id == room.id)
            .map(|m| m.user_id.clone())
            .collect();

        // Delete orphaned DM rooms (no members)
        if members.is_empty() {
            ctx.db.rooms().id().delete(&room.id);
            orphaned_deleted += 1;
            continue;
        }

        // Only process 2-member DM rooms
        if members.len() != 2 {
            continue;
        }

        let mut sorted = members.clone();
        sorted.sort();
        let pair_key = format!("{}:{}", sorted[0], sorted[1]);
        let expected_name = format!("dm:{}", pair_key);

        // Check for duplicate DM pairs — keep the first one seen (or the one with messages)
        if let Some(kept_id) = seen_pairs.get(&pair_key) {
            // Check if this room has messages while the kept one might not
            let this_has_messages = ctx.db.messages().iter().any(|m| m.room_id == room.id);
            let kept_has_messages = ctx.db.messages().iter().any(|m| m.room_id == *kept_id);

            if this_has_messages && !kept_has_messages {
                // This one has messages, delete the kept one instead
                let old_kept = kept_id.clone();
                // Delete old kept room's members
                for member in ctx.db.room_members().iter().filter(|m| m.room_id == old_kept).collect::<Vec<_>>() {
                    ctx.db.room_members().id().delete(&member.id);
                }
                ctx.db.rooms().id().delete(&old_kept);
                duplicates_deleted += 1;
                // Update to keep this one
                seen_pairs.insert(pair_key.clone(), room.id.clone());
            } else {
                // Delete this duplicate
                for member in ctx.db.room_members().iter().filter(|m| m.room_id == room.id).collect::<Vec<_>>() {
                    ctx.db.room_members().id().delete(&member.id);
                }
                ctx.db.rooms().id().delete(&room.id);
                duplicates_deleted += 1;
                continue;
            }
        } else {
            seen_pairs.insert(pair_key, room.id.clone());
        }

        // Migrate room name to new format
        if room.name != expected_name {
            let room_id = room.id.clone();
            ctx.db.rooms().id().delete(&room_id);
            ctx.db.rooms().insert(Room {
                name: expected_name,
                ..room
            });
            migrated += 1;
        }
    }

    log::info!("[migrate_dm_rooms] Done: {} migrated, {} orphans deleted, {} duplicates deleted",
        migrated, orphaned_deleted, duplicates_deleted);
}
