//! Typed hierarchical layout parameters + preset packs + AttrMap bind.
//!
//! [`LayoutContract`](tautcore_model::contract::LayoutContract) still carries a free
//! options map; this module is the sole place Hier turns that map into typed fields.

use tautcore_engine_api::LayoutError;
use tautcore_model::attr::AttrMap;

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

/// What to do with nodes that have no `partition_cell` when a grid is present
/// (partition-grid.md PG-4). Default [`Self::Free`] is the ADR-008 free zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PartitionUnassigned {
    #[default]
    Free,
    Reject,
}

impl PartitionUnassigned {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Reject => "reject",
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

/// How the cross-axis solver places `x` (symmetry-axis.md).
///
/// `Ipsep` is the main path: unconstrained descent on J, then VPSC
/// projection of that step's `x` (not median-as-shared-desired).
/// `Median` is the old desired-packer, kept for A/B.
/// `BkIdeal` is the A0 diagnostic: project Brandes–Köpf ideal through the
/// hard constraint set (incl. chain identity) and skip iterate + fan snap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SymmetryPlace {
    #[default]
    Ipsep,
    Median,
    BkIdeal,
}

impl SymmetryPlace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ipsep => "ipsep",
            Self::Median => "median",
            Self::BkIdeal => "bk",
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
    /// Within-layer main-axis alignment of shorter nodes: `0` = top of band,
    /// `0.5` = center, `1` = bottom (yFiles `layerAlignment`).
    pub layer_alignment: f64,
    /// Track pitch / edge-corridor spacing (D1.0 Channel TrackOrder).
    ///
    /// Consumed as pitch between parallel horizontal tracks and as
    /// MetricBudget LayerGap demand: `layer_gap + (track_count-1)×edge_gap`.
    pub edge_gap: f64,
    /// Weight of cross-rank segments between matching group-boundary dummies
    /// (architecture §8.3). Default 16.0 — stronger than virtual-virtual (8).
    pub group_boundary_weight: f64,
    /// Symmetry objective weight on `|x_h − center_h|` (P4).
    pub lambda_sym: f64,
    /// Median/VPSC boost for twin (2-cycle) spine pairs.
    pub twin_spine_boost: f64,
    /// Median/VPSC boost for exclusive 1:1 spine pairs.
    pub primary_arm_boost: f64,
    /// Exclusive-stem length gain α in `ψ = 1 + α log2(L)` (capped at 2).
    pub stem_length_gain: f64,
    /// Fan-out hop extra weight β in `φ = 1 + β · mass(child) / Σ siblings`.
    pub fan_mass_gain: f64,
    /// J / L2 boost for a long-edge RV segment whose real end is a dangling
    /// sink (no forward children). The hanging sink follows the dummy
    /// corridor (expectations: long same-column back-edges use the dummy
    /// column). Forward sources and through-nodes stay unboosted so a
    /// reverse long edge cannot yank an exclusive 1:1 stem.
    pub chain_end_boost: f64,
    /// Fixed iteration budget for symmetry objective solver.
    pub symmetry_iters: u32,
    /// Cross-axis placer. Default `Median`. `BkIdeal` is a
    /// diagnostic bypass (not a product preset).
    pub symmetry_place: SymmetryPlace,
    /// Built-in ink style (ignored when layout defers to an independent EdgeRouter).
    pub routing_style: RoutingStyle,
    /// Automatic edge grouping (edge-parameters.md §2.3 / yFiles
    /// `automaticEdgeGrouping`): same-source fan-out / same-target fan-in
    /// share one port + bus trunk (SharedPort → Trunk → Bus → Stub).
    /// Default off.
    pub auto_edge_grouping: bool,
    /// Minimum length of the first segment leaving the source port (pixels).
    /// `0` = off. Soft preference in Channel search (D1.2).
    pub min_first_segment: f64,
    /// Minimum length of the last segment entering the target port (pixels).
    /// `0` = off.
    pub min_last_segment: f64,
    /// Normal stub length emitted from a port before the path jogs onto its
    /// first track (pixels). Ink clamps it against the adjacent gap so the
    /// stub never overshoots the layer gap / node gap (ink-and-verification.md §4).
    pub port_stub: f64,
    /// Channel search bend weight factor: `w_bend = route_w_bend · edge_gap` (P5-4).
    pub route_w_bend: f64,
    /// Channel search length weight (P5-4).
    pub route_w_len: f64,
    /// Channel search crossing weight factor: `w_cross = route_w_cross · edge_gap`.
    pub route_w_cross: f64,
    /// Soft bend budget; exceeding emits a relaxation (P5-5).
    pub max_bends_budget: u32,
    pub group_policy: GroupPolicy,
    /// Nodes without a cell when a partition grid is present: free zone
    /// (default) or hard reject (partition-grid.md PG-4).
    pub partition_unassigned: PartitionUnassigned,
    /// StrongMacro only: weight of the cross-group edge alignment term
    /// `Σ w_ab × ((o_a + cx_a) − (o_b + cx_b))²` on macro row offsets
    /// (strong-macro.md §6 SM-3). `0` = pure centering (SM-2 shape). The
    /// Weak path never reads this field.
    pub macro_align_weight: f64,
}

