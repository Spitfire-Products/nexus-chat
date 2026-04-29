//! Message edit history.
//!
//! Each edit creates a history entry so users can view previous versions.

/// A single edit in a message's history.
#[spacetimedb::table(accessor = message_edits, public)]
pub struct MessageEdit {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// The message that was edited
    #[index(btree)]
    pub message_id: String,

    /// Who edited it (platform user_id)
    pub editor_id: String,

    /// Content before the edit
    pub old_content: String,

    /// Content after the edit
    pub new_content: String,

    /// When the edit was made (ms since epoch)
    pub edited_at: u64,

    /// Denormalized room_id for subscription scoping (copied from message's room_id)
    #[index(btree)]
    pub room_id: Option<String>,
}
