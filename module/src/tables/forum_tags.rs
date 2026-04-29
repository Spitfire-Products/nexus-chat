//! Forum tags and post-tag assignments.

/// A tag defined on a forum channel.
#[spacetimedb::table(accessor = forum_tags, public)]
pub struct ForumTag {
    #[primary_key]
    pub id: String,

    /// FK to rooms.id (the forum room)
    #[index(btree)]
    pub room_id: String,

    /// Tag display name
    pub name: String,

    /// Optional emoji shortcode for the tag
    pub emoji: Option<String>,

    /// Optional hex color
    pub color: Option<String>,

    /// Display order
    pub sort_order: u32,
}

/// Links a forum post (thread room) to a tag.
#[spacetimedb::table(accessor = forum_post_tags, public)]
pub struct ForumPostTag {
    /// Composite key: "{thread_room_id}-{tag_id}"
    #[primary_key]
    pub id: String,

    /// FK to rooms.id (the thread room created as a forum post)
    #[index(btree)]
    pub thread_room_id: String,

    /// FK to forum_tags.id
    pub tag_id: String,
}