impl Default for HierarchicalParams {
    fn default() -> Self {
        Self {
            orientation: Orientation::TopToBottom,
            node_gap: 24.0,
            layer_gap: 40.0,
            layer_alignment: 0.5,
            edge_gap: 16.0,
            group_boundary_weight: 16.0,
            lambda_sym: 1.0,
            twin_spine_boost: 8.0,
            primary_arm_boost: 4.0,
            stem_length_gain: 0.5,
            fan_mass_gain: 1.0,
            chain_end_boost: 8.0,
            symmetry_iters: 8,
            symmetry_place: SymmetryPlace::Ipsep,
            routing_style: RoutingStyle::Orthogonal,
            auto_edge_grouping: false,
            min_first_segment: 0.0,
            min_last_segment: 0.0,
            port_stub: 12.0,
            route_w_bend: 10.0,
            route_w_len: 1.0,
            route_w_cross: 3.0,
            max_bends_budget: 6,
            group_policy: GroupPolicy::Weak,
            partition_unassigned: PartitionUnassigned::Free,
            macro_align_weight: 1.0,
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
        if options.contains_key("bus_routing") {
            return Err(LayoutError::message(
                "hierarchical: option `bus_routing` was removed — it mirrored a \
                 yFiles Layout Styles *demo* checkbox (heuristic GridComponent / \
                 BusDescriptor fill), not a HierarchicalLayout API. Use \
                 `auto_edge_grouping` for Hier bus-style edge grouping; explicit \
                 grid/bus substructures are a separate future feature"
                    .to_string(),
            ));
        }
        if options.contains_key("edge_grouping") {
            return Err(LayoutError::message(
                "hierarchical: option `edge_grouping` was renamed to `auto_edge_grouping`"
                    .to_string(),
            ));
        }
        if options.contains_key("cluster_pitch") {
            return Err(LayoutError::message(
                "hierarchical: option `cluster_pitch` was removed; \
                 `auto_edge_grouping` uses yFiles bus geometry (shared port + trunk), \
                 stub spacing comes from target columns"
                    .to_string(),
            ));
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
        if let Some(v) = binder.get_f64_any(&["layer_alignment"]).map_err(bind_err)? {
            params.layer_alignment = v.clamp(0.0, 1.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["edge_gap", "edge_distance"])
            .map_err(bind_err)?
        {
            params.edge_gap = v;
        }
        if let Some(v) = binder
            .get_f64_any(&["group_boundary_weight"])
            .map_err(bind_err)?
        {
            params.group_boundary_weight = v;
        }
        if let Some(v) = binder.get_f64_any(&["lambda_sym"]).map_err(bind_err)? {
            params.lambda_sym = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["twin_spine_boost"])
            .map_err(bind_err)?
        {
            params.twin_spine_boost = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["primary_arm_boost"])
            .map_err(bind_err)?
        {
            params.primary_arm_boost = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["stem_length_gain"])
            .map_err(bind_err)?
        {
            params.stem_length_gain = v.max(0.0);
        }
        if let Some(v) = binder.get_f64_any(&["fan_mass_gain"]).map_err(bind_err)? {
            params.fan_mass_gain = v.max(0.0);
        }
        if let Some(v) = binder.get_f64_any(&["chain_end_boost"]).map_err(bind_err)? {
            params.chain_end_boost = v.max(0.0);
        }
        if let Some(v) = binder.get_f64_any(&["symmetry_iters"]).map_err(bind_err)? {
            params.symmetry_iters = v.round().max(1.0) as u32;
        }
        if let Some(place) = binder
            .get_enum(
                "symmetry_place",
                &[
                    ("ipsep", SymmetryPlace::Ipsep),
                    ("median", SymmetryPlace::Median),
                    ("bk", SymmetryPlace::BkIdeal),
                ],
            )
            .map_err(bind_err)?
        {
            params.symmetry_place = place;
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

        if let Some(v) = binder.get_bool("auto_edge_grouping").map_err(bind_err)? {
            params.auto_edge_grouping = v;
        }

        if let Some(v) = binder
            .get_f64_any(&["min_first_segment"])
            .map_err(bind_err)?
        {
            params.min_first_segment = v;
        }
        if let Some(v) = binder
            .get_f64_any(&["min_last_segment"])
            .map_err(bind_err)?
        {
            params.min_last_segment = v;
        }
        if let Some(v) = binder.get_f64_any(&["port_stub"]).map_err(bind_err)? {
            params.port_stub = v.max(0.0);
        }
        if let Some(v) = binder.get_f64_any(&["route_w_bend"]).map_err(bind_err)? {
            params.route_w_bend = v.max(0.0);
        }
        if let Some(v) = binder.get_f64_any(&["route_w_len"]).map_err(bind_err)? {
            params.route_w_len = v.max(0.0);
        }
        if let Some(v) = binder.get_f64_any(&["route_w_cross"]).map_err(bind_err)? {
            params.route_w_cross = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["max_bends_budget"])
            .map_err(bind_err)?
        {
            params.max_bends_budget = v.round().max(0.0) as u32;
        }

        // Invalid combination (edge-parameters.md §2.4 discipline): grouping
        // geometry is defined for the orthogonal main path only.
        if params.auto_edge_grouping && matches!(params.routing_style, RoutingStyle::Octilinear) {
            return Err(LayoutError::message(
                "hierarchical: `auto_edge_grouping` is unsupported with routing_style `octilinear` \
                 (grouping geometry is defined for the orthogonal main path only)"
                    .to_string(),
            ));
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
        if let Some(p) = binder
            .get_enum(
                "partition_unassigned",
                &[
                    ("free", PartitionUnassigned::Free),
                    ("reject", PartitionUnassigned::Reject),
                ],
            )
            .map_err(bind_err)?
        {
            params.partition_unassigned = p;
        }
        if let Some(v) = binder
            .get_f64_any(&["macro_align_weight"])
            .map_err(bind_err)?
        {
            params.macro_align_weight = v.max(0.0);
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
            "orientation={}|node_gap={:e}|layer_gap={:e}|layer_alignment={:e}|edge_gap={:e}|\
             group_boundary_weight={:e}|lambda_sym={:e}|twin_spine_boost={:e}|\
             primary_arm_boost={:e}|stem_length_gain={:e}|fan_mass_gain={:e}|\
             chain_end_boost={:e}|symmetry_iters={}|\
             symmetry_place={}|\
             routing_style={}|auto_edge_grouping={}|\
             min_first_segment={:e}|min_last_segment={:e}|port_stub={:e}|\
             route_w_bend={:e}|route_w_len={:e}|route_w_cross={:e}|max_bends_budget={}|\
             group_policy={}|partition_unassigned={}|macro_align_weight={:e}",
            self.orientation.as_str(),
            self.node_gap,
            self.layer_gap,
            self.layer_alignment,
            self.edge_gap,
            self.group_boundary_weight,
            self.lambda_sym,
            self.twin_spine_boost,
            self.primary_arm_boost,
            self.stem_length_gain,
            self.fan_mass_gain,
            self.chain_end_boost,
            self.symmetry_iters,
            self.symmetry_place.as_str(),
            self.routing_style.as_str(),
            self.auto_edge_grouping,
            self.min_first_segment,
            self.min_last_segment,
            self.port_stub,
            self.route_w_bend,
            self.route_w_len,
            self.route_w_cross,
            self.max_bends_budget,
            self.group_policy.as_str(),
            self.partition_unassigned.as_str(),
            self.macro_align_weight,
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
    use tautcore_model::attr::AttrValue;

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
            HierarchicalParams {
                partition_unassigned: PartitionUnassigned::Reject,
                ..a
            },
            HierarchicalParams {
                auto_edge_grouping: true,
                ..a
            },
            HierarchicalParams {
                min_first_segment: 8.0,
                ..a
            },
            HierarchicalParams {
                min_last_segment: 8.0,
                ..a
            },
            HierarchicalParams {
                port_stub: a.port_stub + 1.0,
                ..a
            },
            HierarchicalParams {
                group_boundary_weight: a.group_boundary_weight + 1.0,
                ..a
            },
            HierarchicalParams {
                layer_alignment: 1.0,
                ..a
            },
            HierarchicalParams {
                lambda_sym: a.lambda_sym + 0.5,
                ..a
            },
            HierarchicalParams {
                symmetry_place: SymmetryPlace::Median,
                ..a
            },
            HierarchicalParams {
                symmetry_place: SymmetryPlace::BkIdeal,
                ..a
            },
            HierarchicalParams {
                twin_spine_boost: a.twin_spine_boost + 1.0,
                ..a
            },
            HierarchicalParams {
                chain_end_boost: a.chain_end_boost + 1.0,
                ..a
            },
            HierarchicalParams {
                stem_length_gain: a.stem_length_gain + 0.25,
                ..a
            },
            HierarchicalParams {
                fan_mass_gain: a.fan_mass_gain + 0.5,
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
