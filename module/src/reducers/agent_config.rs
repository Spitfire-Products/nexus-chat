//! Agent configuration reducers: create, update, and admin capability overrides.

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::tables::agents::agent_credentials;
use crate::tables::agent_config::{agent_configs, agent_capability_overrides};
use crate::utils::auth::{get_caller_user_id, is_platform_admin};

/// Create initial config for a newly provisioned agent.
/// Callable by the agent's owner or platform admin.
#[spacetimedb::reducer]
pub fn create_agent_config(
    ctx: &ReducerContext,
    agent_user_id: String,
    persona_prompt: Option<String>,
    personality_preset: Option<String>,
    default_model: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
) {
    let Some(caller_user_id) = get_caller_user_id(ctx) else {
        log::warn!("[create_agent_config] Unauthorized: no identity link");
        return;
    };

    // Verify agent exists and caller is owner or admin
    let Some(cred) = ctx.db.agent_credentials().agent_user_id().find(&agent_user_id) else {
        log::warn!("[create_agent_config] Agent {} not found", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    };
    if cred.owner_user_id != caller_user_id && !is_platform_admin(ctx) {
        log::warn!("[create_agent_config] Not owner of agent {}", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    // Don't create if config already exists
    if ctx.db.agent_configs().agent_user_id().find(&agent_user_id).is_some() {
        log::warn!("[create_agent_config] Config already exists for agent {}", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    let now = crate::timestamp_ms(ctx);

    ctx.db.agent_configs().insert(AgentConfig {
        agent_user_id: agent_user_id.clone(),
        owner_user_id: cred.owner_user_id.clone(),
        persona_prompt,
        personality_preset,
        default_model,
        temperature,
        max_tokens,
        // Default capabilities — most enabled, advanced disabled
        can_send_messages: Some(true),
        can_reply_to_threads: Some(true),
        can_react: Some(true),
        can_send_gifs: Some(true),
        can_share_vfs_files: Some(true),
        can_share_vfs_media: Some(true),
        can_use_embeds: Some(true),
        can_create_polls: Some(false),
        can_pin_messages: Some(false),
        can_manage_threads: Some(false),
        // VFS sharing defaults
        require_share_review: Some(true),
        max_file_size_bytes: None,
        allowed_share_types: None,
        shares_per_hour: None,
        // Behavior defaults
        auto_respond_mentions: Some(true),
        owner_only_mentions: Some(false),
        respond_in_threads_only: Some(false),
        max_responses_per_minute: None,
        max_message_length: None,
        // Future fields
        heartbeat_interval_ms: None,
        loop_detection_enabled: None,
        mentorship_stage: None,
        created_at: now,
        updated_at: now,
        // Unified identity pipeline fields
        personality_vector_json: None,
        soul_document: None,
        domain: None,
        demographics_json: None,
    });

    log::info!("[create_agent_config] Config created for agent {}", &agent_user_id[..8.min(agent_user_id.len())]);
}

/// Update agent config — owner or admin can change persona, model, capabilities, rules.
/// Respects AgentCapabilityOverride locks: locked capabilities cannot be changed by non-admin users.
#[spacetimedb::reducer]
pub fn update_agent_config(
    ctx: &ReducerContext,
    agent_user_id: String,
    // Identity / Persona
    persona_prompt: Option<String>,
    personality_preset: Option<String>,
    // Model
    default_model: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    // Capabilities (None = don't change)
    can_send_messages: Option<bool>,
    can_reply_to_threads: Option<bool>,
    can_react: Option<bool>,
    can_send_gifs: Option<bool>,
    can_share_vfs_files: Option<bool>,
    can_share_vfs_media: Option<bool>,
    can_use_embeds: Option<bool>,
    can_create_polls: Option<bool>,
    can_pin_messages: Option<bool>,
    can_manage_threads: Option<bool>,
    // VFS sharing
    require_share_review: Option<bool>,
    max_file_size_bytes: Option<u64>,
    allowed_share_types: Option<String>,
    shares_per_hour: Option<u32>,
    // Behavior
    auto_respond_mentions: Option<bool>,
    owner_only_mentions: Option<bool>,
    respond_in_threads_only: Option<bool>,
    max_responses_per_minute: Option<u32>,
    max_message_length: Option<u32>,
) {
    let Some(caller_user_id) = get_caller_user_id(ctx) else {
        log::warn!("[update_agent_config] Unauthorized: no identity link");
        return;
    };

    let Some(cred) = ctx.db.agent_credentials().agent_user_id().find(&agent_user_id) else {
        log::warn!("[update_agent_config] Agent {} not found", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    };

    let is_admin = is_platform_admin(ctx);
    if cred.owner_user_id != caller_user_id && !is_admin {
        log::warn!("[update_agent_config] Not owner of agent {}", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    }

    let Some(existing) = ctx.db.agent_configs().agent_user_id().find(&agent_user_id) else {
        log::warn!("[update_agent_config] No config for agent {}", &agent_user_id[..8.min(agent_user_id.len())]);
        return;
    };

    // Resolve owner tier for capability override checks
    let owner_tier = ctx.db.chat_users().user_id().find(&cred.owner_user_id)
        .and_then(|u| u.tier.clone())
        .unwrap_or_else(|| "free".to_string());

    // Helper: check if a capability update is allowed (not locked by admin override)
    let is_locked = |cap_name: &str| -> bool {
        if is_admin { return false; } // Admins bypass locks
        let override_id = format!("{}:{}", owner_tier, cap_name);
        ctx.db.agent_capability_overrides().id().find(&override_id)
            .map(|o| o.locked)
            .unwrap_or(false)
    };

    let now = crate::timestamp_ms(ctx);

    // Apply updates, respecting locks for capability fields
    ctx.db.agent_configs().agent_user_id().delete(&agent_user_id);
    ctx.db.agent_configs().insert(AgentConfig {
        agent_user_id,
        owner_user_id: existing.owner_user_id,
        // Persona/model — always updatable by owner
        persona_prompt: if persona_prompt.is_some() { persona_prompt } else { existing.persona_prompt },
        personality_preset: if personality_preset.is_some() { personality_preset } else { existing.personality_preset },
        default_model: if default_model.is_some() { default_model } else { existing.default_model },
        temperature: if temperature.is_some() { temperature } else { existing.temperature },
        max_tokens: if max_tokens.is_some() { max_tokens } else { existing.max_tokens },
        // Capabilities — respect locks
        can_send_messages: if can_send_messages.is_some() && !is_locked("can_send_messages") { can_send_messages } else { existing.can_send_messages },
        can_reply_to_threads: if can_reply_to_threads.is_some() && !is_locked("can_reply_to_threads") { can_reply_to_threads } else { existing.can_reply_to_threads },
        can_react: if can_react.is_some() && !is_locked("can_react") { can_react } else { existing.can_react },
        can_send_gifs: if can_send_gifs.is_some() && !is_locked("can_send_gifs") { can_send_gifs } else { existing.can_send_gifs },
        can_share_vfs_files: if can_share_vfs_files.is_some() && !is_locked("can_share_vfs_files") { can_share_vfs_files } else { existing.can_share_vfs_files },
        can_share_vfs_media: if can_share_vfs_media.is_some() && !is_locked("can_share_vfs_media") { can_share_vfs_media } else { existing.can_share_vfs_media },
        can_use_embeds: if can_use_embeds.is_some() && !is_locked("can_use_embeds") { can_use_embeds } else { existing.can_use_embeds },
        can_create_polls: if can_create_polls.is_some() && !is_locked("can_create_polls") { can_create_polls } else { existing.can_create_polls },
        can_pin_messages: if can_pin_messages.is_some() && !is_locked("can_pin_messages") { can_pin_messages } else { existing.can_pin_messages },
        can_manage_threads: if can_manage_threads.is_some() && !is_locked("can_manage_threads") { can_manage_threads } else { existing.can_manage_threads },
        // VFS sharing
        require_share_review: if require_share_review.is_some() { require_share_review } else { existing.require_share_review },
        max_file_size_bytes: if max_file_size_bytes.is_some() { max_file_size_bytes } else { existing.max_file_size_bytes },
        allowed_share_types: if allowed_share_types.is_some() { allowed_share_types } else { existing.allowed_share_types },
        shares_per_hour: if shares_per_hour.is_some() { shares_per_hour } else { existing.shares_per_hour },
        // Behavior
        auto_respond_mentions: if auto_respond_mentions.is_some() { auto_respond_mentions } else { existing.auto_respond_mentions },
        owner_only_mentions: if owner_only_mentions.is_some() { owner_only_mentions } else { existing.owner_only_mentions },
        respond_in_threads_only: if respond_in_threads_only.is_some() { respond_in_threads_only } else { existing.respond_in_threads_only },
        max_responses_per_minute: if max_responses_per_minute.is_some() { max_responses_per_minute } else { existing.max_responses_per_minute },
        max_message_length: if max_message_length.is_some() { max_message_length } else { existing.max_message_length },
        // Future
        heartbeat_interval_ms: existing.heartbeat_interval_ms,
        loop_detection_enabled: existing.loop_detection_enabled,
        mentorship_stage: existing.mentorship_stage,
        created_at: existing.created_at,
        updated_at: now,
        // Preserve identity pipeline fields (managed by system_update_agent_config)
        personality_vector_json: existing.personality_vector_json,
        soul_document: existing.soul_document,
        domain: existing.domain,
        demographics_json: existing.demographics_json,
    });

    log::info!("[update_agent_config] Config updated for agent");
}

/// Admin: set or update a capability override for a tier.
/// This locks or forces a capability value for all agents owned by users of that tier.
#[spacetimedb::reducer]
pub fn update_agent_capability_override(
    ctx: &ReducerContext,
    tier: String,
    capability: String,
    forced_value: bool,
    locked: bool,
) {
    if !is_platform_admin(ctx) {
        log::warn!("[update_agent_capability_override] Rejected: not platform admin");
        return;
    }

    let id = format!("{}:{}", tier, capability);

    // Delete existing if present
    ctx.db.agent_capability_overrides().id().delete(&id);

    ctx.db.agent_capability_overrides().insert(AgentCapabilityOverride {
        id,
        tier: tier.clone(),
        capability: capability.clone(),
        forced_value,
        locked,
    });

    log::info!("[update_agent_capability_override] Set {}:{} = {} (locked: {})", tier, capability, forced_value, locked);
}

/// Admin: remove a capability override for a tier.
#[spacetimedb::reducer]
pub fn remove_agent_capability_override(
    ctx: &ReducerContext,
    tier: String,
    capability: String,
) {
    if !is_platform_admin(ctx) {
        log::warn!("[remove_agent_capability_override] Rejected: not platform admin");
        return;
    }

    let id = format!("{}:{}", tier, capability);
    ctx.db.agent_capability_overrides().id().delete(&id);
    log::info!("[remove_agent_capability_override] Removed {}:{}", tier, capability);
}
