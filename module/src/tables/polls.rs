//! Polls and poll votes.

/// A poll attached to a message.
#[spacetimedb::table(accessor = polls, public)]
pub struct Poll {
    #[primary_key]
    pub id: String,

    /// FK to messages.id
    #[index(btree)]
    pub message_id: String,

    /// Denormalized for subscription scoping
    #[index(btree)]
    pub room_id: String,

    /// Poll question text
    pub question: String,

    /// JSON array of options: ["Option A", "Option B", "Option C"]
    pub options: String,

    /// Whether users can select multiple options
    pub allow_multiple: bool,

    /// Whether votes are anonymous (hide who voted)
    pub anonymous: bool,

    /// When the poll expires (ms since epoch), None = no expiry
    pub expires_at: Option<u64>,

    /// user_id of the poll creator
    pub created_by: String,

    /// Whether the poll has been manually closed
    pub closed: bool,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}

/// A single vote on a poll option.
#[spacetimedb::table(accessor = poll_votes, public)]
pub struct PollVote {
    /// Composite key: "{poll_id}-{user_id}-{option_index}"
    #[primary_key]
    pub id: String,

    /// FK to polls.id
    #[index(btree)]
    pub poll_id: String,

    /// Denormalized for subscription scoping
    #[index(btree)]
    pub room_id: String,

    /// user_id of the voter
    pub user_id: String,

    /// Zero-based index into the poll's options array
    pub option_index: u32,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
