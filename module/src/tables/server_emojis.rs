//! Custom server emojis — uploaded by server members, usable in messages and reactions.

/// A custom emoji belonging to a server.
#[spacetimedb::table(accessor = server_emojis, public)]
pub struct ServerEmoji {
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// Shortcode name (e.g. "pepe", "kekw")
    pub name: String,

    /// Base64-encoded image data (PNG or GIF, max ~256KB)
    pub image_data: String,

    /// Whether this is an animated emoji (GIF)
    pub animated: bool,

    /// user_id of the uploader
    pub uploaded_by: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
