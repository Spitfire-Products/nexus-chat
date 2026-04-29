//! Typing indicators.
//!
//! Short-lived rows that indicate a user is typing in a room.
//! Automatically cleaned up by scheduled expiry jobs after 4 seconds.

/// Active typing indicator.
#[spacetimedb::table(accessor = typing_indicators, public)]
pub struct TypingIndicator {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// Room where user is typing
    #[index(btree)]
    pub room_id: String,

    /// User who is typing (platform user_id)
    #[index(btree)]
    pub user_id: String,

    /// When this indicator expires (ms since epoch)
    pub expires_at: u64,
}
