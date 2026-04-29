//! Emoji reactions on messages.
//!
//! Users can toggle reactions on/off. One reaction per user per emoji per message.

/// An emoji reaction on a message.
#[spacetimedb::table(accessor = reactions, public)]
pub struct Reaction {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// The message being reacted to
    #[index(btree)]
    pub message_id: String,

    /// Who reacted (platform user_id)
    #[index(btree)]
    pub user_id: String,

    /// Emoji identifier: "thumbsup", "heart", "laughing", "surprised", "crying"
    pub emoji: String,

    /// When the reaction was added (ms since epoch)
    pub created_at: u64,

    /// Denormalized room_id for subscription scoping (copied from message's room_id)
    #[index(btree)]
    pub room_id: Option<String>,
}
