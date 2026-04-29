//! Chat Tables
//!
//! All SpacetimeDB table definitions for the chat module.

// === Core tables ===
pub mod auth;
pub mod users;
pub mod rooms;
pub mod room_members;
pub mod messages;
pub mod message_edits;
pub mod typing_indicators;
pub mod reactions;
pub mod read_positions;
pub mod scheduled_messages;
pub mod room_invitations;
pub mod drafts;
pub mod scheduled_jobs;
pub mod servers;
pub mod server_members;

// === Discord parity tables ===
pub mod audit_log;
pub mod auto_mod_rules;
pub mod bookmarks;
pub mod channel_categories;
pub mod channel_overrides;
pub mod forum_tags;
pub mod member_roles;
pub mod member_timeouts;
pub mod message_attachments;
pub mod notification_settings;
pub mod pinned_messages;
pub mod polls;
pub mod server_emojis;
pub mod server_invites;
pub mod server_roles;
pub mod starred_channels;
pub mod stickers;
pub mod user_profiles;
pub mod webhooks;
pub mod user_blocks;
pub mod agents;
pub mod agent_config;
pub mod system_config;

// === Shadow indexes (shadowing-stork plan — TeV reduction) ===
// Non-null room_id projections of reactions and message_edits, allowing
// room-scoped subscriptions without hitting the Option<T>-can't-filter
// limit. Maintained by dual-write in toggle_reaction / edit_message
// and by cascade-delete in messages/rooms/ephemeral cleanup paths.
pub mod reaction_room_index;
pub mod message_edit_room_index;

// Re-export table structs for convenience
pub use auth::*;
pub use users::*;
pub use rooms::*;
pub use room_members::*;
pub use messages::*;
pub use message_edits::*;
pub use typing_indicators::*;
pub use reactions::*;
pub use read_positions::*;
pub use scheduled_messages::*;
pub use room_invitations::*;
pub use drafts::*;
pub use scheduled_jobs::*;
pub use servers::*;
pub use server_members::*;

// Discord parity re-exports
pub use audit_log::*;
pub use auto_mod_rules::*;
pub use bookmarks::*;
pub use channel_categories::*;
pub use channel_overrides::*;
pub use forum_tags::*;
pub use member_roles::*;
pub use member_timeouts::*;
pub use message_attachments::*;
pub use notification_settings::*;
pub use pinned_messages::*;
pub use polls::*;
pub use server_emojis::*;
pub use server_invites::*;
pub use server_roles::*;
pub use starred_channels::*;
pub use stickers::*;
pub use user_profiles::*;
pub use webhooks::*;
pub use user_blocks::*;
pub use agents::*;
pub use agent_config::*;
pub use system_config::*;
pub use reaction_room_index::*;
pub use message_edit_room_index::*;
