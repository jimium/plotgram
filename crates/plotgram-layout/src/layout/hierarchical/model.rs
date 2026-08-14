//! Internal Plan-lite IR shared by the hierarchical core's phases.
//!
//! Not a public contract: these types never leave `layout::hierarchical`. Real
//! naming keeps everything keyed by the original graph's `String` ids (there is
//! no `NodeId`/`EdgeId` newtype upstream in `plotgram-model`) plus dense
//! `usize` indices for O(1) phase-internal lookups. Dense indices never leak
//! into tie-break comparisons — every sort that needs a stable key uses
//! [`ElemKey`] / declaration order, never an index.

use std::collections::BTreeMap;

use plotgram_model::partition::{PartitionCell, PartitionGrid};
use plotgram_model::port::PortConstraint;
use plotgram_model::NodeShape;

/// Secondary-axis clamp side for a group-boundary dummy (architecture §8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BoundarySide {
    Left,
    Right,
}

/// Which author axis is currently bound to kernel cross or main
/// (partition-grid.md §5.1). Default [`Self::Columns`] matches TB/BT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PartitionAxisKind {
    #[default]
    Columns,
    Rows,
}

impl PartitionAxisKind {
    pub fn as_noun(self) -> &'static str {
        match self {
            Self::Columns => "columns",
            Self::Rows => "rows",
        }
    }
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
    /// Zero-width Left/Right clamp for `(cross-axis id × rank)`
    /// (partition-grid.md PG-1/PG-4). Cross-axis bands are global
    /// full-height (canonical x); every id owns L/R clamps on **every**
    /// rank (unlike groups, which span only their member ranks). `axis` is
    /// the author column id (TB/BT) or row id (LR/RL).
    PartitionBoundary {
        axis: String,
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

    pub fn is_partition_boundary(&self) -> bool {
        matches!(self, Self::PartitionBoundary { .. })
    }

    /// Either clamp family (group / partition) — same ordering & metric
    /// treatment (zero-width, boundary-weight segments, nesting blocks).
    pub fn is_boundary(&self) -> bool {
        matches!(
            self,
            Self::GroupBoundary { .. } | Self::PartitionBoundary { .. }
        )
    }

    pub fn is_zero_width(&self) -> bool {
        matches!(
            self,
            Self::Virtual { .. }
                | Self::GroupBoundary { .. }
                | Self::PartitionBoundary { .. }
                | Self::OrderPad { .. }
        )
    }
}

/// The graph of real nodes only (pre-properify), dense-indexed for FAS/rank.
///
/// Self-loops (`source == target`) are extracted out and never appear in
/// [`Self::edges`] — they do not participate in ranking/ordering (Compose
/// `SelfLoop` fact, expanded back to geometry only in Ink). Undirected edges
/// with zero rank span follow the same bypass pattern into
/// [`Self::intra_layer`] (Compose `split_intra_layer`, Ink `intralayer`).
#[derive(Debug, Default)]
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
    /// Undirected edges whose endpoints landed on the same rank — excluded
    /// from ordering/properify/channel, routed as side-links in Ink.
    pub intra_layer: Vec<RealEdge>,
    /// Orthogonal partition grid (ADR-008; PG-0 input wiring). `None` = the
    /// diagram has no `partition` block — every consumer gates on this.
    pub partition: Option<PartitionGrid>,
    /// index -> author cell assignment (`cell_col` / `cell_row` after lift),
    /// parallel to `ids`. Never consulted when [`Self::partition`] is `None`.
    pub partition_cell: Vec<Option<PartitionCell>>,
}

/// One real-to-real edge before properify.
#[derive(Debug, Clone, Default)]
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
    /// Edge weight for layout optimization (NS / ordering / VPSC).
    /// Default 1.0; `critical: true` sugar lifts to 2.0.
    /// (edge-parameters.md §2.5).
    pub weight: f64,
    /// Non-hierarchical edge (yFiles `UNDIRECTED_EDGES`): imposes no rank
    /// constraint — skipped by FAS/ranking; span 0 → [`RealGraph::intra_layer`].
    pub undirected: bool,
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
#[derive(Debug, Default)]
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
    /// Consumed **cross-axis** ids, declaration order (partition-grid.md
    /// PG-1/PG-4). TB/BT → author columns; LR/RL → author rows. Empty =
    /// cross-axis NOT consumed: no `pb:` elems, no band constraints, no
    /// ordering special-casing — the §10 single gate.
    pub partition_columns: Vec<String>,
    /// Whether [`Self::partition_columns`] holds author columns or rows.
    pub partition_cross_kind: PartitionAxisKind,
    /// elem index -> index into [`Self::partition_columns`] for elems owned
    /// by a cross-axis block: assigned real nodes and every elem of a group
    /// block whose members sit in exactly one cross cell. `None` = free zone;
    /// elems appended after consumption (order pads) read past the end and
    /// are free by construction.
    pub partition_elem_col: Vec<Option<usize>>,
    /// Consumed **main-axis** ids, declaration order (PG-3/PG-4). TB/BT →
    /// author rows; LR/RL → author columns. Empty = main-axis NOT consumed.
    pub partition_rows: Vec<String>,
    /// Whether [`Self::partition_rows`] holds author rows or columns.
    pub partition_main_kind: PartitionAxisKind,
    /// elem index -> index into [`Self::partition_rows`].
    pub partition_elem_row: Vec<Option<usize>>,
    /// Inclusive `[lo, hi]` rank interval per [`Self::partition_rows`] entry.
    pub partition_row_intervals: Vec<(u32, u32)>,
}

impl RealGraph {
    /// `edge_id` → index into [`Self::edges`]. Built once per phase entry.
    pub fn edge_index_map(&self) -> BTreeMap<String, usize> {
        self.edges
            .iter()
            .enumerate()
            .map(|(i, e)| (e.edge_id.clone(), i))
            .collect()
    }
}

impl PlanGraph {
    /// `edge_id` → segment indices (declaration / properify order).
    pub fn segments_by_edge(&self) -> BTreeMap<String, Vec<usize>> {
        let mut m: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, s) in self.segments.iter().enumerate() {
            m.entry(s.edge_id.clone()).or_default().push(i);
        }
        m
    }

    /// Elem → position within its layer (`usize::MAX` if missing).
    pub fn layer_positions(&self) -> Vec<usize> {
        let mut pos = vec![usize::MAX; self.elems.len()];
        for layer in &self.layers {
            for (i, &e) in layer.iter().enumerate() {
                pos[e] = i;
            }
        }
        pos
    }
}
