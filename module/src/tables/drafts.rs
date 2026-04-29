//! Cross-device message drafts.
//!
//! Persists in-progress message text per user per room.
//! Syncs across devices in real-time via SpacetimeDB subscriptions.

/// A message draft for a user in a room.
#[spacetimedb::table(accessor = drafts, public)]
pub struct Draft {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// Room the draft is for
    #[index(btree)]
    pub room_id: String,

    /// Author's platform user_id
    #[index(btree)]
    pub user_id: String,

    /// Draft message content
    pub content: String,

    /// Last updated timestamp (ms since epoch)
    pub updated_at: u64,
}
