//! 正交边路由模块（固定磁吸点方案）
//!
//! 设计要点：
//! - 每个矩形节点的边线连接点为固定「磁吸点（slot）」，仿照画图软件：
//!   上/下边各 3 个候选点，左/右边各 1 个候选点。实际锚点按该边的边数
//!   均匀分布（`(rank+1)/(count+1)`），保证不重叠且对称。
//! - 端口（连接到节点哪条边）由两节点的几何关系**确定性**地选出，而非
//!   对 16 种端口组合打分，避免惩罚项相互博弈导致的诡异折线。
//! - 对齐且尺寸相同的节点对（如垂直链上的相邻节点），相同 slot 分数落在
//!   相同坐标 → 自然生成平行直线（如「响应」与「请求」对称）。
//! - 错位节点对（如认证服务 ↔ 数据库/缓存），slot 不对齐 → 自然生成折线。

use crate::layout::algorithm_config::{AlgorithmOptionSpec, OptionKind};
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, EdgeRoutingStrategy, EdgeSnapConfig, LayoutResult, NodeLayout, PathGeometry, Port};
use crate::layout::edge::common::edge_geometry::{
    arrow_type_tag, canonical_pair, edge_line_style_signature, node_center, undirected_pair_key,
};
use crate::layout::edge::common::parallel_edges::build_parallel_aware_edge_labels;
use crate::layout::edge::common::self_loop;
use crate::layout::edge::common::label_avoidance::resolve_label_overlaps_with_config;
use crate::layout::edge::common::label_candidate::LabelPlacementConfig;
use crate::types::DiagramType;
use crate::ast::{Diagram};
use std::collections::HashMap;

const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
];

pub(super) mod profile;
pub(super) mod channel_load;
pub(super) mod context;
pub(super) mod corridor_route;
pub(super) mod feedback_side;
pub(super) mod lane_assignment;
pub(super) mod layer_order;
pub(super) mod path;
pub(super) mod scoring;
pub(super) mod simplify;
pub(super) mod slot;
pub(super) mod slot_replan;
pub(super) mod conflict_reroute;
pub(super) mod sanitize;
pub(super) mod straighten;
pub(super) mod stub_fix;

// Re-exports for cross-submodule access via `use super::*;`
pub(super) use profile::OrthoRoutingProfile;
pub(super) use channel_load::{channel_load_penalty, ChannelLoadMap};
pub(super) use context::{EndpointPair, PreparedObstacles, RoutingContext, SegmentGrid};
pub(super) use lane_assignment::{
    apply_corridor_planned_offsets, assign_lanes, separate_unrelated_trunk_overlaps,
};
pub(super) use path::{select_best_path_with_scorer_stats, PathSelectStats, RoutedSegment};
#[allow(unused_imports)] // SpacingViolationKind/segments_violate_spacing/path_edge_spacing_violations used in X-1
pub(super) use scoring::{CandidateScorer, DefaultScorer, GROUP_OBSTACLE_PAD, NODE_OBSTACLE_PAD, path_avoids_group_interiors, path_is_clean, path_is_clean_from_edges, path_length, SpacingViolationKind, segments_violate_spacing, path_edge_spacing_violations, count_all_edge_spacing_violations};
pub(super) use simplify::{simplify_path, simplify_path_preserving_stubs};
#[allow(unused_imports)] // used by tests via `use super::*;`
pub(super) use simplify::is_collinear;
#[allow(unused_imports)] // choose_pair_sides is used by tests
pub(super) use slot::{
    choose_docking_strategy, choose_pair_sides, choose_pair_sides_with_group, is_vertical_port, slot_anchor, slot_fraction,
    slot_fraction_around, DockingStrategy, Endpoint,
};
pub(super) use slot_replan::replan_slots;
pub(super) use conflict_reroute::reroute_conflicting_edges;
pub use sanitize::{sanitize_orthogonal_edges, sanitize_orthogonal_edges_ext};
pub(super) use straighten::straighten_preferred_alignments;
pub(super) use stub_fix::fix_reverse_stub_ports;

/// 相邻磁吸点之间的理想间距（像素）；边长不足时自动压缩。
/// 引用共享常量（与 port 容量估算共用）。
use crate::layout::constants::ORTHO_SLOT_PITCH as SLOT_PITCH;

/// 紧凑分布模式（2-3 条边）的磁吸点间距
const COMPACT_SLOT_PITCH: f64 = 16.0;

/// 侧通道绕行时距障碍节点的留白
const CHANNEL_MARGIN: f64 = 18.0;

pub(crate) const ORTHOGONAL_OPTIONS: &[AlgorithmOptionSpec] = &[
    AlgorithmOptionSpec {
        key: "slot_pitch",
        kind: OptionKind::PositiveNumber,
        default: SLOT_PITCH,
        description: "节点边上相邻磁吸点间距",
    },
    AlgorithmOptionSpec {
        key: "channel_margin",
        kind: OptionKind::PositiveNumber,
        default: CHANNEL_MARGIN,
        description: "侧通道距障碍节点的留白",
    },
];

/// 可调美学参数（由 LayoutPlan 解析后注入路由实例）
#[derive(Clone, Copy, Default)]
pub struct OrthoConfig {
    /// 相邻磁吸点间距
    pub slot_pitch: f64,
    /// 侧通道距障碍节点的留白
    pub channel_margin: f64,
}

impl OrthoConfig {
    pub fn from_spec_defaults() -> Self {
        Self {
            slot_pitch: ORTHOGONAL_OPTIONS[0].default,
            channel_margin: ORTHOGONAL_OPTIONS[1].default,
        }
    }
}

/// 正交边路由策略（构造时注入已解析的 option）。
pub struct OrthogonalRouting {
    config: OrthoConfig,
}

impl Default for OrthogonalRouting {
    fn default() -> Self {
        Self::from_options(&crate::layout::plan::ResolvedAlgoOptions::from_spec_defaults(
            ORTHOGONAL_OPTIONS,
        ))
    }
}

impl OrthogonalRouting {
    pub fn from_options(options: &crate::layout::plan::ResolvedAlgoOptions) -> Self {
        Self {
            config: OrthoConfig {
                slot_pitch: options.get_or_default(&ORTHOGONAL_OPTIONS[0]),
                channel_margin: options.get_or_default(&ORTHOGONAL_OPTIONS[1]),
            },
        }
    }
}

impl EdgeRoutingStrategy for OrthogonalRouting {
    fn name(&self) -> &'static str {
        "orthogonal"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn supports_custom(&self) -> bool {
        true
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        ORTHOGONAL_OPTIONS
    }

