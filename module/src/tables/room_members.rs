//! Room membership and roles.
//!
//! Tracks which users are in which rooms and their role within each room.
//! Roles: "member", "admin", "banned"

/// Room membership record.
#[spacetimedb::table(accessor = room_members, public)]
pub struct RoomMember {
    /// Client-generated UUID
    #[primary_key]
    pub id: String,

    /// Room ID
    #[index(btree)]
    pub room_id: String,

    /// Platform user ID
    #[index(btree)]
    pub user_id: String,

    /// Role: "member", "admin", "banned"
    pub role: String,

    /// When the user joined (ms since epoch)
    pub joined_at: u64,
}
