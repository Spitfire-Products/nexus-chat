//! Identity link table for security infrastructure.
//! Maps SpacetimeDB connection identities to application user IDs.

/// Identity link: maps SpacetimeDB hex identity to application user_id.
/// Each module instance maintains its own identity links.
#[spacetimedb::table(accessor = user_identity_links, public)]
pub struct UserIdentityLink {
    #[primary_key]
    #[index(btree)]
    pub stdb_identity: String,
    #[index(btree)]
    pub user_id: String,
    pub created_at: u64,
    pub last_seen_at: u64,
}
