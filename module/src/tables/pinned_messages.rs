//! Pinned messages — messages pinned to the top of a channel.

/// A pinned message reference.
#[spacetimedb::table(accessor = pinned_messages, public)]
pub struct PinnedMessage {
    #[primary_key]
    pub id: String,

    /// FK to rooms.id
    #[index(btree)]
    pub room_id: String,

    /// FK to messages.id
    pub message_id: String,

    /// user_id who pinned it
    pub pinned_by: String,

    /// When it was pinned (ms since epoch)
    pub pinned_at: u64,
}
