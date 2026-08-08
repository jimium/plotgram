//! Internal Plan-lite IR shared by the hierarchical core's phases.
//!
//! Not a public contract: these types never leave `layout::hierarchical`. Real
//! naming keeps everything keyed by the original graph's `String` ids (there is
//! no `NodeId`/`EdgeId` newtype upstream in `plotgram-model`) plus dense
//! `usize` indices for O(1) phase-internal lookups. Dense indices never leak
//! into tie-break comparisons — every sort that needs a stable key uses
//! [`ElemKey`] / declaration order, never an index.

use std::collections::BTreeMap;

use plotgram_model::port::PortConstraint;
use plotgram_model::NodeShape;

/// Secondary-axis clamp side for a group-boundary dummy (architecture §8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BoundarySide {
    Left,
    Right,
}

/// Stable identity for a node-shaped element in the working graph: a real
/// graph node, a long-edge properify dummy, or a group Left/Right clamp.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElemKey {
    Real(String),
    /// One rank-step of `edge_id`'s dummy chain. `ordinal` counts from the
    /// edge's working source (0 = first dummy after the source).
    Virtual {
        edge_id: String,
        ordinal: u32,
    },
    /// Zero-width Left/Right clamp for `(group × rank)`. `rank` is part of
    /// the key so the same group can own independent clamps on every layer.
    GroupBoundary {
        group: String,
        rank: u32,
        side: BoundarySide,
    },
    /// Zero-width order pad inserted so group Left clamps share a common
    /// raw layer index across ranks (Channel host tracks stay geometric).
    OrderPad {
        rank: u32,
        ordinal: u32,
    },
}

impl ElemKey {
    /// Long-edge corridor dummy only — group boundaries / pads are **not**
    /// virtual (BK / symmetry must not treat them as long-edge corridors).
    pub fn is_virtual(&self) -> bool {
        matches!(self, Self::Virtual { .. })
    }

    pub fn is_group_boundary(&self) -> bool {
        matches!(self, Self::GroupBoundary { .. })
    }

    pub fn is_zero_width(&self) -> bool {
        matches!(
            self,
            Self::Virtual { .. } | Self::GroupBoundary { .. } | Self::OrderPad { .. }
        )
    }
}

/// The graph of real nodes only (pre-properify), dense-indexed for FAS/rank.
///
/// Self-loops (`source == target`) are extracted out and never appear in
/// [`Self::edges`] — they do not participate in ranking/ordering (Compose
/// `SelfLoop` fact, expanded back to geometry only in Ink).
pub struct RealGraph {
    /// index -> node id, declaration order (`Graph::all_node_ids`).
    pub ids: Vec<String>,
    pub index_of: BTreeMap<String, usize>,
    /// index -> root..leaf group id path (empty = top-level).
    pub group_path: Vec<Vec<String>>,
    /// index -> resolved node shape (`None` on the IR → [`NodeShape::DEFAULT`]).
    pub shapes: Vec<NodeShape>,
    /// Declaration order; self-loops excluded.
    pub edges: Vec<RealEdge>,
    /// `(edge_id, node_idx)` for edges with `source == target`, declaration order.
    pub self_loops: Vec<(String, usize)>,
}

/// One real-to-real edge before properify.
#[derive(Debug, Clone)]
pub struct RealEdge {
    pub edge_id: String,
    /// Always `Graph::Edge` semantics: arrowhead, head/tail label, and the
    /// final `EdgePlacement` source/target must trace back to these, never to
    /// `working_*`.
    pub original_source: usize,
    pub original_target: usize,
    /// Cycle-removal working direction; may be the swap of original.
    pub working_source: usize,
    pub working_target: usize,
    pub reversed: bool,
    /// Author port pin, physical-frame side (as written in the DSL) —
    /// converted to canonical TB before any internal port decision reads it.
    pub from_port: Option<PortConstraint>,
    pub to_port: Option<PortConstraint>,
    /// Author critical-path mark → extra ordering / alignment weight
    /// (edge-parameters.md §2.5).
    pub critical: bool,
}

/// One node-shaped element after properify: a real node, edge dummy, or
/// group-boundary clamp.
#[derive(Debug, Clone)]
pub struct Elem {
    pub key: ElemKey,
    /// Root..leaf group id path; empty for top-level real nodes and for
    /// long-edge virtuals. Group-boundary clamps carry the path of the group
    /// they clamp (including that group as the leaf).
    pub group_path: Vec<String>,
    pub rank: u32,
}

/// One rank-to-rank hop of a (possibly multi-rank) edge, post-properify.
///
/// A short edge (span 1) has exactly one segment `ordinal = 0` connecting its
/// two real-node elements directly. A long edge has one segment per rank gap.
#[derive(Debug, Clone)]
pub struct Segment {
    pub edge_id: String,
    pub ordinal: u32,
    /// Elem index at the lower rank.
    pub from: usize,
    /// Elem index at the higher rank (`rank(to) == rank(from) + 1`).
    pub to: usize,
}

/// Properified working graph: dense elements + rank-adjacent segments.
pub struct PlanGraph {
    pub elems: Vec<Elem>,
    pub index_of: BTreeMap<ElemKey, usize>,
    /// Declaration-order tie-break for real elements (matches `RealGraph::ids`
    /// order); virtual elements sort after all reals, by `(edge_id, ordinal)`.
    pub decl_index: Vec<usize>,
    pub segments: Vec<Segment>,
    /// rank -> ordered element indices (order = declaration order pre-sort;
    /// [`crate::layout::hierarchical::compose::order`] rewrites this).
    pub layers: Vec<Vec<usize>>,
}
