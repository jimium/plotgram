//! StrongMacro group policy (SM-2 nested recursion): first stage the
//! semantic blocks (macro blocks), then place each block's nodes with the
//! **same** Hier stack; expansion normalizes everything into the identical
//! global `PlanGraph` schema the Weak path produces. `channel/` / `ink/`
//! never read `group_policy`; the hierarchical orchestrator may inject
//! Strong-specific hooks (`TailFrames::Fixed`, `group_obstacles`) into the
//! shared tail (strong-macro.md §5.1 / §5.3).
//!
//! Nested groups are macro blocks too: a container block's scope entries are
//! its child group blocks (plus one single-node block per direct member
//! node); each scope runs its own block-level FAS + super ranking
//! post-order, SM-C stacks scopes recursively, and finalize's
//! `union(members ∪ child frames) + pad` reproduces the whole frame tree
//! from content placed at `frame origin + pad` (one pad layer per level).
//!
//! Pipeline: SM-A intra (per leaf block) → SM-B super-graph ranking (per
//! scope, post-order) → SM-C macro-block placement (the sole group-frame
//! writer) → SM-D expand → shared Channel/Ink tail.

mod expand;
mod intra;
mod macro_block;
mod super_graph;

use std::collections::{BTreeMap, BTreeSet};

use plotgram_algo::orientation::{self as algo_orient, Size};
use plotgram_engine_api::{LayoutError, LayoutInput};
use plotgram_model::geometry::Rect;
use plotgram_model::graph::Group;

use crate::layout::hierarchical::compose::bundle::BundlePlan;
use crate::layout::hierarchical::compose::ports::EdgePorts;
use crate::layout::hierarchical::model::RealGraph;
use crate::layout::hierarchical::params::HierarchicalParams;

use intra::IntraResult;

/// One macro block (SM-2): a group block (leaf group or container of nested
/// group blocks) or a single ungrouped real node.
struct Block {
    /// `Some(group id)` for group blocks; `None` for single-node blocks.
    group_id: Option<String>,
    /// Scope entries of a container block (child group blocks + one block
    /// per direct member node), declaration order; empty for leaf / node
    /// blocks. Also the local super-graph node set.
    child_idx: Vec<usize>,
    /// Super rank **within the parent scope** (written by SM-B).
    super_rank: u32,
    /// Local solve (written by SM-A; leaf group + single-node blocks only).
    intra: IntraResult,
    /// Packed size + content pad (written by SM-C).
    width: f64,
    height: f64,
    content_pad: (f64, f64),
    /// Frame in the parent scope's content space (written by SM-C; global
    /// for top-scope blocks, whose parent space is the canvas origin).
    local_frame: Rect,
    /// Global content origin = global frame origin + pads (SM-C propagate).
    content_origin: (f64, f64),
    /// Group ids carrying a label (top-pad bookkeeping).
    labeled: BTreeSet<String>,
    /// Direct member global `RealGraph` indices (leaf group + node blocks;
    /// SM-A input; empty for containers).
    member_idx: Vec<usize>,
}

impl Block {
    fn leaf_group(gid: String, member_idx: Vec<usize>, labeled: &BTreeSet<String>) -> Self {
        Self {
            group_id: Some(gid),
            child_idx: Vec::new(),
            super_rank: 0,
            intra: IntraResult::default(),
            width: 0.0,
            height: 0.0,
            content_pad: (0.0, 0.0),
            local_frame: Rect::new(0.0, 0.0, 0.0, 0.0),
            content_origin: (0.0, 0.0),
            labeled: labeled.clone(),
            member_idx,
        }
    }

    fn container(gid: String, child_idx: Vec<usize>, labeled: &BTreeSet<String>) -> Self {
        Self {
            group_id: Some(gid),
            child_idx,
            super_rank: 0,
            intra: IntraResult::default(),
            width: 0.0,
            height: 0.0,
            content_pad: (0.0, 0.0),
            local_frame: Rect::new(0.0, 0.0, 0.0, 0.0),
            content_origin: (0.0, 0.0),
            labeled: labeled.clone(),
            member_idx: Vec::new(),
        }
    }

    fn single(gi: usize, labeled: &BTreeSet<String>) -> Self {
        Self {
            group_id: None,
            child_idx: Vec::new(),
            super_rank: 0,
            intra: IntraResult::default(),
            width: 0.0,
            height: 0.0,
            content_pad: (0.0, 0.0),
            local_frame: Rect::new(0.0, 0.0, 0.0, 0.0),
            content_origin: (0.0, 0.0),
            labeled: labeled.clone(),
            member_idx: vec![gi],
        }
    }

    fn is_container(&self) -> bool {
        !self.child_idx.is_empty()
    }
}

