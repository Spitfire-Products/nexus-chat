//! message_edit_room_index — non-null room_id projection of message edits
//! for room-scoped subscription filtering.
//!
//! Same rationale as reaction_room_index: `message_edits.room_id` is
//! `Option<String>` (added denorm) and subscriptions can't filter
//! Option<T>. This shadow index makes room-scoped subscriptions
//! filterable: `SELECT * FROM message_edit_room_index WHERE room_id = '...'`.
//!
//! Maintained by dual-write in `edit_message` (and matching cascade
//! deletes in messages/rooms/ephemeral cleanup paths). Carries the
//! latest `new_content` so the "edited" indicator UI can render without
//! joining back to `message_edits`. Full edit history (with `old_content`)
//! stays in the source table — fetch lazily when the user opens a
//! message's edit history detail panel.
//!
//! grounded-charting-egret successor (`shadowing-stork` plan).

#[spacetimedb::table(accessor = message_edit_room_index, public)]
pub struct MessageEditRoomIndex {
    /// Same value as message_edits.id — primary key on the source row.
    #[primary_key]
    pub edit_id: String,

    /// Non-null. Indexed for room-scoped subscriptions.
    #[index(btree)]
    pub room_id: String,

    /// FK into messages.
    pub message_id: String,

    /// Who edited (platform user_id).
    pub editor_id: String,

    /// Mirror of message_edits.edited_at.
    pub edited_at: u64,

    /// Latest content after the edit. Indicator UI renders this without
    /// needing to subscribe to the full edit history table.
    pub new_content: String,
}
