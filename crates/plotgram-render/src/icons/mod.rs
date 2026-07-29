//! Icon system: resolve and render node decoration icons.
//!
//! Resolution priority: `icon: none` → explicit `icon: <id>` → kind inference → none.

pub mod catalog;
pub mod render;

pub use catalog::{IconCategory, IconDef};
pub use render::IconLayout;

use plotgram_model::graph::Node;

/// Resolve the icon for a node, considering shape compatibility.
///
/// 1. `icon: none` → None
/// 2. `icon: <id>` → explicit lookup (by id or alias)
/// 3. `kind` → inference table
/// 4. Shape compatibility check (skip icon if incompatible)
pub fn resolve_icon(node: &Node, shape: &str) -> Option<&'static IconDef> {
    // Check explicit icon attribute
    if let Some(icon_val) = node.attrs.get("icon").and_then(|v| v.as_str()) {
        if icon_val == "none" {
            return None;
        }
        if let Some(icon) = catalog::icon_by_key(icon_val) {
            if render::is_compatible(icon, shape) {
                return Some(icon);
            }
            // Explicit icon but incompatible shape → skip
            return None;
        }
    }

    // Infer from kind
    if let Some(kind) = node.kind() {
        if let Some(icon) = catalog::icon_for_kind(kind) {
            if render::is_compatible(icon, shape) {
                return Some(icon);
            }
        }
    }

    None
}
