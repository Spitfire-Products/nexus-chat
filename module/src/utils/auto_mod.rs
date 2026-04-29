//! Auto-moderation content checking.
//!
//! Evaluates message content against server auto-mod rules
//! and returns an action (allow, block, flag, or timeout).

use spacetimedb::{ReducerContext, Table};
use crate::tables::AutoModRule;
use crate::tables::auto_mod_rules::auto_mod_rules;

/// Result of auto-mod evaluation.
#[derive(Debug, Clone)]
pub enum AutoModAction {
    /// Message is allowed through.
    Allow,
    /// Message is blocked (not sent). Includes reason.
    Block(String),
    /// Message is flagged for moderator review. Includes reason.
    Flag(String),
    /// User should be timed out. Includes duration in seconds and reason.
    Timeout(u64, String),
}

/// Check message content against all enabled auto-mod rules for the server.
///
/// Rules are evaluated in order. The most severe action wins:
/// Timeout > Block > Flag > Allow
pub fn check_auto_mod(
    ctx: &ReducerContext,
    server_id: &str,
    room_id: &str,
    user_id: &str,
    content: &str,
) -> AutoModAction {
    let rules: Vec<AutoModRule> = ctx.db.auto_mod_rules().iter()
        .filter(|r| r.server_id == server_id && r.enabled)
        .collect();

    if rules.is_empty() {
        return AutoModAction::Allow;
    }

    let content_lower = content.to_lowercase();
    let mut worst_action = AutoModAction::Allow;

    for rule in &rules {
        // Check exemptions
        if is_exempt(&rule.exempt_roles, &rule.exempt_channels, ctx, server_id, room_id, user_id) {
            continue;
        }

        let triggered = match rule.rule_type.as_str() {
            "blocked_words" => check_blocked_words(&rule.config, &content_lower),
            "spam_filter" => check_spam_filter(&rule.config, content),
            "mention_limit" => check_mention_limit(&rule.config, content),
            "link_filter" => check_link_filter(&rule.config, content),
            "caps_filter" => check_caps_filter(&rule.config, content),
            _ => None,
        };

        if let Some(reason) = triggered {
            let action = parse_action(&rule.action, &reason);
            worst_action = more_severe(worst_action, action);
        }
    }

    worst_action
}

// =============================================================================
// Rule checkers
// =============================================================================

fn check_blocked_words(config_json: &str, content_lower: &str) -> Option<String> {
    // Config: {"words": ["bad1", "bad2"]}
    // Simple JSON parsing without serde
    let words = extract_json_string_array(config_json, "words");
    for word in &words {
        let word_lower = word.to_lowercase();
        if content_lower.contains(&word_lower) {
            return Some(format!("Blocked word: {}", word));
        }
    }
    None
}

fn check_spam_filter(config_json: &str, content: &str) -> Option<String> {
    // Config: {"max_repeated_chars": 10, "max_repeated_words": 5}
    let max_chars = extract_json_u64(config_json, "max_repeated_chars").unwrap_or(10);
    let max_words = extract_json_u64(config_json, "max_repeated_words").unwrap_or(5);

    // Check repeated characters (e.g., "aaaaaaaaaa")
    let mut repeat_count: u64 = 1;
    let chars: Vec<char> = content.chars().collect();
    for i in 1..chars.len() {
        if chars[i] == chars[i - 1] {
            repeat_count += 1;
            if repeat_count > max_chars {
                return Some(format!("Repeated character spam ({}+ repeated)", max_chars));
            }
        } else {
            repeat_count = 1;
        }
    }

    // Check repeated words
    let words: Vec<&str> = content.split_whitespace().collect();
    if words.len() >= 2 {
        let mut word_repeat: u64 = 1;
        for i in 1..words.len() {
            if words[i].eq_ignore_ascii_case(words[i - 1]) {
                word_repeat += 1;
                if word_repeat > max_words {
                    return Some(format!("Repeated word spam ({}+ repeated)", max_words));
                }
            } else {
                word_repeat = 1;
            }
        }
    }

    None
}

fn check_mention_limit(config_json: &str, content: &str) -> Option<String> {
    let max_mentions = extract_json_u64(config_json, "max_mentions").unwrap_or(5);
    let mention_count = content.matches("<@").count() as u64;
    if mention_count > max_mentions {
        return Some(format!("Too many mentions ({}/{})", mention_count, max_mentions));
    }
    None
}

