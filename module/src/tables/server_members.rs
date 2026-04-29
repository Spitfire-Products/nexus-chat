//! Server memberships — tracks which users belong to which servers.
//!
//! Roles: "owner", "admin", "moderator", "member", "banned".

/// A server membership record.
#[spacetimedb::table(accessor = server_members, public)]
pub struct ServerMember {
    /// Composite key: "{server_id}-{user_id}"
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// Platform user_id
    #[index(btree)]
    pub user_id: String,

    /// Role: "owner", "admin", "moderator", "member", "banned"
    pub role: String,

    /// When the user joined this server (ms since epoch)
    pub joined_at: u64,

    // === New fields for Discord parity ===

    /// Server-specific display name override
    pub nickname: Option<String>,

    /// Communication timeout expiry (ms since epoch) — cannot send messages until this time
    pub timeout_until: Option<u64>,

    /// Server-deafened (future voice support)
    pub deaf: bool,

    /// Server-muted (future voice support)
    pub mute: bool,
}
