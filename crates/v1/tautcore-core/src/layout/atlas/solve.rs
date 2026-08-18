//! Atlas 度量相核心（Stage 2–5）。
//!
//! Stage 5：入口经 [`solve_from_contract`]；flowchart / architecture 共用 Hierarchical
//! Dialect。Weak = 原 group_divide；StrongMacro = 原 two_phase。

use std::collections::{BTreeMap, HashMap};

use crate::ast::Diagram;
use crate::layout::algorithm_config::{ArchitectureV2LayoutConfig, SugiyamaLayoutConfig};
use crate::layout::atlas::channel_metric::{
    build_channel_metric_from_slots_with_opts, build_channel_metric_with_opts,
    inflate_layer_gaps, ChannelMetric, PhaseIEdgeOrder,
};
use crate::layout::atlas::plan::Plan;
use crate::layout::atlas::dialect::contraction::{self, weak as group_divide};
use crate::layout::atlas::dialect::{
    compile_hierarchical, GroupPolicy, GroupSizing, HierarchicalContract, HierarchicalPreset,
    HierarchicalProfile, Scheme,
};
use crate::layout::atlas::plan::Slot;
use crate::layout::atlas::relaxation::{RelaxLevel, RelaxationLadder};
use crate::layout::kernel::coordinate::channel_ir::{
    expand_layer_order_gaps, expand_nodes_by_cross_gap_demands, publish_cross_track_coords,
    publish_main_track_coords,
};
use crate::layout::kernel::coordinate::main_axis::solve_main_axis_with_cross_tracks;
use crate::layout::kernel::coordinate::model::{CoordinateProblem, SolverStatus};
use crate::layout::kernel::coordinate::session::LayoutSession;
use crate::layout::kernel::group::bounds::GroupPadding;
use crate::layout::kernel::layered::coordinate::assign_coordinates_brandes_koepf_with_main_tops;
use crate::layout::kernel::layered::layered_kernel::{LayeredDraft, LayeredKernel};
use crate::layout::kernel::layered::postprocess;
use crate::layout::kernel::layered::preset::{self, SugiyamaPreset};
use crate::layout::types::{EdgeRoutingStyle, LayoutHints, LayoutResult};
use crate::layout::{GroupTable, NodeLayout};
use crate::types::DiagramType;

/// 分治路径单缝抬升上限（避免全局 rank 上 lane 峰值把画布撑爆）。
const DIVIDE_MAX_GAP_BAND: f64 = 8.0 * 2.0 + 4.0 * 18.0; // 2*clearance + 4*pitch ≈ 88

fn cap_gap_demands(dem: &BTreeMap<usize, f64>, cap: f64) -> BTreeMap<usize, f64> {
    dem.iter()
        .map(|(&k, &v)| (k, v.min(cap)))
        .collect()
}

/// Atlas 度量相输出。
pub struct AtlasSolveOutput {
    pub nodes: HashMap<String, NodeLayout>,
    pub groups: GroupTable,
    pub cross_problem: Option<CoordinateProblem>,
    /// Main 轴问题（含 Track 变量）；metric 成功时 flat/weak/strong 均为 Some（R3-1）。
    pub main_problem: Option<CoordinateProblem>,
    /// 度量相通道选路产物（Stage 4 落笔消费）；失败时为 None。
    pub channel: Option<ChannelMetric>,
    /// Cross track 中心线坐标（TrackId.0 → y）；Main 走廊 x 由 InkContext 间隙表兜底。
    pub track_coords: BTreeMap<u32, f64>,
    /// flat 路径的分层 draft；分治路径为 None。
    pub draft: Option<LayeredDraft>,
    pub hints: LayoutHints,
    pub canvas_padding: f64,
    pub total_width: f64,
    pub total_height: f64,
    pub relaxation: RelaxationLadder,
    /// Stage 5：驱动本次求解的 Dialect 契约。
    pub contract: HierarchicalContract,
}

