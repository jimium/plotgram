//! MetricBudget DemandBoard (channel-d1.md · D1.3.4 Corridor Demand).
//!
//! Channel / TrackOrder publish LayerGap lower bounds **before** Metric
//! consumes them. Freeze is the only legal hand-off (write-authority H3):
//! no publish after freeze, no Metric callback into the board.

use std::collections::{BTreeMap, BTreeSet};

use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;
use crate::layout::hierarchical::group_frame::{group_shell_bands, GROUP_FRAME_GAP};
use crate::layout::hierarchical::metric::partition_bands::{
    PartitionBandPlan, PARTITION_EMPTY_BAND_MIN,
};
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

/// Typed MetricBudget demand keys (coordinate-and-demand.md §1.1 subset).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DemandKey {
    /// Main-axis gap between layer `r` and `r+1` (px lower bound).
    LayerGap(u32),
    /// StrongMacro scope-local: main-axis gap between macro rows `r` and
    /// `r+1` (px lower bound; one board per scope — SM-3).
    MacroRowGap(u32),
    /// StrongMacro scope-local: cross-axis gap between adjacent entries
    /// `k` and `k+1` within one macro row (px lower bound; SM-3).
    MacroColGap(u32),
    /// Cross-axis minimum band width for a consumed partition column
    /// (px lower bound; partition-grid.md PG-1). Published for
    /// observability only — the cross solve reads the band plan
    /// directly, never through demand resolution.
    PartitionBandMinSize(String),
}

/// Max-merge demand board with a single freeze epoch (MetricBudget).
#[derive(Debug, Clone, Default)]
pub struct DemandBoard {
    values: BTreeMap<DemandKey, f64>,
    frozen: bool,
}

impl DemandBoard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Publish a lower bound. Same key keeps the max. Panics after freeze
    /// or when `lower_bound` is non-finite / negative (InternalInvariant).
    pub fn publish(&mut self, key: DemandKey, lower_bound: f64) {
        assert!(
            !self.frozen,
            "DemandBoard: publish after freeze (phase-order invariant)"
        );
        assert!(
            lower_bound.is_finite() && lower_bound >= 0.0,
            "DemandBoard: InternalInvariant — publish requires finite non-negative \
             lower_bound, got {lower_bound}"
        );
        self.values
            .entry(key)
            .and_modify(|v| *v = (*v).max(lower_bound))
            .or_insert(lower_bound);
    }

    /// Freeze MetricBudget — further [`publish`] is a contract violation.
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn get(&self, key: DemandKey) -> Option<f64> {
        self.values.get(&key).copied()
    }

    /// Published [`DemandKey::LayerGap`] lower bounds keyed by seam index
    /// (MetricVerifier demand floor; coordinate-and-demand.md §9).
    pub fn layer_gap_lower_bounds(&self) -> BTreeMap<u32, f64> {
        self.values
            .iter()
            .filter_map(|(k, v)| match k {
                DemandKey::LayerGap(seam) => Some((*seam, *v)),
                _ => None,
            })
            .collect()
    }
}

/// Channel producer: interior Cross lane counts → LayerGap px lower bounds.
///
/// Formula (params.md): `layer_gap + (track_count - 1) × edge_gap`.
/// Outer Cross lines (stack top/bottom) are **not** published — D1.3.1/2 keep
/// them rare, and inflating canvas padding from outer occupancy is rejected
/// by D1.3.4 (prefer raising interior seams only).
pub fn publish_channel_layer_gap_demand(
    board: &mut DemandBoard,
    track_order: &TrackOrderPlan,
    base_layer_gap: f64,
    edge_gap: f64,
) {
    for (&corridor, &count) in &track_order.rank_gap_track_counts {
        if count == 0 {
            continue;
        }
        let demand = base_layer_gap + (count.saturating_sub(1) as f64) * edge_gap;
        board.publish(DemandKey::LayerGap(corridor), demand);
    }
}

