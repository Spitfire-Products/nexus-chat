//! Mention parsing utilities.
//!
//! Extracts @user, @role, @everyone, and @here mentions from message content.
//! Convention: <@user_id>, <@&role_id>, @everyone, @here

/// Parsed mention data from a message.
#[derive(Debug, Clone, Default)]
pub struct MentionData {
    /// List of mentioned user IDs (from `<@user_id>` patterns)
    pub user_ids: Vec<String>,
    /// List of mentioned role IDs (from `<@&role_id>` patterns)
    pub role_ids: Vec<String>,
    /// Whether @everyone was mentioned
    pub everyone: bool,
    /// Whether @here was mentioned
    pub here: bool,
}

/// Parse mentions from message content.
///
/// Recognized patterns:
/// - `<@user_id>` — user mention
/// - `<@&role_id>` — role mention
/// - `@everyone` — everyone mention
/// - `@here` — here mention (online users only)
pub fn parse_mentions(content: &str) -> MentionData {
    let mut data = MentionData::default();

    // Check @everyone and @here
    if content.contains("@everyone") {
        data.everyone = true;
    }
    if content.contains("@here") {
        data.here = true;
    }

    // Parse <@user_id> and <@&role_id> patterns
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            if chars.peek() == Some(&'@') {
                chars.next(); // consume '@'
                let is_role = chars.peek() == Some(&'&');
                if is_role {
                    chars.next(); // consume '&'
                }
                // Read until '>'
                let mut id = String::new();
                for c in chars.by_ref() {
                    if c == '>' {
                        break;
                    }
                    id.push(c);
                }
                if !id.is_empty() {
                    if is_role {
                        data.role_ids.push(id);
                    } else {
                        data.user_ids.push(id);
                    }
                }
            }
        }
    }

    // Deduplicate
    data.user_ids.sort();
    data.user_ids.dedup();
    data.role_ids.sort();
    data.role_ids.dedup();

    data
}

/// Serialize user IDs to JSON array string for storage.
pub fn user_ids_to_json(ids: &[String]) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let inner: Vec<String> = ids.iter().map(|id| format!("\"{}\"", id)).collect();
    format!("[{}]", inner.join(","))
}

/// Serialize role IDs to JSON array string for storage.
pub fn role_ids_to_json(ids: &[String]) -> String {
    user_ids_to_json(ids) // Same format
}
