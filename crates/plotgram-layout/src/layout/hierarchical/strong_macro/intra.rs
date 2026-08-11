//! SM-A intra (SM-2): one leaf block's local solve via the shared Hier
//! stack — rank → properify → order → ports → main/cross — on the subgraph
//! induced by the block's members, taken from the post-FAS global
//! `RealGraph` (working direction, acyclic by construction). Containers get
//! no intra solve; their envelope comes from the placed child blocks
//! (SM-C). No second Sugiyama delegation stack (strong-macro.md §5.3 rule 3).

use std::collections::BTreeMap;

use plotgram_engine_api::LayoutError;
use plotgram_algo::orientation::Size;
use plotgram_model::geometry::Rect;

use crate::layout::hierarchical::compose;
use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::metric;
use crate::layout::hierarchical::model::{ElemKey, RealGraph};
use crate::layout::hierarchical::params::HierarchicalParams;

/// One block's local solution (canonical TB, local origin).
pub(super) struct IntraResult {
    /// Real-node local frames: local elem index → frame.
    pub local_real_frames: Vec<Rect>,
    /// Local real node id → local elem index.
    pub local_real_elem: BTreeMap<String, usize>,
    /// Per local layer: ordered **real** node ids (local Compose owns this
    /// order; the global expand must not rewrite it — write-authority).
    pub layer_order: Vec<Vec<String>>,
    /// Union of the real-node frames.
    pub content_bbox: Rect,
    /// Intra-block edge ports that fed the local VPSC solve. Expand reassigns
    /// ports on the global plan for Channel; same-leaf edges then reuse these
    /// **sides** so routing matches the coordinates Metric already wrote.
    pub ports: BTreeMap<String, EdgePorts>,
}

impl Default for IntraResult {
    fn default() -> Self {
        Self {
            local_real_frames: Vec::new(),
            local_real_elem: BTreeMap::new(),
            layer_order: Vec::new(),
            content_bbox: Rect::new(0.0, 0.0, 0.0, 0.0),
            ports: BTreeMap::new(),
        }
    }
}

/// Induce the dense subgraph over `keep` (global indices), preserving
/// declaration order and the post-FAS working direction.
fn induced_subgraph(real_graph: &RealGraph, keep: &[usize]) -> RealGraph {
    let keep_set: std::collections::BTreeSet<usize> = keep.iter().copied().collect();
    let mut ids = Vec::with_capacity(keep.len());
    let mut index_of = BTreeMap::new();
    let mut group_path = Vec::new();
    let mut shapes = Vec::new();
    let mut partition_cell = Vec::new();
    for &gi in keep {
        index_of.insert(real_graph.ids[gi].clone(), ids.len());
        ids.push(real_graph.ids[gi].clone());
        group_path.push(real_graph.group_path[gi].clone());
        shapes.push(real_graph.shapes[gi]);
        partition_cell.push(real_graph.partition_cell.get(gi).cloned().flatten());
    }
    let mut edges = Vec::new();
    for e in &real_graph.edges {
        if !(keep_set.contains(&e.working_source) && keep_set.contains(&e.working_target)) {
            continue;
        }
        let mut le = e.clone();
        le.original_source = index_of[&real_graph.ids[e.original_source]];
        le.original_target = index_of[&real_graph.ids[e.original_target]];
        le.working_source = index_of[&real_graph.ids[e.working_source]];
        le.working_target = index_of[&real_graph.ids[e.working_target]];
        edges.push(le);
    }
    let self_loops = real_graph
        .self_loops
        .iter()
        .filter(|(_, n)| keep_set.contains(n))
        .map(|(id, n)| (id.clone(), index_of[&real_graph.ids[*n]]))
        .collect();
    RealGraph {
        ids,
        index_of,
        group_path,
        shapes,
        edges,
        self_loops,
        intra_layer: Vec::new(),
        // Author facts pass through unchanged (intra plans still read cell;
        // global band writing happens once after expand — partition-grid §6.3).
        partition: real_graph.partition.clone(),
        partition_cell,
    }
}

