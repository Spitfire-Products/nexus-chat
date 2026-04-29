//! System configuration — PRIVATE table for cross-module auth secrets.
//!
//! NOT public — never sent to any client. Used for server-to-server
//! authentication between SpacetimeDB modules (e.g., nexus-cortex
//! calling system_send_bot_message with a system_token).
//!
//! Keys:
//!   "cortex_system_token" — Shared secret for nexus-cortex cross-module calls

/// A key-value system configuration entry. Private — never sent to clients.
#[spacetimedb::table(accessor = system_config)]
pub struct SystemConfig {
    /// Configuration key (e.g., "cortex_system_token")
    #[primary_key]
    pub key: String,

    /// Configuration value (secret)
    pub value: String,

    /// Last update timestamp (ms since epoch)
    pub updated_at: u64,
}
