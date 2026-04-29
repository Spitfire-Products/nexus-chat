//! Audit log — tracks administrative actions in servers.

/// An audit log entry for a server action.
#[spacetimedb::table(accessor = audit_log, public)]
pub struct AuditLogEntry {
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// Action type: "MEMBER_KICK", "MEMBER_BAN", "ROLE_CREATE",
    /// "CHANNEL_CREATE", "EMOJI_CREATE", "INVITE_CREATE", etc.
    pub action: String,

    /// user_id who performed the action
    pub actor_id: String,

    /// Type of affected entity: "user", "role", "channel", "server", "emoji", "invite"
    pub target_type: String,

    /// ID of the affected entity
    pub target_id: String,

    /// Optional JSON with extra details (old name, new name, reason, etc.)
    pub details: Option<String>,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
