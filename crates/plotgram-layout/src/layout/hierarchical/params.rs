//! Typed hierarchical layout parameters + preset packs + AttrMap bind.
//!
//! [`LayoutContract`](plotgram_model::contract::LayoutContract) still carries a free
//! options map; this module is the sole place Hier turns that map into typed fields.

use plotgram_engine_api::LayoutError;
use plotgram_model::attr::AttrMap;

use crate::params::{BindError, BindWarning, OptionsBinder};

/// Layout orientation (core may implement TB only and rotate — Stage pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    TopToBottom,
    BottomToTop,
    LeftToRight,
    RightToLeft,
}

impl Orientation {
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::TopToBottom | Self::BottomToTop)
    }

    pub fn is_horizontal(self) -> bool {
        !self.is_vertical()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopToBottom => "top-to-bottom",
            Self::BottomToTop => "bottom-to-top",
            Self::LeftToRight => "left-to-right",
            Self::RightToLeft => "right-to-left",
        }
    }
}

/// Group contraction policy (profile parameter — not a second layouter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupPolicy {
    Weak,
    StrongMacro,
}

impl GroupPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weak => "weak",
            Self::StrongMacro => "strong-macro",
        }
    }
}

/// Same-rank group frame width policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSizing {
    Fit,
    Equal,
}

impl GroupSizing {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fit => "fit",
            Self::Equal => "equal",
        }
    }
}

/// Macro-row / inter-group alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupAlign {
    Start,
    Center,
    End,
}

impl GroupAlign {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
        }
    }
}

/// Built-in edge geometry style when Hier owns ink (`edge_routing` absent).
///
/// Distinct from diagram-level `edge_routing:` (independent EdgeRouter after layout).
/// Aligns with yFiles Hierarchical routing-style: orthogonal · polyline · octilinear · curved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingStyle {
    #[default]
    Orthogonal,
    Polyline,
    Octilinear,
    Curved,
}

impl RoutingStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Orthogonal => "orthogonal",
            Self::Polyline => "polyline",
            Self::Octilinear => "octilinear",
            Self::Curved => "curved",
        }
    }
}

/// Named packs that only replace a subset of [`HierarchicalParams`] defaults.
///
/// Not a diagram profile: does not change group policy / orientation semantics.
/// Order in bind: `Default` → `preset.apply` → explicit field overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HierarchicalPreset {
    #[default]
    Default,
    /// Tighter spacing defaults.
    Compact,
    /// Looser spacing defaults.
    Spacious,
}

impl HierarchicalPreset {
    pub fn from_atom(s: &str) -> Option<Self> {
        match s {
            "default" | "standard" => Some(Self::Default),
            "compact" => Some(Self::Compact),
            "spacious" => Some(Self::Spacious),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Compact => "compact",
            Self::Spacious => "spacious",
        }
    }

    /// Overlay this pack onto `params` (starts from [`HierarchicalParams::default`]).
    pub fn apply(self, params: &mut HierarchicalParams) {
        match self {
            Self::Default => {}
            Self::Compact => {
                params.node_gap = 16.0;
                params.layer_gap = 28.0;
                params.edge_gap = 12.0;
            }
            Self::Spacious => {
                params.node_gap = 32.0;
                params.layer_gap = 56.0;
                params.edge_gap = 20.0;
            }
        }
    }
}

/// Fully determined hierarchical parameters (no `Option` — bind fills every field).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HierarchicalParams {
    pub orientation: Orientation,
    /// Cross-axis gap between adjacent nodes in a layer.
    pub node_gap: f64,
    /// Main-axis gap between consecutive layers.
    pub layer_gap: f64,
    /// Reserved edge/track spacing demand (routing / measure).
    ///
    /// **Not consumed in this build** — no Channel/track machinery exists yet
    /// (mvp-scope note §2.5). Explicit DSL options are therefore rejected in
    /// [`Self::bind`] (architecture.md §1.1: a bindable-but-unconsumed
    /// parameter counts as unsupported); the field stays as the future
    /// demand channel and is only ever written by preset packs.
    pub edge_gap: f64,
    /// Built-in ink style (ignored when layout defers to an independent EdgeRouter).
    pub routing_style: RoutingStyle,
    pub group_policy: GroupPolicy,
    /// **Not consumed in this build** (no group-frame writer yet) — explicit
    /// options rejected in [`Self::bind`]; see `edge_gap` above.
    pub group_sizing: GroupSizing,
    /// **Not consumed in this build** — same treatment as `group_sizing`.
    pub group_align: GroupAlign,
}