fn check_link_filter(config_json: &str, content: &str) -> Option<String> {
    // Config: {"blocked_domains": ["spam.com"], "allow_all": false}
    let allow_all = extract_json_bool(config_json, "allow_all").unwrap_or(true);
    if !allow_all {
        // Simple URL detection
        if content.contains("http://") || content.contains("https://") {
            let blocked = extract_json_string_array(config_json, "blocked_domains");
            let content_lower = content.to_lowercase();
            for domain in &blocked {
                if content_lower.contains(&domain.to_lowercase()) {
                    return Some(format!("Blocked domain: {}", domain));
                }
            }
            if blocked.is_empty() && !allow_all {
                return Some("Links are not allowed in this server".to_string());
            }
        }
    }
    None
}

fn check_caps_filter(config_json: &str, content: &str) -> Option<String> {
    let max_caps_pct = extract_json_u64(config_json, "max_caps_percent").unwrap_or(70);
    let min_length = extract_json_u64(config_json, "min_length").unwrap_or(10) as usize;

    if content.len() < min_length {
        return None;
    }

    let total_alpha = content.chars().filter(|c| c.is_alphabetic()).count();
    if total_alpha == 0 {
        return None;
    }

    let upper_count = content.chars().filter(|c| c.is_uppercase()).count();
    let caps_pct = (upper_count * 100) / total_alpha;

    if caps_pct as u64 > max_caps_pct {
        return Some(format!("Excessive caps ({}%)", caps_pct));
    }

    None
}

// =============================================================================
// Helpers
// =============================================================================

fn is_exempt(
    exempt_roles_json: &str,
    exempt_channels_json: &str,
    ctx: &ReducerContext,
    server_id: &str,
    room_id: &str,
    user_id: &str,
) -> bool {
    // Check channel exemption
    let exempt_channels = extract_json_string_array(exempt_channels_json, "");
    if exempt_channels.iter().any(|c| c == room_id) {
        return true;
    }

    // Check role exemption
    let exempt_roles = extract_json_string_array(exempt_roles_json, "");
    if !exempt_roles.is_empty() {
        use crate::tables::member_roles::member_roles;
        let user_roles: Vec<String> = ctx.db.member_roles().iter()
            .filter(|mr| mr.server_id == server_id && mr.user_id == user_id)
            .map(|mr| mr.role_id.clone())
            .collect();
        if user_roles.iter().any(|r| exempt_roles.contains(r)) {
            return true;
        }
    }

    false
}

fn parse_action(action_str: &str, reason: &str) -> AutoModAction {
    match action_str {
        "block" => AutoModAction::Block(reason.to_string()),
        "flag" => AutoModAction::Flag(reason.to_string()),
        s if s.starts_with("timeout_") => {
            let duration: u64 = s[8..].parse().unwrap_or(60);
            AutoModAction::Timeout(duration, reason.to_string())
        }
        _ => AutoModAction::Block(reason.to_string()),
    }
}

fn more_severe(a: AutoModAction, b: AutoModAction) -> AutoModAction {
    let severity = |action: &AutoModAction| -> u8 {
        match action {
            AutoModAction::Allow => 0,
            AutoModAction::Flag(_) => 1,
            AutoModAction::Block(_) => 2,
            AutoModAction::Timeout(_, _) => 3,
        }
    };
    if severity(&b) > severity(&a) { b } else { a }
}

/// Simple JSON string array extractor (no serde dependency).
/// Handles: ["a", "b", "c"] or {"key": ["a", "b"]}
pub fn extract_json_string_array(json: &str, key: &str) -> Vec<String> {
    let search = if key.is_empty() {
        json.to_string()
    } else {
        // Find "key": [...] section
        let needle = format!("\"{}\"", key);
        if let Some(pos) = json.find(&needle) {
            json[pos..].to_string()
        } else {
            return Vec::new();
        }
    };

    let mut result = Vec::new();
    let mut in_string = false;
    let mut current = String::new();
    let mut found_bracket = false;

    for ch in search.chars() {
        if ch == '[' && !in_string {
            found_bracket = true;
            continue;
        }
        if !found_bracket {
            continue;
        }
        if ch == ']' && !in_string {
            break;
        }
        if ch == '"' {
            if in_string {
                result.push(current.clone());
                current.clear();
            }
            in_string = !in_string;
            continue;
        }
        if in_string {
            current.push(ch);
        }
    }

    result
}

/// Simple JSON number extractor.
fn extract_json_u64(json: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{}\"", key);
    let pos = json.find(&needle)?;
    let after = &json[pos + needle.len()..];
    // Skip : and whitespace
    let after = after.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    // Parse number
    let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    num_str.parse().ok()
}

/// Simple JSON bool extractor.
fn extract_json_bool(json: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{}\"", key);
    let pos = json.find(&needle)?;
    let after = &json[pos + needle.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        None
    }
}
