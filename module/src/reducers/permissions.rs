//! Permission reducers: kick_user, ban_user, promote_to_admin, demote_from_admin.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::room_members::room_members;
use crate::tables::typing_indicators::typing_indicators;
use crate::utils::validation::{require_admin, find_membership};

/// Kick a user from a room (admin only). They can rejoin later.
#[spacetimedb::reducer]
pub fn kick_user(ctx: &ReducerContext, room_id: String, target_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[kick_user] Unauthorized");
        return;
    };

    if require_admin(ctx, &room_id, &user_id).is_none() {
        return;
    }

    // Can't kick yourself
    if target_user_id == user_id {
        log::warn!("[kick_user] Can't kick yourself");
        return;
    }

    if let Some(target_membership) = find_membership(ctx, &room_id, &target_user_id) {
        // Can't kick another admin
        if target_membership.role == "admin" {
            log::warn!("[kick_user] Can't kick another admin");
            return;
        }
        ctx.db.room_members().id().delete(&target_membership.id);

        // Clean up their typing indicators
        let typing: Vec<TypingIndicator> = ctx.db.typing_indicators().iter()
            .filter(|t| t.room_id == room_id && t.user_id == target_user_id)
            .collect();
        for t in typing {
            ctx.db.typing_indicators().id().delete(&t.id);
        }
    }
}

/// Ban a user from a room (admin only). They cannot rejoin until unbanned.
#[spacetimedb::reducer]
pub fn ban_user(ctx: &ReducerContext, room_id: String, target_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[ban_user] Unauthorized");
        return;
    };

    if require_admin(ctx, &room_id, &user_id).is_none() {
        return;
    }

    if target_user_id == user_id {
        log::warn!("[ban_user] Can't ban yourself");
        return;
    }

    if let Some(target_membership) = find_membership(ctx, &room_id, &target_user_id) {
        if target_membership.role == "admin" {
            log::warn!("[ban_user] Can't ban another admin");
            return;
        }
        // Set role to banned instead of removing
        ctx.db.room_members().id().delete(&target_membership.id);
        ctx.db.room_members().insert(RoomMember {
            role: "banned".to_string(),
            ..target_membership
        });

        // Clean up typing
        let typing: Vec<TypingIndicator> = ctx.db.typing_indicators().iter()
            .filter(|t| t.room_id == room_id && t.user_id == target_user_id)
            .collect();
        for t in typing {
            ctx.db.typing_indicators().id().delete(&t.id);
        }
    }
}

/// Promote a member to channel admin (admin only).
#[spacetimedb::reducer]
pub fn promote_to_admin(ctx: &ReducerContext, room_id: String, target_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[promote_to_admin] Unauthorized");
        return;
    };

    if require_admin(ctx, &room_id, &user_id).is_none() {
        return;
    }

    if let Some(target_membership) = find_membership(ctx, &room_id, &target_user_id) {
        if target_membership.role == "admin" {
            return; // Already admin
        }
        if target_membership.role == "banned" {
            log::warn!("[promote_to_admin] Can't promote a banned user");
            return;
        }
        ctx.db.room_members().id().delete(&target_membership.id);
        ctx.db.room_members().insert(RoomMember {
            role: "admin".to_string(),
            ..target_membership
        });
    }
}

/// Demote a channel admin back to member (admin only). Cannot demote yourself.
#[spacetimedb::reducer]
pub fn demote_from_admin(ctx: &ReducerContext, room_id: String, target_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[demote_from_admin] Unauthorized");
        return;
    };

    if require_admin(ctx, &room_id, &user_id).is_none() {
        return;
    }

    // Can't demote yourself
    if target_user_id == user_id {
        log::warn!("[demote_from_admin] Can't demote yourself");
        return;
    }

    if let Some(target_membership) = find_membership(ctx, &room_id, &target_user_id) {
        if target_membership.role != "admin" {
            return; // Not an admin, nothing to demote
        }
        ctx.db.room_members().id().delete(&target_membership.id);
        ctx.db.room_members().insert(RoomMember {
            role: "member".to_string(),
            ..target_membership
        });
    }
}
