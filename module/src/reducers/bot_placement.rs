//! User-level bot placement reducers.
//!
//! These reducers let users control where their personal agents operate.
//! Only the agent owner can add/remove their bot from servers and channels.
//! Platform bot placement is managed by admins via admin_add_to_server/admin_add_to_room.
//!
//! Authorization for each reducer:
//! 1. Caller must be a registered user (get_caller_user_id)
//! 2. Agent must exist and be a bot (is_bot == true)
//! 3. Caller must own the agent (bot_owner_user_id == caller)
//! 4. Caller must be a member of the target server/room

use spacetimedb::{ReducerContext, Table};
use crate::tables::users::chat_users;
use crate::tables::rooms::rooms;
use crate::tables::room_members::{RoomMember, room_members};
use crate::tables::server_members::{ServerMember, server_members};
use crate::tables::servers::chat_servers;
use crate::utils::validation::{find_server_membership, find_membership};

/// Verify the caller owns the specified agent. Returns the caller's user_id on success.
fn verify_agent_ownership(ctx: &ReducerContext, agent_user_id: &str) -> Option<String> {
    let caller_user_id = crate::utils::auth::get_caller_user_id(ctx)?;

    let bot = ctx.db.chat_users().user_id().find(&agent_user_id.to_string())?;
    if bot.is_bot != Some(true) {
        log::warn!("[bot_placement] {} is not a bot", &agent_user_id[..8.min(agent_user_id.len())]);
        return None;
    }

    // Verify ownership: bot_owner_user_id must match caller
    match bot.bot_owner_user_id {
        Some(ref owner) if owner == &caller_user_id => Some(caller_user_id),
        _ => {
            log::warn!(
                "[bot_placement] Caller {} does not own agent {}",
                &caller_user_id[..8.min(caller_user_id.len())],
                &agent_user_id[..8.min(agent_user_id.len())]
            );
            None
        }
    }
}

/// Add the caller's own agent to a server.
///
/// Cascade: auto-joins the agent to all public (non-private) rooms in the server
/// where the owner is also a member.
#[spacetimedb::reducer]
pub fn add_own_agent_to_server(ctx: &ReducerContext, agent_user_id: String, server_id: String) {
    let caller_user_id = match verify_agent_ownership(ctx, &agent_user_id) {
        Some(uid) => uid,
        None => return,
    };

    // Verify server exists
    if ctx.db.chat_servers().id().find(&server_id).is_none() {
        log::warn!("[add_own_agent_to_server] Server {} not found", &server_id[..8.min(server_id.len())]);
        return;
    }

    // Verify caller is a member of this server
    if find_server_membership(ctx, &server_id, &caller_user_id).is_none() {
        log::warn!(
            "[add_own_agent_to_server] Owner {} is not a member of server {}",
            &caller_user_id[..8.min(caller_user_id.len())],
            &server_id[..8.min(server_id.len())]
        );
        return;
    }

    // Check if agent is already a server member
    if let Some(existing) = find_server_membership(ctx, &server_id, &agent_user_id) {
        if existing.role == "banned" {
            ctx.db.server_members().id().delete(&existing.id);
        } else {
            log::info!("[add_own_agent_to_server] Agent already in server");
            return;
        }
    }

    let now = crate::utils::time::timestamp_ms(ctx);

    // Add agent to server
    let member_id = format!("{}-{}", server_id, agent_user_id);
    ctx.db.server_members().insert(ServerMember {
        id: member_id,
        server_id: server_id.clone(),
        user_id: agent_user_id.clone(),
        role: "member".to_string(),
        joined_at: now,
        nickname: None,
        timeout_until: None,
        deaf: false,
        mute: false,
    });

    // Cascade: auto-join public rooms where owner is a member
    let mut rooms_joined = 0u32;
    let server_rooms: Vec<_> = ctx.db.rooms().iter()
        .filter(|r| r.server_id.as_deref() == Some(&server_id) && !r.is_private)
        .map(|r| r.id.clone())
        .collect();

    for room_id in &server_rooms {
        // Only join rooms where the owner is also a member
        if find_membership(ctx, room_id, &caller_user_id).is_some() {
            // Skip if agent already in room
            if find_membership(ctx, room_id, &agent_user_id).is_some() {
                continue;
            }
            let rm_id = format!("{}:{}", room_id, agent_user_id);
            ctx.db.room_members().insert(RoomMember {
                id: rm_id,
                room_id: room_id.clone(),
                user_id: agent_user_id.clone(),
                role: "member".to_string(),
                joined_at: now,
            });
            rooms_joined += 1;
        }
    }

    log::info!(
        "[add_own_agent_to_server] Agent {} added to server {} ({} rooms auto-joined)",
        &agent_user_id[..8.min(agent_user_id.len())],
        &server_id[..8.min(server_id.len())],
        rooms_joined
    );
}

