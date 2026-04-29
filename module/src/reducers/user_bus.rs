//! User bus channel reducers — part of the murmuring-braided-mesh plan.
//!
//! A "user bus channel" is a per-user bus lane on a shared CHAT server. When
//! user A opts in on server S, a channel named `bus-user-${user_short_id}`
//! is created on S. A's personas and A's trusted users can write; server
//! members can read (unless A flips it to invite-only).
//!
//! This lets multi-user servers coordinate per-user workflows without
//! collisions: user B requesting work from A's persona FA posts in A's bus,
//! and FA's response lands there (NOT in a shared #analysis channel).
//!
//! Kept as a thin wrapper around existing room machinery: creates a deterministic
//! room name + id, joins the caller as admin, sets `is_private=false` by default
//! (members can see it but only the owner + personas + trusted can write).
//! Further permission tuning happens via existing `channel_overrides` reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::rooms::{Room, rooms};
use crate::tables::room_members::{RoomMember, room_members};
use crate::tables::server_members::server_members;
use crate::utils::validation::ensure_chat_user;

/// Create a user-specific bus channel on a shared server.
///
/// Arguments:
///   - `server_id`: Server to create the bus channel on. Caller must be a
///     member (verified via server_members table).
///   - `invite_only`: If true, channel is created with `is_private=true`
///     (only the caller sees it by default; explicit joins required).
///     If false, server members can read; writes still gated by channel
///     overrides maintained separately.
///
/// Channel naming / id:
///   - name = `bus-user-${short}` where short = first 12 chars of user_id
///   - id = `bus-user-${user_id}-${server_id}` (deterministic, collision-free)
///
/// Idempotency: If the channel already exists, re-join caller as admin and
/// return without error. Useful for re-opt-in.
#[spacetimedb::reducer]
pub fn create_user_bus_channel(
    ctx: &ReducerContext,
    server_id: String,
    invite_only: bool,
) {
    if server_id.is_empty() {
        log::warn!("[create_user_bus_channel] Rejected: empty server_id");
        return;
    }

    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_user_bus_channel] Unauthorized: no identity link");
        return;
    };

    // Caller must be a member of the target server.
    let member_id = format!("{}-{}", server_id, user_id);
    if ctx.db.server_members().id().find(&member_id).is_none() {
        log::warn!(
            "[create_user_bus_channel] Rejected: caller {} is not a member of server {}",
            &user_id[..8.min(user_id.len())], server_id
        );
        return;
    }

    let sender_hex = crate::utils::auth::sender_hex(ctx);
    ensure_chat_user(ctx, &user_id, &sender_hex);

    let short = &user_id[..12.min(user_id.len())];
    let channel_name = format!("bus-user-{}", short);
    let channel_id = format!("bus-user-{}-{}", user_id, server_id);
    let now = crate::timestamp_ms(ctx);

    // Idempotency: channel exists?
    if let Some(_existing) = ctx.db.rooms().id().find(&channel_id) {
        // Rejoin caller as admin if not already a member.
        let room_member_id = format!("{}-{}", channel_id, user_id);
        if ctx.db.room_members().id().find(&room_member_id).is_none() {
            ctx.db.room_members().insert(RoomMember {
                id: room_member_id,
                room_id: channel_id.clone(),
                user_id: user_id.clone(),
                role: "admin".to_string(),
                joined_at: now,
            });
            log::info!("[create_user_bus_channel] Re-joined caller to existing bus channel {}", channel_id);
        } else {
            log::info!("[create_user_bus_channel] No-op: bus channel {} already exists", channel_id);
        }
        return;
    }

    // Create the room
    ctx.db.rooms().insert(Room {
        id: channel_id.clone(),
        name: channel_name,
        created_by: user_id.clone(),
        is_private: invite_only,
        is_dm: false,
        created_at: now,
        server_id: Some(server_id.clone()),
        required_tier: None,
        description: Some(format!(
            "Agent bus lane for user {} — personas coordinate workflows here",
            short
        )),
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

    // Owner becomes admin of the room
    let room_member_id = format!("{}-{}", channel_id, user_id);
    ctx.db.room_members().insert(RoomMember {
        id: room_member_id,
        room_id: channel_id.clone(),
        user_id: user_id.clone(),
        role: "admin".to_string(),
        joined_at: now,
    });

    log::info!(
        "[create_user_bus_channel] Created {} on server {} (invite_only={})",
        channel_id, server_id, invite_only
    );
}

/// Delete the caller's user-bus channel on a given server (e.g. opt-out).
/// Only deletes if the caller is the room's creator OR a server admin.
#[spacetimedb::reducer]
pub fn delete_user_bus_channel(
    ctx: &ReducerContext,
    server_id: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_user_bus_channel] Unauthorized: no identity link");
        return;
    };

    let channel_id = format!("bus-user-{}-{}", user_id, server_id);
    let Some(existing) = ctx.db.rooms().id().find(&channel_id) else {
        log::warn!("[delete_user_bus_channel] No bus channel to delete for {}", channel_id);
        return;
    };

    // Only the creator may delete their bus channel (platform admin bypass is
    // handled elsewhere by existing admin tooling).
    if existing.created_by != user_id {
        log::warn!(
            "[delete_user_bus_channel] Rejected: caller {} is not creator of {}",
            user_id, channel_id
        );
        return;
    }

    // Wipe room members
    let member_ids: Vec<String> = ctx.db.room_members().iter()
        .filter(|m| m.room_id == channel_id)
        .map(|m| m.id.clone())
        .collect();
    for id in member_ids {
        ctx.db.room_members().id().delete(&id);
    }

    ctx.db.rooms().id().delete(&channel_id);

    log::info!("[delete_user_bus_channel] Deleted bus channel {}", channel_id);
}
