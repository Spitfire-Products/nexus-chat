//! Message attachments — files, images, GIFs attached to messages.

/// A file attachment on a message.
#[spacetimedb::table(accessor = message_attachments, public)]
pub struct MessageAttachment {
    #[primary_key]
    pub id: String,

    /// FK to messages.id
    #[index(btree)]
    pub message_id: String,

    /// Denormalized for subscription scoping
    #[index(btree)]
    pub room_id: String,

    /// Original file name
    pub file_name: String,

    /// External URL or base64 data URI for small files
    pub file_url: String,

    /// File size in bytes
    pub file_size: u64,

    /// MIME type: "image/png", "image/gif", "application/pdf", etc.
    pub content_type: String,

    /// Image width in pixels (for images/GIFs)
    pub width: Option<u32>,

    /// Image height in pixels (for images/GIFs)
    pub height: Option<u32>,

    /// Whether the attachment is hidden behind a spoiler
    pub is_spoiler: bool,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
