//! User profile reducers.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::user_profiles::user_profiles;
use crate::tables::users::chat_users;

const MAX_ABOUT_ME_LEN: usize = 190;
const MAX_BANNER_DATA_LEN: usize = 700_000; // ~512KB base64
const MAX_PRONOUNS_LEN: usize = 32;
const MAX_STATUS_LEN: usize = 128;

/// Update the caller's extended profile.
#[spacetimedb::reducer]
pub fn update_profile(
    ctx: &ReducerContext,
    about_me: Option<String>,
    banner_color: Option<String>,
    banner_data: Option<String>,
    pronouns: Option<String>,
    accent_color: Option<String>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[update_profile] Unauthorized");
        return;
    };

    // Validate lengths
    if let Some(ref about) = about_me {
        if about.len() > MAX_ABOUT_ME_LEN {
            log::warn!("[update_profile] about_me too long: {}", about.len());
            return;
        }
    }
    if let Some(ref data) = banner_data {
        if data.len() > MAX_BANNER_DATA_LEN {
            log::warn!("[update_profile] banner_data too large: {}", data.len());
            return;
        }
    }
    if let Some(ref p) = pronouns {
        if p.len() > MAX_PRONOUNS_LEN {
            return;
        }
    }

    let now = crate::timestamp_ms(ctx);

    // Upsert
    if ctx.db.user_profiles().user_id().find(&user_id).is_some() {
        ctx.db.user_profiles().user_id().delete(&user_id);
    }

    ctx.db.user_profiles().insert(UserProfile {
        user_id,
        about_me,
        banner_color,
        banner_data,
        pronouns,
        accent_color,
        updated_at: now,
    });
}

/// Set the caller's avatar (base64 image).
#[spacetimedb::reducer]
pub fn set_avatar(ctx: &ReducerContext, avatar_data: Option<String>) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[set_avatar] Unauthorized");
        return;
    };

    if let Some(ref data) = avatar_data {
        if data.len() > MAX_BANNER_DATA_LEN {
            log::warn!("[set_avatar] avatar_data too large: {}", data.len());
            return;
        }
    }

    let Some(user) = ctx.db.chat_users().user_id().find(&user_id) else {
        log::warn!("[set_avatar] User {} not found", user_id);
        return;
    };

    ctx.db.chat_users().user_id().delete(&user_id);
    ctx.db.chat_users().insert(ChatUser {
        avatar_data,
        ..user
    });
}

/// Set the caller's custom status text and emoji.
#[spacetimedb::reducer]
pub fn set_custom_status(
    ctx: &ReducerContext,
    text: Option<String>,
    emoji: Option<String>,
) {
    let Some(user_id) = crate::utils::auth::get_caller_user_id(ctx) else {
        log::warn!("[set_custom_status] Unauthorized");
        return;
    };

    if let Some(ref t) = text {
        if t.len() > MAX_STATUS_LEN {
            return;
        }
    }

    let Some(user) = ctx.db.chat_users().user_id().find(&user_id) else {
        log::warn!("[set_custom_status] User {} not found", user_id);
        return;
    };

    ctx.db.chat_users().user_id().delete(&user_id);
    ctx.db.chat_users().insert(ChatUser {
        custom_status: text,
        custom_status_emoji: emoji,
        ..user
    });
}
