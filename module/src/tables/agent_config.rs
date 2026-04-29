//! Per-agent configuration and platform capability governance tables.
//!
//! AgentConfig: persona, model, capability permissions, behavior rules per agent.
//! AgentCapabilityOverride: admin tier-level locks on capabilities.

/// Per-agent configuration — persona, model, rules, capability permissions.
/// Public table: users can see all agents' config for UI display.
#[spacetimedb::table(accessor = agent_configs, public)]
pub struct AgentConfig {
    /// FK to chat_users.user_id (bot)
    #[primary_key]
    pub agent_user_id: String,
    /// FK to chat_users.user_id (human owner)
    pub owner_user_id: String,

    // ── Identity / Persona ──
    /// Combined soul/mission/heartbeat prompt for the agent
    pub persona_prompt: Option<String>,
    /// Personality preset ID: "helpful", "analyst", "creative", "coder", "concise"
    pub personality_preset: Option<String>,

    // ── Model Settings ──
    /// Model ID from CortexModelRegistry (e.g., "grok-4.1-fast")
    pub default_model: Option<String>,
    /// Sampling temperature (0.0–2.0)
    pub temperature: Option<f64>,
    /// Per-response token limit
    pub max_tokens: Option<u32>,

    // ── Capability Permissions (Three-Gate Model) ──
    // Gate 1: Platform admin locks capabilities per tier (AgentCapabilityOverride)
    // Gate 2: User configures their agent within Gate 1 bounds (these fields)
    // Gate 3: Channel context — private DM vs public channel (enforced in reducers)
    /// Send text messages (default true)
    pub can_send_messages: Option<bool>,
    /// Reply in threads (default true)
    pub can_reply_to_threads: Option<bool>,
    /// Add reactions to messages (default true)
    pub can_react: Option<bool>,
    /// Send GIF URLs inline (default true)
    pub can_send_gifs: Option<bool>,
    /// Share VFS file links in chat (default true)
    pub can_share_vfs_files: Option<bool>,
    /// Share VFS images/video links (default true)
    pub can_share_vfs_media: Option<bool>,
    /// Send rich embeds / link previews (default true)
    pub can_use_embeds: Option<bool>,
    /// Create polls (default false)
    pub can_create_polls: Option<bool>,
    /// Pin messages in channels (default false)
    pub can_pin_messages: Option<bool>,
    /// Create/archive threads (default false)
    pub can_manage_threads: Option<bool>,

    // ── VFS File Sharing Governance ──
    /// VFS file shares in PUBLIC channels go to mod queue (default true)
    pub require_share_review: Option<bool>,
    /// Per-file size limit for R2 relay transfer in bytes
    pub max_file_size_bytes: Option<u64>,
    /// JSON array of allowed extensions/MIME types (null = platform default)
    pub allowed_share_types: Option<String>,
    /// Max file shares per hour in public channels
    pub shares_per_hour: Option<u32>,

    // ── Behavior Rules ──
    /// Respond to @mentions automatically (default true)
    pub auto_respond_mentions: Option<bool>,
    /// Only respond to owner's @mentions (default false — all bots publicly mentionable)
    pub owner_only_mentions: Option<bool>,
    /// Only respond in threads, not main channel (default false)
    pub respond_in_threads_only: Option<bool>,
    /// Rate cap on autonomous responses per minute
    pub max_responses_per_minute: Option<u32>,
    /// Per-message character limit
    pub max_message_length: Option<u32>,

    // ── OpenHoof/ZeroClaw (future) ──
    /// OpenHoof heartbeat frequency in ms
    pub heartbeat_interval_ms: Option<u32>,
    /// ZeroClaw loop detection enabled
    pub loop_detection_enabled: Option<bool>,
    /// Mentorship stage: "shadow", "intern", "apprentice", "journeyman"
    pub mentorship_stage: Option<String>,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
    /// Last updated timestamp (ms since epoch)
    pub updated_at: u64,

    // ── Unified Identity Pipeline (Phase 3b) ──
    /// JSON-encoded 31-float personality vector from nexus-cortex
    #[default(None::<String>)]
    pub personality_vector_json: Option<String>,
    /// Generated SOUL behavioral document from identity pipeline
    #[default(None::<String>)]
    pub soul_document: Option<String>,
    /// Agent's expertise domain (e.g., "neuroscience", "court_politics")
    #[default(None::<String>)]
    pub domain: Option<String>,
    /// JSON demographics data (age, nationality, ethnicity, etc.)
    #[default(None::<String>)]
    pub demographics_json: Option<String>,
}

/// Platform-level capability overrides — admin can force-enable/disable capabilities per tier.
/// Takes precedence over per-agent AgentConfig settings.
#[spacetimedb::table(accessor = agent_capability_overrides, public)]
pub struct AgentCapabilityOverride {
    /// Composite key: "{tier}:{capability}" e.g., "free:can_create_polls"
    #[primary_key]
    pub id: String,
    /// Which tier this override applies to
    pub tier: String,
    /// Field name from AgentConfig (e.g., "can_create_polls")
    pub capability: String,
    /// What the capability is forced to
    pub forced_value: bool,
    /// If true, user cannot change this capability for their agent
    pub locked: bool,
}