impl Default for HierarchicalParams {
    fn default() -> Self {
        Self {
            orientation: Orientation::TopToBottom,
            node_gap: 24.0,
            layer_gap: 40.0,
            edge_gap: 16.0,
            routing_style: RoutingStyle::Orthogonal,
            group_policy: GroupPolicy::Weak,
            group_sizing: GroupSizing::Fit,
            group_align: GroupAlign::Center,
        }
    }
}

impl HierarchicalParams {
    /// Bind free-form DSL options → typed params.
    ///
    /// Order: algorithm [`Default`] ← optional `preset` overlays ← explicit fields.
    /// Unknown keys become warnings (not errors). Bad types / bad atoms → error.
    pub fn bind(options: &AttrMap) -> Result<BindResult, LayoutError> {
        // Options this build cannot consume must not bind silently
        // (architecture.md §1.1 + anti-pattern #13). Unsupported stays a hard
        // failure even though `LayoutDiagnostics` now exists — a warning is
        // reserved for non-fatal observations, never for unsupported options.
        for key in ["edge_gap", "edge_distance", "group_sizing", "group_align"] {
            if options.contains_key(key) {
                return Err(LayoutError::message(format!(
                    "hierarchical: option `{key}` is unsupported in this build \
                     (no consumer yet; see docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md §2.7)"
                )));
            }
        }

        let mut binder = OptionsBinder::new(options);

        let preset = match binder.get_atom("preset").map_err(bind_err)? {
            Some(raw) => HierarchicalPreset::from_atom(raw).ok_or_else(|| {
                LayoutError::message(format!(
                    "option `preset`: unknown value `{raw}` (expected one of: default, compact, spacious)"
                ))
            })?,
            None => HierarchicalPreset::Default,
        };

        let mut params = Self::default();
        preset.apply(&mut params);

        if let Some(o) = binder
            .get_enum_any(
                &["orientation", "direction"],
                &[
                    ("top-to-bottom", Orientation::TopToBottom),
                    ("ttb", Orientation::TopToBottom),
                    ("bottom-to-top", Orientation::BottomToTop),
                    ("btt", Orientation::BottomToTop),
                    ("left-to-right", Orientation::LeftToRight),
                    ("ltr", Orientation::LeftToRight),
                    ("right-to-left", Orientation::RightToLeft),
                    ("rtl", Orientation::RightToLeft),
                ],
            )
            .map_err(bind_err)?
        {
            params.orientation = o;
        }

        if let Some(v) = binder
            .get_f64_any(&["node_gap", "node_distance"])
            .map_err(bind_err)?
        {
            params.node_gap = v;
        }
        if let Some(v) = binder
            .get_f64_any(&["layer_gap", "layer_distance", "layer_to_layer_distance"])
            .map_err(bind_err)?
        {
            params.layer_gap = v;
        }

        if let Some(rs) = binder
            .get_enum(
                "routing_style",
                &[
                    ("orthogonal", RoutingStyle::Orthogonal),
                    ("polyline", RoutingStyle::Polyline),
                    ("octilinear", RoutingStyle::Octilinear),
                    ("curved", RoutingStyle::Curved),
                ],
            )
            .map_err(bind_err)?
        {
            params.routing_style = rs;
        }

        if let Some(p) = binder
            .get_enum(
                "group_policy",
                &[
                    ("weak", GroupPolicy::Weak),
                    ("strong-macro", GroupPolicy::StrongMacro),
                    ("strong_macro", GroupPolicy::StrongMacro),
                ],
            )
            .map_err(bind_err)?
        {
            params.group_policy = p;
        }
        Ok(BindResult {
            params,
            preset,
            warnings: binder.unknown_warnings(),
        })
    }