/// Add the caller's own agent to a specific room.
///
/// The agent must already be a member of the room's server (if server-scoped).
#[spacetimedb::reducer]
pub fn add_own_agent_to_room(ctx: &ReducerContext, agent_user_id: String, room_id: String) {
    let caller_user_id = match verify_agent_ownership(ctx, &agent_user_id) {
        Some(uid) => uid,
        None => return,
    };

    // Verify room exists
    let room = match ctx.db.rooms().id().find(&room_id) {
        Some(r) => r,
        None => {
            log::warn!("[add_own_agent_to_room] Room {} not found", &room_id[..8.min(room_id.len())]);
            return;
        }
    };

    // If room belongs to a server, verify agent is a server member
    if let Some(ref sid) = room.server_id {
        if find_server_membership(ctx, sid, &agent_user_id).is_none() {
            log::warn!("[add_own_agent_to_room] Agent not in server — add to server first");
            return;
        }
    }

    // Verify caller is a member of the room
    if find_membership(ctx, &room_id, &caller_user_id).is_none() {
        log::warn!("[add_own_agent_to_room] Owner not a member of room");
        return;
    }

    // Check if agent already in room
    if find_membership(ctx, &room_id, &agent_user_id).is_some() {
        return;
    }

    let now = crate::utils::time::timestamp_ms(ctx);
    let rm_id = format!("{}:{}", room_id, agent_user_id);
    ctx.db.room_members().insert(RoomMember {
        id: rm_id,
        room_id,
        user_id: agent_user_id,
        role: "member".to_string(),
        joined_at: now,
    });
}

/// Remove the caller's own agent from a server.
///
/// Cascade: also removes the agent from ALL rooms in that server.
#[spacetimedb::reducer]
pub fn remove_own_agent_from_server(ctx: &ReducerContext, agent_user_id: String, server_id: String) {
    if verify_agent_ownership(ctx, &agent_user_id).is_none() {
        return;
    }

    // Remove server membership
    let member_id = format!("{}-{}", server_id, agent_user_id);
    if ctx.db.server_members().id().find(&member_id).is_some() {
        ctx.db.server_members().id().delete(&member_id);
    }

    // Cascade: remove from all rooms in this server
    let server_room_ids: Vec<String> = ctx.db.rooms().iter()
        .filter(|r| r.server_id.as_deref() == Some(&server_id))
        .map(|r| r.id.clone())
        .collect();

    let mut removed = 0u32;
    for room_id in &server_room_ids {
        let rm: Vec<String> = ctx.db.room_members().iter()
            .filter(|m| m.room_id == *room_id && m.user_id == agent_user_id)
            .map(|m| m.id.clone())
            .collect();
        for id in rm {
            ctx.db.room_members().id().delete(&id);
            removed += 1;
        }
    }

    log::info!(
        "[remove_own_agent_from_server] Agent {} removed from server {} ({} room memberships removed)",
        &agent_user_id[..8.min(agent_user_id.len())],
        &server_id[..8.min(server_id.len())],
        removed
    );
}

/// Remove the caller's own agent from a single room.
#[spacetimedb::reducer]
pub fn remove_own_agent_from_room(ctx: &ReducerContext, agent_user_id: String, room_id: String) {
    if verify_agent_ownership(ctx, &agent_user_id).is_none() {
        return;
    }

    let to_remove: Vec<String> = ctx.db.room_members().iter()
        .filter(|m| m.room_id == room_id && m.user_id == agent_user_id)
        .map(|m| m.id.clone())
        .collect();

    for id in to_remove {
        ctx.db.room_members().id().delete(&id);
    }
}