/// Solve one block's local layout. `size_by_id` maps node id → canonical TB
/// size. `layer_order` records the local plan's per-layer real-node order
/// restricted to reals (virtuals stay local; the global plan re-properifies).
/// `isolate` lists loop-edge endpoints that must occupy dedicated layers
/// when they share one with siblings (side-corridor clearance; see the
/// caller).
pub(super) fn layout_intra(
    real_graph: &RealGraph,
    members: &[usize],
    size_by_id: &BTreeMap<String, Size>,
    params: &HierarchicalParams,
    orientation: plotgram_algo::orientation::Orientation,
    isolate: &std::collections::BTreeSet<String>,
) -> Result<IntraResult, LayoutError> {
    let mut local_graph = induced_subgraph(real_graph, members);
    let mut ranks = compose::rank::assign_ranks(&local_graph)?;
    // Corridor isolation: move each listed endpoint that shares its layer
    // with a sibling into its own fresh bottom layer (declaration order).
    let mut max_rank = ranks.iter().copied().max().unwrap_or(0);
    for (li, id) in local_graph.ids.iter().enumerate() {
        if !isolate.contains(id) {
            continue;
        }
        let shared = ranks
            .iter()
            .enumerate()
            .any(|(oi, &r)| oi != li && r == ranks[li]);
        if shared {
            max_rank += 1;
            ranks[li] = max_rank;
        }
    }
    compose::properify::split_intra_layer(&mut local_graph, &ranks);
    let mut plan = compose::properify::properify(&local_graph, &ranks);
    let edge_weights: BTreeMap<String, f64> = local_graph
        .edges
        .iter()
        .map(|e| (e.edge_id.clone(), e.weight))
        .collect();
    // No group-boundary dummies inside a block — intra order is written here
    // once; expand keeps it verbatim.
    compose::order::order_layers(&mut plan, &edge_weights, 0.0);
    let assignment = compose::ports::assign_ports(
        &local_graph,
        &plan,
        orientation,
        params.auto_edge_grouping,
    )?;

    let size_of = |elem_idx: usize| -> Size {
        match &plan.elems[elem_idx].key {
            ElemKey::Real(id) => size_by_id[id],
            _ => Size::new(0.0, 0.0),
        }
    };
    let prelim_gaps: Vec<f64> = vec![params.layer_gap; plan.layers.len().saturating_sub(1)];
    let main = metric::main_axis::assign_main_axis(
        &plan,
        &size_of,
        &prelim_gaps,
        params.layer_alignment,
    );
    // `params.group_policy == StrongMacro` disables the Weak-only drawn-frame
    // compact inside the objective (symmetry_objective guard).
    let cross = metric::cross_axis::assign_cross_axis(
        &plan,
        &local_graph,
        &assignment.ports,
        &size_of,
        &main,
        params,
    )
    .map_err(|e| LayoutError::message(format!("hierarchical strong-macro: intra VPSC solve failed: {e}")))?;

    let frame_of = |i: usize| {
        let s = size_of(i);
        Rect::new(cross[i] - s.width / 2.0, main[i], s.width, s.height)
    };

    let mut local_real_elem = BTreeMap::new();
    let mut local_real_frames = Vec::new();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (i, elem) in plan.elems.iter().enumerate() {
        let ElemKey::Real(id) = &elem.key else {
            continue;
        };
        let f = frame_of(i);
        local_real_elem.insert(id.clone(), local_real_frames.len());
        local_real_frames.push(f);
        min_x = min_x.min(f.x);
        min_y = min_y.min(f.y);
        max_x = max_x.max(f.right());
        max_y = max_y.max(f.bottom());
    }

    let mut layer_order = Vec::with_capacity(plan.layers.len());
    for layer in &plan.layers {
        let mut ids = Vec::new();
        for &e in layer {
            if let ElemKey::Real(id) = &plan.elems[e].key {
                ids.push(id.clone());
            }
        }
        layer_order.push(ids);
    }
    // A block always has at least one member (empty groups are skipped).
    let content_bbox = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
    Ok(IntraResult {
        local_real_frames,
        local_real_elem,
        layer_order,
        content_bbox,
        ports: assignment.ports,
    })
}