/// 由 Profile 选 Sugiyama preset（不读 DiagramType）。
pub fn preset_from_profile(profile: &HierarchicalProfile) -> SugiyamaPreset {
    match profile.preset {
        HierarchicalPreset::Flowchart => preset::FLOWCHART_PRESET,
        HierarchicalPreset::Architecture => preset::ARCHITECTURE_PRESET,
        HierarchicalPreset::State => preset::STATE_PRESET,
    }
}

/// 按图类型选 preset（探针 / Adapter 兼容；生产路径优先 [`preset_from_profile`]）。
pub fn preset_for(diagram: &Diagram) -> SugiyamaPreset {
    match diagram.diagram_type {
        DiagramType::Flowchart => preset::FLOWCHART_PRESET,
        DiagramType::Er => preset::ER_PRESET,
        DiagramType::State => preset::STATE_PRESET,
        DiagramType::Architecture => preset::ARCHITECTURE_PRESET,
        _ => preset::GENERIC_PRESET,
    }
}

/// Atlas 度量相主入口。
pub fn solve_atlas(diagram: &Diagram) -> AtlasSolveOutput {
    solve_atlas_with_config(diagram, &SugiyamaLayoutConfig::default())
}

/// 带布局 option 的度量相入口（消费 `group_padding` 等）。
pub fn solve_atlas_with_config(
    diagram: &Diagram,
    config: &SugiyamaLayoutConfig,
) -> AtlasSolveOutput {
    let contract = compile_hierarchical(diagram);
    solve_from_contract(diagram, &contract, config)
}

/// Dialect 契约驱动的度量相（Stage 5 唯一坐标入口）。
pub fn solve_from_contract(
    diagram: &Diagram,
    contract: &HierarchicalContract,
    config: &SugiyamaLayoutConfig,
) -> AtlasSolveOutput {
    solve_from_contract_with_prev(diagram, contract, config, None)
}

/// 带上次 Plan 的度量相：槽位不变时跳过相 I 选路搜索。
pub fn solve_from_contract_with_prev(
    diagram: &Diagram,
    contract: &HierarchicalContract,
    config: &SugiyamaLayoutConfig,
    prev_plan: Option<&Plan>,
) -> AtlasSolveOutput {
    let padding = group_padding_for(contract, config);
    let config = *config;
    let profile = contract.profile;

    let mut out = if contraction::should_contract(diagram) {
        match profile.group_policy {
            GroupPolicy::Weak => {
                solve_atlas_weak(diagram, padding, config, &profile, prev_plan)
            }
            GroupPolicy::StrongMacro => {
                solve_atlas_strong(diagram, padding, &profile, prev_plan)
            }
        }
    } else {
        solve_atlas_flat(diagram, padding, &profile, prev_plan)
    };
    out.contract = contract.clone();
    out
}

fn group_padding_for(
    contract: &HierarchicalContract,
    config: &SugiyamaLayoutConfig,
) -> GroupPadding {
    match contract.profile.preset {
        HierarchicalPreset::Architecture => {
            if (config.group_padding - crate::layout::constants::ARCH_V2_GROUP_PADDING).abs()
                < f64::EPSILON
            {
                GroupPadding::architecture()
            } else {
                GroupPadding::uniform(config.group_padding, 16.0)
            }
        }
        HierarchicalPreset::Flowchart | HierarchicalPreset::State => {
            GroupPadding::uniform(config.group_padding, 16.0)
        }
    }
}

