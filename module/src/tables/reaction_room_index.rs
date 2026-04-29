//! reaction_room_index — non-null room_id projection of reactions for
//! room-scoped subscription filtering.
//!
//! `reactions.room_id` is `Option<String>` because it was added as a
//! denormalization after the table launched (per STDB 2.0 schema-change
//! rules, new fields must be Option<T>). STDB 2.0/2.1 subscriptions can
//! NOT filter on Option<T> columns — only `=` on non-null primitives —
//! so the denorm column was unfilterable, leaving every client subscribed
//! to every reaction across every room (TeV explosion).
//!
//! This shadow index carries `room_id: String` (non-null from creation),
//! so subscriptions can scope by room: `SELECT * FROM reaction_room_index
//! WHERE room_id = '...'`.
//!
//! Maintained by dual-write in `toggle_reaction` (and matching cascade
//! deletes in messages/rooms/ephemeral cleanup paths). The index carries
//! enough fields (message_id, user_id, emoji, created_at) to render the
//! reaction UI without joining back to `reactions`.
//!
//! grounded-charting-egret successor (`shadowing-stork` plan).

#[spacetimedb::table(accessor = reaction_room_index, public)]
pub struct ReactionRoomIndex {
    /// Same value as reactions.id — primary key on the source row.
    #[primary_key]
    pub reaction_id: String,

    /// Non-null. Indexed for room-scoped subscriptions.
    #[index(btree)]
    pub room_id: String,

    /// FK into messages — clients can group reactions by message without
    /// a join.
    pub message_id: String,

    /// Who reacted (platform user_id).
    pub user_id: String,

    /// Emoji identifier — same string the source reactions table uses.
    pub emoji: String,

    /// Mirror of reactions.created_at.
    pub created_at: u64,
}
