//! Validation and rate limiting helpers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::{ChatUser, RoomMember};
use crate::tables::users::chat_users;
use crate::tables::room_members::room_members;

/// Minimum gap between messages (400ms)
pub const MESSAGE_RATE_LIMIT_MS: u64 = 400;

/// Minimum gap between typing indicators (1 second in microseconds)
pub const TYPING_RATE_LIMIT_MS: u64 = 1_000_000;

/// Typing indicator TTL (4 seconds in microseconds)
pub const TYPING_TTL_MS: u64 = 4_000_000;

/// Maximum display name length
pub const MAX_DISPLAY_NAME_LEN: usize = 32;

/// Maximum message content length
pub const MAX_MESSAGE_LEN: usize = 4000;

/// Maximum room name length
pub const MAX_ROOM_NAME_LEN: usize = 64;

/// Validate that a string is a valid emoji reaction.
/// Accepts:
/// - Unicode emoji characters (e.g. "👍", "❤️", "😂") — any non-empty string ≤ 64 bytes
/// - Pepe/meme emojis in colon format (e.g. ":pepe-happy:", ":pepe-sad:")
/// - Custom server emojis in "custom:" format (validated separately in reducer)
pub fn is_valid_emoji(emoji: &str) -> bool {
    if emoji.is_empty() || emoji.len() > 64 {
        return false;
    }
    // Pepe/meme emojis: :name:
    if emoji.starts_with(':') && emoji.ends_with(':') && emoji.len() > 2 {
        let inner = &emoji[1..emoji.len() - 1];
        return inner.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    }
    // Custom server emojis
    if emoji.starts_with("custom:") {
        return emoji.len() > 7;
    }
    // Standard Unicode emoji — accept any non-empty string within length limit.
    // We trust the frontend to send valid emoji. The length limit prevents abuse.
    true
}

/// Allowed presence statuses
pub const ALLOWED_STATUSES: &[&str] = &["online", "away", "dnd", "invisible", "offline"];

/// Find a user's membership in a room, or None.
pub fn find_membership(ctx: &ReducerContext, room_id: &str, user_id: &str) -> Option<RoomMember> {
    ctx.db.room_members().iter()
        .find(|m| m.room_id == room_id && m.user_id == user_id)
}

/// Require that the caller is a member of the room (not banned).
/// Returns the membership record or logs a warning and returns None.
pub fn require_membership(ctx: &ReducerContext, room_id: &str, user_id: &str) -> Option<RoomMember> {
    match find_membership(ctx, room_id, user_id) {
        Some(m) if m.role != "banned" => Some(m),
        Some(_) => {
            log::warn!("[require_membership] User {} is banned from room {}", user_id, room_id);
            None
        }
        None => {
            log::warn!("[require_membership] User {} is not a member of room {}", user_id, room_id);
            None
        }
    }
}

/// Require that the caller is an admin of the room.
pub fn require_admin(ctx: &ReducerContext, room_id: &str, user_id: &str) -> Option<RoomMember> {
    match find_membership(ctx, room_id, user_id) {
        Some(m) if m.role == "admin" => Some(m),
        _ => {
            log::warn!("[require_admin] User {} is not an admin of room {}", user_id, room_id);
            None
        }
    }
}

/// Ensure a chat_users row exists for the given user_id.
/// Creates a default entry if missing. Returns the user row.
pub fn ensure_chat_user(ctx: &ReducerContext, user_id: &str, stdb_identity: &str) -> ChatUser {
    let now = crate::timestamp_ms(ctx);
    if let Some(existing) = ctx.db.chat_users().user_id().find(&user_id.to_string()) {
        existing
    } else {
        let user = ChatUser {
            user_id: user_id.to_string(),
            stdb_identity: stdb_identity.to_string(),
            display_name: format!("User-{}", &user_id[..8.min(user_id.len())]),
            status: "online".to_string(),
            online: true,
            last_message_at: 0,
            last_typing_at: 0,
            last_seen_at: now,
            created_at: now,
            tier: None,
            avatar_data: None,
            custom_status: None,
            custom_status_emoji: None,
            platform_role: None,
            is_bot: None,
            is_platform_agent: None,
            bot_owner_user_id: None,
            is_swarm_member: None,
            is_steward_projection: None,
        };
        ctx.db.chat_users().insert(user.clone());
        user
    }
}