/// StrongMacro front: build the global plan + final canonical frames, then
/// hand off to the shared Channel/Ink tail (see `super::compute`). Returns
/// the expanded working graph (post `split_intra_layer`) for the tail.
pub(super) fn layout(
    input: LayoutInput<'_>,
    params: &HierarchicalParams,
    orientation: algo_orient::Orientation,
    mut real_graph: RealGraph,
    canonical_size: &[Size],
) -> Result<
    (
        RealGraph,
        crate::layout::hierarchical::model::PlanGraph,
        BTreeMap<String, EdgePorts>,
        Vec<BundlePlan>,
        Vec<plotgram_model::geometry::Rect>,
        Vec<plotgram_model::result::GroupPlacement>,
        BTreeSet<String>,
    ),
    LayoutError,
> {
    let labeled = super::collect_labeled_groups(&input.graph.groups);
    let size_by_id: BTreeMap<String, Size> = real_graph
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), canonical_size[i]))
        .collect();

    // Block tree (SM-2): the group tree post-order; nested groups are macro
    // blocks of their own. Top scope = root group blocks (declaration order)
    // + one block per ungrouped node.
    let mut blocks: Vec<Block> = Vec::new();
    let mut parent_of: BTreeMap<usize, usize> = BTreeMap::new();
    let mut block_of_node: Vec<usize> = vec![0; real_graph.ids.len()];
    let mut covered: Vec<bool> = vec![false; real_graph.ids.len()];
    let mut top_scope: Vec<usize> = Vec::new();
    for g in &input.graph.groups {
        if let Some(bi) = build_group_block(
            g,
            &real_graph,
            &labeled,
            &mut covered,
            &mut block_of_node,
            &mut parent_of,
            &mut blocks,
        ) {
            top_scope.push(bi);
        }
    }
    for gi in 0..real_graph.ids.len() {
        if covered[gi] {
            continue;
        }
        let bi = blocks.len();
        blocks.push(Block::single(gi, &labeled));
        block_of_node[gi] = bi;
        top_scope.push(bi);
    }

    if blocks.is_empty() {
        return Err(LayoutError::message(
            "hierarchical strong-macro: graph has no blocks (no nodes)",
        ));
    }

    // SM-B (per scope, post-order): block contraction can reintroduce cycles
    // at every scope; break deepest scopes first, then rank. Blocks were
    // pushed post-order, so index order == post-order. Along the way,
    // collect the cross-entry edge statistics SM-C consumes (SM-3).
    let mut flipped_ids: BTreeSet<String> = BTreeSet::new();
    let mut pair_stats: BTreeMap<Option<usize>, macro_block::ScopePairStats> = BTreeMap::new();
    let container_scopes: Vec<usize> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.is_container())
        .map(|(i, _)| i)
        .collect();
    for bi in container_scopes {
        let scope = blocks[bi].child_idx.clone();
        let slots = scope_slots(&scope, &parent_of, &block_of_node);
        flipped_ids.extend(super_graph::break_scope_cycles(&slots, &mut real_graph));
        let ranks = super_graph::assign_scope_ranks(&slots, scope.len(), &real_graph)?;
        for (&c, &r) in scope.iter().zip(&ranks) {
            blocks[c].super_rank = r;
        }
        pair_stats.insert(Some(bi), scope_pair_stats(&slots, &real_graph));
    }
    let top_slots = scope_slots(&top_scope, &parent_of, &block_of_node);
    flipped_ids.extend(super_graph::break_scope_cycles(&top_slots, &mut real_graph));
    let top_ranks = super_graph::assign_scope_ranks(&top_slots, top_scope.len(), &real_graph)?;
    for (&c, &r) in top_scope.iter().zip(&top_ranks) {
        blocks[c].super_rank = r;
    }
    pair_stats.insert(None, scope_pair_stats(&top_slots, &real_graph));

    // Endpoints of scope-flipped (loop) edges get dedicated local layers:
    // the side-corridor approach must never cross a same-layer sibling (the
    // Weak VPSC pulls such endpoints into their own column; StrongMacro
    // states it explicitly — strong-macro.md §5.2).
    let mut corridor_isolated: BTreeSet<String> = BTreeSet::new();
    for e in &real_graph.edges {
        if flipped_ids.contains(&e.edge_id) {
            corridor_isolated.insert(real_graph.ids[e.working_source].clone());
            corridor_isolated.insert(real_graph.ids[e.working_target].clone());
        }
    }

    // SM-A: per-leaf-block local solve on the (scope-)cycle-free working
    // graph. Containers carry no intra solve — SM-C derives their envelope
    // from the placed child frames.
    for b in blocks.iter_mut() {
        if b.is_container() {
            continue;
        }
        b.intra = intra::layout_intra(
            &real_graph,
            b.member_idx.as_slice(),
            &size_by_id,
            params,
            orientation,
            &corridor_isolated,
        )?;
    }

    // SM-C: macro-block writer places every frame (sole frame writer) —
    // including group frames (group-frame-d2.md §6.3: Strong must not stack a
    // second VPSC group-frame solve on top of MacroBlockWriter).
    macro_block::place_blocks(&mut blocks, &top_scope, params, &pair_stats);
    let group_placements = group_placements_from_blocks(&blocks);

    // SM-D: normalize to the global Plan schema. Port sides for intra-block
    // edges are owned by the local Metric solve; the global assign fills
    // cross-block edges and Ordered slots, then we restore those sides.
    let expanded = expand::expand(real_graph, &blocks, &top_scope, &block_of_node)?;
    let mut assignment = super::compose::ports::assign_ports(
        &expanded.real_graph,
        &expanded.plan,
        orientation,
        params.auto_edge_grouping,
    )?;
    reconcile_intra_port_sides(&blocks, &mut assignment.ports);

    // Canonical frames in plan-elem order for the shared tail.
    let canonical_frames = expanded.canonical_frames;

    Ok((
        expanded.real_graph,
        expanded.plan,
        assignment.ports,
        assignment.bundles,
        canonical_frames,
        group_placements,
        labeled,
    ))
}