fn solve_atlas_flat(
    diagram: &Diagram,
    padding: GroupPadding,
    profile: &HierarchicalProfile,
    prev_plan: Option<&Plan>,
) -> AtlasSolveOutput {
    let preset = preset_from_profile(profile);
    let draft = LayeredKernel::compute(diagram, &preset);
    let mut ladder = RelaxationLadder::pristine();

    let layer_heights =
        postprocess::compute_layer_heights(&draft.layers, &draft.sizes, &draft.preset);

    let (layer_tops, main_problem, track_coords, metric_opt) =
        match build_channel_metric_with_opts(
            diagram,
            &draft,
            prev_plan,
            PhaseIEdgeOrder::Forward,
        ) {
            Ok(metric) => {
                let cross_n = metric.cross_gap_demands(diagram).len();
                let main_n = metric.main_gap_demands().len();
                if cross_n + main_n > 0 {
                    crate::perf_log!(
                        "[atlas] channel metric: {} cross-gap, {} main-gap demands",
                        cross_n,
                        main_n
                    );
                }
                let mut metric = metric;
                let (tops, problem, coords, _still_bad) = run_main_axis_ladder_l0_l3(
                    diagram,
                    &layer_heights,
                    &draft.per_layer_gaps,
                    draft.padding,
                    draft.preset.layer_gap,
                    &mut metric,
                    &mut ladder,
                    || {
                        build_channel_metric_with_opts(
                            diagram,
                            &draft,
                            prev_plan,
                            PhaseIEdgeOrder::Reverse,
                        )
                        .ok()
                    },
                );
                (Some(tops), Some(problem), coords, Some(metric))
            }
            Err(e) => {
                crate::perf_log!("[atlas] channel metric failed: {e}");
                ladder.push(
                    RelaxLevel::L4,
                    "metric/channel",
                    format!("build failed: {e}"),
                );
                (None, None, BTreeMap::new(), None)
            }
        };

    let gaps_fallback = metric_opt
        .as_ref()
        .map(|m| inflate_layer_gaps(&draft.per_layer_gaps, &m.cross_gap_demands(diagram)))
        .unwrap_or_else(|| draft.per_layer_gaps.clone());

    // M5：Atlas 恒规范空间（horizontal=false + emit_canonical）；LTR 末端 orientation
    let (mut nodes, solved_problem) = assign_coordinates_brandes_koepf_with_main_tops(
        &draft.dag,
        &draft.proper_graph,
        &draft.layers,
        &draft.sizes,
        false,
        &draft.preset,
        &gaps_fallback,
        draft.has_order_bias,
        &draft.end_ids,
        layer_tops.as_deref(),
        true,
    );

    // Main 走廊 → Cross 轴 order 缝 + publish（与 weak/strong 共用尾段）
    let mut track_coords = track_coords;
    if let Some(metric) = &metric_opt {
        let id_layers: Vec<Vec<String>> = draft
            .layers
            .iter()
            .map(|layer| {
                layer
                    .iter()
                    .filter_map(|&n| match &draft.proper_graph[n].kind {
                        crate::layout::kernel::layered::graph::LayerNodeKind::Real(d) => {
                            Some(draft.dag[*d].clone())
                        }
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        finalize_metric_tail_on_nodes(
            &mut nodes,
            metric,
            &id_layers,
            &mut track_coords,
            true,
            None,
        );
        if !metric.main_gap_demands().is_empty() {
            crate::layout::kernel::layered::postprocess::normalize_layout_to_padding(
                &mut nodes,
                draft.padding,
            );
        }
    }

    let groups = if diagram.groups.is_empty() {
        GroupTable::default()
    } else {
        LayoutSession::new(diagram, &nodes, padding)
            .materialize()
            .groups
    };

    let canvas_padding = draft.padding;
    let (total_width, total_height) =
        crate::layout::kernel::common::canvas_bounds::canvas_size(&nodes, &groups, canvas_padding);

    let hints = LayoutHints {
        edge_routing_style: EdgeRoutingStyle::Orthogonal,
        sugiyama_ranks: Some(draft.sugiyama_ranks.clone()),
        same_layer_edges: draft.same_layer_edges.clone(),
        feedback_hubs: draft.feedback_hubs.clone(),
        coordinate_problem: solved_problem.clone().map(Box::new),
        ..Default::default()
    };

    AtlasSolveOutput {
        nodes,
        groups,
        cross_problem: solved_problem,
        main_problem,
        channel: metric_opt,
        track_coords,
        draft: Some(draft),
        hints,
        canvas_padding,
        total_width,
        total_height,
        relaxation: ladder,
        contract: HierarchicalContract::from_scheme(&Scheme::hierarchical_flow_ortho()),
    }
}

/// R3-2：Main LP 松弛阶梯 L0→L1→L2 Reverse→L3（flat / weak / strong 共用）。
///
/// `rebuild_reverse`：L1 不可行时全量重跑相 I（Reverse 禁止 skip；不做热启动）。
/// 返回的 `still_bad` 表示 L2 后仍不可行（已落到 L3 启发式 tops）。
fn run_main_axis_ladder_l0_l3<F>(
    diagram: &Diagram,
    layer_heights: &[f64],
    base_gaps: &[f64],
    first_top: f64,
    default_gap: f64,
    metric: &mut ChannelMetric,
    ladder: &mut RelaxationLadder,
    rebuild_reverse: F,
) -> (Vec<f64>, CoordinateProblem, BTreeMap<u32, f64>, bool)
where
    F: FnOnce() -> Option<ChannelMetric>,
{
    let (mut tops, mut problem, mut coords, still_bad) = try_main_axis_l0_l1(
        diagram,
        layer_heights,
        base_gaps,
        first_top,
        default_gap,
        metric,
        ladder,
    );
    if !still_bad {
        return (tops, problem, coords, false);
    }

    // L2：主轴 L1 后仍不可行 → 反向边序重跑相 I（改争用）
    ladder.push(
        RelaxLevel::L2,
        "metric/relax",
        "phase-I reverse edge order after L1 infeasible",
    );
    let mut still_bad = true;
    if let Some(m2) = rebuild_reverse() {
        *metric = m2;
        let r2 = try_main_axis_l0_l1(
            diagram,
            layer_heights,
            base_gaps,
            first_top,
            default_gap,
            metric,
            ladder,
        );
        tops = r2.0;
        problem = r2.1;
        coords = r2.2;
        still_bad = r2.3;
    }
    if still_bad {
        ladder.push(
            RelaxLevel::L3,
            "metric/relax",
            "fallback inflate_layer_gaps heuristic tops (skirt→Demand)",
        );
        let gaps = inflate_layer_gaps(base_gaps, &metric.cross_gap_demands(diagram));
        tops = vec![first_top; layer_heights.len()];
        for i in 1..layer_heights.len() {
            let gap = gaps.get(i - 1).copied().unwrap_or(default_gap);
            tops[i] = tops[i - 1] + layer_heights[i - 1] + gap;
        }
    }
    (tops, problem, coords, still_bad)
}

/// L0→L1 主轴求解；返回 `still_bad=true` 表示需 L2/L3。
fn try_main_axis_l0_l1(
    diagram: &Diagram,
    layer_heights: &[f64],
    base_gaps: &[f64],
    first_top: f64,
    default_gap: f64,
    metric: &ChannelMetric,
    ladder: &mut RelaxationLadder,
) -> (Vec<f64>, CoordinateProblem, BTreeMap<u32, f64>, bool) {
    let _ = default_gap;
    let mut scale = 1.0;
    let mut last = solve_main_axis_with_cross_tracks(
        layer_heights,
        base_gaps,
        first_top,
        default_gap,
        &metric.substrate,
        &metric.occupancy,
        scale,
        diagram,
        &metric.plan,
    );

    if last.status == SolverStatus::Infeasible || !last.audit_passed {
        ladder.push(RelaxLevel::L1, "metric/relax", "main-axis infeasible, scale pitch 0.85");
        scale = 0.85;
        last = solve_main_axis_with_cross_tracks(
            layer_heights,
            base_gaps,
            first_top,
            default_gap,
            &metric.substrate,
            &metric.occupancy,
            scale,
            diagram,
            &metric.plan,
        );
    }

    let still_bad = last.status == SolverStatus::Infeasible || !last.audit_passed;
    if still_bad {
        return (last.layer_tops, last.problem, last.track_coords, true);
    }
    (last.layer_tops, last.problem, last.track_coords, false)
}

fn solve_atlas_weak(
    diagram: &Diagram,
    padding: GroupPadding,
    config: SugiyamaLayoutConfig,
    profile: &HierarchicalProfile,
    prev_plan: Option<&Plan>,
) -> AtlasSolveOutput {
    let _ = profile;
    let mut ladder = RelaxationLadder::pristine();
    // R3-4：contract → place (LK|stack) → expand
    let mut out = contraction::solve_weak_contract_expand(diagram, config);
    let draft = out.draft.take();
    let cross_problem = out.cross_problem.take();

    let (channel, track_coords, main_problem) = metric_from_slots_publish_tracks(
        diagram,
        &mut out.nodes,
        &out.sugiyama_ranks,
        out.slots,
        prev_plan,
        &mut ladder,
        SlotChannelOpts {
            y_apply: YLayerApplyPolicy::VerticalOnly(out.mode),
            expand_main: true,
            first_top: out.canvas_padding,
            default_gap: preset::FLOWCHART_PRESET.layer_gap,
            ladder_site: "metric/divide",
            success_note: "Main LP metric tail + track_coords published (R3-4)",
            fail_note: "divide channel metric build failed",
            perf_tag: "divide",
        },
    );

    let groups_table = LayoutSession::new(diagram, &out.nodes, padding)
        .materialize()
        .groups;

    let assembled = group_divide::assemble_divide_result(
        diagram,
        out.nodes,
        groups_table.as_map().clone(),
        &out.order,
        out.mode,
        out.sugiyama_ranks,
        out.canvas_padding,
    );

    AtlasSolveOutput {
        nodes: assembled.nodes,
        groups: assembled.groups,
        cross_problem,
        main_problem,
        channel,
        track_coords,
        draft,
        hints: assembled.hints,
        canvas_padding: out.canvas_padding,
        total_width: assembled.total_width,
        total_height: assembled.total_height,
        relaxation: ladder,
        contract: HierarchicalContract::from_scheme(&Scheme::hierarchical_flow_ortho()),
    }
}

fn solve_atlas_strong(
    diagram: &Diagram,
    padding: GroupPadding,
    profile: &HierarchicalProfile,
    prev_plan: Option<&Plan>,
) -> AtlasSolveOutput {
    let mut ladder = RelaxationLadder::pristine();
    // Profile → group_sizing：Equal 时恢复 Uniform 条带（覆盖 G-pre 恒 Fit）
    crate::layout::recipes::architecture::group_sizing::set_override_for_solve(
        match profile.group_sizing {
            GroupSizing::Equal => {
                crate::layout::recipes::architecture::group_sizing::GroupSizingPolicy::Uniform
            }
            GroupSizing::Fit => {
                crate::layout::recipes::architecture::group_sizing::GroupSizingPolicy::Fit
            }
        },
    );

    let arch_config = ArchitectureV2LayoutConfig::default();
    // R3-5：ContractionMeta → macro place → expand → 显式 slots
    let mut out = contraction::solve_strong_contract_expand(diagram, arch_config);
    crate::layout::recipes::architecture::group_sizing::clear_override_for_solve();

    let canvas_padding = out.canvas_padding;
    let (channel, track_coords, main_problem) = metric_from_slots_publish_tracks(
        diagram,
        &mut out.nodes,
        &out.sugiyama_ranks,
        out.slots,
        prev_plan,
        &mut ladder,
        SlotChannelOpts {
            y_apply: YLayerApplyPolicy::Always,
            expand_main: true,
            first_top: canvas_padding,
            default_gap: preset::ARCHITECTURE_PRESET.layer_gap,
            ladder_site: "metric/strong",
            success_note: "Main LP metric tail + track_coords published (R3-5)",
            fail_note: "strong-macro channel metric build failed",
            perf_tag: "strong-macro",
        },
    );

    let groups = LayoutSession::new(diagram, &out.nodes, padding)
        .materialize()
        .groups;

    let (total_width, total_height) = crate::layout::kernel::common::canvas_bounds::canvas_size(
        &out.nodes,
        &groups,
        canvas_padding,
    );

    let hints = LayoutHints {
        edge_routing_style: EdgeRoutingStyle::Orthogonal,
        sugiyama_ranks: Some(out.sugiyama_ranks),
        ..Default::default()
    };

    AtlasSolveOutput {
        nodes: out.nodes,
        groups,
        cross_problem: None,
        main_problem,
        channel,
        track_coords,
        draft: None,
        hints,
        canvas_padding,
        total_width,
        total_height,
        relaxation: ladder,
        contract: HierarchicalContract::from_scheme(&Scheme::hierarchical_arch_equal_track_ortho()),
    }
}

/// R3-1：层顶 Y 平移门控（Horizontal 跳过，与旧启发式 Y expand 语义一致）。
enum YLayerApplyPolicy {
    Always,
    VerticalOnly(group_divide::ArrangementMode),
}

struct SlotChannelOpts {
    y_apply: YLayerApplyPolicy,
    expand_main: bool,
    first_top: f64,
    default_gap: f64,
    ladder_site: &'static str,
    success_note: &'static str,
    fail_note: &'static str,
    perf_tag: &'static str,
}

/// R3-1：按 rank 取层高（该 rank 节点 max height）。
fn layer_heights_from_ranked_nodes(
    nodes: &HashMap<String, NodeLayout>,
    ranks: &HashMap<String, usize>,
) -> Vec<f64> {
    let max_rank = ranks.values().copied().max().unwrap_or(0);
    let mut heights = vec![0.0_f64; max_rank + 1];
    for (id, &r) in ranks {
        if let Some(n) = nodes.get(id) {
            heights[r] = heights[r].max(n.height);
        }
    }
    for h in &mut heights {
        if *h <= 0.0 {
            *h = 40.0;
        }
    }
    heights
}

/// R3-1：层间 base gap（长度 = heights.len()-1），用 default_gap 填满。
fn base_gaps_from_ranked_nodes(layer_count: usize, default_gap: f64) -> Vec<f64> {
    if layer_count == 0 {
        return Vec::new();
    }
    vec![default_gap; layer_count.saturating_sub(1)]
}

/// R3-1：按 Main LP 层顶只扩不缩地抬缝（不改变 rank0 绝对位置，不压缩已有间距）。
///
/// 对相邻 rank：若当前 `min_y[r+1]-min_y[r]` 小于 LP 的 `tops[r+1]-tops[r]`，
/// 则把 rank≥r+1 的节点整体下移差额。避免绝对覆盖打散 divide / strong 布局。
fn apply_layer_tops_to_nodes(
    nodes: &mut HashMap<String, NodeLayout>,
    ranks: &HashMap<String, usize>,
    layer_tops: &[f64],
) {
    if layer_tops.len() < 2 {
        return;
    }
    let max_rank = ranks.values().copied().max().unwrap_or(0);
    let n = (max_rank + 1).min(layer_tops.len());

    let mut rank_min_y = vec![f64::INFINITY; n];
    for (id, &r) in ranks {
        if r >= n {
            continue;
        }
        if let Some(node) = nodes.get(id) {
            rank_min_y[r] = rank_min_y[r].min(node.y);
        }
    }

    let mut shift_from = vec![0.0_f64; n];
    for r in 0..n.saturating_sub(1) {
        if !rank_min_y[r].is_finite() || !rank_min_y[r + 1].is_finite() {
            continue;
        }
        let required_span = layer_tops[r + 1] - layer_tops[r];
        let current_span = rank_min_y[r + 1] - rank_min_y[r];
        let need = required_span - current_span;
        if need > 1e-9 {
            for k in (r + 1)..n {
                shift_from[k] += need;
                if rank_min_y[k].is_finite() {
                    rank_min_y[k] += need;
                }
            }
        }
    }

    if shift_from.iter().all(|&d| d.abs() <= 1e-9) {
        return;
    }
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    for id in ids {
        let Some(&r) = ranks.get(&id) else {
            continue;
        };
        if r >= n {
            continue;
        }
        let d = shift_from[r];
        if d.abs() <= 1e-9 {
            continue;
        }
        if let Some(node) = nodes.get_mut(&id) {
            node.y += d;
        }
    }
}

/// 节点定稿后：Main order 缝 expand + publish Main/Cross track（flat / weak / strong 共用）。
fn finalize_metric_tail_on_nodes(
    nodes: &mut HashMap<String, NodeLayout>,
    metric: &ChannelMetric,
    id_layers: &[Vec<String>],
    track_coords: &mut BTreeMap<u32, f64>,
    expand_main: bool,
    main_cap: Option<f64>,
) {
    if expand_main {
        let main_dem = metric.main_gap_demands();
        if !main_dem.is_empty() {
            let dem = match main_cap {
                Some(c) => cap_gap_demands(&main_dem, c),
                None => main_dem,
            };
            expand_layer_order_gaps(nodes, id_layers, &dem, false);
        }
    }
    publish_main_track_coords(
        track_coords,
        &metric.substrate,
        &metric.plan,
        nodes,
        false,
    );
    publish_cross_track_coords(
        track_coords,
        &metric.substrate,
        &metric.plan,
        nodes,
        false,
    );
}

/// R3-2 glue：slots → 相 I → [`run_metric_tail_after_channel`]（含 L2 Reverse）。
fn metric_from_slots_publish_tracks(
    diagram: &Diagram,
    nodes: &mut HashMap<String, NodeLayout>,
    ranks: &HashMap<String, usize>,
    slots: BTreeMap<String, Slot>,
    prev_plan: Option<&Plan>,
    ladder: &mut RelaxationLadder,
    opts: SlotChannelOpts,
) -> (
    Option<ChannelMetric>,
    BTreeMap<u32, f64>,
    Option<CoordinateProblem>,
) {
    let slots_for_l2 = slots.clone();
    let Ok(mut metric) = build_channel_metric_from_slots_with_opts(
        diagram,
        slots,
        prev_plan,
        PhaseIEdgeOrder::Forward,
    ) else {
        ladder.push(RelaxLevel::L4, "metric/channel", opts.fail_note);
        return (None, BTreeMap::new(), None);
    };

    let (_tops, main_problem, track_coords) = run_metric_tail_after_channel(
        diagram,
        nodes,
        ranks,
        &mut metric,
        ladder,
        &opts,
        || {
            build_channel_metric_from_slots_with_opts(
                diagram,
                slots_for_l2,
                prev_plan,
                PhaseIEdgeOrder::Reverse,
            )
            .ok()
        },
    );
    ladder.push(RelaxLevel::L3, opts.ladder_site, opts.success_note);
    (Some(metric), track_coords, Some(main_problem))
}

/// R3-2 共享度量尾段（相 I 之后）：Main LP L0–L3（含 L2 Reverse）→（可选）Y 层顶 → expand Main + publish。
///
/// flat 因 BK，仅复用 [`run_main_axis_ladder_l0_l3`] + [`finalize_metric_tail_on_nodes`]；本入口供 weak/strong。
fn run_metric_tail_after_channel<F>(
    diagram: &Diagram,
    nodes: &mut HashMap<String, NodeLayout>,
    ranks: &HashMap<String, usize>,
    metric: &mut ChannelMetric,
    ladder: &mut RelaxationLadder,
    opts: &SlotChannelOpts,
    rebuild_reverse: F,
) -> (Vec<f64>, CoordinateProblem, BTreeMap<u32, f64>)
where
    F: FnOnce() -> Option<ChannelMetric>,
{
    let layer_heights = layer_heights_from_ranked_nodes(nodes, ranks);
    let base_gaps = base_gaps_from_ranked_nodes(layer_heights.len(), opts.default_gap);
    let (layer_tops, main_problem, mut track_coords, still_bad) = run_main_axis_ladder_l0_l3(
        diagram,
        &layer_heights,
        &base_gaps,
        opts.first_top,
        opts.default_gap,
        metric,
        ladder,
        rebuild_reverse,
    );

    let allow_y = match &opts.y_apply {
        YLayerApplyPolicy::Always => true,
        YLayerApplyPolicy::VerticalOnly(mode) => {
            if matches!(mode, group_divide::ArrangementMode::Vertical) {
                true
            } else {
                ladder.push(
                    RelaxLevel::L3,
                    opts.ladder_site,
                    "skip cross-gap Y expand on horizontal arrangement",
                );
                false
            }
        }
    };

    if allow_y {
        apply_layer_tops_to_nodes(nodes, ranks, &layer_tops);
        // Main LP（含 L2）仍失败时回退启发式 Y expand（保留 DIVIDE_MAX_GAP_BAND）
        if still_bad {
            let dem = metric.cross_gap_demands(diagram);
            if !dem.is_empty() {
                let capped = cap_gap_demands(&dem, DIVIDE_MAX_GAP_BAND);
                expand_nodes_by_cross_gap_demands(nodes, ranks, &capped, false);
                crate::perf_log!(
                    "[atlas] {} channel: Main LP failed → heuristic Y expand ({} demands)",
                    opts.perf_tag,
                    dem.len()
                );
            }
        } else {
            let cross_n = metric.cross_gap_demands(diagram).len();
            let main_n = metric.main_gap_demands().len();
            if cross_n + main_n > 0 {
                crate::perf_log!(
                    "[atlas] {} channel metric: {} cross-gap, {} main-gap (Main LP)",
                    opts.perf_tag,
                    cross_n,
                    main_n
                );
            }
        }
    }

    let mut by_rank: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (id, &r) in ranks {
        by_rank.entry(r).or_default().push(id.clone());
    }
    for ids in by_rank.values_mut() {
        ids.sort();
    }
    let id_layers: Vec<Vec<String>> = by_rank.into_values().collect();
    // weak/strong 未跑 BK：节点未落到 LP 绝对层顶。清空 LP track，改由节点几何 publish，
    // 避免「LP 绝对 Y + 未对齐节点」错位穿组；Main LP 仍通过抬缝 / main_problem 生效。
    track_coords.clear();
    finalize_metric_tail_on_nodes(
        nodes,
        metric,
        &id_layers,
        &mut track_coords,
        opts.expand_main,
        Some(DIVIDE_MAX_GAP_BAND),
    );

    (layer_tops, main_problem, track_coords)
}

/// 从 AtlasSolveOutput 组装 LayoutResult（不含 edges）。
pub fn assemble_layout_result(output: &AtlasSolveOutput, diagram: &Diagram) -> LayoutResult {
    let mut result = LayoutResult {
        nodes: output.nodes.clone(),
        groups: output.groups.clone(),
        edges: vec![],
        total_width: output.total_width,
        total_height: output.total_height,
        hints: output.hints.clone(),
    };

    if let Some(draft) = &output.draft {
        if let Some(finish) = draft.preset.finish_layout {
            finish(&mut result, &draft.preset);
        }
    }

    let _ = diagram;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nl(x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn r31_helpers_heights_gaps_and_delta_tops() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), nl(0.0, 10.0, 40.0, 20.0));
        nodes.insert("b".into(), nl(50.0, 12.0, 40.0, 30.0));
        nodes.insert("c".into(), nl(0.0, 100.0, 40.0, 25.0));
        let mut ranks = HashMap::new();
        ranks.insert("a".into(), 0usize);
        ranks.insert("b".into(), 0usize);
        ranks.insert("c".into(), 1usize);

        let heights = layer_heights_from_ranked_nodes(&nodes, &ranks);
        assert_eq!(heights, vec![30.0, 25.0]);
        assert_eq!(base_gaps_from_ranked_nodes(heights.len(), 56.0), vec![56.0]);

        apply_layer_tops_to_nodes(&mut nodes, &ranks, &[0.0, 120.0]);
        // 当前 span=90，LP 要求 120 → 只扩不缩，rank1 +30
        assert!((nodes["a"].y - 10.0).abs() < 1e-9);
        assert!((nodes["b"].y - 12.0).abs() < 1e-9);
        assert!((nodes["c"].y - 130.0).abs() < 1e-9);

        // 更紧的 tops 不压缩
        apply_layer_tops_to_nodes(&mut nodes, &ranks, &[0.0, 50.0]);
        assert!((nodes["c"].y - 130.0).abs() < 1e-9);
    }
}
