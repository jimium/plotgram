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
    /// Main 轴问题（含 Track 变量）；分治路径可能为 None。
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
                let (mut tops, mut problem, mut coords, still_bad) = try_main_axis_l0_l1(
                    diagram,
                    &layer_heights,
                    &draft.per_layer_gaps,
                    draft.padding,
                    draft.preset.layer_gap,
                    &metric,
                    &mut ladder,
                );
                // L2：主轴 L1 后仍不可行 → 反向边序重跑相 I（改争用）
                if still_bad {
                    ladder.push(
                        RelaxLevel::L2,
                        "metric/relax",
                        "phase-I reverse edge order after L1 infeasible",
                    );
                    // L2 必全量相 I（Reverse 禁止 skip）；传入 prev 不改变 Reverse 语义，
                    // 仅保持调用面一致（不做 Reverse 热启动）。
                    if let Ok(m2) = build_channel_metric_with_opts(
                        diagram,
                        &draft,
                        prev_plan,
                        PhaseIEdgeOrder::Reverse,
                    ) {
                        metric = m2;
                        let r2 = try_main_axis_l0_l1(
                            diagram,
                            &layer_heights,
                            &draft.per_layer_gaps,
                            draft.padding,
                            draft.preset.layer_gap,
                            &metric,
                            &mut ladder,
                        );
                        tops = r2.0;
                        problem = r2.1;
                        coords = r2.2;
                        if r2.3 {
                            ladder.push(
                                RelaxLevel::L3,
                                "metric/relax",
                                "fallback inflate_layer_gaps heuristic tops (skirt→Demand)",
                            );
                            let gaps = inflate_layer_gaps(
                                &draft.per_layer_gaps,
                                &metric.cross_gap_demands(diagram),
                            );
                            tops = vec![draft.padding; layer_heights.len()];
                            for i in 1..layer_heights.len() {
                                let gap = gaps
                                    .get(i - 1)
                                    .copied()
                                    .unwrap_or(draft.preset.layer_gap);
                                tops[i] = tops[i - 1] + layer_heights[i - 1] + gap;
                            }
                        }
                    } else {
                        ladder.push(
                            RelaxLevel::L3,
                            "metric/relax",
                            "fallback inflate_layer_gaps heuristic tops (skirt→Demand)",
                        );
                        let gaps =
                            inflate_layer_gaps(&draft.per_layer_gaps, &metric.cross_gap_demands(diagram));
                        tops = vec![draft.padding; layer_heights.len()];
                        for i in 1..layer_heights.len() {
                            let gap = gaps
                                .get(i - 1)
                                .copied()
                                .unwrap_or(draft.preset.layer_gap);
                            tops[i] = tops[i - 1] + layer_heights[i - 1] + gap;
                        }
                    }
                }
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

    // Main 走廊 → Cross 轴 order 缝
    let mut track_coords = track_coords;
    if let Some(metric) = &metric_opt {
        let main_dem = metric.main_gap_demands();
        if !main_dem.is_empty() {
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
            expand_layer_order_gaps(&mut nodes, &id_layers, &main_dem, false);
            crate::layout::kernel::layered::postprocess::normalize_layout_to_padding(
                &mut nodes,
                draft.padding,
            );
        }
        // M4/R5：节点定稿后发布 Main + Cross track 中心（不覆盖已有 LP）
        publish_main_track_coords(
            &mut track_coords,
            &metric.substrate,
            &metric.plan,
            &nodes,
            false,
        );
        publish_cross_track_coords(
            &mut track_coords,
            &metric.substrate,
            &metric.plan,
            &nodes,
            false,
        );
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

fn solve_main_with_ladder(
    diagram: &Diagram,
    layer_heights: &[f64],
    base_gaps: &[f64],
    first_top: f64,
    default_gap: f64,
    metric: &ChannelMetric,
    ladder: &mut RelaxationLadder,
) -> (Vec<f64>, CoordinateProblem, BTreeMap<u32, f64>) {
    let (tops, problem, coords, still_bad) =
        try_main_axis_l0_l1(diagram, layer_heights, base_gaps, first_top, default_gap, metric, ladder);
    if !still_bad {
        return (tops, problem, coords);
    }
    // L3：裙边 Demand 抬缝（inflate_layer_gaps）作启发式层顶
    ladder.push(
        RelaxLevel::L3,
        "metric/relax",
        "fallback inflate_layer_gaps heuristic tops (skirt→Demand)",
    );
    let gaps = inflate_layer_gaps(base_gaps, &metric.cross_gap_demands(diagram));
    let mut tops = vec![first_top; layer_heights.len()];
    for i in 1..layer_heights.len() {
        let gap = gaps.get(i - 1).copied().unwrap_or(default_gap);
        tops[i] = tops[i - 1] + layer_heights[i - 1] + gap;
    }
    (tops, problem, coords)
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
    let mut out = group_divide::divide_flowchart_nodes(diagram, config);

    let slots = slots_from_ranks(&out.nodes, &out.sugiyama_ranks);
    let (channel, track_coords) = metric_from_slots_publish_tracks(
        diagram,
        &mut out.nodes,
        &out.sugiyama_ranks,
        slots,
        prev_plan,
        &mut ladder,
        SlotChannelOpts {
            cross_expand: CrossGapExpandPolicy::VerticalOnly(out.mode),
            expand_main: true,
            ladder_site: "metric/divide",
            success_note: "Main track_coords published; no Cross Main LP on divide",
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
        cross_problem: None,
        main_problem: None,
        channel,
        track_coords,
        draft: None,
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
    let seed = contraction::contract_strong_macro(diagram, arch_config);
    crate::layout::recipes::architecture::group_sizing::clear_override_for_solve();

    let ranks = seed.hints.sugiyama_ranks.clone().unwrap_or_default();
    let mut nodes = seed.nodes;
    let canvas_padding = arch_config.padding;

    let slots = slots_from_ranks(&nodes, &ranks);
    let (channel, track_coords) = metric_from_slots_publish_tracks(
        diagram,
        &mut nodes,
        &ranks,
        slots,
        prev_plan,
        &mut ladder,
        SlotChannelOpts {
            cross_expand: CrossGapExpandPolicy::Always,
            expand_main: false,
            ladder_site: "metric/strong",
            success_note: "architecture contraction; Main track_coords published",
            fail_note: "strong-macro channel metric build failed",
            perf_tag: "strong-macro",
        },
    );

    // Atlas：组框以 LayoutSession 为准（与 flowchart 一致）；seed.groups 作参考丢弃
    let groups = LayoutSession::new(diagram, &nodes, padding)
        .materialize()
        .groups;

    let (total_width, total_height) =
        crate::layout::kernel::common::canvas_bounds::canvas_size(&nodes, &groups, canvas_padding);

    let hints = LayoutHints {
        edge_routing_style: EdgeRoutingStyle::Orthogonal,
        sugiyama_ranks: Some(ranks),
        ..Default::default()
    };

    AtlasSolveOutput {
        nodes,
        groups,
        cross_problem: None,
        main_problem: None,
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

/// R3 首片：weak/strong 共用的 cross-gap 展开策略（行为与抽取前一致）。
enum CrossGapExpandPolicy {
    /// strong：有 demand 即展开 Y。
    Always,
    /// weak：仅 Vertical 堆叠时展开；Horizontal 只记 ladder。
    VerticalOnly(group_divide::ArrangementMode),
}

struct SlotChannelOpts {
    cross_expand: CrossGapExpandPolicy,
    expand_main: bool,
    ladder_site: &'static str,
    success_note: &'static str,
    fail_note: &'static str,
    perf_tag: &'static str,
}

/// R3 首片 glue：slots → channel metric →（可选）gap expand → publish track coords。
///
/// 零行为抽取：不改变 weak/strong 各自的 expand 门控与 ladder 文案语义。
fn metric_from_slots_publish_tracks(
    diagram: &Diagram,
    nodes: &mut HashMap<String, NodeLayout>,
    ranks: &HashMap<String, usize>,
    slots: BTreeMap<String, Slot>,
    prev_plan: Option<&Plan>,
    ladder: &mut RelaxationLadder,
    opts: SlotChannelOpts,
) -> (Option<ChannelMetric>, BTreeMap<u32, f64>) {
    let mut track_coords = BTreeMap::new();
    let Ok(metric) = build_channel_metric_from_slots_with_opts(
        diagram,
        slots,
        prev_plan,
        PhaseIEdgeOrder::Forward,
    ) else {
        ladder.push(RelaxLevel::L4, "metric/channel", opts.fail_note);
        return (None, track_coords);
    };

    let dem = metric.cross_gap_demands(diagram);
    if !dem.is_empty() {
        match &opts.cross_expand {
            CrossGapExpandPolicy::Always => {
                let capped = cap_gap_demands(&dem, DIVIDE_MAX_GAP_BAND);
                expand_nodes_by_cross_gap_demands(nodes, ranks, &capped, false);
                crate::perf_log!(
                    "[atlas] {} channel: {} cross-gap demands",
                    opts.perf_tag,
                    dem.len()
                );
            }
            CrossGapExpandPolicy::VerticalOnly(mode) => {
                crate::perf_log!(
                    "[atlas] divide channel metric: {} cross-gap demands (mode={:?})",
                    dem.len(),
                    mode
                );
                if matches!(mode, group_divide::ArrangementMode::Vertical) {
                    let capped = cap_gap_demands(&dem, DIVIDE_MAX_GAP_BAND);
                    expand_nodes_by_cross_gap_demands(nodes, ranks, &capped, false);
                } else {
                    ladder.push(
                        RelaxLevel::L3,
                        opts.ladder_site,
                        "skip cross-gap Y expand on horizontal arrangement",
                    );
                }
            }
        }
    }

    if opts.expand_main {
        let main_dem = metric.main_gap_demands();
        if !main_dem.is_empty() {
            let capped = cap_gap_demands(&main_dem, DIVIDE_MAX_GAP_BAND);
            let mut by_rank: BTreeMap<usize, Vec<String>> = BTreeMap::new();
            for (id, &r) in ranks {
                by_rank.entry(r).or_default().push(id.clone());
            }
            let layers: Vec<Vec<String>> = by_rank.into_values().collect();
            expand_layer_order_gaps(nodes, &layers, &capped, false);
        }
    }

    publish_main_track_coords(
        &mut track_coords,
        &metric.substrate,
        &metric.plan,
        nodes,
        false,
    );
    publish_cross_track_coords(
        &mut track_coords,
        &metric.substrate,
        &metric.plan,
        nodes,
        false,
    );
    ladder.push(RelaxLevel::L3, opts.ladder_site, opts.success_note);
    (Some(metric), track_coords)
}

fn slots_from_ranks(
    nodes: &HashMap<String, NodeLayout>,
    ranks: &HashMap<String, usize>,
) -> BTreeMap<String, Slot> {
    let mut by_rank: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (id, &r) in ranks {
        if nodes.contains_key(id) {
            by_rank.entry(r).or_default().push(id.clone());
        }
    }
    let mut slots = BTreeMap::new();
    for (rank, mut ids) in by_rank {
        ids.sort_by(|a, b| {
            let xa = nodes.get(a).map(|n| n.x).unwrap_or(0.0);
            let xb = nodes.get(b).map(|n| n.x).unwrap_or(0.0);
            xa.partial_cmp(&xb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
        for (order, id) in ids.into_iter().enumerate() {
            slots.insert(id, Slot { rank, order });
        }
    }
    slots
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
