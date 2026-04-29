//! Audit log helpers.
//!
//! The audit log is write-only from reducers (no user-facing write reducers).
//! Users read via subscription: `SELECT * FROM audit_log WHERE server_id = '...'`

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::audit_log::audit_log;

/// Log an audit event. Called internally by other reducers.
pub fn log_audit_event(
    ctx: &ReducerContext,
    id: &str,
    server_id: &str,
    action: &str,
    actor_id: &str,
    target_type: &str,
    target_id: &str,
    details: Option<String>,
) {
    let now = crate::timestamp_ms(ctx);
    ctx.db.audit_log().insert(AuditLogEntry {
        id: id.to_string(),
        server_id: server_id.to_string(),
        action: action.to_string(),
        actor_id: actor_id.to_string(),
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
        details,
        created_at: now,
    });
}