/// Group shell producer: rank gaps adjacent to a group frame edge must cover
/// the shell bands plus a corridor core, or frames derived by the engine's
/// finalize coincide / overlap and Cross lanes pierce the pad bands.
///
/// For every gap with a band: `demand = bands + max(GROUP_FRAME_GAP,
/// (track_count + 1) × edge_gap)` — the track term leaves one `edge_gap`
/// clearance on each side of the centered lanes. Sibling groups that *share*
/// a rank are separated on the cross axis instead (symmetry_objective clamp
/// separation), so they contribute no main-axis demand here.
pub fn publish_group_layer_gap_demand(
    board: &mut DemandBoard,
    plan: &PlanGraph,
    labeled: &BTreeSet<String>,
    track_order: &TrackOrderPlan,
    edge_gap: f64,
) {
    let bands = group_shell_bands(plan, labeled);
    for (r, &(below, above)) in bands.gap.iter().enumerate() {
        let pads = below + above;
        if pads <= 0.0 {
            continue;
        }
        let count = track_order
            .rank_gap_track_counts
            .get(&(r as u32))
            .copied()
            .unwrap_or(0);
        let core = ((count + 1) as f64 * edge_gap).max(GROUP_FRAME_GAP);
        board.publish(DemandKey::LayerGap(r as u32), pads + core);
    }
}

/// Resolved main-axis gap between layer `r` and `r+1` (length = n_layers - 1).
///
/// Requires a frozen board. Each gap is at least `base_layer_gap`, raised by
/// any published [`DemandKey::LayerGap`] lower bound.
pub fn resolved_layer_gaps(n_layers: usize, base_layer_gap: f64, board: &DemandBoard) -> Vec<f64> {
    assert!(
        board.is_frozen(),
        "DemandBoard: resolved_layer_gaps requires freeze"
    );
    if n_layers == 0 {
        return Vec::new();
    }
    let n_gaps = n_layers - 1;
    let mut gaps = vec![base_layer_gap; n_gaps];
    for r in 0..n_gaps {
        if let Some(d) = board.get(DemandKey::LayerGap(r as u32)) {
            gaps[r] = gaps[r].max(d);
        }
    }
    gaps
}

/// Cap on demand-driven gap expansion (in `edge_gap` lanes). Replaces the
/// Atlas-era empirical pixel constants (v1 `CROSS_EDGE_GROUP_GAP_SCALE = 8.0`
/// / `CROSS_EDGE_PAIR_VERTICAL_GAP_SCALE = 10.0` with `MAX_EXTRA = 56.0`):
/// lanes are priced at the shared `edge_gap` pitch and bounded, so demand
/// scales with routing vocabulary instead of magic numbers (SM-3). Gate
/// capacity reuses the same cap (group-frame-d2.md §8.11).
pub const MACRO_DEMAND_MAX_EXTRA_LANES: u32 = 4;

/// StrongMacro producer (SM-3): cross-group edge counts between macro rows /
/// adjacent row entries → gap lower bounds on a scope-local board.
///
/// `row_counts` maps seam index `r` (gap between macro rows `r` and `r+1`)
/// to the number of edges crossing it; `col_counts` maps the earlier slot
/// index of a **row-adjacent** entry pair (gap between that entry and the
/// next same-rank entry in declaration order) likewise. Formula per
/// gap: `base + min((count − 1) × edge_gap, 4 × edge_gap)` — one free lane
/// fits in the base gap, every extra edge buys one `edge_gap` lane, capped.
pub fn publish_macro_pair_demand(
    board: &mut DemandBoard,
    row_gap_base: f64,
    col_gap_base: f64,
    row_counts: &BTreeMap<u32, usize>,
    col_counts: &BTreeMap<u32, usize>,
    edge_gap: f64,
) {
    let extra = |count: usize| {
        ((count.saturating_sub(1)) as f64 * edge_gap)
            .min(MACRO_DEMAND_MAX_EXTRA_LANES as f64 * edge_gap)
    };
    for (&seam, &count) in row_counts {
        if count <= 1 {
            continue;
        }
        board.publish(DemandKey::MacroRowGap(seam), row_gap_base + extra(count));
    }
    for (&adj, &count) in col_counts {
        if count <= 1 {
            continue;
        }
        board.publish(DemandKey::MacroColGap(adj), col_gap_base + extra(count));
    }
}

