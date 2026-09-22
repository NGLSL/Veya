//! Stable content hash for InternalClipboardWrite matching and aggregation keys.

/// FNV-1a 64-bit, hex-encoded. Deterministic and dependency-free.
pub fn content_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
