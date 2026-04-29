//! Per-user per-room read position tracking.
//!
//! Tracks the last message each user has read in each room,
//! enabling unread count badges and "seen by" indicators.

/// Read position for a user in a room.
#[spacetimedb::table(accessor = read_positions, public)]
pub struct ReadPosition {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// Room ID
    #[index(btree)]
    pub room_id: String,

    /// User ID (platform)
    #[index(btree)]
    pub user_id: String,

    /// Last read message ID in this room
    pub last_read_message_id: String,

    /// When the position was last updated (ms since epoch)
    pub updated_at: u64,
}
