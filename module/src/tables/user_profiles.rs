//! Extended user profiles — about me, banners, pronouns.

/// Extended profile information for a chat user.
#[spacetimedb::table(accessor = user_profiles, public)]
pub struct UserProfile {
    /// Platform user_id (1:1 with chat_users)
    #[primary_key]
    pub user_id: String,

    /// About me text (max 190 chars, Discord limit)
    pub about_me: Option<String>,

    /// Hex color for profile banner background
    pub banner_color: Option<String>,

    /// Base64-encoded banner image (optional, large)
    pub banner_data: Option<String>,

    /// Pronouns (e.g. "he/him", "she/her", "they/them")
    pub pronouns: Option<String>,

    /// Hex color for UI accent
    pub accent_color: Option<String>,

    /// Last updated timestamp (ms since epoch)
    pub updated_at: u64,
}
