//! User blocks — per-user block list to hide messages and prevent DMs.

/// A user-initiated block relationship.
#[spacetimedb::table(accessor = user_blocks, public)]
pub struct UserBlock {
    /// Composite key: "{blocker_id}:{blocked_id}"
    #[primary_key]
    pub id: String,

    /// The user who initiated the block
    #[index(btree)]
    pub blocker_id: String,

    /// The user who is blocked
    #[index(btree)]
    pub blocked_id: String,

    /// When the block was created (ms since epoch)
    pub created_at: u64,
}
