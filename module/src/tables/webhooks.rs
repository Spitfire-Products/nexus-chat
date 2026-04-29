//! Webhooks — external integrations that can post messages.
//!
//! The webhook table itself is PRIVATE (no `public` attribute) to protect tokens.
//! Webhook messages are public for display.

/// A webhook definition (PRIVATE — never sent to clients).
#[spacetimedb::table(accessor = webhooks)]
pub struct Webhook {
    #[primary_key]
    pub id: String,

    /// FK to rooms.id
    #[index(btree)]
    pub room_id: String,

    /// Webhook display name
    pub name: String,

    /// Optional avatar URL
    pub avatar_url: Option<String>,

    /// Secret token for webhook authentication
    pub token: String,

    /// user_id who created this webhook
    pub created_by: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}

/// A message sent via webhook (public for display).
#[spacetimedb::table(accessor = webhook_messages, public)]
pub struct WebhookMessage {
    #[primary_key]
    pub id: String,

    /// Denormalized for subscription scoping
    #[index(btree)]
    pub room_id: String,

    /// FK to webhooks.id
    pub webhook_id: String,

    /// Snapshot of webhook name at send time
    pub webhook_name: String,

    /// Snapshot of webhook avatar at send time
    pub webhook_avatar: Option<String>,

    /// Message content
    pub content: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,

    /// Optional per-message sender display name (for bridges)
    #[default(None::<String>)]
    pub sender_name: Option<String>,

    /// Optional per-message sender avatar URL (for bridges)
    #[default(None::<String>)]
    pub sender_avatar: Option<String>,
}
