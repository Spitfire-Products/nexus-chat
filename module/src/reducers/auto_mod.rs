//! Auto-moderation rule management reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::auto_mod_rules::auto_mod_rules;
use crate::utils::permissions::*;

const MAX_AUTO_MOD_RULES_PER_SERVER: usize = 20;

/// Create an auto-mod rule.
#[spacetimedb::reducer]
pub fn create_auto_mod_rule(
    ctx: &ReducerContext,
    id: String,
    server_id: String,
    rule_type: String,
    config: String,
    action: String,
    exempt_roles: String,
    exempt_channels: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[create_auto_mod_rule] Unauthorized");
        return;
    };

    if require_server_permission(ctx, &server_id, &user_id, PERM_MANAGE_SERVER, "create_auto_mod_rule").is_none() {
        return;
    }

    let valid_types = ["blocked_words", "spam_filter", "mention_limit", "link_filter", "caps_filter"];
    if !valid_types.contains(&rule_type.as_str()) {
        log::warn!("[create_auto_mod_rule] Invalid rule_type: {}", rule_type);
        return;
    }

    let valid_actions = ["block", "flag", "timeout_60", "timeout_300", "timeout_3600"];
    if !valid_actions.contains(&action.as_str()) {
        log::warn!("[create_auto_mod_rule] Invalid action: {}", action);
        return;
    }

    let count = ctx.db.auto_mod_rules().iter()
        .filter(|r| r.server_id == server_id)
        .count();
    if count >= MAX_AUTO_MOD_RULES_PER_SERVER {
        log::warn!("[create_auto_mod_rule] Server {} has too many rules", server_id);
        return;
    }

    let now = crate::timestamp_ms(ctx);
    ctx.db.auto_mod_rules().insert(AutoModRule {
        id,
        server_id,
        rule_type,
        config,
        enabled: true,
        action,
        exempt_roles,
        exempt_channels,
        created_by: user_id,
        created_at: now,
    });
}

/// Update an auto-mod rule.
#[spacetimedb::reducer]
pub fn update_auto_mod_rule(
    ctx: &ReducerContext,
    id: String,
    config: String,
    enabled: bool,
    action: String,
    exempt_roles: String,
    exempt_channels: String,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[update_auto_mod_rule] Unauthorized");
        return;
    };

    let Some(rule) = ctx.db.auto_mod_rules().id().find(&id) else {
        log::warn!("[update_auto_mod_rule] Rule {} not found", id);
        return;
    };

    if require_server_permission(ctx, &rule.server_id, &user_id, PERM_MANAGE_SERVER, "update_auto_mod_rule").is_none() {
        return;
    }

    ctx.db.auto_mod_rules().id().delete(&id);
    ctx.db.auto_mod_rules().insert(AutoModRule {
        config,
        enabled,
        action,
        exempt_roles,
        exempt_channels,
        ..rule
    });
}

/// Delete an auto-mod rule.
#[spacetimedb::reducer]
pub fn delete_auto_mod_rule(ctx: &ReducerContext, id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[delete_auto_mod_rule] Unauthorized");
        return;
    };

    let Some(rule) = ctx.db.auto_mod_rules().id().find(&id) else {
        log::warn!("[delete_auto_mod_rule] Rule {} not found", id);
        return;
    };

    if require_server_permission(ctx, &rule.server_id, &user_id, PERM_MANAGE_SERVER, "delete_auto_mod_rule").is_none() {
        return;
    }

    ctx.db.auto_mod_rules().id().delete(&id);
}
