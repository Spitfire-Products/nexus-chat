//! Notification settings reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::notification_settings::notification_settings;

/// Set notification level for a server or room.
#[spacetimedb::reducer]
pub fn set_notification_level(
    ctx: &ReducerContext,
    target_type: String,
    target_id: String,
    level: String,
    suppress_everyone: bool,
    suppress_roles: bool,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[set_notification_level] Unauthorized");
        return;
    };

    if target_type != "server" && target_type != "room" {
        log::warn!("[set_notification_level] Invalid target_type: {}", target_type);
        return;
    }

    let valid_levels = ["all", "mentions", "none"];
    if !valid_levels.contains(&level.as_str()) {
        log::warn!("[set_notification_level] Invalid level: {}", level);
        return;
    }

    let setting_id = format!("{}-{}-{}", user_id, target_type, target_id);
    let now = crate::timestamp_ms(ctx);

    // Upsert
    if ctx.db.notification_settings().id().find(&setting_id).is_some() {
        ctx.db.notification_settings().id().delete(&setting_id);
    }

    ctx.db.notification_settings().insert(NotificationSetting {
        id: setting_id,
        user_id,
        target_type,
        target_id,
        level,
        suppress_everyone,
        suppress_roles,
        muted_until: None,
        updated_at: now,
    });
}

/// Temporarily mute a channel.
#[spacetimedb::reducer]
pub fn mute_channel(ctx: &ReducerContext, room_id: String, duration_ms: u64) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[mute_channel] Unauthorized");
        return;
    };

    let now = crate::timestamp_ms(ctx);
    let muted_until = now + duration_ms;

    let setting_id = format!("{}-room-{}", user_id, room_id);

    // Upsert
    let existing = ctx.db.notification_settings().id().find(&setting_id);
    if let Some(setting) = existing {
        ctx.db.notification_settings().id().delete(&setting_id);
        ctx.db.notification_settings().insert(NotificationSetting {
            muted_until: Some(muted_until),
            updated_at: now,
            ..setting
        });
    } else {
        ctx.db.notification_settings().insert(NotificationSetting {
            id: setting_id,
            user_id,
            target_type: "room".to_string(),
            target_id: room_id,
            level: "all".to_string(),
            suppress_everyone: false,
            suppress_roles: false,
            muted_until: Some(muted_until),
            updated_at: now,
        });
    }
}

/// Unmute a channel.
#[spacetimedb::reducer]
pub fn unmute_channel(ctx: &ReducerContext, room_id: String) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[unmute_channel] Unauthorized");
        return;
    };

    let setting_id = format!("{}-room-{}", user_id, room_id);

    if let Some(setting) = ctx.db.notification_settings().id().find(&setting_id) {
        let now = crate::timestamp_ms(ctx);
        ctx.db.notification_settings().id().delete(&setting_id);
        ctx.db.notification_settings().insert(NotificationSetting {
            muted_until: None,
            updated_at: now,
            ..setting
        });
    }
}