    fn route(&self, diagram: &Diagram, result: LayoutResult) -> LayoutResult {
        route_edges_orthogonal(diagram, result, self.config)
    }

    fn route_after_node_moves(
        &self,
        diagram: &Diagram,
        result: LayoutResult,
        moved_node_ids: &std::collections::HashSet<String>,
    ) -> LayoutResult {
        reroute_edges_touching_nodes(diagram, result, self.config, moved_node_ids)
    }

    fn route_preserve(
        &self,
        diagram: &Diagram,
        result: LayoutResult,
        preserve_edges: &std::collections::HashSet<usize>,
    ) -> LayoutResult {
        reroute_edges_preserve(diagram, result, self.config, preserve_edges)
    }

    /// orthogonal 输出 Polyline（折线路径），需要 refine 检测穿障并推开问题节点。
    fn supports_refine(&self) -> bool {
        true
    }

    fn edge_snap_config(&self) -> EdgeSnapConfig {
        EdgeSnapConfig::default_orthogonal()
    }
}

/// 从节点边界向外延伸的短线段，避免一出线就折回节点内部
const PORT_CLEARANCE: f64 = 16.0;

/// slot 在节点边上分布时保留的边界余量（占边长比例）
const SLOT_MARGIN_RATIO: f64 = 0.12;

/// 路径穿过节点时的惩罚，确保候选路径优先绕开障碍物
const NODE_CROSSING_PENALTY: f64 = 10_000.0;

/// 已路由边段重叠惩罚
const EDGE_OVERLAP_PENALTY: f64 = 1_200.0;
/// 平行边重叠判定阈值（与 refine/segments_conflict_xy 共享）
use crate::layout::constants::ORTHO_PARALLEL_GAP as EDGE_PARALLEL_GAP;

/// X-1: stub 段保护长度——从端点出发的第一段（stub）在此长度内不做硬间距检查，
/// 因为同节点相邻 slot 的 stub 天然平行近距（slot_pitch 可能小于 EDGE_PARALLEL_GAP）。
pub(super) const STUB_GUARD_LENGTH: f64 = 24.0;
/// 每个折点的惩罚（鼓励更少拐弯）
const BEND_PENALTY: f64 = 16.0;

/// 侧通道距障碍节点的最小留白（即便被分组边框挤压也要保留）
const MIN_CHANNEL_CLEARANCE: f64 = 10.0;

/// 坐标比较容差
pub(super) const EPS: f64 = 0.1;

/// 在节点布局完成后，为所有边计算正交路径与标签位置
pub fn route_edges_orthogonal(
    diagram: &Diagram,
    result: LayoutResult,
    cfg: OrthoConfig,
) -> LayoutResult {
    route_edges_orthogonal_inner(diagram, result, cfg, None)
}

/// 节点位移后的增量重路由：仅重算端点落在 `moved_node_ids` 上的边。
///
/// 若需重路由的边占比过高（≥ 85%），回退为全图重路由以保持质量与简单性。
pub fn reroute_edges_touching_nodes(
    diagram: &Diagram,
    result: LayoutResult,
    cfg: OrthoConfig,
    moved_node_ids: &std::collections::HashSet<String>,
) -> LayoutResult {
    if moved_node_ids.is_empty() {
        return result;
    }
    let n = diagram.relations.len();
    if n == 0 {
        return result;
    }
    let mut preserve = std::collections::HashSet::new();
    for (i, rel) in diagram.relations.iter().enumerate() {
        if !moved_node_ids.contains(rel.from.as_str())
            && !moved_node_ids.contains(rel.to.as_str())
        {
            preserve.insert(i);
        }
    }
    if preserve.is_empty() || (preserve.len() as f64 / n as f64) < crate::layout::post_route_hook::MIN_PRESERVE_RATIO {
        return route_edges_orthogonal(diagram, result, cfg);
    }
    route_edges_orthogonal_inner(diagram, result, cfg, Some(preserve))
}

/// refine / 局部更新：保留 `preserve_edges` 中的边，仅重算其余边。
///
/// 若可保留边占比过低（< 15%），回退为全图重路由。
pub fn reroute_edges_preserve(
    diagram: &Diagram,
    result: LayoutResult,
    cfg: OrthoConfig,
    preserve_edges: &std::collections::HashSet<usize>,
) -> LayoutResult {
    let n = diagram.relations.len();
    if n == 0 || preserve_edges.is_empty() {
        return route_edges_orthogonal(diagram, result, cfg);
    }
    if (preserve_edges.len() as f64 / n as f64) < crate::layout::post_route_hook::MIN_PRESERVE_RATIO {
        return route_edges_orthogonal(diagram, result, cfg);
    }
    route_edges_orthogonal_inner(diagram, result, cfg, Some(preserve_edges.clone()))
}