/// Global group frames from the MacroBlockWriter (canonical TB). Block index
/// order is post-order DFS of the group forest — the same emission order as
/// the Weak Metric writer / finalize fallback.
fn group_placements_from_blocks(blocks: &[Block]) -> Vec<plotgram_model::result::GroupPlacement> {
    blocks
        .iter()
        .filter_map(|b| {
            let id = b.group_id.as_ref()?;
            // Global frame origin = content origin − pads (SM-C contract).
            let (ox, oy) = b.content_origin;
            let (px, py) = b.content_pad;
            Some(plotgram_model::result::GroupPlacement {
                id: id.clone(),
                frame: Rect::new(ox - px, oy - py, b.width, b.height),
            })
        })
        .collect()
}

/// Restore leaf-intra port **sides** onto the global assignment for edges
/// that participated in a local VPSC solve. Cross-block edges keep the
/// global decision; `along` / end-bus clusters stay global (they depend on
/// the expanded plan's neighbors and layer order).
fn reconcile_intra_port_sides(blocks: &[Block], ports: &mut BTreeMap<String, EdgePorts>) {
    for b in blocks {
        if b.is_container() {
            continue;
        }
        for (edge_id, local) in &b.intra.ports {
            if let Some(global) = ports.get_mut(edge_id) {
                global.source.side = local.source.side;
                global.target.side = local.target.side;
            }
        }
    }
}

/// Post-order group-tree walk building the block tree. Returns the group's
/// block index, or `None` when the group holds no real members (recursively)
/// — finalize draws no frame for empty groups either.
fn build_group_block(
    group: &Group,
    real_graph: &RealGraph,
    labeled: &BTreeSet<String>,
    covered: &mut [bool],
    block_of_node: &mut [usize],
    parent_of: &mut BTreeMap<usize, usize>,
    blocks: &mut Vec<Block>,
) -> Option<usize> {
    // Post-order: child group blocks first (they keep their own frames).
    let mut child_group_idxs = Vec::new();
    for child in &group.groups {
        if let Some(ci) = build_group_block(
            child,
            real_graph,
            labeled,
            covered,
            block_of_node,
            parent_of,
            blocks,
        ) {
            child_group_idxs.push(ci);
        }
    }
    let mut members = Vec::new();
    for n in &group.nodes {
        if let Some(&gi) = real_graph.index_of.get(&n.id) {
            members.push(gi);
        }
    }
    if members.is_empty() && child_group_idxs.is_empty() {
        return None;
    }

    let bi = blocks.len();
    if child_group_idxs.is_empty() {
        blocks.push(Block::leaf_group(
            group.id.clone(),
            members.clone(),
            labeled,
        ));
        for &gi in &members {
            covered[gi] = true;
            block_of_node[gi] = bi;
        }
    } else {
        // Container: direct member nodes become single-node scope entries
        // (declared first, mirroring the top-level order), then child group
        // blocks in declaration order.
        let mut entries = Vec::new();
        for &gi in &members {
            let ni = blocks.len();
            blocks.push(Block::single(gi, labeled));
            covered[gi] = true;
            block_of_node[gi] = ni;
            entries.push(ni);
        }
        entries.extend(child_group_idxs.iter().copied());
        blocks.push(Block::container(group.id.clone(), entries.clone(), labeled));
        for &c in &entries {
            parent_of.insert(c, bi);
        }
    }
    Some(bi)
}

