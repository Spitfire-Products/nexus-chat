//! Cryptographic helpers for data obfuscation.
//!
//! Used to hash sensitive fields (like blocked user IDs) before storing in
//! public tables. The hash is deterministic so both server and client can
//! compute the same output for lookup/comparison.
//!
//! This is NOT cryptographic security — it's obfuscation to prevent casual
//! data exposure. A determined attacker with access to the algorithm and
//! the user ID space could brute-force matches.

/// Hash a blocked user ID for storage in the `user_blocks` table.
///
/// Uses a MurmurHash3-inspired finalizer seeded with the blocker's ID,
/// producing a 16-char hex string. The blocker_id acts as a per-user salt.
///
/// # Arguments
/// * `blocker_id` - The user who is doing the blocking (salt)
/// * `blocked_id` - The user being blocked (sensitive value to obfuscate)
///
/// # Returns
/// A 16-character hex string (64-bit hash).
///
/// # Important
/// The matching TypeScript implementation lives in ChatProvider.tsx.
/// Both MUST produce identical output for the same inputs.
/// Only safe for ASCII inputs (user IDs are ULID-format, always ASCII).
pub fn hash_blocked_id(blocker_id: &str, blocked_id: &str) -> String {
    let input = format!("{}::{}", blocker_id, blocked_id);
    let mut h1: u32 = 0xdeadbeef;
    let mut h2: u32 = 0x41c6ce57;

    for &b in input.as_bytes() {
        h1 = (h1 ^ b as u32).wrapping_mul(2654435761);
        h2 = (h2 ^ b as u32).wrapping_mul(1597334677);
    }

    // Avalanche / finalization
    h1 = (h1 ^ (h1 >> 16)).wrapping_mul(2246822507);
    h1 ^= (h2 ^ (h2 >> 13)).wrapping_mul(3266489909);
    h2 = (h2 ^ (h2 >> 16)).wrapping_mul(2246822507);
    h2 ^= (h1 ^ (h1 >> 13)).wrapping_mul(3266489909);

    format!("{:08x}{:08x}", h2, h1)
}