/// 正交路由内核。`preserve_edges` 中的边保留已有路径，仅将其段加入避让索引。
fn route_edges_orthogonal_inner(
    diagram: &Diagram,
    mut result: LayoutResult,
    cfg: OrthoConfig,
    preserve_edges: Option<std::collections::HashSet<usize>>,
) -> LayoutResult {
    let relations = &diagram.relations;
    let n = relations.len();
    let self_loop_idx = self_loop::self_loop_indices(relations);
    let profile = OrthoRoutingProfile::for_diagram_type(diagram.diagram_type.clone());
    let parallel_gap = profile.parallel_gap;

    let routing_algo = crate::layout::group::routing_algo_for_diagram(diagram);
    let group_ctx = crate::layout::group::GroupRoutingContext::from_layout(
        diagram,
        &result,
        routing_algo,
    );
    let mut group_routing = group_ctx.routing_hints();
    if let Some(existing) = &result.hints.group_routing {
        if !existing.side_gutters.is_empty() {
            group_routing.side_gutters = existing.side_gutters.clone();
        }
    }
    result.hints.group_routing = Some(group_routing);

    // 预排序节点/分组 ID，避免路由循环内重复排序（方案 2）
    let obstacles = PreparedObstacles::build(&result.nodes, &group_ctx);

    let horizontal =
        crate::layout::resolve_effective_direction(diagram) == Some("left-to-right");
    let feedback_assignment = feedback_side::assign_feedback_sides(
        diagram,
        relations,
        &result.nodes,
        result.hints.sugiyama_ranks.as_ref(),
        horizontal,
    );

    let corridor_plan =
        corridor_route::plan_corridor_routes(relations, &group_ctx, &profile);

    // ── 1. 按无向节点对分组，并确定每条边的端口（连接边） ──
    let t1 = crate::layout::perf::Instant::now();
    let mut pair_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }

    let mut from_side = vec![Port::Bottom; n];
    let mut to_side = vec![Port::Top; n];
    let mut lane = vec![0usize; n];

    let mut pair_keys: Vec<String> = pair_groups.keys().cloned().collect();
    pair_keys.sort();
    for key in &pair_keys {
        let indices = &pair_groups[key];
        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());

        let (Some(a_nl), Some(b_nl)) =
            (result.nodes.get(can_from), result.nodes.get(can_to))
        else {
            continue;
        };

        let (side_a, side_b) = choose_pair_sides_with_group(a_nl, b_nl, can_from, can_to, Some(&group_ctx));

        for (l, &i) in indices.iter().enumerate() {
            let rel = &relations[i];
            if let Some(hint) = feedback_assignment.hints.get(&i) {
                from_side[i] = hint.from_side;
                to_side[i] = hint.to_side;
                lane[i] = hint.lane;
                continue;
            }
            if rel.from.as_str() == can_from {
                from_side[i] = side_a;
                to_side[i] = side_b;
            } else {
                from_side[i] = side_b;
                to_side[i] = side_a;
            }
            lane[i] = l;
        }
    }

    // ── 1b. 端口选择全局协调（同侧偏好，G8 修复） ──
    //
    // choose_pair_sides 逐对独立选端口，同一节点的多条边可能分散在不同侧出发，
    // 导致节点附近不必要的交叉。此阶段对每个节点的多条边做"同侧偏好"协调：
    // 统计各侧边数，让少数派边在几何可接受时切换到多数派侧。
    coordinate_port_sides(relations, &result.nodes, &mut from_side, &mut to_side, Some(&group_ctx));
    apply_feedback_side_overrides(
        relations,
        &feedback_assignment,
        &mut from_side,
        &mut to_side,
        &mut lane,
    );
    crate::perf_log!("[perf]     step1_ports: {:.2}ms", t1.elapsed().as_secs_f64() * 1000.0);

    // ── 2. 为每个连接点分配磁吸 slot 坐标 ──
    //
    // 并线分组遵循三条设计规范：
    //   1. 不同箭头类型（Active/Passive/Bidirectional）不并线
    //   2. 不同线型（虚线/实线/dash pattern）不并线
    //   3. 仅当边「从同一节点出发」或「到达同一节点」时才并线（OR 语义）
    //      - 同源出边（都从 X 出发）→ 可并线
    //      - 同宿入边（都到达 X）→ 可并线
    //      - 一条出边 + 一条入边（在 X 上方向相反）→ 不并线
    //
    // 因此分组键 = (node_id, side, is_from, arrow_type, line_style)。
    // is_from 是端点级属性：同一条边在 from 端 is_from=true、在 to 端 is_from=false。
    // 同一 (node_id, side) 上可能存在多个并线子组：先为各子组分配互不重叠的
    // 锚点带中心（base_frac），再让子组内连接点围绕该中心按 DockingStrategy 分布。
    let mut bundling_endpoints: HashMap<String, Vec<Endpoint>> = HashMap::new();
    for i in 0..n {
        let rel = &relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        let (Some(from_nl), Some(to_nl)) =
            (result.nodes.get(from_id), result.nodes.get(to_id))
        else {
            continue;
        };
        let from_center = node_center(from_nl);
        let to_center = node_center(to_nl);
        let fcx = from_center.x;
        let fcy = from_center.y;
        let tcx = to_center.x;
        let tcy = to_center.y;

        bundling_endpoints
            .entry(endpoint_bundling_key(from_id, from_side[i], true, rel))
            .or_default()
            .push(Endpoint {
                edge_index: i,
                is_from: true,
                target_x: tcx,
                target_y: tcy,
                lane: lane[i],
                node_id: from_id.to_string(),
                side: from_side[i],
                anchor: Point::zero(),
            });
        bundling_endpoints
            .entry(endpoint_bundling_key(to_id, to_side[i], false, rel))
            .or_default()
            .push(Endpoint {
                edge_index: i,
                is_from: false,
                target_x: fcx,
                target_y: fcy,
                lane: lane[i],
                node_id: to_id.to_string(),
                side: to_side[i],
                anchor: Point::zero(),
            });
    }

    // 按 (node_id, side) 聚合并线子组，便于在同一节点同一侧上为各子组分配互不重叠的锚点带
    let mut side_groups: HashMap<(String, Port), Vec<Vec<Endpoint>>> = HashMap::new();
    for (_, endpoints) in bundling_endpoints {
        if endpoints.is_empty() {
            continue;
        }
        let node_id = endpoints[0].node_id.clone();
        let side = endpoints[0].side;
        side_groups.entry((node_id, side)).or_default().push(endpoints);
    }

    // endpoint_map: (edge_index, is_from) -> Endpoint (with anchor filled in)
    let mut endpoint_map: HashMap<(usize, bool), Endpoint> = HashMap::new();
    let mut side_group_keys: Vec<(String, Port)> = side_groups.keys().cloned().collect();
    side_group_keys.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (node_id, side) in side_group_keys {
        let Some(mut sub_groups) = side_groups.remove(&(node_id.clone(), side)) else {
            continue;
        };
        let Some(nl) = result.nodes.get(&node_id) else {
            continue;
        };
        let vertical_side = is_vertical_port(side);
        let edge_len = if vertical_side { nl.width } else { nl.height };

        // 子组内沿切线方向排序：上/下边按目标 x，左/右边按目标 y；同位置再按 lane
        for endpoints in sub_groups.iter_mut() {
            endpoints.sort_by(|p, q| {
                let pk = if vertical_side { p.target_x } else { p.target_y };
                let qk = if vertical_side { q.target_x } else { q.target_y };
                pk.partial_cmp(&qk)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(p.lane.cmp(&q.lane))
                    .then(p.edge_index.cmp(&q.edge_index))
            });
        }
        // 子组间按 (arrow_type, line_style, min_edge_index) 排序。
        // 排序键不含 is_from：同一 edge 在两端节点的 is_from 相反，若用 is_from
        // 排序会导致两端排名不一致 → base_frac 不同 → 路径非直线。min_edge_index
        // 作为稳定 tiebreaker，保证同一 edge 在两端子组中获得相同排名。
        sub_groups.sort_by(|a, b| {
            sub_group_sort_key(a, relations)
                .cmp(&sub_group_sort_key(b, relations))
                .then_with(|| {
                    a.iter()
                        .map(|e| e.edge_index)
                        .min()
                        .cmp(&b.iter().map(|e| e.edge_index).min())
                })
        });

        let k = sub_groups.len();
        for (group_rank, endpoints) in sub_groups.iter().enumerate() {
            let count = endpoints.len();
            let strategy = choose_docking_strategy(count);
            // 子组锚点带中心：单子组时居中(0.5)；多子组时按 slot_fraction 分布以避免重叠
            let base_frac = if k <= 1 {
                0.5
            } else {
                slot_fraction(group_rank, k, edge_len, cfg.slot_pitch)
            };

            for (rank, ep) in endpoints.iter().enumerate() {
                // 根据汇流策略选择 slot 分数：
                // - Single/Concentrate：所有边共享子组中心（base_frac），实现入口合并
                // - Compact：围绕子组中心紧凑分布（pitch 上限 16px），接近汇流但仍可区分
                let frac = match strategy {
                    DockingStrategy::Single | DockingStrategy::Concentrate => base_frac,
                    DockingStrategy::Compact => {
                        let pitch = cfg.slot_pitch.min(COMPACT_SLOT_PITCH);
                        slot_fraction_around(rank, count, edge_len, pitch, base_frac)
                    }
                };
                let anchor = slot_anchor(nl, side, frac);
                endpoint_map.insert(
                    (ep.edge_index, ep.is_from),
                    Endpoint {
                        edge_index: ep.edge_index,
                        is_from: ep.is_from,
                        target_x: ep.target_x,
                        target_y: ep.target_y,
                        lane: ep.lane,
                        node_id: ep.node_id.clone(),
                        side: ep.side,
                        anchor,
                    },
                );
            }
        }
    }

    // 平行边切线偏移：仅 A↔B 正反向对对称错开；同向多边由 slot 分布处理。
    let parallel = super::common::parallel_edges::group_parallel_edges(
        relations,
        crate::layout::constants::DEFAULT_EDGE_OFFSET,
    );
    let mut reverse_pairs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pair_groups: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }
    for (key, indices) in &pair_groups {
        if indices.len() < 2 {
            continue;
        }
        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());
        let mut has_forward = false;
        let mut has_backward = false;
        for &i in indices {
            let rel = &relations[i];
            if rel.from.as_str() == can_from && rel.to.as_str() == can_to {
                has_forward = true;
            } else {
                has_backward = true;
            }
        }
        if has_forward && has_backward {
            reverse_pairs.insert(key.clone());
        }
    }
    for ((edge_index, _), ep) in endpoint_map.iter_mut() {
        let rel = &relations[*edge_index];
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        if !reverse_pairs.contains(&key) {
            continue;
        }
        let offset = parallel.offsets[*edge_index];
        if offset.abs() < EPS {
            continue;
        }
        if is_vertical_port(ep.side) {
            ep.anchor.x += offset;
        } else {
            ep.anchor.y += offset;
        }
    }

    // ── 2b. 直连偏好对齐：正对端口边的 slot 锚点对齐修正 ──
    //
    // 问题场景：两个垂直/水平排列的节点，端口选择为正对(Bottom→Top/Left→Right)，
    // 但因两端同侧边数不同导致 slot frac 不对称，锚点不对齐，产生不必要的小弯折。
    // 例如：db_master(bottom有1条出边) → db_replica(top有2条入边)，
    // master端锚点居中，replica端锚点偏左/偏右，路径走Z字而非直线。
    //
    // 修正策略（移至 4c replan_slots 之后执行）：对正对端口且节点投影有重叠的边，
    // 将「自由度较高」端（该侧同方向仅1条边的Single端点）的锚点切线坐标调整为与另一端对齐，
    // 形成直线路径。两端都有多个边时若节点中心线高度对齐（<16px），也强制对齐。
    let t_align = crate::layout::perf::Instant::now();
    crate::perf_log!("[perf]     step2b_straighten: {:.2}ms (moved to 4c)", t_align.elapsed().as_secs_f64() * 1000.0);

    // ── 3. 分层批量边序（有 rank 时低层先占通道；feedback 全局延后） ──
    let t2 = crate::layout::perf::Instant::now();
    let node_degree = layer_order::compute_node_degrees(relations);
    let feedback_edge_set: std::collections::HashSet<usize> =
        feedback_assignment.hints.keys().copied().collect();
    let edge_order = layer_order::compute_edge_order_with_feedback(
        relations,
        result.hints.sugiyama_ranks.as_ref(),
        &node_degree,
        Some(&feedback_edge_set),
    );
    crate::perf_log!("[perf]     step2_slots+step3_order: {:.2}ms", t2.elapsed().as_secs_f64() * 1000.0);

    // ── 4. 逐边构建路径 ──
    let incremental = preserve_edges.is_some();
    let mut edges: Vec<EdgeLayout> = if incremental {
        result.edges.clone()
    } else {
        (0..n).map(|_| EdgeLayout::empty()).collect()
    };
    let mut grid = SegmentGrid::new();

    // P2-1: 路由 debug 统计
    let mut ortho_stats = crate::layout::OrthoDebugStats {
        edge_count: n,
        ..Default::default()
    };

    for &i in &edge_order {
        let t_edge = crate::layout::perf::Instant::now();
        let rel = &relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        if from_id == to_id {
            if let Some(nl) = result.nodes.get(from_id) {
                let loop_idx = self_loop_idx.get(&i).copied().unwrap_or(0);
                edges[i] = self_loop::route_self_loop(
                    rel,
                    nl,
                    loop_idx,
                    self_loop::SelfLoopStyle::Orthogonal,
                );
            }
            continue;
        }

        if let Some(ref preserve) = preserve_edges {
            if preserve.contains(&i) && edges[i].path_len() >= 2 {
                let path: Vec<Point> = edges[i].path_points().into_owned();
                grid.insert_path(&path, i);
                continue;
            }
        }

        let (Some(_from_nl), Some(_to_nl)) =
            (result.nodes.get(from_id), result.nodes.get(to_id))
        else {
            continue;
        };

        let Some(from_ep) = endpoint_map.get(&(i, true)) else {
            continue;
        };
        let Some(to_ep) = endpoint_map.get(&(i, false)) else {
            continue;
        };

        let mut corridor_boost = result
            .hints
            .space_budget
            .as_ref()
            .map(|b| b.corridor_boost_requested)
            .unwrap_or(false);
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };
        let strict = should_strict_group_transit(
            &profile,
            &group_ctx,
            from_id,
            to_id,
            corridor_plan.chains.contains_key(&i),
        );

        let mut path_stats = PathSelectStats::default();
        let mut path = validated_corridor_path(
            i,
            from_ep.anchor,
            to_ep.anchor,
            from_id,
            to_id,
            &corridor_plan,
            &group_ctx,
            &result.nodes,
            &obstacles,
            cfg.channel_margin,
        )
        .unwrap_or_else(|| {
            let ctx = RoutingContext::new(
                &result.nodes,
                &group_ctx,
                &grid,
                &cfg,
                &profile,
                &obstacles,
                None,
            )
            .with_strict_group_transit(strict)
            .with_corridor_boost(corridor_boost);
            select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut path_stats),
                false,
            )
        });
        // S2：0 候选/退化 → 升走廊预算再路由一次（加大外框垫），禁止静默脏折线
        if path_stats.degraded && !corridor_boost {
            corridor_boost = true;
            if let Some(budget) = result.hints.space_budget.as_mut() {
                budget.request_corridor_boost();
            }
            let mut boost_stats = PathSelectStats::default();
            let ctx = RoutingContext::new(
                &result.nodes,
                &group_ctx,
                &grid,
                &cfg,
                &profile,
                &obstacles,
                None,
            )
            .with_strict_group_transit(strict)
            .with_corridor_boost(true);
            let boosted = select_best_path_with_scorer_stats(
                &ctx,
                &pair,
                &DefaultScorer,
                Some(&mut boost_stats),
                false,
            );
            if !boost_stats.degraded || boost_stats.candidate_count > path_stats.candidate_count {
                path = boosted;
                path_stats = boost_stats;
            }
        }
        ortho_stats.total_candidates += path_stats.candidate_count;
        ortho_stats.hard_filter_reject_count += path_stats.hard_filter_reject_count;
        if path_stats.degraded {
            ortho_stats.degraded_count += 1;
            if let Some(budget) = result.hints.space_budget.as_mut() {
                budget.request_corridor_boost();
            }
        }

        // 标签位置：平行/反向边错开 t + 法向偏移，避免双向边标签重叠
        let labels = if path.len() >= 2 {
            match relations.get(i) {
                Some(rel) => build_parallel_aware_edge_labels(
                    rel,
                    i,
                    relations,
                    &parallel.offsets,
                    &path,
                ),
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };

        grid.insert_path(&path, i);

        let mut edge = EdgeLayout {
            // 临时占位，下面用 set_polyline_points 根据 path 点数自动选择 Straight/Polyline
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: from_side[i],
            to_port: to_side[i],
        };
        edge.set_polyline_points(path);

        edges[i] = edge;
        crate::perf_log!(
            "[perf]     edge[{}] {}->{}: {} candidates, {:.2}ms",
            i, from_id, to_id, path_stats.candidate_count,
            t_edge.elapsed().as_secs_f64() * 1000.0
        );
    }

    // ── 4b. 后置交叉检测：修正 slot 排序与实际路由方向不一致的锚点 ──
    let t_fix = crate::layout::perf::Instant::now();
    //
    // slot 排序（步骤 2）按对端节点中心坐标排列，但当边的实际路由方向与对端位置
    // 方向不一致时（如需要绕过中间节点），排序结果会导致出边交叉。
    // 典型场景：节点 A 底部两条出边，左边 slot 的边实际向右绕行，右边 slot 的边
    // 直下，两者在节点下方交叉。交换 slot 后即可消除交叉。
    replan_slots(
        &result.nodes,
        &relations,
        &from_side,
        &to_side,
        &mut endpoint_map,
        &mut edges,
        &mut grid,
        &cfg,
        &group_ctx,
        &obstacles,
        &corridor_plan,
        &mut ortho_stats,
        &profile,
    );

    // ── 4c. 直连偏好对齐：正对端口边的 slot 锚点对齐修正 ──
    // 在 replan_slots 之后执行，确保anchor位置是最终的slot排序结果。
    // 修改anchor后需要重路由受影响的边，因此放在 X-1 reroute 之前。
    let t_align2 = crate::layout::perf::Instant::now();
    let old_endpoints: HashMap<(usize, bool), Endpoint> = endpoint_map.clone();
    // 仅 reverse_pairs 边携带非零 parallel offset；straighten 对齐到 center±offset
    let mut straighten_offsets = vec![0.0; n];
    for ((edge_index, _), _) in endpoint_map.iter() {
        let rel = &relations[*edge_index];
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        if reverse_pairs.contains(&key) {
            straighten_offsets[*edge_index] = parallel.offsets[*edge_index];
        }
    }
    straighten_preferred_alignments(
        &result.nodes,
        n,
        &from_side,
        &to_side,
        &mut endpoint_map,
        &straighten_offsets,
    );

    // 找出anchor被修改的边，需要重路由
    let mut align_reroute: Vec<usize> = Vec::new();
    for i in 0..n {
        for &is_from in &[true, false] {
            let old_ep = old_endpoints.get(&(i, is_from));
            let new_ep = endpoint_map.get(&(i, is_from));
            if let (Some(o), Some(ne)) = (old_ep, new_ep) {
                if (o.anchor.x - ne.anchor.x).abs() > EPS || (o.anchor.y - ne.anchor.y).abs() > EPS {
                    align_reroute.push(i);
                    break;
                }
            }
        }
    }
    if !align_reroute.is_empty() {
        align_reroute.sort_unstable();
        grid.remove_by_edges(&align_reroute);
        for &ei in &align_reroute {
            let Some(from_ep) = endpoint_map.get(&(ei, true)) else { continue };
            let Some(to_ep) = endpoint_map.get(&(ei, false)) else { continue };
            let (from_id, to_id) = relations
                .get(ei)
                .map(|rel| (rel.from.as_str(), rel.to.as_str()))
                .unwrap_or(("", ""));
            let mut path_stats = PathSelectStats::default();
            let candidate = validated_corridor_path(
                ei,
                from_ep.anchor,
                to_ep.anchor,
                from_id,
                to_id,
                &corridor_plan,
                &group_ctx,
                &result.nodes,
                &obstacles,
                cfg.channel_margin,
            )
            .unwrap_or_else(|| {
                let pair = EndpointPair {
                    from: from_ep.clone(),
                    to: to_ep.clone(),
                };
                let boost = result
                    .hints
                    .space_budget
                    .as_ref()
                    .map(|b| b.corridor_boost_requested)
                    .unwrap_or(false);
                let ctx = RoutingContext::new(
                    &result.nodes,
                    &group_ctx,
                    &grid,
                    &cfg,
                    &profile,
                    &obstacles,
                    None,
                )
                .with_strict_group_transit(should_strict_group_transit(
                    &profile,
                    &group_ctx,
                    from_id,
                    to_id,
                    corridor_plan.chains.contains_key(&ei),
                ))
                .with_corridor_boost(boost);
                select_best_path_with_scorer_stats(
                    &ctx,
                    &pair,
                    &DefaultScorer,
                    Some(&mut path_stats),
                    false,
                )
            });
            if candidate.len() >= 2 {
                grid.insert_path(&candidate, ei);
                let labels = match relations.get(ei) {
                    Some(rel) => build_parallel_aware_edge_labels(
                        rel,
                        ei,
                        relations,
                        &parallel.offsets,
                        &candidate,
                    ),
                    None => Vec::new(),
                };
                let mut edge = EdgeLayout {
                    geometry: PathGeometry::Polyline { points: Vec::new() },
                    labels,
                    from_port: from_side[ei],
                    to_port: to_side[ei],
                };
                edge.set_polyline_points(candidate);
                edges[ei] = edge;
            }
        }
    }
    crate::perf_log!("[perf]     4c_straighten_align: {:.2}ms (aligned {} edges)", t_align2.elapsed().as_secs_f64() * 1000.0, align_reroute.len());

    // ── 4d. X-1: 多轮冲突消解重路由 ──
    let t_x1 = crate::layout::perf::Instant::now();
    reroute_conflicting_edges(
        &result.nodes,
        &relations,
        &from_side,
        &to_side,
        &endpoint_map,
        &mut edges,
        &mut grid,
        &cfg,
        &group_ctx,
        &obstacles,
        &corridor_plan,
        &mut ortho_stats,
        &profile,
    );
    crate::perf_log!("[perf]     x1_reroute: {:.2}ms", t_x1.elapsed().as_secs_f64() * 1000.0);

    // ── 4e. X-2: 反向 stub 检测与端口翻转 ──
    //
    // 问题场景：由于分组障碍物/走廊限制，choose_pair_sides 基于几何中心选择的端口
    // 在实际路由时被证明是"反向"的——路径从端口出发后不得不沿反方向折返穿过节点
    // 投影平面才能到达目标，导致箭头方向与主路径方向冲突（视觉上"搞笑箭头"）。
    //
    // 修正策略：路由完成后检测路径上的反向stub端点，将其端口翻转到对面（Bottom↔Top,
    // Left↔Right），重新计算anchor并重路由。若新路径无反向stub且质量可接受，则接受。
    let t_flip = crate::layout::perf::Instant::now();
    fix_reverse_stub_ports(
        &result.nodes,
        &relations,
        &mut from_side,
        &mut to_side,
        &mut endpoint_map,
        &mut edges,
        &mut grid,
        &cfg,
        &group_ctx,
        &obstacles,
        &corridor_plan,
        &mut ortho_stats,
        &profile,
    );
    crate::perf_log!("[perf]     x2_flip_stub: {:.2}ms (flipped {} edges)", t_flip.elapsed().as_secs_f64() * 1000.0, ortho_stats.flipped_stub_edges);

    // ── 4f. X-3: Lane Assignment 车道分配 ──
    // 对 bundling 无法合并的残余平行段，通过平移 cross-axis 坐标分离重合段。
    // 不插入 Z 字弯，保持正交性。
    let t_lane = crate::layout::perf::Instant::now();
    let lane_stats = assign_lanes(
        &mut edges,
        &mut grid,
        &result.nodes,
        &obstacles.sorted_node_ids,
        relations,
        &from_side,
        &to_side,
        parallel_gap,
    );
    ortho_stats.lane_groups = lane_stats.lane_groups;
    ortho_stats.lane_segments_shifted = lane_stats.segments_shifted;
    ortho_stats.lane_shifts_failed = lane_stats.shifts_failed;
    if profile.corridor_lane_offsets {
        let corridor_shifted = apply_corridor_planned_offsets(
            &mut edges,
            &mut grid,
            &result.nodes,
            &obstacles.sorted_node_ids,
            relations,
            &from_side,
            &to_side,
            &corridor_plan,
            &group_ctx,
        );
        ortho_stats.lane_segments_shifted += corridor_shifted;
        if profile.separate_unrelated_trunks {
            for _ in 0..2 {
                let shifted = separate_unrelated_trunk_overlaps(
                    &mut edges,
                    Some(&mut grid),
                    relations,
                    &from_side,
                    &to_side,
                    &result.nodes,
                    &obstacles.sorted_node_ids,
                    parallel_gap,
                    &profile,
                );
                ortho_stats.lane_segments_shifted += shifted;
                if shifted == 0 {
                    break;
                }
            }
        }
    }
    crate::perf_log!(
        "[perf]     x3_lane_assignment: {:.2}ms ({} groups, {} shifted, {} failed)",
        t_lane.elapsed().as_secs_f64() * 1000.0,
        lane_stats.lane_groups,
        lane_stats.segments_shifted,
        lane_stats.shifts_failed
    );

    // ── 4g. 锯齿消毒：端点反向 stub + 微折折叠（lane/corridor 之后）──
    sanitize_orthogonal_edges(&mut edges, relations, &from_side, &to_side);

    // ── 4c. X-0: 统计边间距违规（排除 stub 段） ──
    let (exact_overlap_pairs, tight_spacing_pairs) =
        count_all_edge_spacing_violations(&edges, &grid, parallel_gap);
    ortho_stats.edge_exact_overlap_pairs = exact_overlap_pairs;
    ortho_stats.edge_tight_spacing_pairs = tight_spacing_pairs;

    // ── 5. 标签自动避让 ──
    let label_config = LabelPlacementConfig::for_diagram_type(diagram.diagram_type.clone());
    resolve_label_overlaps_with_config(&mut edges, &result.nodes, &result.groups, label_config);
    crate::perf_log!("[perf]     fix_inversions+labels: {:.2}ms", t_fix.elapsed().as_secs_f64() * 1000.0);

    result.edges = edges;
    // P2-1: 导出 orthogonal 路由 debug 统计
    result.hints.orthogonal_debug = Some(ortho_stats);
    result
}