/// Check message rate limit. Returns true if allowed, false if too fast.
pub fn check_message_rate(user: &ChatUser, now: u64) -> bool {
    now.saturating_sub(user.last_message_at) >= MESSAGE_RATE_LIMIT_MS
}

/// Check typing rate limit. Returns true if allowed, false if too fast.
pub fn check_typing_rate(user: &ChatUser, now: u64) -> bool {
    now.saturating_sub(user.last_typing_at) >= TYPING_RATE_LIMIT_MS
}

// ============================================================================
// Tier helpers
// ============================================================================

/// Convert a tier string to a numeric level for comparison.
pub fn tier_level(tier: &str) -> u32 {
    match tier {
        "team" => 3,
        "creator" => 2,
        "pro" => 1,
        _ => 0, // "free" or unknown
    }
}

/// Check if a user's tier meets a required tier level.
/// Empty required tier = no restriction (always passes).
pub fn meets_tier(user_tier: Option<&str>, required: &str) -> bool {
    if required.is_empty() {
        return true;
    }
    tier_level(user_tier.unwrap_or("free")) >= tier_level(required)
}

/// Compute the effective tier requirement for a room, considering both
/// the room's own `required_tier` and the server's `default_tier`.
/// Returns the higher of the two (empty string = no restriction).
pub fn effective_room_tier(
    room_tier: Option<&str>,
    server_tier: Option<&str>,
) -> String {
    let rt = room_tier.unwrap_or("");
    let st = server_tier.unwrap_or("");
    if tier_level(rt) >= tier_level(st) {
        rt.to_string()
    } else {
        st.to_string()
    }
}

// ============================================================================
// Server membership helpers
// ============================================================================

use crate::tables::ServerMember;
use crate::tables::server_members::server_members;
use crate::tables::servers::chat_servers;

/// Find a user's membership in a server, or None.
pub fn find_server_membership(ctx: &ReducerContext, server_id: &str, user_id: &str) -> Option<ServerMember> {
    let member_id = format!("{}-{}", server_id, user_id);
    ctx.db.server_members().id().find(&member_id)
}

/// Require server admin or owner role. Returns membership or None.
pub fn require_server_admin(ctx: &ReducerContext, server_id: &str, user_id: &str) -> Option<ServerMember> {
    match find_server_membership(ctx, server_id, user_id) {
        Some(m) if m.role == "owner" || m.role == "admin" => Some(m),
        _ => {
            log::warn!("[require_server_admin] User {} is not admin/owner of server {}", user_id, server_id);
            None
        }
    }
}

/// Require server owner role specifically. Returns membership or None.
pub fn require_server_owner(ctx: &ReducerContext, server_id: &str, user_id: &str) -> Option<ServerMember> {
    match find_server_membership(ctx, server_id, user_id) {
        Some(m) if m.role == "owner" => Some(m),
        _ => {
            log::warn!("[require_server_owner] User {} is not owner of server {}", user_id, server_id);
            None
        }
    }
}

/// Get the effective tier requirement for a room, looking up its server if needed.
pub fn get_effective_room_tier(ctx: &ReducerContext, room: &crate::tables::Room) -> String {
    let room_tier = room.required_tier.as_deref();
    let server_tier = room.server_id.as_ref()
        .and_then(|sid| ctx.db.chat_servers().id().find(sid))
        .map(|s| s.default_tier)
        .unwrap_or_default();
    let st_ref = if server_tier.is_empty() { None } else { Some(server_tier.as_str()) };
    effective_room_tier(room_tier, st_ref)
}
