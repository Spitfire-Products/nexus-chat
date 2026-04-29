//! Member timeouts — temporary communication bans in servers.

/// A timeout restricting a member from sending messages.
#[spacetimedb::table(accessor = member_timeouts, public)]
pub struct MemberTimeout {
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// Platform user_id of the timed-out member
    #[index(btree)]
    pub user_id: String,

    /// Optional reason for the timeout
    pub reason: Option<String>,

    /// When the timeout expires (ms since epoch)
    pub expires_at: u64,

    /// user_id of the moderator who issued the timeout
    pub issued_by: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