/// 走廊边路径重建：有计划且通过穿障/穿组校验时返回路径。
/// Iteration 2：是否对该边启用穿组硬约束（拒绝 `best_nodes_only` 穿无关组）。
///
/// - 已有 corridor chain → 强制 strict（应走走廊，禁止穿组软降级）
/// - 其余边保持 false：仍可用高 `GROUP_TRANSIT_PENALTY` 软惩罚；无走廊时硬否决
///   会导致直线/脏路径退化（见 k8s-multi-namespace 回归）
fn should_strict_group_transit(
    _profile: &OrthoRoutingProfile,
    _group_ctx: &crate::layout::group::GroupRoutingContext,
    _from_id: &str,
    _to_id: &str,
    has_corridor_chain: bool,
) -> bool {
    has_corridor_chain
}

fn validated_corridor_path(
    edge_index: usize,
    from_anchor: Point,
    to_anchor: Point,
    from_id: &str,
    to_id: &str,
    corridor_plan: &corridor_route::CorridorRoutePlan,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    nodes: &HashMap<String, NodeLayout>,
    obstacles: &PreparedObstacles,
    stub_len: f64,
) -> Option<Vec<Point>> {
    if !corridor_plan.chains.contains_key(&edge_index) {
        return None;
    }
    let candidate = corridor_route::try_build_corridor_path(
        edge_index,
        from_anchor,
        to_anchor,
        from_id,
        to_id,
        corridor_plan,
        group_ctx,
        stub_len,
    )?;
    if candidate.len() >= 2
        && path_is_clean(
            &candidate,
            from_id,
            to_id,
            nodes,
            group_ctx,
            &obstacles.sorted_node_ids,
        )
        && path_avoids_group_interiors(
            &candidate,
            from_id,
            to_id,
            group_ctx,
            &obstacles.sorted_group_ids,
        )
    {
        Some(candidate)
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════
//  通用辅助
// ═══════════════════════════════════════════════════════════

/// 构建端点并线分组键。
///
/// 键相同的端点才允许共享锚点（并线）。键由五个维度组成，分别对应三条并线原则：
/// - `node_id` + `side`：同一节点同一侧（并线的前提位置条件）
/// - `is_from`：端点方向相同（原则 3 的 OR 语义实现）
///   - `is_from=true` 的端点都属于"从 node_id 出发"的边 → 同源出边之间可并线
///   - `is_from=false` 的端点都属于"到达 node_id"的边 → 同宿入边之间可并线
///   - 一条出边 + 一条入边（is_from 不同）→ 不并线
/// - `arrow_type_tag`：同箭头类型（原则 1）
/// - `edge_line_style_signature`：同线型（原则 2）
fn endpoint_bundling_key(
    node_id: &str,
    side: Port,
    is_from: bool,
    rel: &crate::ast::Relation,
) -> String {
    format!(
        "{node_id}|{side:?}|{is_from}|{}|{}",
        arrow_type_tag(&rel.arrow),
        edge_line_style_signature(rel),
    )
}

/// 取一个并线子组的排序键 `(arrow_tag, line_style, min_edge_index)`。
///
/// 排序键**不含** `is_from`：同一 edge 在 from 端 `is_from=true`、在 to 端
/// `is_from=false`，若将 is_from 纳入排序，两端子组排名会不一致，导致同一
/// edge 在两端获得不同的 `base_frac`，路径出现弯折。用 `min_edge_index` 做
/// 稳定 tiebreaker 可保证同一 edge 在两端子组中获得相同排名 → 相同 base_frac
/// → 直线路径。
fn sub_group_sort_key(
    endpoints: &[Endpoint],
    relations: &[crate::ast::Relation],
) -> (&'static str, String, usize) {
    let min_edge = endpoints.iter().map(|e| e.edge_index).min().unwrap_or(0);
    let rel = &relations[min_edge];
    (
        arrow_type_tag(&rel.arrow),
        edge_line_style_signature(rel),
        min_edge,
    )
}

// ═══════════════════════════════════════════════════════════
//  P1-3: 回环边侧向通道覆盖（在端口协调后强制执行）
// ═══════════════════════════════════════════════════════════

fn apply_feedback_side_overrides(
    relations: &[crate::ast::Relation],
    assignment: &feedback_side::FeedbackSideAssignment,
    from_side: &mut [Port],
    to_side: &mut [Port],
    lane: &mut [usize],
) {
    for (&edge_index, hint) in &assignment.hints {
        if edge_index >= relations.len() {
            continue;
        }
        from_side[edge_index] = hint.from_side;
        to_side[edge_index] = hint.to_side;
        lane[edge_index] = hint.lane;
    }
}

// ═══════════════════════════════════════════════════════════
//  P0-3: 端口选择全局协调（同侧偏好）
// ═══════════════════════════════════════════════════════════

/// 端口选择全局协调：对每个节点的多条边做"同侧偏好"协调。
///
/// `choose_pair_sides` 逐对独立选端口，同一节点的多条边可能分散在不同侧出发，
/// 导致节点附近不必要的交叉。此函数统计各侧边数，让少数派边在几何可接受时
/// 切换到多数派侧。
///
/// 协调以 pair_group 为最小单元（保持组内端口对一致性），出边/入边分开协调。
/// 确定性：节点按 node_id 排序，多数派 tiebreak 用最小 edge_index。
fn coordinate_port_sides(
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &mut [Port],
    to_side: &mut [Port],
    _group_ctx: Option<&crate::layout::group::GroupRoutingContext>,
) {
    use std::collections::{BTreeMap, BTreeSet};
    let n = relations.len();
    if n == 0 {
        return;
    }

    // 1. 重建 pair_groups: pair_key -> (can_from, can_to, edge_indices)
    let mut pair_info: BTreeMap<String, (String, String, Vec<usize>)> = BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        let (can_from, can_to) = canonical_pair(rel.from.as_str(), rel.to.as_str());
        pair_info
            .entry(key)
            .or_insert_with(|| (can_from.to_string(), can_to.to_string(), Vec::new()))
            .2
            .push(i);
    }

    // 2. 收集每个节点的端口信息: node_id -> Vec<(pair_key, edge_index, is_from, side_on_node)>
    let mut node_ports: BTreeMap<String, Vec<(String, usize, bool, Port)>> = BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        node_ports
            .entry(rel.from.to_string())
            .or_default()
            .push((key.clone(), i, true, from_side[i]));
        node_ports
            .entry(rel.to.to_string())
            .or_default()
            .push((key.clone(), i, false, to_side[i]));
    }

    // 3. 按确定性顺序协调每个节点（已切换的 pair_group 不再处理，避免振荡）
    let mut switched_pairs: BTreeSet<String> = BTreeSet::new();

    for (node_id, ports) in &node_ports {
        let Some(node_nl) = nodes.get(node_id) else {
            continue;
        };

        // 分离出边和入边（排除已切换的 pair_group）
        let mut out_ports: Vec<&(String, usize, bool, Port)> = Vec::new();
        let mut in_ports: Vec<&(String, usize, bool, Port)> = Vec::new();
        for entry in ports {
            if switched_pairs.contains(&entry.0) {
                continue;
            }
            if entry.2 {
                out_ports.push(entry);
            } else {
                in_ports.push(entry);
            }
        }

        // 协调出边（≥2 条才有协调意义）
        if out_ports.len() >= 2 {
            if let Some(majority_side) = find_majority_side(&out_ports) {
                for entry in &out_ports {
                    let pair_key = &entry.0;
                    let side = entry.3;
                    if side == majority_side || switched_pairs.contains(pair_key.as_str()) {
                        continue;
                    }
                    if let Some(other_nl) = pair_other_node(pair_key, node_id, &pair_info, nodes) {
                        if side_acceptable(node_nl, other_nl, majority_side) {
                            switch_pair_side(
                                pair_key,
                                node_id,
                                majority_side,
                                &pair_info,
                                relations,
                                from_side,
                                to_side,
                            );
                            switched_pairs.insert(pair_key.clone());
                        }
                    }
                }
            }
        }

        // 协调入边
        if in_ports.len() >= 2 {
            if let Some(majority_side) = find_majority_side(&in_ports) {
                for entry in &in_ports {
                    let pair_key = &entry.0;
                    let side = entry.3;
                    if side == majority_side || switched_pairs.contains(pair_key.as_str()) {
                        continue;
                    }
                    if let Some(other_nl) = pair_other_node(pair_key, node_id, &pair_info, nodes) {
                        if side_acceptable(node_nl, other_nl, majority_side) {
                            switch_pair_side(
                                pair_key,
                                node_id,
                                majority_side,
                                &pair_info,
                                relations,
                                from_side,
                                to_side,
                            );
                            switched_pairs.insert(pair_key.clone());
                        }
                    }
                }
            }
        }
    }
}

