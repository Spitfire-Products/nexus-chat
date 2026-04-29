//! Stickers — larger emoji-like images for server expression.

/// A sticker (server-specific or global).
#[spacetimedb::table(accessor = stickers, public)]
pub struct Sticker {
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id — empty = global/built-in
    #[index(btree)]
    pub server_id: String,

    /// Sticker display name
    pub name: String,

    /// Description text
    pub description: String,

    /// Base64-encoded image data (PNG, max ~512KB)
    pub image_data: String,

    /// Comma-separated search tags
    pub tags: String,

    /// user_id of the uploader
    pub uploaded_by: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
