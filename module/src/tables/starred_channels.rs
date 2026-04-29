//! Starred channels — per-user watchlist of favourite channels.

/// A user's starred/watchlisted channel.
#[spacetimedb::table(accessor = starred_channels, public)]
pub struct StarredChannel {
    /// Composite key: "{user_id}:{room_id}"
    #[primary_key]
    pub id: String,

    /// The user who starred this channel
    #[index(btree)]
    pub user_id: String,

    /// The room/channel being starred
    #[index(btree)]
    pub room_id: String,

    /// When the channel was starred (ms since epoch)
    pub starred_at: u64,
}