/// 查找多数派端口。tiebreak：count 降序 → 最小 edge_index 升序 → 固定端口顺序。
fn find_majority_side(ports: &[&(String, usize, bool, Port)]) -> Option<Port> {
    let port_order = [Port::Top, Port::Bottom, Port::Left, Port::Right];
    let mut counts: [(usize, usize); 4] = [(0, usize::MAX); 4]; // (count, min_edge_index)
    for entry in ports {
        let edge_index = entry.1;
        let side = entry.3;
        for (idx, p) in port_order.iter().enumerate() {
            if side == *p {
                counts[idx].0 += 1;
                counts[idx].1 = counts[idx].1.min(edge_index);
                break;
            }
        }
    }
    let mut best_idx: Option<usize> = None;
    for (idx, (count, min_edge)) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let is_better = match best_idx {
            None => true,
            Some(bi) => {
                let (bc, be) = counts[bi];
                count > &bc
                    || (*count == bc && min_edge < &be)
                    || (*count == bc && min_edge == &be && idx < bi)
            }
        };
        if is_better {
            best_idx = Some(idx);
        }
    }
    best_idx.map(|idx| port_order[idx])
}

/// 获取 pair_group 中 node_id 之外另一个节点的布局
fn pair_other_node<'a>(
    pair_key: &str,
    node_id: &str,
    pair_info: &std::collections::BTreeMap<String, (String, String, Vec<usize>)>,
    nodes: &'a HashMap<String, NodeLayout>,
) -> Option<&'a NodeLayout> {
    let (can_from, can_to, _) = pair_info.get(pair_key)?;
    let other_id = if can_from == node_id {
        can_to.as_str()
    } else {
        can_from.as_str()
    };
    nodes.get(other_id)
}

