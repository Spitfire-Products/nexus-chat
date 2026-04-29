//! Per-user notification preferences for servers and channels.

/// A notification setting for a server or channel.
#[spacetimedb::table(accessor = notification_settings, public)]
pub struct NotificationSetting {
    /// Composite key: "{user_id}-{target_type}-{target_id}"
    #[primary_key]
    pub id: String,

    /// Platform user_id
    #[index(btree)]
    pub user_id: String,

    /// "server" or "room"
    pub target_type: String,

    /// server_id or room_id
    pub target_id: String,

    /// Notification level: "all", "mentions", "none"
    pub level: String,

    /// Suppress @everyone/@here mentions
    pub suppress_everyone: bool,

    /// Suppress @role mentions
    pub suppress_roles: bool,

    /// Temporary mute expiry (ms since epoch) — None = not muted
    pub muted_until: Option<u64>,

    /// Last updated timestamp (ms since epoch)
    pub updated_at: u64,
}