/// Per-scope cross-entry edge statistics (SM-3): ordered slot pairs →
/// (count, weight sum). Deterministic edge-scan order; slot pairs keyed
/// `(lo, hi)`. Direction is irrelevant (gap demand + alignment pull).
fn scope_pair_stats(
    slots: &[Option<usize>],
    real_graph: &RealGraph,
) -> macro_block::ScopePairStats {
    let mut pairs: BTreeMap<(usize, usize), (usize, f64)> = BTreeMap::new();
    for e in &real_graph.edges {
        let (Some(a), Some(b)) = (slots[e.working_source], slots[e.working_target]) else {
            continue;
        };
        if a == b {
            continue;
        }
        let key = if a < b { (a, b) } else { (b, a) };
        let entry = pairs.entry(key).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += e.weight;
    }
    macro_block::ScopePairStats { pairs }
}

/// Per-node scope slot: `Some(k)` when the node's deepest block sits inside
/// `scope[k]`'s subtree, `None` for nodes outside the scope. Walking up
/// `parent_of` resolves nested descendants to their scope entry.
fn scope_slots(
    scope: &[usize],
    parent_of: &BTreeMap<usize, usize>,
    block_of_node: &[usize],
) -> Vec<Option<usize>> {
    let slot_of_block: BTreeMap<usize, usize> =
        scope.iter().enumerate().map(|(k, &b)| (b, k)).collect();
    block_of_node
        .iter()
        .map(|&d| {
            let mut cur = d;
            loop {
                if let Some(&k) = slot_of_block.get(&cur) {
                    return Some(k);
                }
                match parent_of.get(&cur) {
                    Some(&p) => cur = p,
                    None => return None,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::HierarchicalLayout;
    use plotgram_engine_api::{EdgeGeometryMode, LayoutAlgorithm};
    use plotgram_model::attr::{AttrMap, AttrValue};
    use plotgram_model::geometry::Size;
    use plotgram_model::graph::{Arrow, Edge, Graph, Node, NodeRole};
    use plotgram_model::sizes::NodeSizes;

    fn node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            label: None,
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs: AttrMap::new(),
        }
    }

    fn edge(id: &str, source: &str, target: &str) -> Edge {
        Edge {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            arrow: Arrow::Forward,
            label: None,
            head_label: None,
            tail_label: None,
            from_port: None,
            to_port: None,
            weight: None,
            undirected: false,
            attrs: AttrMap::new(),
        }
    }

    fn group(id: &str, label: &str, members: Vec<Node>) -> Group {
        Group {
            id: id.to_string(),
            label: Some(label.to_string()),
            attrs: AttrMap::new(),
            nodes: members,
            edges: vec![],
            groups: vec![],
        }
    }

    /// rest-api-backend shape: three groups chained edge → app → data.
    fn three_group_chain() -> (Graph, NodeSizes) {
        let graph = Graph {
            nodes: vec![],
            edges: vec![
                edge("e0", "web", "lb"),
                edge("e1", "lb", "api"),
                edge("e2", "api", "biz"),
                edge("e3", "biz", "db"),
            ],
            groups: vec![
                group("edge", "接入层", vec![node("web"), node("lb")]),
                group("app", "业务层", vec![node("api"), node("biz")]),
                group("data", "数据层", vec![node("db")]),
            ],
            partition: None,
        };
        let mut sizes = NodeSizes::new();
        for id in ["web", "lb", "api", "biz", "db"] {
            sizes.insert(id, Size::new(80.0, 32.0));
        }
        (graph, sizes)
    }

    /// mech.macro-nested shape: platform{ runtime{scheduler, executor},
    /// storage{meta_db} } + workload{ job_a, job_b }.
    fn nested_platform() -> (Graph, NodeSizes) {
        let runtime = Group {
            id: "runtime".to_string(),
            label: Some("运行时".to_string()),
            attrs: AttrMap::new(),
            nodes: vec![node("scheduler"), node("executor")],
            edges: vec![],
            groups: vec![],
        };
        let storage = Group {
            id: "storage".to_string(),
            label: Some("存储".to_string()),
            attrs: AttrMap::new(),
            nodes: vec![node("meta_db")],
            edges: vec![],
            groups: vec![],
        };
        let platform = Group {
            id: "platform".to_string(),
            label: Some("平台".to_string()),
            attrs: AttrMap::new(),
            nodes: vec![],
            edges: vec![],
            groups: vec![runtime, storage],
        };
        let graph = Graph {
            nodes: vec![],
            edges: vec![
                edge("e0", "job_a", "scheduler"),
                edge("e1", "job_b", "scheduler"),
                edge("e2", "scheduler", "executor"),
                edge("e3", "scheduler", "meta_db"),
                edge("e4", "executor", "meta_db"),
            ],
            groups: vec![
                platform,
                group("workload", "工作负载", vec![node("job_a"), node("job_b")]),
            ],
            partition: None,
        };
        let mut sizes = NodeSizes::new();
        for id in ["scheduler", "executor", "meta_db", "job_a", "job_b"] {
            sizes.insert(id, Size::new(80.0, 32.0));
        }
        (graph, sizes)
    }

    fn strong_options() -> AttrMap {
        let mut options = AttrMap::new();
        options.insert(
            "group_policy".to_string(),
            AttrValue::Str("strong-macro".to_string()),
        );
        options
    }

    fn layout_strong(graph: &Graph, sizes: &NodeSizes) -> plotgram_engine_api::LayoutOutput {
        layout_strong_with(graph, sizes, &strong_options())
    }

    fn layout_strong_with(
        graph: &Graph,
        sizes: &NodeSizes,
        options: &AttrMap,
    ) -> plotgram_engine_api::LayoutOutput {
        HierarchicalLayout
            .layout(LayoutInput {
                graph,
                node_sizes: sizes,
                options,
                edge_geometry: EdgeGeometryMode::Builtin,
            })
            .expect("strong-macro layout must succeed")
    }

    fn frame_center(out: &plotgram_engine_api::LayoutOutput, id: &str) -> (f64, f64) {
        let n = out.nodes.iter().find(|n| n.id == id).expect("node present");
        (
            n.frame.x + n.frame.width / 2.0,
            n.frame.y + n.frame.height / 2.0,
        )
    }

    #[test]
    fn strong_macro_bind_parses_both_spellings() {
        // Table: spelling → binds without error and changes the params hash.
        let base_hash = HierarchicalParams::default().hash();
        for spelling in ["strong-macro", "strong_macro"] {
            let bound = HierarchicalParams::bind(
                &[(
                    "group_policy".to_string(),
                    AttrValue::Str(spelling.to_string()),
                )]
                .into_iter()
                .collect(),
            )
            .expect("group_policy strong-macro must bind");
            assert_eq!(
                bound.params.group_policy,
                crate::layout::hierarchical::GroupPolicy::StrongMacro
            );
            assert_ne!(bound.params.hash(), base_hash, "spelling {spelling}");
        }
    }

    #[test]
    fn macro_align_weight_binds_and_hashes() {
        let base_hash = HierarchicalParams::default().hash();
        let bound = HierarchicalParams::bind(
            &[("macro_align_weight".to_string(), AttrValue::Num(0.0))]
                .into_iter()
                .collect(),
        )
        .expect("macro_align_weight must bind");
        assert_eq!(bound.params.macro_align_weight, 0.0);
        assert_ne!(bound.params.hash(), base_hash);
        // Negative clamps to zero.
        let bound = HierarchicalParams::bind(
            &[("macro_align_weight".to_string(), AttrValue::Num(-3.0))]
                .into_iter()
                .collect(),
        )
        .expect("negative macro_align_weight must bind");
        assert_eq!(bound.params.macro_align_weight, 0.0);
    }

    #[test]
    fn three_group_chain_stacks_vertically_centered_and_disjoint() {
        let (graph, sizes) = three_group_chain();
        let out = layout_strong(&graph, &sizes);

        let frame_of: BTreeMap<&str, plotgram_model::geometry::Rect> =
            out.nodes.iter().map(|n| (n.id.as_str(), n.frame)).collect();

        // Vertical stacking: 接入层 above 业务层 above 数据层.
        let group_top = |ids: &[&str]| {
            ids.iter()
                .map(|id| frame_of[*id].y)
                .fold(f64::INFINITY, f64::min)
        };
        let edge_top = group_top(&["web", "lb"]);
        let app_top = group_top(&["api", "biz"]);
        let data_top = group_top(&["db"]);
        assert!(edge_top < app_top, "接入层 must sit above 业务层");
        assert!(app_top < data_top, "业务层 must sit above 数据层");

        // Rows center-align: group centers deviate < 1px from the canvas
        // center for this symmetric fixture.
        let center_of = |ids: &[&str]| {
            let min_x = ids
                .iter()
                .map(|id| frame_of[*id].x)
                .fold(f64::INFINITY, f64::min);
            let max_x = ids
                .iter()
                .map(|id| frame_of[*id].right())
                .fold(f64::NEG_INFINITY, f64::max);
            (min_x + max_x) / 2.0
        };
        let centers = [
            center_of(&["web", "lb"]),
            center_of(&["api", "biz"]),
            center_of(&["db"]),
        ];
        let spread = centers.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - centers.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(spread < 1.0, "rows must center-align, spread = {spread}");

        // No node overlap.
        let frames: Vec<_> = out.nodes.iter().map(|n| n.frame).collect();
        for i in 0..frames.len() {
            for j in (i + 1)..frames.len() {
                let (a, b) = (&frames[i], &frames[j]);
                let overlap =
                    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom();
                assert!(
                    !overlap,
                    "nodes {} and {} overlap",
                    out.nodes[i].id, out.nodes[j].id
                );
            }
        }

        // Every edge present and endpoints exact on frames (verifiers ran).
        assert_eq!(out.edges.len(), 4);
    }

    #[test]
    fn strong_macro_runs_are_bit_identical() {
        let (graph, sizes) = three_group_chain();
        let a = layout_strong(&graph, &sizes);
        let b = layout_strong(&graph, &sizes);
        assert_eq!(a.nodes.len(), b.nodes.len());
        for (na, nb) in a.nodes.iter().zip(b.nodes.iter()) {
            assert_eq!(na.id, nb.id);
            assert_eq!(na.frame.x.to_bits(), nb.frame.x.to_bits());
            assert_eq!(na.frame.y.to_bits(), nb.frame.y.to_bits());
            assert_eq!(na.frame.width.to_bits(), nb.frame.width.to_bits());
            assert_eq!(na.frame.height.to_bits(), nb.frame.height.to_bits());
        }
        assert_eq!(a.edges.len(), b.edges.len());
        for (ea, eb) in a.edges.iter().zip(b.edges.iter()) {
            assert_eq!(ea.id, eb.id);
            assert_eq!(ea.path.samples().len(), eb.path.samples().len());
            for (pa, pb) in ea.path.samples().iter().zip(eb.path.samples().iter()) {
                assert_eq!(pa.x.to_bits(), pb.x.to_bits());
                assert_eq!(pa.y.to_bits(), pb.y.to_bits());
            }
        }
    }

    #[test]
    fn strong_macro_layout_matches_snapshot() {
        let (graph, sizes) = three_group_chain();
        let out = layout_strong(&graph, &sizes);
        let frames: Vec<serde_json::Value> = out
            .nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "x": n.frame.x,
                    "y": n.frame.y,
                    "w": n.frame.width,
                    "h": n.frame.height,
                })
            })
            .collect();
        insta::assert_json_snapshot!(frames);
    }

    #[test]
    fn nested_macro_stacking_containment_and_disjoint() {
        use crate::layout::hierarchical::group_frame::{GROUP_LABEL_TOP_PAD, GROUP_PAD};

        let (graph, sizes) = nested_platform();
        let out = layout_strong(&graph, &sizes);
        let frame_of: BTreeMap<&str, Rect> =
            out.nodes.iter().map(|n| (n.id.as_str(), n.frame)).collect();

        // Top scope: workload stacks above platform (job → scheduler edges).
        let top_of = |ids: &[&str]| {
            ids.iter()
                .map(|id| frame_of[*id].y)
                .fold(f64::INFINITY, f64::min)
        };
        let workload_top = top_of(&["job_a", "job_b"]);
        let platform_top = top_of(&["scheduler", "executor", "meta_db"]);
        assert!(
            workload_top < platform_top,
            "workload must sit above platform"
        );

        // Container scope: runtime rows above storage rows (local super
        // order), with sibling frames vertically disjoint.
        let runtime_bottom = ["scheduler", "executor"]
            .iter()
            .map(|id| frame_of[*id].bottom())
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            runtime_bottom < frame_of["meta_db"].y,
            "runtime must sit above storage"
        );

        // Nesting containment: the child group frame (member bbox + pads) as
        // finalize derives it must fit inside the parent group frame.
        let bbox_of = |ids: &[&str]| {
            let min_x = ids
                .iter()
                .map(|id| frame_of[*id].x)
                .fold(f64::INFINITY, f64::min);
            let min_y = ids
                .iter()
                .map(|id| frame_of[*id].y)
                .fold(f64::INFINITY, f64::min);
            let max_x = ids
                .iter()
                .map(|id| frame_of[*id].right())
                .fold(f64::NEG_INFINITY, f64::max);
            let max_y = ids
                .iter()
                .map(|id| frame_of[*id].bottom())
                .fold(f64::NEG_INFINITY, f64::max);
            Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
        };
        let runtime_bbox = bbox_of(&["scheduler", "executor"]);
        let platform_bbox = bbox_of(&["scheduler", "executor", "meta_db"]);
        let child_frame = Rect::new(
            runtime_bbox.x - GROUP_PAD,
            runtime_bbox.y - GROUP_LABEL_TOP_PAD,
            runtime_bbox.width + GROUP_PAD * 2.0,
            runtime_bbox.height + GROUP_LABEL_TOP_PAD + GROUP_PAD,
        );
        let parent_frame = Rect::new(
            platform_bbox.x - GROUP_PAD,
            platform_bbox.y - GROUP_LABEL_TOP_PAD,
            platform_bbox.width + GROUP_PAD * 2.0,
            platform_bbox.height + GROUP_LABEL_TOP_PAD + GROUP_PAD,
        );
        assert!(parent_frame.x <= child_frame.x, "child frame leaks left");
        assert!(parent_frame.y <= child_frame.y, "child frame leaks above");
        assert!(
            child_frame.right() <= parent_frame.right(),
            "child frame leaks right"
        );
        assert!(
            child_frame.bottom() <= parent_frame.bottom(),
            "child frame leaks below"
        );

        // No node overlap.
        let frames: Vec<_> = out.nodes.iter().map(|n| n.frame).collect();
        for i in 0..frames.len() {
            for j in (i + 1)..frames.len() {
                let (a, b) = (&frames[i], &frames[j]);
                let overlap =
                    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom();
                assert!(
                    !overlap,
                    "nodes {} and {} overlap",
                    out.nodes[i].id, out.nodes[j].id
                );
            }
        }

        // Every edge present (verifiers ran).
        assert_eq!(out.edges.len(), 5);
    }

    #[test]
    fn strong_macro_nested_runs_are_bit_identical() {
        let (graph, sizes) = nested_platform();
        let a = layout_strong(&graph, &sizes);
        let b = layout_strong(&graph, &sizes);
        assert_eq!(a.nodes.len(), b.nodes.len());
        for (na, nb) in a.nodes.iter().zip(b.nodes.iter()) {
            assert_eq!(na.id, nb.id);
            assert_eq!(na.frame.x.to_bits(), nb.frame.x.to_bits());
            assert_eq!(na.frame.y.to_bits(), nb.frame.y.to_bits());
            assert_eq!(na.frame.width.to_bits(), nb.frame.width.to_bits());
            assert_eq!(na.frame.height.to_bits(), nb.frame.height.to_bits());
        }
        assert_eq!(a.edges.len(), b.edges.len());
        for (ea, eb) in a.edges.iter().zip(b.edges.iter()) {
            assert_eq!(ea.id, eb.id);
            assert_eq!(ea.path.samples().len(), eb.path.samples().len());
            for (pa, pb) in ea.path.samples().iter().zip(eb.path.samples().iter()) {
                assert_eq!(pa.x.to_bits(), pb.x.to_bits());
                assert_eq!(pa.y.to_bits(), pb.y.to_bits());
            }
        }
    }

    #[test]
    fn strong_macro_nested_layout_matches_snapshot() {
        let (graph, sizes) = nested_platform();
        let out = layout_strong(&graph, &sizes);
        let frames: Vec<serde_json::Value> = out
            .nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "x": n.frame.x,
                    "y": n.frame.y,
                    "w": n.frame.width,
                    "h": n.frame.height,
                })
            })
            .collect();
        insta::assert_json_snapshot!(frames);
    }

    /// SM-3 fixture: g1{a} + g2{b} share row 0 (no edge between them);
    /// g3{c} sits in row 1 connected to `a` only.
    fn off_center_two_plus_one() -> (Graph, NodeSizes) {
        let graph = Graph {
            nodes: vec![],
            edges: vec![edge("e0", "a", "c")],
            groups: vec![
                group("g1", "G1", vec![node("a")]),
                group("g2", "G2", vec![node("b")]),
                group("g3", "G3", vec![node("c")]),
            ],
            partition: None,
        };
        let mut sizes = NodeSizes::new();
        for id in ["a", "b", "c"] {
            sizes.insert(id, Size::new(80.0, 32.0));
        }
        (graph, sizes)
    }

    #[test]
    fn macro_alignment_pulls_row_toward_connected_block() {
        let (graph, sizes) = off_center_two_plus_one();

        // Default weight: the single cross-row edge pulls g3's block center
        // onto g1's block center (exact for one term after the sweeps).
        let out = layout_strong(&graph, &sizes);
        let (cx_a, _) = frame_center(&out, "a");
        let (cx_c, _) = frame_center(&out, "c");
        assert!(
            (cx_c - cx_a).abs() < 1e-9,
            "g3 must align under g1: cx_c = {cx_c}, cx_a = {cx_a}"
        );

        // Weight 0 → SM-2 pure centering: g3 centered under the widest row,
        // i.e. its center sits on the row-0 frame center (derived from the
        // placed g1 / g2 frames, not assumed).
        let mut options = strong_options();
        options.insert("macro_align_weight".to_string(), AttrValue::Num(0.0));
        let out0 = layout_strong_with(&graph, &sizes, &options);
        let fa = out0.nodes.iter().find(|n| n.id == "a").unwrap().frame;
        let fb = out0.nodes.iter().find(|n| n.id == "b").unwrap().frame;
        let row_left = fa.x - 16.0; // g1 frame left = node left − GROUP_PAD
        let row_right = fb.right() + 16.0;
        let row_center = (row_left + row_right) / 2.0;
        let (cx_c0, _) = frame_center(&out0, "c");
        assert!(
            (cx_c0 - row_center).abs() < 1e-9,
            "weight 0 must keep centered placement: cx_c = {cx_c0}, row center {row_center}"
        );
        // And centering must not coincide with the alignment target here.
        let (cx_a0, _) = frame_center(&out0, "a");
        assert!(
            (cx_c0 - cx_a0).abs() > 1.0,
            "fixture must distinguish centering from alignment"
        );
    }

    #[test]
    fn macro_row_seam_gap_grows_with_edge_count_and_caps() {
        // Two groups g1{a} / g2{c}, k parallel edges a→c. The vertical node
        // clearance = GROUP_PAD (below a) + seam + label top pad (above c).
        let cases: &[(usize, f64)] = &[
            (1, 16.0 + 40.0 + 24.0),   // base seam
            (2, 16.0 + 56.0 + 24.0),   // +1 lane
            (3, 16.0 + 72.0 + 24.0),   // +2 lanes
            (5, 16.0 + 104.0 + 24.0),  // cap = 4 lanes
            (10, 16.0 + 104.0 + 24.0), // cap holds
        ];
        for &(k, want) in cases {
            let edges: Vec<Edge> = (0..k).map(|i| edge(&format!("e{i}"), "a", "c")).collect();
            let graph = Graph {
                nodes: vec![],
                edges,
                groups: vec![
                    group("g1", "G1", vec![node("a")]),
                    group("g2", "G2", vec![node("c")]),
                ],
                partition: None,
            };
            let mut sizes = NodeSizes::new();
            for id in ["a", "c"] {
                sizes.insert(id, Size::new(80.0, 32.0));
            }
            let out = layout_strong(&graph, &sizes);
            let ya = out.nodes.iter().find(|n| n.id == "a").unwrap().frame.y;
            let yc = out.nodes.iter().find(|n| n.id == "c").unwrap().frame.y;
            let clearance = yc - (ya + 32.0);
            assert!(
                (clearance - want).abs() < 1e-9,
                "k = {k}: clearance {clearance}, want {want}"
            );
        }
    }

    #[test]
    fn macro_row_gap_demand_from_nested_edges() {
        // nested_platform: 2 edges cross each seam (workload→platform and
        // runtime→storage), so both seams widen by one edge_gap lane.
        let (graph, sizes) = nested_platform();
        let out = layout_strong(&graph, &sizes);
        let y_of = |id: &str| out.nodes.iter().find(|n| n.id == id).unwrap().frame.y;
        // Top seam: workload member bottom → platform/runtime member top
        // spans GROUP_PAD + seam(40+16) + platform label pad + runtime
        // label pad (one pad layer per nesting level).
        let top_clearance = y_of("scheduler") - (y_of("job_a") + 32.0);
        assert!(
            (top_clearance - (16.0 + 56.0 + 24.0 + 24.0)).abs() < 1e-9,
            "top seam clearance {top_clearance}"
        );
    }

    #[test]
    fn macro_col_gap_grows_with_same_row_edge_count() {
        // g1{a1,a2,a3} / g2{b1,b2,b3} share a row (undirected edges impose
        // no rank); each count uses distinct node pairs so no two edges are
        // collinear. Horizontal frame clearance = col gap only (GROUP_PAD
        // lives inside each block frame).
        let all_pairs: Vec<(&str, &str)> = vec![
            ("a1", "b1"),
            ("a2", "b2"),
            ("a3", "b3"),
            ("a1", "b2"),
            ("a2", "b3"),
            ("a3", "b1"),
            ("a1", "b3"),
            ("a2", "b1"),
            ("a3", "b2"),
        ];
        let cases: &[(usize, f64)] = &[
            (1, 24.0), // GROUP_FRAME_GAP base
            (2, 40.0), // +1 lane
            (5, 88.0), // cap = 4 lanes
            (9, 88.0), // cap holds
        ];
        for &(k, want) in cases {
            let edges: Vec<Edge> = all_pairs[..k]
                .iter()
                .enumerate()
                .map(|(i, (s, t))| {
                    let mut e = edge(&format!("e{i}"), s, t);
                    e.undirected = true;
                    e
                })
                .collect();
            let graph = Graph {
                nodes: vec![],
                edges,
                groups: vec![
                    group("g1", "G1", vec![node("a1"), node("a2"), node("a3")]),
                    group("g2", "G2", vec![node("b1"), node("b2"), node("b3")]),
                ],
                partition: None,
            };
            let mut sizes = NodeSizes::new();
            for id in ["a1", "a2", "a3", "b1", "b2", "b3"] {
                sizes.insert(id, Size::new(80.0, 32.0));
            }
            let out = layout_strong(&graph, &sizes);
            let g1_right = ["a1", "a2", "a3"]
                .iter()
                .map(|id| {
                    out.nodes
                        .iter()
                        .find(|n| &n.id == id)
                        .unwrap()
                        .frame
                        .right()
                })
                .fold(f64::NEG_INFINITY, f64::max)
                + 16.0;
            let g2_left = ["b1", "b2", "b3"]
                .iter()
                .map(|id| out.nodes.iter().find(|n| &n.id == id).unwrap().frame.x)
                .fold(f64::INFINITY, f64::min)
                - 16.0;
            let clearance = g2_left - g1_right;
            assert!(
                (clearance - want).abs() < 1e-9,
                "k = {k}: clearance {clearance}, want {want}"
            );
        }
    }
}
