//! Member timeout reducers.

use spacetimedb::{ReducerContext, ScheduleAt, Table};
use crate::tables::*;
use crate::tables::member_timeouts::member_timeouts;
use crate::tables::server_members::server_members;
use crate::tables::scheduled_jobs::timeout_expiry_jobs;
use crate::utils::permissions::*;

/// Timeout a member (restrict from sending messages for duration_seconds).
#[spacetimedb::reducer]
pub fn timeout_member(
    ctx: &ReducerContext,
    server_id: String,
    target_user_id: String,
    duration_seconds: u64,
    reason: Option<String>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[timeout_member] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MODERATE_MEMBERS, "timeout_member").is_none() {
        return;
    }

    // Can't timeout owner
    if is_server_owner(ctx, &server_id, &target_user_id) {
        log::warn!("[timeout_member] Cannot timeout server owner");
        return;
    }

    // Clamp duration: 60 seconds to 28 days
    let clamped = duration_seconds.clamp(60, 28 * 24 * 3600);

    let now = crate::timestamp_ms(ctx);
    let expires_at = now + (clamped * 1000);
    let timeout_id = format!("{}-{}-{}", server_id, target_user_id, now);

    // Remove existing timeout if any
    let existing: Vec<MemberTimeout> = ctx.db.member_timeouts().iter()
        .filter(|t| t.server_id == server_id && t.user_id == target_user_id)
        .collect();
    for t in existing {
        ctx.db.member_timeouts().id().delete(&t.id);
    }

    ctx.db.member_timeouts().insert(MemberTimeout {
        id: timeout_id.clone(),
        server_id: server_id.clone(),
        user_id: target_user_id.clone(),
        reason,
        expires_at,
        issued_by: user_id.clone(),
        created_at: now,
    });

    // Also update server_member.timeout_until
    let member_id = format!("{}-{}", server_id, target_user_id);
    if let Some(member) = ctx.db.server_members().id().find(&member_id) {
        ctx.db.server_members().id().delete(&member_id);
        ctx.db.server_members().insert(ServerMember {
            timeout_until: Some(expires_at),
            ..member
        });
    }

    // Schedule expiry job
    let expires_micros = (expires_at as i64) * 1000;
    ctx.db.timeout_expiry_jobs().insert(TimeoutExpiryJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(spacetimedb::Timestamp::from_micros_since_unix_epoch(expires_micros)),
        timeout_id,
    });

    // Audit log
    crate::reducers::audit_log::log_audit_event(
        ctx,
        &format!("audit-timeout-{}-{}", server_id, now),
        &server_id,
        "MEMBER_TIMEOUT",
        &user_id,
        "user",
        &target_user_id,
        Some(format!("{{\"duration_seconds\":{}}}", clamped)),
    );
}

/// Remove a timeout from a member.
#[spacetimedb::reducer]
pub fn remove_timeout(ctx: &ReducerContext, server_id: String, target_user_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[remove_timeout] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MODERATE_MEMBERS, "remove_timeout").is_none() {
        return;
    }

    // Remove timeout record
    let timeouts: Vec<MemberTimeout> = ctx.db.member_timeouts().iter()
        .filter(|t| t.server_id == server_id && t.user_id == target_user_id)
        .collect();
    for t in timeouts {
        ctx.db.member_timeouts().id().delete(&t.id);
    }

    // Clear timeout_until on server member
    let member_id = format!("{}-{}", server_id, target_user_id);
    if let Some(member) = ctx.db.server_members().id().find(&member_id) {
        ctx.db.server_members().id().delete(&member_id);
        ctx.db.server_members().insert(ServerMember {
            timeout_until: None,
            ..member
        });
    }
}

/// Scheduled reducer: expire a member timeout.
#[spacetimedb::reducer]
pub fn expire_member_timeout(ctx: &ReducerContext, arg: TimeoutExpiryJob) {
    let Some(timeout) = ctx.db.member_timeouts().id().find(&arg.timeout_id) else {
        return; // Already removed
    };

    // Clear timeout_until on server member
    let member_id = format!("{}-{}", timeout.server_id, timeout.user_id);
    if let Some(member) = ctx.db.server_members().id().find(&member_id) {
        ctx.db.server_members().id().delete(&member_id);
        ctx.db.server_members().insert(ServerMember {
            timeout_until: None,
            ..member
        });
    }

    ctx.db.member_timeouts().id().delete(&arg.timeout_id);
    log::info!("[expire_member_timeout] Timeout {} expired", arg.timeout_id);
}