/// Gate capacity producer (group-frame-d2.md §8.11): per-group main-axis
/// gate crossing counts → LayerGap lower bounds on the adjacent seams.
///
/// For each group the member rank extent `[min_r, max_r]` defines two
/// main-axis gates: the top frame in seam `min_r − 1`, the bottom frame in
/// seam `max_r`. An edge whose endpoints split inside/outside a group and
/// whose outside endpoint sits beyond that extent crosses the matching gate
/// (side-by-side crossings use cross-axis gates — no LayerGap demand).
/// Formula per seam, capped like Macro demand (**not** an unbounded
/// “capacity = crossing count” truth): one crossing fits the base gap, every
/// extra crossing buys one `edge_gap` lane, at most
/// [`MACRO_DEMAND_MAX_EXTRA_LANES`]. Max-merge keeps this a floor alongside
/// the channel / group-shell producers. Returns the number of seams that
/// published (§8.13 observability).
pub fn publish_gate_capacity_demand(
    board: &mut DemandBoard,
    plan: &PlanGraph,
    graph: &RealGraph,
    base_layer_gap: f64,
    edge_gap: f64,
) -> usize {
    let n_layers = plan.layers.len();
    let rank_of = |node_idx: usize| -> u32 {
        let id = &graph.ids[node_idx];
        let ei = plan.index_of[&ElemKey::Real(id.clone())];
        plan.elems[ei].rank
    };
    // Member rank extent per group id (BTreeMap → deterministic order).
    let mut extents: BTreeMap<&str, (u32, u32)> = BTreeMap::new();
    for (ni, path) in graph.group_path.iter().enumerate() {
        if path.is_empty() {
            continue;
        }
        let rank = rank_of(ni);
        for g in path {
            let e = extents.entry(g.as_str()).or_insert((rank, rank));
            e.0 = e.0.min(rank);
            e.1 = e.1.max(rank);
        }
    }
    // Crossing count per (group, seam).
    let mut counts: BTreeMap<(&str, u32), usize> = BTreeMap::new();
    for edge in &graph.edges {
        let s_path = &graph.group_path[edge.original_source];
        let t_path = &graph.group_path[edge.original_target];
        let s_rank = rank_of(edge.original_source);
        let t_rank = rank_of(edge.original_target);
        for (&g, &(min_r, max_r)) in &extents {
            let s_in = s_path.iter().any(|x| x == g);
            let t_in = t_path.iter().any(|x| x == g);
            if s_in == t_in {
                continue;
            }
            let r_out = if s_in { t_rank } else { s_rank };
            let seam = if r_out < min_r {
                if min_r == 0 {
                    continue;
                }
                min_r - 1
            } else if r_out > max_r {
                if max_r as usize >= n_layers.saturating_sub(1) {
                    continue;
                }
                max_r
            } else {
                // Outside endpoint within the extent → cross-axis gate.
                continue;
            };
            *counts.entry((g, seam)).or_insert(0) += 1;
        }
    }
    let mut published = 0usize;
    for (&(_, seam), &count) in &counts {
        if count <= 1 {
            continue;
        }
        let extra =
            ((count - 1) as f64 * edge_gap).min(MACRO_DEMAND_MAX_EXTRA_LANES as f64 * edge_gap);
        board.publish(DemandKey::LayerGap(seam), base_layer_gap + extra);
        published += 1;
    }
    published
}

