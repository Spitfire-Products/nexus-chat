//! Channel permission overrides — per-channel allow/deny for roles or members.

/// A permission override for a specific channel (room).
#[spacetimedb::table(accessor = channel_overrides, public)]
pub struct ChannelOverride {
    #[primary_key]
    pub id: String,

    /// FK to rooms.id
    #[index(btree)]
    pub room_id: String,

    /// "role" or "member"
    pub target_type: String,

    /// role_id or user_id depending on target_type
    pub target_id: String,

    /// Permission bits to explicitly allow
    pub allow: u64,

    /// Permission bits to explicitly deny
    pub deny: u64,
}
