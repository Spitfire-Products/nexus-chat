//! Chat rooms / channels.
//!
//! Rooms are the top-level containers for messages.
//! Types: "text", "forum", "announcement", "rules".
//! Can be public, private, DMs, or threads (parent_room_id set).

/// A chat room (channel, thread, forum, or DM).
#[spacetimedb::table(accessor = rooms, public)]
pub struct Room {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// Room display name (e.g. "general", "random")
    pub name: String,

    /// Creator's platform user_id
    #[index(btree)]
    pub created_by: String,

    /// Whether this room is private (invite-only)
    pub is_private: bool,

    /// Whether this is a direct message room (exactly 2 members)
    pub is_dm: bool,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,

    // === Server & Tier fields ===

    /// FK to chat_servers.id — None = standalone room
    pub server_id: Option<String>,

    /// Minimum tier required to access — None = no restriction
    pub required_tier: Option<String>,

    /// Room topic/description
    pub description: Option<String>,

    /// Display order within a server/category
    pub sort_order: Option<u32>,

    // === Channel type & category ===

    /// Room type: "text" (default), "forum", "announcement", "rules"
    pub room_type: String,

    /// FK to channel_categories.id — None = uncategorized
    pub category_id: Option<String>,

    /// Channel topic shown in header
    pub topic: Option<String>,

    /// Slowmode delay in seconds (0 or None = off)
    pub slowmode_seconds: Option<u32>,

    /// Whether the channel is marked NSFW
    pub nsfw: bool,

    // === Thread fields ===

    /// For threads: FK to the parent room
    pub parent_room_id: Option<String>,

    /// Whether this thread is archived
    pub archived: bool,

    /// Whether this thread is locked (no new messages)
    pub locked: bool,

    /// Auto-archive after N minutes: 60, 1440, 4320, 10080
    pub auto_archive_minutes: Option<u32>,

    // === Forum fields ===

    /// Forum default sort: "latest_activity" or "creation_date"
    pub default_sort_order: Option<String>,

    // === Channel content rules ===

    /// Whether attachments (images, files) are allowed in this channel.
    /// None = allowed (default). Some(false) = text-only channel.
    #[default(None::<bool>)]
    pub allow_attachments: Option<bool>,

    /// Whether link embeds/previews are allowed.
    /// None = allowed (default). Some(false) = no embeds.
    #[default(None::<bool>)]
    pub allow_embeds: Option<bool>,

    /// Whether reactions are allowed on messages.
    /// None = allowed (default). Some(false) = no reactions.
    #[default(None::<bool>)]
    pub allow_reactions: Option<bool>,

    /// Channel-specific rules text shown to users (displayed in channel header/settings).
    /// E.g. "Text only. No GIFs or memes. Keep discussion on-topic."
    #[default(None::<String>)]
    pub rules_text: Option<String>,
}
