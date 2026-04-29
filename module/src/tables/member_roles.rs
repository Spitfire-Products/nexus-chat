//! Member-role assignments — which users have which roles in a server.

/// A role assignment linking a server member to a role.
#[spacetimedb::table(accessor = member_roles, public)]
pub struct MemberRole {
    /// Composite key: "{server_id}-{user_id}-{role_id}"
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// Platform user_id
    #[index(btree)]
    pub user_id: String,

    /// FK to server_roles.id
    pub role_id: String,

    /// When this role was assigned (ms since epoch)
    pub assigned_at: u64,
}