/// 切换 pair_group 中 node_id 侧的端口为 new_side，保持组内端口对一致性。
fn switch_pair_side(
    pair_key: &str,
    node_id: &str,
    new_side: Port,
    pair_info: &std::collections::BTreeMap<String, (String, String, Vec<usize>)>,
    relations: &[crate::ast::Relation],
    from_side: &mut [Port],
    to_side: &mut [Port],
) {
    let Some((can_from, _can_to, edge_indices)) = pair_info.get(pair_key) else {
        return;
    };
    for &i in edge_indices {
        let rel = &relations[i];
        let is_can_from_from = rel.from.as_str() == can_from.as_str();
        if can_from == node_id {
            // node_id 的端口是 side_a
            if is_can_from_from {
                from_side[i] = new_side;
            } else {
                to_side[i] = new_side;
            }
        } else {
            // node_id == can_to，端口是 side_b
            if is_can_from_from {
                to_side[i] = new_side;
            } else {
                from_side[i] = new_side;
            }
        }
    }
}

/// 判断 `side` 作为 `from` 节点连接 `to` 节点的端口是否几何可接受。
///
/// 复用 `choose_pair_sides` 的阈值逻辑（`slot.rs` `dy.abs() >= dx.abs() * 0.4`）。
/// 若该方向的对端节点位移比例低于阈值，则代价过高、不可接受。
fn side_acceptable(from: &NodeLayout, to: &NodeLayout, side: Port) -> bool {
    let fc = node_center(from);
    let tc = node_center(to);
    let dx = tc.x - fc.x;
    let dy = tc.y - fc.y;
    let ox = range_overlap_local(from.x, from.x + from.width, to.x, to.x + to.width);
    let oy = range_overlap_local(from.y, from.y + from.height, to.y, to.y + to.height);

    match side {
        Port::Top | Port::Bottom => {
            if oy > EPS && ox <= EPS {
                return false;
            }
            let direction_ok = match side {
                Port::Bottom => dy > EPS,
                Port::Top => dy < -EPS,
                _ => unreachable!(),
            };
            if !direction_ok {
                return false;
            }
            if ox <= EPS && oy <= EPS {
                return dy.abs() >= dx.abs() * 0.4 - EPS;
            }
            if ox > EPS && oy > EPS {
                return dy.abs() >= dx.abs() - EPS;
            }
            true
        }
        Port::Left | Port::Right => {
            if ox > EPS && oy <= EPS {
                return false;
            }
            let direction_ok = match side {
                Port::Right => dx > EPS,
                Port::Left => dx < -EPS,
                _ => unreachable!(),
            };
            if !direction_ok {
                return false;
            }
            if ox <= EPS && oy <= EPS {
                return dx.abs() >= dy.abs() * 0.4 - EPS;
            }
            if ox > EPS && oy > EPS {
                return dx.abs() >= dy.abs() - EPS;
            }
            true
        }
    }
}

fn range_overlap_local(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> f64 {
    (a_max.min(b_max) - a_min.max(b_min)).max(0.0)
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
#[path = "orthogonal_tests.rs"]
mod tests;
