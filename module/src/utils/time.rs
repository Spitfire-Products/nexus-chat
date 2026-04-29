//! Time utilities for the chat module.
//!
//! Provides server-authoritative timestamp functions using
//! the ReducerContext timestamp (not system clock).

use spacetimedb::ReducerContext;

/// Get current timestamp in microseconds since Unix epoch.
///
/// Uses the reducer context's server-authoritative timestamp,
/// NOT std::time — which may differ across nodes.
///
/// SpacetimeDB convention: timestamps are microseconds (u64).
/// The UI (MessageItem.tsx) divides by 1000 to get JS Date milliseconds.
pub fn timestamp_ms(ctx: &ReducerContext) -> u64 {
    ctx.timestamp.to_duration_since_unix_epoch()
        .unwrap_or_default()
        .as_micros() as u64
}
