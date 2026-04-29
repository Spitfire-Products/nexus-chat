//! Scheduled messages — messages composed for future delivery.
//!
//! The author can view pending scheduled messages and cancel them.
//! A scheduled delivery job fires the reducer at the specified time.

/// A message scheduled for future delivery.
#[spacetimedb::table(accessor = scheduled_messages, public)]
pub struct ScheduledMessage {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// Target room
    #[index(btree)]
    pub room_id: String,

    /// Author's platform user_id
    #[index(btree)]
    pub author_id: String,

    /// Message content to send
    pub content: String,

    /// When the message should be delivered (ms since epoch)
    pub send_at: u64,

    /// The scheduled job ID (for cancellation)
    pub job_id: u64,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
