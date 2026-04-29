//! Channel categories — group rooms within a server.

/// A channel category (e.g. "Text Channels", "Voice Channels").
#[spacetimedb::table(accessor = channel_categories, public)]
pub struct ChannelCategory {
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// Category display name
    pub name: String,

    /// Display order within server
    pub sort_order: u32,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
