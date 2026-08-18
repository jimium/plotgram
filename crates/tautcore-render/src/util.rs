//! Small shared helpers for SVG string building.

/// Deterministic hash from an id string plus a salt (sketch jitter seeds).
///
/// Salt separates element families so a node and an edge sharing an id
/// prefix don't jitter identically (nodes: 0, edges: 7, groups: 13).
pub fn hash_id(id: &str, salt: u64) -> u64 {
    let mut h = salt;
    for b in id.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    h
}

/// Escape XML special characters in text / attribute values.
pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
