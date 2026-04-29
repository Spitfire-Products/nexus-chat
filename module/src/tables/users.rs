//! Chat user profiles.
//!
//! Maps platform user_id to chat-specific display data, presence, and rate-limit timestamps.
//! The user_id is the primary key — one row per platform user, shared across devices.

/// Chat user profile and presence.
#[derive(Clone)]
#[spacetimedb::table(accessor = chat_users, public)]
pub struct ChatUser {
    /// Platform user ID (from user_identity_links)
    #[primary_key]
    pub user_id: String,

    /// SpacetimeDB identity hex of last-connected device (for lifecycle hooks)
    #[index(btree)]
    pub stdb_identity: String,

    /// Display name (1-32 chars)
    pub display_name: String,

    /// Presence status: "online", "away", "dnd", "invisible", "offline"
    pub status: String,

    /// Whether the user is currently connected
    pub online: bool,

    /// Rate limit: last message sent (ms since epoch)
    pub last_message_at: u64,

    /// Rate limit: last typing indicator sent (ms since epoch)
    pub last_typing_at: u64,

    /// Last activity timestamp (ms since epoch) — for "last seen X ago"
    pub last_seen_at: u64,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,

    // === Tier field (synced from platform) ===

    /// Platform subscription tier: "free", "pro", "creator", "team" — None = "free"
    pub tier: Option<String>,

    // === New fields for Discord parity ===

    /// Base64-encoded avatar image
    pub avatar_data: Option<String>,

    /// Custom status text (e.g. "Playing Nexus Terminal")
    pub custom_status: Option<String>,

    /// Emoji for custom status display
    pub custom_status_emoji: Option<String>,

    /// Platform role synced via AuthBridge: "admin", "developer", "moderator", "user"
    #[default(None::<String>)]
    pub platform_role: Option<String>,

    /// Whether this user is a bot/agent (provisioned via provision_agent)
    #[default(None::<bool>)]
    pub is_bot: Option<bool>,

    /// Whether this bot is a platform-level agent (admin-created, server-invitable).
    /// None/false = personal user agent. Only platform agents appear in Admin Bots invite list.
    #[default(None::<bool>)]
    pub is_platform_agent: Option<bool>,

    /// Owner user_id for bots — the human who created/controls this agent.
    /// None for human users. Mirrors agent_credentials.owner_user_id (which is private).
    #[default(None::<String>)]
    pub bot_owner_user_id: Option<String>,

    /// Marks bots provisioned through the NPC swarm system. The ONLY
    /// differentiator between swarm-origin and personal bots once the
    /// unified identity pipeline is in place.
    ///   true  = provisioned by swarm / observer flow (hidden from My Agents)
    ///   false = user-created personal bot (shown in My Agents)
    ///   None  = legacy row predating this field; treated as false at read time
    /// A "promote" action flips true → false to make a swarm bot permanent.
    #[default(None::<bool>)]
    pub is_swarm_member: Option<bool>,

    /// Marks a row as a minimal steward projection — a CHAT-addressable
    /// handle for a browser-local window steward
    /// (murmuring-braided-mesh plan). Steward projections:
    ///   - Live only in `#bus-*` channels (never added to general channels)
    ///   - Have NO persona, no generated SOUL, no memories
    ///   - Exist purely so NPCs and personal bots can @mention them to
    ///     trigger workspace actions via the browser-side bridge
    ///   - Are per-swarm or per-user-per-server scoped via user_id naming
    ///   - Do NOT count toward swarm spawn limits
    /// None at read-time = not a steward projection. Default for legacy rows.
    #[default(None::<bool>)]
    pub is_steward_projection: Option<bool>,
}