/// Partition band producer (partition-grid.md PG-1): minimum band width for
/// consumed columns that have no assigned member anywhere. Non-empty columns
/// get their minimum size from members inside the cross solve and publish
/// nothing here. The cross solve reads [`PartitionBandPlan`] directly (hard
/// constraints in symmetry_objective), so this is observability only —
/// DemandBoard still resolves main-axis LayerGap seams exclusively.
pub fn publish_partition_band_demand(board: &mut DemandBoard, bands: &PartitionBandPlan) {
    for (ci, column) in bands.columns.iter().enumerate() {
        if bands.empty.get(ci).copied().unwrap_or(false) {
            board.publish(
                DemandKey::PartitionBandMinSize(column.clone()),
                PARTITION_EMPTY_BAND_MIN,
            );
        }
    }
}

/// Empty main-axis bands raise the dummy-rank seam to the empty-band floor
/// (partition-grid.md PG-3). Non-empty rows do not publish — stacking already
/// sizes them from member heights.
pub fn publish_partition_row_gap_demand(board: &mut DemandBoard, plan: &PlanGraph) {
    if plan.partition_rows.is_empty() {
        return;
    }
    let n_layers = plan.layers.len();
    for (ri, _) in plan.partition_rows.iter().enumerate() {
        let empty = plan
            .partition_elem_row
            .iter()
            .zip(plan.elems.iter())
            .all(|(slot, elem)| *slot != Some(ri) || !matches!(elem.key, ElemKey::Real(_)));
        if !empty {
            continue;
        }
        let (lo, _) = plan
            .partition_row_intervals
            .get(ri)
            .copied()
            .unwrap_or((0, 0));
        let r = lo as usize;
        if r + 1 < n_layers {
            board.publish(DemandKey::LayerGap(lo), PARTITION_EMPTY_BAND_MIN);
        } else if r > 0 {
            board.publish(
                DemandKey::LayerGap((r as u32) - 1),
                PARTITION_EMPTY_BAND_MIN,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::track_order::TrackOrderPlan;

    #[test]
    fn partition_band_demand_publishes_only_empty_columns() {
        let bands = PartitionBandPlan {
            columns: vec!["a".to_string(), "b".to_string()],
            gap: 24.0,
            empty: vec![false, true],
        };
        let mut board = DemandBoard::new();
        publish_partition_band_demand(&mut board, &bands);
        board.freeze();
        assert_eq!(
            board.get(DemandKey::PartitionBandMinSize("a".to_string())),
            None
        );
        assert_eq!(
            board.get(DemandKey::PartitionBandMinSize("b".to_string())),
            Some(PARTITION_EMPTY_BAND_MIN)
        );
    }

    #[test]
    fn expands_gap_by_track_pitch() {
        let mut plan = TrackOrderPlan::default();
        plan.rank_gap_track_counts.insert(0, 3);
        let mut board = DemandBoard::new();
        publish_channel_layer_gap_demand(&mut board, &plan, 40.0, 16.0);
        board.freeze();
        let gaps = resolved_layer_gaps(2, 40.0, &board);
        assert_eq!(gaps, vec![40.0 + 2.0 * 16.0]);
    }

    #[test]
    fn single_track_keeps_base() {
        let mut plan = TrackOrderPlan::default();
        plan.rank_gap_track_counts.insert(0, 1);
        let mut board = DemandBoard::new();
        publish_channel_layer_gap_demand(&mut board, &plan, 40.0, 16.0);
        board.freeze();
        let gaps = resolved_layer_gaps(2, 40.0, &board);
        assert_eq!(gaps, vec![40.0]);
    }

    #[test]
    fn layer_gap_demand_monotone_in_track_count() {
        let mut prev = 0.0;
        for count in 1..=6 {
            let mut plan = TrackOrderPlan::default();
            plan.rank_gap_track_counts.insert(0, count);
            let mut board = DemandBoard::new();
            publish_channel_layer_gap_demand(&mut board, &plan, 40.0, 16.0);
            board.freeze();
            let g = resolved_layer_gaps(2, 40.0, &board)[0];
            assert!(
                g + 1e-12 >= prev,
                "count {count}: gap {g} < previous {prev}"
            );
            prev = g;
        }
    }

    #[test]
    fn group_band_demand_reserves_frame_pads() {
        use crate::layout::hierarchical::model::{Elem, ElemKey};

        fn real(id: &str, rank: u32, group: &str) -> Elem {
            Elem {
                key: ElemKey::Real(id.into()),
                group_path: vec![group.into()],
                rank,
            }
        }
        // g1 ends at rank 0, g2 (labeled) starts at rank 1, g3 shares rank 1
        // with g2 → seam 0|1 carries both bands; g3's top band adds nothing
        // (unlabeled 16 < labeled 24).
        let elems = vec![real("a", 0, "g1"), real("b", 1, "g2"), real("c", 1, "g3")];
        let layers = vec![vec![0], vec![1, 2]];
        let plan = crate::layout::hierarchical::model::PlanGraph {
            elems,
            index_of: Default::default(),
            decl_index: vec![0, 1, 2],
            segments: Vec::new(),
            layers,
            ..Default::default()
        };
        let labeled: BTreeSet<String> = ["g2".to_string()].into_iter().collect();
        let mut board = DemandBoard::new();
        publish_group_layer_gap_demand(
            &mut board,
            &plan,
            &labeled,
            &TrackOrderPlan::default(),
            16.0,
        );
        board.freeze();
        // bottom band 16 + top band 24 + frame-gap core 24 = 64.
        assert_eq!(board.get(DemandKey::LayerGap(0)), Some(64.0));
        assert_eq!(resolved_layer_gaps(2, 40.0, &board), vec![64.0]);
    }

    #[test]
    fn group_band_demand_grows_with_tracks() {
        use crate::layout::hierarchical::model::{Elem, ElemKey};

        let elems = vec![
            Elem {
                key: ElemKey::Real("a".into()),
                group_path: vec!["g1".into()],
                rank: 0,
            },
            Elem {
                key: ElemKey::Real("b".into()),
                group_path: vec!["g2".into()],
                rank: 1,
            },
        ];
        let plan = crate::layout::hierarchical::model::PlanGraph {
            elems,
            index_of: Default::default(),
            decl_index: vec![0, 1],
            segments: Vec::new(),
            layers: vec![vec![0], vec![1]],
            ..Default::default()
        };
        let labeled = BTreeSet::new();
        let mut tracks = TrackOrderPlan::default();
        tracks.rank_gap_track_counts.insert(0, 2);
        let mut board = DemandBoard::new();
        publish_group_layer_gap_demand(&mut board, &plan, &labeled, &tracks, 16.0);
        board.freeze();
        // bands 32 + track core (2+1)*16 = 80.
        assert_eq!(board.get(DemandKey::LayerGap(0)), Some(80.0));
    }

    #[test]
    fn max_merge_keeps_higher_bound() {
        let mut board = DemandBoard::new();
        board.publish(DemandKey::LayerGap(0), 40.0);
        board.publish(DemandKey::LayerGap(0), 72.0);
        board.publish(DemandKey::LayerGap(0), 50.0);
        board.freeze();
        assert_eq!(board.get(DemandKey::LayerGap(0)), Some(72.0));
    }

    #[test]
    #[should_panic(expected = "publish after freeze")]
    fn publish_after_freeze_panics() {
        let mut board = DemandBoard::new();
        board.freeze();
        board.publish(DemandKey::LayerGap(0), 40.0);
    }

    #[test]
    #[should_panic(expected = "finite non-negative")]
    fn publish_rejects_non_finite_or_negative() {
        let mut board = DemandBoard::new();
        board.publish(DemandKey::LayerGap(0), f64::NAN);
    }

    #[test]
    #[should_panic(expected = "requires freeze")]
    fn resolve_before_freeze_panics() {
        let board = DemandBoard::new();
        let _ = resolved_layer_gaps(2, 40.0, &board);
    }

    #[test]
    fn macro_pair_demand_grows_with_edge_count_and_caps() {
        // Table: count → expected MacroRowGap / MacroColGap demand.
        let row_base = 40.0;
        let col_base = 24.0;
        let edge_gap = 16.0;
        let cases: &[(usize, f64, f64)] = &[
            (1, row_base, col_base), // single edge → no demand raise
            (2, row_base + 16.0, col_base + 16.0),
            (5, row_base + 64.0, col_base + 64.0), // cap = 4 lanes
            (99, row_base + 64.0, col_base + 64.0), // cap holds
        ];
        for &(count, want_row, want_col) in cases {
            let mut board = DemandBoard::new();
            let rows = [(0, count)].into_iter().collect();
            let cols = [(0, count)].into_iter().collect();
            publish_macro_pair_demand(&mut board, row_base, col_base, &rows, &cols, edge_gap);
            let row = board.get(DemandKey::MacroRowGap(0)).unwrap_or(row_base);
            let col = board.get(DemandKey::MacroColGap(0)).unwrap_or(col_base);
            assert_eq!(row, want_row, "row count {count}");
            assert_eq!(col, want_col, "col count {count}");
        }
    }

    /// Gate capacity fixture builder: `nodes[i] = (id, rank, group_path)`;
    /// edges reference node indices. Builds a minimal PlanGraph/RealGraph.
    fn gate_fixture(
        nodes: &[(&str, u32, &[&str])],
        edges: &[(usize, usize)],
    ) -> (
        crate::layout::hierarchical::model::PlanGraph,
        crate::layout::hierarchical::model::RealGraph,
    ) {
        use crate::layout::hierarchical::model::{Elem, ElemKey, RealEdge};

        let elems: Vec<Elem> = nodes
            .iter()
            .map(|(id, rank, groups)| Elem {
                key: ElemKey::Real((*id).into()),
                group_path: groups.iter().map(|g| (*g).to_string()).collect(),
                rank: *rank,
            })
            .collect();
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let n_layers = nodes.iter().map(|n| n.1).max().map(|m| m + 1).unwrap_or(0) as usize;
        let mut layers = vec![Vec::new(); n_layers];
        for (i, e) in elems.iter().enumerate() {
            layers[e.rank as usize].push(i);
        }
        let plan = crate::layout::hierarchical::model::PlanGraph {
            elems,
            index_of,
            decl_index: (0..nodes.len()).collect(),
            segments: Vec::new(),
            layers,
            ..Default::default()
        };
        let mut id_index = BTreeMap::new();
        for (i, (id, _, _)) in nodes.iter().enumerate() {
            id_index.insert((*id).to_string(), i);
        }
        let graph = crate::layout::hierarchical::model::RealGraph {
            ids: nodes.iter().map(|n| n.0.to_string()).collect(),
            index_of: id_index,
            group_path: nodes
                .iter()
                .map(|n| n.2.iter().map(|g| (*g).to_string()).collect())
                .collect(),
            shapes: vec![tautcore_model::NodeShape::DEFAULT; nodes.len()],
            edges: edges
                .iter()
                .enumerate()
                .map(|(i, &(s, t))| RealEdge {
                    edge_id: format!("e{i}"),
                    original_source: s,
                    original_target: t,
                    working_source: s,
                    working_target: t,
                    weight: 1.0,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        (plan, graph)
    }

    /// `count` outside nodes at rank 0, `count` group-g members at rank 1,
    /// one edge o_k → i_k per k — all crossing g's top gate at seam 0.
    fn gate_crossing_fixture(
        count: usize,
        tag: &str,
    ) -> (
        crate::layout::hierarchical::model::PlanGraph,
        crate::layout::hierarchical::model::RealGraph,
    ) {
        let g_slice: &[&str] = &["g"];
        let empty: &[&str] = &[];
        let mut nodes: Vec<(&str, u32, &[&str])> = Vec::new();
        let mut edges = Vec::new();
        for k in 0..count {
            let id: &'static str = Box::leak(format!("{tag}_o{count}_{k}").into_boxed_str());
            nodes.push((id, 0, empty));
        }
        for k in 0..count {
            let id: &'static str = Box::leak(format!("{tag}_i{count}_{k}").into_boxed_str());
            nodes.push((id, 1, g_slice));
            edges.push((k, count + k));
        }
        gate_fixture(&nodes, &edges)
    }

    #[test]
    fn gate_capacity_demand_table_and_cap() {
        // Table: crossings per gate → LayerGap demand. base = 40, edge_gap =
        // 16, cap = 4 lanes (Macro-aligned; §8.11).
        let cases: &[(usize, Option<f64>)] = &[
            (1, None), // single crossing → no demand raise
            (2, Some(40.0 + 16.0)),
            (5, Some(40.0 + 64.0)),  // cap = 4 lanes
            (99, Some(40.0 + 64.0)), // cap holds
        ];
        for &(count, want) in cases {
            let (plan, graph) = gate_crossing_fixture(count, "tab");
            let mut board = DemandBoard::new();
            publish_gate_capacity_demand(&mut board, &plan, &graph, 40.0, 16.0);
            assert_eq!(
                board.get(DemandKey::LayerGap(0)),
                want,
                "crossing count {count}"
            );
        }
    }

    #[test]
    fn gate_capacity_seam_mapping_and_nesting() {
        // g spans ranks 1..2 (nested h at rank 2). Edges: two enter g from
        // rank 0 (top seam 0), two leave g to rank 3 (bottom seam 2), one of
        // which crosses only h (h bottom gate, seam 2, count 1 → no raise).
        let nodes: Vec<(&str, u32, &[&str])> = vec![
            ("u0", 0, &[]),
            ("u1", 0, &[]),
            ("m1", 1, &["g"]),
            ("m2", 2, &["g", "h"]),
            ("d0", 3, &[]),
            ("d1", 3, &[]),
        ];
        let edges = vec![(0, 2), (1, 2), (3, 4), (3, 5)];
        let (plan, graph) = gate_fixture(&nodes, &edges);
        let mut board = DemandBoard::new();
        publish_gate_capacity_demand(&mut board, &plan, &graph, 40.0, 16.0);
        assert_eq!(board.get(DemandKey::LayerGap(0)), Some(56.0), "g top gate");
        assert_eq!(
            board.get(DemandKey::LayerGap(2)),
            Some(56.0),
            "g bottom gate"
        );
        assert_eq!(
            board.get(DemandKey::LayerGap(1)),
            None,
            "no interior crossing"
        );
    }

    #[test]
    fn gate_capacity_side_by_side_crossing_publishes_nothing() {
        // Outside endpoint within the member rank extent → cross-axis gate,
        // no LayerGap demand.
        let nodes: Vec<(&str, u32, &[&str])> = vec![("m0", 0, &["g"]), ("x0", 0, &[])];
        let edges = vec![(0, 1)];
        let (plan, graph) = gate_fixture(&nodes, &edges);
        let mut board = DemandBoard::new();
        publish_gate_capacity_demand(&mut board, &plan, &graph, 40.0, 16.0);
        assert!(board.layer_gap_lower_bounds().is_empty());
    }

    #[test]
    fn gate_capacity_monotone_in_crossing_count() {
        let mut prev = 0.0;
        for count in 1..=8usize {
            let (plan, graph) = gate_crossing_fixture(count, "mono");
            let mut board = DemandBoard::new();
            publish_gate_capacity_demand(&mut board, &plan, &graph, 40.0, 16.0);
            let got = board.get(DemandKey::LayerGap(0)).unwrap_or(40.0);
            assert!(got + 1e-12 >= prev, "count {count}: {got} < {prev}");
            prev = got;
        }
    }
}