    /// Deterministic `params_hash` (roadmap phase C; architecture.md §3.4:
    /// attribute layout regressions to params vs code).
    ///
    /// FNV-1a 64 over a fixed-order canonical string — dependency-free and
    /// stable across platforms/toolchains (`std::hash::DefaultHasher` gives
    /// no such guarantee), WASM-safe. f64 fields use scientific notation so
    /// formatting never diverges between builds.
    pub fn hash(&self) -> String {
        let canonical = format!(
            "orientation={}|node_gap={:e}|layer_gap={:e}|edge_gap={:e}|\
             routing_style={}|group_policy={}|group_sizing={}|group_align={}",
            self.orientation.as_str(),
            self.node_gap,
            self.layer_gap,
            self.edge_gap,
            self.routing_style.as_str(),
            self.group_policy.as_str(),
            self.group_sizing.as_str(),
            self.group_align.as_str(),
        );
        format!("{:016x}", fnv1a_64(canonical.as_bytes()))
    }
}

/// FNV-1a 64-bit — small, deterministic, no deps (see [`HierarchicalParams::hash`]).
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Result of [`HierarchicalParams::bind`].
#[derive(Debug, Clone)]
pub struct BindResult {
    pub params: HierarchicalParams,
    /// Preset applied on top of [`HierarchicalParams::default`] (or `Default` if omitted).
    pub preset: HierarchicalPreset,
    pub warnings: Vec<BindWarning>,
}

fn bind_err(err: BindError) -> LayoutError {
    LayoutError::message(err.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::attr::AttrValue;

    fn options_with(keys: &[&str]) -> AttrMap {
        keys.iter()
            .map(|k| ((*k).to_string(), AttrValue::Num(1.0)))
            .collect()
    }

    #[test]
    fn params_hash_is_deterministic_and_param_sensitive() {
        let a = HierarchicalParams::default();
        assert_eq!(a.hash(), a.hash());
        assert_eq!(a.hash().len(), 16);
        assert!(a.hash().chars().all(|c| c.is_ascii_hexdigit()));

        // Any bound field change must move the hash (table over fields).
        let variants = [
            HierarchicalParams {
                node_gap: a.node_gap + 1.0,
                ..a
            },
            HierarchicalParams {
                layer_gap: a.layer_gap + 1.0,
                ..a
            },
            HierarchicalParams {
                orientation: Orientation::LeftToRight,
                ..a
            },
            HierarchicalParams {
                routing_style: RoutingStyle::Polyline,
                ..a
            },
            HierarchicalParams {
                group_policy: GroupPolicy::StrongMacro,
                ..a
            },
        ];
        for v in &variants {
            assert_ne!(a.hash(), v.hash(), "hash must track {v:?}");
        }
    }

    #[test]
    fn bind_surfaces_unknown_option_warnings() {
        // Table: unknown keys in → one warning per key out (BTreeMap order).
        let cases: &[(&[&str], &[&str])] = &[
            (&[], &[]),
            (&["bogus"], &["bogus"]),
            (&["aaa", "zzz"], &["aaa", "zzz"]),
        ];
        for (keys, expected) in cases {
            let bound = HierarchicalParams::bind(&options_with(keys)).unwrap();
            assert_eq!(bound.warnings.len(), expected.len(), "keys={keys:?}");
            for (w, key) in bound.warnings.iter().zip(*expected) {
                assert!(w.message.contains(key), "{}", w.message);
            }
        }
    }

    #[test]
    fn known_options_bind_without_warnings() {
        let mut options = AttrMap::new();
        options.insert("node_gap".to_string(), AttrValue::Num(30.0));
        let bound = HierarchicalParams::bind(&options).unwrap();
        assert!(bound.warnings.is_empty());
        assert_eq!(bound.params.node_gap, 30.0);
    }
}
