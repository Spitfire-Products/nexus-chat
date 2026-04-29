//! Room invitations for private rooms.
//!
//! When a member of a private room invites another user,
//! an invitation is created. The invitee can accept or decline.

/// An invitation to join a private room.
#[derive(Clone)]
#[spacetimedb::table(accessor = room_invitations, public)]
pub struct RoomInvitation {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// The private room
    #[index(btree)]
    pub room_id: String,

    /// Who sent the invitation (platform user_id)
    pub inviter_id: String,

    /// Who is being invited (platform user_id)
    #[index(btree)]
    pub invitee_id: String,

    /// Status: "pending", "accepted", "declined"
    pub status: String,

    /// Created timestamp (ms since epoch)
    pub created_at: u64,
}
