//! Chat servers — top-level grouping for rooms.
//!
//! Servers are Discord-style containers that hold multiple rooms/channels.
//! They support public viewing (guests), tier gating, and audience binding.

/// A chat server (Discord-style guild).
#[spacetimedb::table(accessor = chat_servers, public)]
pub struct ChatServer {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// Server display name (e.g. "Blaster Lab", "General")
    pub name: String,

    /// Server description (empty string if none)
    pub description: String,

    /// Links to Audience system — empty string = no binding
    #[index(btree)]
    pub audience_id: String,

    /// Creator/owner user_id
    pub owner_user_id: String,

    /// Can unauthenticated visitors view this server's public channels?
    pub is_public: bool,

    /// Minimum tier to participate — "" = no restriction, else "free"/"pro"/"creator"/"team"
    pub default_tier: String,

    /// Server icon URL — empty string if none
    pub icon_url: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,

    /// Last updated timestamp (ms since epoch)
    pub updated_at: u64,

    /// Server template type (e.g. "swarm", "team", "community").
    /// None = manually created (no template). Used by swarm auto-provisioning.
    #[default(None::<String>)]
    pub template: Option<String>,
}
