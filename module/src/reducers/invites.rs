//! Server invite reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::server_invites::server_invites;
use crate::tables::servers::chat_servers;
use crate::tables::server_members::server_members;
use crate::utils::permissions::*;
use crate::utils::validation::{find_server_membership, ensure_chat_user};

/// Create a server invite code.
#[spacetimedb::reducer]
pub fn create_server_invite(
    ctx: &ReducerContext,
    code: String,
    server_id: String,
    max_uses: Option<u32>,
    expires_at: Option<u64>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_server_invite] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_CREATE_INVITES, "create_server_invite").is_none() {
        return;
    }

    // Validate code format (alphanumeric, 4-16 chars)
    if code.len() < 4 || code.len() > 16 || !code.chars().all(|c| c.is_alphanumeric()) {
        log::warn!("[create_server_invite] Invalid invite code: {}", code);
        return;
    }

    if ctx.db.server_invites().code().find(&code).is_some() {
        log::warn!("[create_server_invite] Code {} already exists", code);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.server_invites().insert(ServerInvite {
        code,
        server_id,
        created_by: user_id,
        max_uses,
        uses: 0,
        expires_at,
        created_at: now,
    });
}

/// Delete a server invite. Creator or MANAGE_SERVER.
#[spacetimedb::reducer]
pub fn delete_server_invite(ctx: &ReducerContext, code: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_server_invite] Unauthorized");
        return;
    };

    let Some(invite) = ctx.db.server_invites().code().find(&code) else {
        log::warn!("[delete_server_invite] Invite {} not found", code);
        return;
    };

    let is_creator = invite.created_by == user_id;
    if !is_creator {
        if require_server_permission(ctx, &invite.server_id, &user_id, PERM_MANAGE_SERVER, "delete_server_invite").is_none() {
            return;
        }
    }

    ctx.db.server_invites().code().delete(&code);
}

/// Use a server invite to join a server.
#[spacetimedb::reducer]
pub fn use_server_invite(ctx: &ReducerContext, code: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[use_server_invite] Unauthorized");
        return;
    };

    let Some(invite) = ctx.db.server_invites().code().find(&code) else {
        log::warn!("[use_server_invite] Invite {} not found or expired", code);
        return;
    };

    let now = crate::timestamp_ms(ctx);

    // Check expiry
    if let Some(expires) = invite.expires_at {
        if now > expires {
            log::warn!("[use_server_invite] Invite {} has expired", code);
            // Clean up expired invite
            ctx.db.server_invites().code().delete(&code);
            return;
        }
    }

    // Check max uses
    if let Some(max) = invite.max_uses {
        if invite.uses >= max {
            log::warn!("[use_server_invite] Invite {} has reached max uses", code);
            return;
        }
    }

    // Check server exists
    if ctx.db.chat_servers().id().find(&invite.server_id).is_none() {
        log::warn!("[use_server_invite] Server {} not found", invite.server_id);
        return;
    }

    // Check not already a member or banned
    if let Some(existing) = find_server_membership(ctx, &invite.server_id, &user_id) {
        if existing.role == "banned" {
            log::warn!("[use_server_invite] User {} is banned from server {}", user_id, invite.server_id);
            return;
        }
        // Already a member
        return;
    }

    // Ensure chat user exists
    let sender_hex = crate::utils::auth::sender_hex(ctx);
    ensure_chat_user(ctx, &user_id, &sender_hex);

    // Join the server
    let member_id = format!("{}-{}", invite.server_id, user_id);
    ctx.db.server_members().insert(ServerMember {
        id: member_id,
        server_id: invite.server_id.clone(),
        user_id,
        role: "member".to_string(),
        joined_at: now,
        nickname: None,
        timeout_until: None,
        deaf: false,
        mute: false,
    });

    // Increment use count
    ctx.db.server_invites().code().delete(&code);
    ctx.db.server_invites().insert(ServerInvite {
        uses: invite.uses + 1,
        ..invite
    });
}
