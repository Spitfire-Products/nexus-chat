//! Auto-moderation rules — configurable content filters per server.

/// An auto-moderation rule for a server.
#[spacetimedb::table(accessor = auto_mod_rules, public)]
pub struct AutoModRule {
    #[primary_key]
    pub id: String,

    /// FK to chat_servers.id
    #[index(btree)]
    pub server_id: String,

    /// Rule type: "blocked_words", "spam_filter", "mention_limit", "link_filter", "caps_filter"
    pub rule_type: String,

    /// JSON config for the rule (varies by type)
    pub config: String,

    /// Whether this rule is currently active
    pub enabled: bool,

    /// Action on trigger: "block", "flag", "timeout_60", "timeout_300", "timeout_3600"
    pub action: String,

    /// JSON array of exempt role_ids
    pub exempt_roles: String,

    /// JSON array of exempt room_ids
    pub exempt_channels: String,

    /// user_id who created this rule
    pub created_by: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
