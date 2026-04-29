//! Scheduled job tables for time-based automation.
//!
//! These tables use SpacetimeDB's scheduled table feature to trigger
//! reducers at specific times. The row is auto-deleted after the reducer runs.

use spacetimedb::ScheduleAt;
use crate::reducers::{expire_typing_indicator, delete_ephemeral_message, send_scheduled_message, expire_member_timeout};

/// Scheduled job: expire a typing indicator after 4 seconds.
#[spacetimedb::table(accessor = typing_expiry_jobs, scheduled(expire_typing_indicator))]
pub struct TypingExpiryJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    /// The typing indicator to delete
    pub typing_indicator_id: String,
}

/// Scheduled job: delete an ephemeral message after its TTL expires.
#[spacetimedb::table(accessor = ephemeral_cleanup_jobs, scheduled(delete_ephemeral_message))]
pub struct EphemeralCleanupJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    /// The message to delete
    pub message_id: String,
}

/// Scheduled job: deliver a scheduled message at its send time.
#[spacetimedb::table(accessor = scheduled_delivery_jobs, scheduled(send_scheduled_message))]
pub struct ScheduledDeliveryJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    /// The scheduled_messages row to convert to a real message
    pub scheduled_message_id: String,
}

/// Scheduled job: expire a member timeout.
#[spacetimedb::table(accessor = timeout_expiry_jobs, scheduled(expire_member_timeout))]
pub struct TimeoutExpiryJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    /// The timeout to remove
    pub timeout_id: String,
}
