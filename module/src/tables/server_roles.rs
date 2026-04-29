//! Server roles with permission bitfields.

/// A role within a server (e.g. "Moderator", "VIP", "@everyone").
#[spacetimedb::table(accessor = server_roles, public)]
pub struct ServerRole {
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// Role display name
    pub name: String,

    /// Hex color for display (e.g. "#ff5733")
    pub color: String,

    /// Permission bitfield (see utils/permissions.rs for constants)
    pub permissions: u64,

    /// Position in role hierarchy (lower = higher priority)
    pub sort_order: u32,

    /// True for the @everyone role (one per server, auto-created)
    pub is_default: bool,

    /// Whether this role can be @mentioned
    pub mentionable: bool,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
