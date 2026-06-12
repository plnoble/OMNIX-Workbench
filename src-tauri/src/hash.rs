//! Shared hash utilities used across multiple modules.

/// Compute FNV-1a 64-bit hash of the given content.
/// Returns a hex-formatted string prefixed with "fnv-".
///
/// Used by:
/// - `sync_engine.rs` for skill content hash computation
/// - `code_graph.rs` for architecture graph file fingerprinting
pub fn fnv1a_hash(content: &str) -> String {
    let data = content.as_bytes();
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    format!("fnv-{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_deterministic() {
        let a = fnv1a_hash("hello world");
        let b = fnv1a_hash("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn test_fnv1a_different_inputs() {
        let a = fnv1a_hash("hello");
        let b = fnv1a_hash("world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_fnv1a_empty() {
        let h = fnv1a_hash("");
        assert!(h.starts_with("fnv-"));
    }
}
