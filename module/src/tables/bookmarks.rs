//! Bookmarks — user-saved messages for quick reference.

/// A bookmarked message.
#[spacetimedb::table(accessor = bookmarks, public)]
pub struct Bookmark {
    #[primary_key]
    pub id: String,

    /// Platform user_id who bookmarked
    #[index(btree)]
    pub user_id: String,

    /// FK to messages.id
    pub message_id: String,

    /// FK to rooms.id (for display context)
    pub room_id: String,

    /// Optional personal note about this bookmark
    pub note: Option<String>,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
