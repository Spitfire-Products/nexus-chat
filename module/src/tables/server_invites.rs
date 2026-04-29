//! Server invites — shareable invite codes for joining servers.

/// A server invite code.
#[spacetimedb::table(accessor = server_invites, public)]
pub struct ServerInvite {
    /// Short alphanumeric code (e.g. "AbC12x")
    #[primary_key]
    pub code: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// user_id who created the invite
    pub created_by: String,

    /// Maximum uses allowed — None = unlimited
    pub max_uses: Option<u32>,

    /// Current use count
    pub uses: u32,

    /// When the invite expires (ms since epoch) — None = never
    pub expires_at: Option<u64>,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
