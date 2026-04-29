//! Chat messages.
//!
//! Messages belong to a room and are authored by a user.
//! Supports threading, ephemeral messages, polls, replies, mentions, and attachments.

/// A chat message.
#[spacetimedb::table(accessor = messages, public)]
pub struct Message {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// Room this message belongs to
    #[index(btree)]
    pub room_id: String,

    /// Author's platform user_id
    #[index(btree)]
    pub author_id: String,

    /// Message text content
    pub content: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,

    /// Last edited timestamp (ms since epoch), None if never edited
    pub edited_at: Option<u64>,

    /// Parent message ID for threading, None for top-level messages
    pub parent_message_id: Option<String>,

    /// Whether this is an ephemeral (auto-deleting) message
    pub is_ephemeral: bool,

    /// When the ephemeral message expires (ms since epoch), None for permanent
    pub expires_at: Option<u64>,

    // === New fields for Discord parity ===

    /// Message type: "default", "system", "reply", "thread_starter", "poll"
    pub message_type: String,

    /// Inline reply target (distinct from parent_message_id threading)
    pub reply_to_id: Option<String>,

    /// JSON array of sticker IDs used in this message
    pub sticker_ids: Option<String>,

    /// Whether this message contains @everyone or @here
    pub mention_everyone: bool,

    /// JSON array of mentioned user_ids
    pub mentioned_user_ids: Option<String>,

    /// JSON array of mentioned role_ids
    pub mentioned_role_ids: Option<String>,

    /// Bitfield flags: PINNED=1, SUPPRESS_EMBEDS=2, URGENT=4
    pub flags: u64,

    /// Server-stamped: true if the author was a bot at send time.
    /// Set automatically by send_message — not client-provided.
    /// Receiving bots use this to identify untrusted bot-authored content.
    #[default(None::<bool>)]
    pub is_bot_author: Option<bool>,
}
