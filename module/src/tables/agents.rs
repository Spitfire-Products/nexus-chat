//! Agent credential and governance tables.
//!
//! Supports first-class agent ChatUsers with standalone SpacetimeDB connections.
//! The `agent_credentials` table is PRIVATE — owner_secret never sent to clients.

use spacetimedb::ScheduleAt;
use crate::reducers::agents::cleanup_idle_agents;

/// Agent credential store — PRIVATE table (not sent to clients, protects owner_secret).
/// Each row maps a bot ChatUser to its human owner and stores the secret used
/// by the agent's standalone connection to prove ownership during register_agent_identity.
#[spacetimedb::table(accessor = agent_credentials)]
pub struct AgentCredential {
    /// FK to chat_users.user_id — the bot user
    #[primary_key]
    pub agent_user_id: String,
    /// Human who owns this agent (always human, never another agent — chains up)
    pub owner_user_id: String,
    /// If spawned by another agent, tracks parent. None = provisioned by human directly.
    pub parent_agent_id: Option<String>,
    /// Secret token for agent identity registration (caller-generated UUID)
    pub owner_secret: String,
    /// Whether this agent can spawn sub-agents
    pub can_spawn: bool,
    /// Created timestamp (ms since epoch)
    pub created_at: u64,
    /// Last activity timestamp (ms since epoch) — for idle cleanup of sub-agents
    pub last_active_at: u64,
    /// Whether this is a platform-level agent (admin-provisioned, anyone can @mention).
    /// None/false = user agent (owner-only @mentions by default).
    #[default(None::<bool>)]
    pub is_platform_agent: Option<bool>,
}

/// Agent spawn limits — PUBLIC table (tier-gated caps).
/// Seeded via seed_agent_spawn_limits reducer.
#[spacetimedb::table(accessor = agent_spawn_limits, public)]
pub struct AgentSpawnLimit {
    /// Tier name: "free", "pro", "creator", "team"
    #[primary_key]
    pub tier: String,
    /// Maximum concurrent active agents per human owner
    pub max_concurrent_agents: u32,
    /// Bot message rate limit in milliseconds (default 5000 = 5s).
    /// Forces bots to write complete thoughts instead of rapid-fire short messages.
    #[default(None::<u32>)]
    pub bot_rate_limit_ms: Option<u32>,
    /// Maximum user-created servers per human owner for this tier.
    /// None = use agent limit as fallback (backwards compat).
    #[default(None::<u32>)]
    pub max_servers: Option<u32>,
}

/// Scheduled cleanup for idle sub-agents.
/// Sub-agents (those with parent_agent_id) are cleaned up after 1 hour idle.
/// Main agents (provisioned by humans) persist until manually deprovisioned.
#[spacetimedb::table(accessor = agent_cleanup_jobs, scheduled(cleanup_idle_agents))]
pub struct AgentCleanupJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}
