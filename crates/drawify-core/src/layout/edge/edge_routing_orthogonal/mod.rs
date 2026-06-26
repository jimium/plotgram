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
use crate::layout::{EdgeLayout, EdgeRoutingStrategy, LayoutResult, NodeLayout, PathGeometry, Port};
use crate::layout::edge::common::edge_geometry::{
    arrow_type_tag, build_edge_labels, canonical_pair, edge_line_style_signature, node_center,
    parse_label_t, point_at_path_t, undirected_pair_key,
};
use crate::layout::edge::common::label_avoidance::resolve_label_overlaps;
use crate::types::DiagramType;
use crate::ast::{Diagram};
use std::collections::HashMap;

const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
];

pub(super) mod context;
pub(super) mod layer_order;
pub(super) mod path;
pub(super) mod scoring;
pub(super) mod simplify;
pub(super) mod slot;

// Re-exports for cross-submodule access via `use super::*;`
pub(super) use context::{EndpointPair, RoutingContext};
pub(super) use path::{append_routed_segments, select_best_path_with_scorer_stats, PathSelectStats, RoutedSegment};
pub(super) use scoring::{CandidateScorer, DefaultScorer, GROUP_OBSTACLE_PAD, NODE_OBSTACLE_PAD, path_avoids_group_interiors, path_is_clean, path_length};
pub(super) use simplify::{simplify_path, simplify_path_preserving_stubs};
#[allow(unused_imports)] // used by tests via `use super::*;`
pub(super) use simplify::is_collinear;
pub(super) use slot::{
    choose_docking_strategy, choose_pair_sides, is_vertical_port, slot_anchor, slot_fraction,
    slot_fraction_around, DockingStrategy, Endpoint,
};
// P1-1: port_outward 仅在 mod.rs 内使用，不重导出
use path::port_outward;

/// 相邻磁吸点之间的理想间距（像素）；边长不足时自动压缩。
/// 引用共享常量（与 friendliness/port_conflict 共用）。
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
    AlgorithmOptionSpec {
        key: "bundling",
        kind: OptionKind::Number {
            min: 0.0,
            max: 1.0,
            exclude_min: false,
        },
        default: 0.0,
        description: "是否启用 Edge Bundling（边捆绑，默认关闭）。启用后将在路由后处理阶段将相似边捆绑共享主干",
    },
];

/// 可调美学参数（由 LayoutPlan 解析后注入路由实例）
#[derive(Clone, Copy, Default)]
pub struct OrthoConfig {
    /// 相邻磁吸点间距
    pub slot_pitch: f64,
    /// 侧通道距障碍节点的留白
    pub channel_margin: f64,
    /// Edge Bundling §7.3: 是否启用边捆绑（默认 false）。
    ///
    /// 启用后在路由后处理阶段（repulse 之后、finalize 之前）执行 bundling。
    /// 仅对 orthogonal 路由有效。
    pub bundling: bool,
}

impl OrthoConfig {
    pub fn from_spec_defaults() -> Self {
        Self {
            slot_pitch: ORTHOGONAL_OPTIONS[0].default,
            channel_margin: ORTHOGONAL_OPTIONS[1].default,
            bundling: ORTHOGONAL_OPTIONS[2].default > 0.5,
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
                bundling: options.get_or_default(&ORTHOGONAL_OPTIONS[2]) > 0.5,
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

    /// orthogonal 输出 Polyline（折线路径），需要 refine 检测穿障并推开问题节点。
    fn supports_refine(&self) -> bool {
        true
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

/// 每个折点的惩罚（鼓励更少拐弯）
const BEND_PENALTY: f64 = 16.0;

/// 侧通道距障碍节点的最小留白（即便被分组边框挤压也要保留）
const MIN_CHANNEL_CLEARANCE: f64 = 10.0;

/// 坐标比较容差
const EPS: f64 = 0.1;

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
    if preserve.is_empty() || (preserve.len() as f64 / n as f64) < 0.15 {
        return route_edges_orthogonal(diagram, result, cfg);
    }
    route_edges_orthogonal_inner(diagram, result, cfg, Some(preserve))
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

    let routing_algo = crate::layout::group::routing_algo_for_diagram(diagram);
    let group_ctx = crate::layout::group::GroupRoutingContext::from_layout(
        diagram,
        &result,
        routing_algo,
    );
    result.hints.group_routing = Some(group_ctx.routing_hints());

    // ── 1. 按无向节点对分组，并确定每条边的端口（连接边） ──
    let mut pair_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }

    let mut from_side = vec![Port::Bottom; n];
    let mut to_side = vec![Port::Top; n];
    let mut lane = vec![0usize; n];

    for indices in pair_groups.values() {
        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());

        let (Some(a_nl), Some(b_nl)) =
            (result.nodes.get(can_from), result.nodes.get(can_to))
        else {
            continue;
        };

        let (side_a, side_b) = choose_pair_sides(a_nl, b_nl);

        for (l, &i) in indices.iter().enumerate() {
            let rel = &relations[i];
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
    coordinate_port_sides(relations, &result.nodes, &mut from_side, &mut to_side);

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
    for ((node_id, side), mut sub_groups) in side_groups {
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
            sub_group_sort_key(a, relations).cmp(&sub_group_sort_key(b, relations))
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

    // ── 3. 分层批量边序（有 rank 时低层先占通道，层内按连接度） ──
    let node_degree = layer_order::compute_node_degrees(relations);
    let edge_order = layer_order::compute_edge_order(
        relations,
        result.hints.sugiyama_ranks.as_ref(),
        &node_degree,
    );

    // ── 4. 逐边构建路径 ──
    let incremental = preserve_edges.is_some();
    let mut edges: Vec<EdgeLayout> = if incremental {
        result.edges.clone()
    } else {
        (0..n).map(|_| EdgeLayout::empty()).collect()
    };
    let mut routed_segments: Vec<RoutedSegment> = Vec::new();

    // P2-1: 路由 debug 统计
    let mut ortho_stats = crate::layout::OrthoDebugStats {
        edge_count: n,
        ..Default::default()
    };

    for &i in &edge_order {
        let rel = &relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        if let Some(ref preserve) = preserve_edges {
            if preserve.contains(&i) && edges[i].path_len() >= 2 {
                let path: Vec<Point> = edges[i].path_points().into_owned();
                append_routed_segments(&mut routed_segments, &path, i);
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

        let ctx = RoutingContext {
            nodes: &result.nodes,
            group_ctx: &group_ctx,
            routed_segments: &routed_segments,
            cfg: &cfg,
        };
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };

        // P2-1: 收集路径选择统计
        let mut path_stats = PathSelectStats::default();
        let path = select_best_path_with_scorer_stats(
            &ctx,
            &pair,
            &DefaultScorer,
            Some(&mut path_stats),
        );
        ortho_stats.total_candidates += path_stats.candidate_count;
        ortho_stats.hard_filter_reject_count += path_stats.hard_filter_reject_count;
        if path_stats.degraded {
            ortho_stats.degraded_count += 1;
        }

        // 标签位置：根据 label_position 锚点沿路径取点
        let labels = if path.len() >= 2 {
            match relations.get(i) {
                Some(rel) => {
                    let middle_t = parse_label_t(rel);
                    build_edge_labels(rel, middle_t, Point::new(0.0, 0.0), |t| point_at_path_t(&path, t))
                }
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };

        append_routed_segments(&mut routed_segments, &path, i);

        let mut edge = EdgeLayout {
            // 临时占位，下面用 set_polyline_points 根据 path 点数自动选择 Straight/Polyline
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: from_side[i],
            to_port: to_side[i],
        };
        edge.set_polyline_points(path);

        edges[i] = edge;
    }

    // ── 4b. 后置交叉检测：修正 slot 排序与实际路由方向不一致的锚点 ──
    //
    // slot 排序（步骤 2）按对端节点中心坐标排列，但当边的实际路由方向与对端位置
    // 方向不一致时（如需要绕过中间节点），排序结果会导致出边交叉。
    // 典型场景：节点 A 底部两条出边，左边 slot 的边实际向右绕行，右边 slot 的边
    // 直下，两者在节点下方交叉。交换 slot 后即可消除交叉。
    fix_slot_inversions(
        &result.nodes,
        &relations,
        &from_side,
        &to_side,
        &mut endpoint_map,
        &mut edges,
        &mut routed_segments,
        &cfg,
        &group_ctx,
        &mut ortho_stats,
    );

    // ── 5. 标签自动避让 ──
    // §4.10.1: 启用 bundling 时跳过路由内 label 避障，
    // 由后置 label 流水线（relayout_edge_labels_after_bundling）统一处理。
    if !cfg.bundling {
        resolve_label_overlaps(&mut edges, &result.nodes, &result.groups);
    }

    result.edges = edges;
    // P2-1: 导出 orthogonal 路由 debug 统计
    result.hints.orthogonal_debug = Some(ortho_stats);
    result
}

// ═══════════════════════════════════════════════════════════
//  后置交叉检测：修正 slot 排序与实际路由方向的倒挂
// ═══════════════════════════════════════════════════════════

/// 后置交叉检测：在路径构建完成后，检查同一节点同一侧的边是否因 slot 排序
/// 与实际路由方向不一致而产生不必要的交叉，若发现则交换锚点并重路由。
///
/// ## 问题根因
///
/// slot 排序（步骤 2）按对端节点中心坐标排列锚点。当边的实际路由方向与对端
/// 位置方向不一致时（典型：需要绕过中间节点），排序结果会导致出边交叉。
///
/// 例如：节点 A 底部两条出边——
/// - 边 1 目标节点在 A 正下方（target_x ≈ A.center_x），slot 排在右侧
/// - 边 2 目标节点在 A 远下方偏左，但路由需先向右绕行，slot 排在左侧
/// 结果：边 2 从左侧出发后向右跨越边 1，产生交叉。交换两槽即可消除。
///
/// ## 检测方法
///
/// 对同一并线子组内的边，提取每条边从锚点出发后第一个非 stub 水平/垂直位移
/// 方向（"有效出口方向"），若左侧 slot 的边有效出口方向向右、右侧 slot 的边
/// 有效出口方向向左，则判定为倒挂。
///
/// ## 修复方法
///
/// 交换倒挂边的锚点坐标，然后重路由受影响的边。采用迭代至收敛策略：
/// 每轮只交换一对相邻倒挂边，交换后重新计算有效出口方向，直到无新倒挂。
/// 迭代上限防止异常情况下死循环。
fn fix_slot_inversions(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    endpoint_map: &mut HashMap<(usize, bool), Endpoint>,
    edges: &mut Vec<EdgeLayout>,
    routed_segments: &mut Vec<RoutedSegment>,
    cfg: &OrthoConfig,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
) {
    use std::collections::BTreeMap;

    let n = edges.len();

    // 1. 按并线分组键 (node_id, side, is_from, arrow_type, line_style) 分组，
    //    仅在同一并线子组内检测倒挂，避免破坏不同子组间的锚点带分布。
    let mut bundling_groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        if edges[i].path_is_empty() {
            continue;
        }
        // from 端
        if let Some(ep) = endpoint_map.get(&(i, true)) {
            let rel = &relations[i];
            let key = endpoint_bundling_key(&ep.node_id, ep.side, true, rel);
            bundling_groups.entry(key).or_default().push(i);
        }
        // to 端
        if let Some(ep) = endpoint_map.get(&(i, false)) {
            let rel = &relations[i];
            let key = endpoint_bundling_key(&ep.node_id, ep.side, false, rel);
            bundling_groups.entry(key).or_default().push(i);
        }
    }

    // 2. 对每组迭代检测倒挂并交换，直到收敛
    //    每轮只交换一对，交换后重路由会改变有效出口方向，
    //    需要重新计算才能发现新产生的倒挂。
    const MAX_ITERATIONS: usize = 8; // 安全上限，防止异常情况死循环

    for (_bundling_key, edge_indices) in &bundling_groups {
        if edge_indices.len() < 2 {
            continue;
        }

        // 同一子组内的边共享同一 (node_id, side)
        let first_ep = endpoint_map.get(&(edge_indices[0], true))
            .or_else(|| endpoint_map.get(&(edge_indices[0], false)));
        let Some(first_ep) = first_ep else { continue };
        let side = first_ep.side;
        let vertical_side = is_vertical_port(side);

        for _iter in 0..MAX_ITERATIONS {
            // 每轮重新收集锚点坐标和有效出口方向（上一轮交换可能改变路径）
            let mut edge_dirs: Vec<(usize, f64, f64)> = Vec::new(); // (edge_index, tangent_coord, effective_dir)

            for &ei in edge_indices {
                let is_from_actual = endpoint_map
                    .get(&(ei, true))
                    .map_or(false, |ep| ep.side == side);

                let ep_key = (ei, is_from_actual);
                let Some(ep) = endpoint_map.get(&ep_key) else {
                    continue;
                };

                let tangent_coord = if vertical_side { ep.anchor.x } else { ep.anchor.y };

                // Concentrate 模式下所有边共享同一锚点，交换无意义，跳过
                // 检测方式：如果该边与组内已有边的切线坐标差 < EPS，则视为同锚点
                if !edge_dirs.is_empty() {
                    let min_dist = edge_dirs.iter().map(|(_, tc, _)| (tc - tangent_coord).abs()).fold(f64::MAX, f64::min);
                    if min_dist < EPS {
                        continue;
                    }
                }

                let effective_dir = compute_effective_exit_dir(edges, ei, is_from_actual, side);
                edge_dirs.push((ei, tangent_coord, effective_dir));
            }

            if edge_dirs.len() < 2 {
                break;
            }

            // 按切线坐标排序（左侧/上侧在前）
            edge_dirs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            // 检测相邻对是否倒挂，找到第一对即交换后重新开始下一轮
            let mut found_inversion = false;
            for w in edge_dirs.windows(2) {
                let (ei_left, _, dir_left) = w[0];
                let (ei_right, _, dir_right) = w[1];

                // 倒挂判定：左侧/上侧边的有效方向与右侧/下侧边的有效方向交叉
                let inverted = if vertical_side {
                    // 垂直端口：左槽向右(+)、右槽向左(-) → 倒挂
                    dir_left > EPS && dir_right < -EPS
                } else {
                    // 水平端口：上槽向下(+)、下槽向上(-) → 倒挂
                    dir_left > EPS && dir_right < -EPS
                };

                if inverted {
                    swap_endpoint_anchors(
                        ei_left,
                        ei_right,
                        endpoint_map,
                        edges,
                        routed_segments,
                        nodes,
                        relations,
                        from_side,
                        to_side,
                        cfg,
                        group_ctx,
                        ortho_stats,
                    );
                    found_inversion = true;
                    break; // 交换一对后立即重新开始下一轮迭代
                }
            }

            if !found_inversion {
                break; // 无倒挂，收敛完成
            }
        }
    }
}

/// 从已路由的边路径中提取"有效出口方向"——即锚点出发后第一个非 stub 的
/// 切线位移分量。
///
/// - 垂直端口 (Top/Bottom)：返回水平位移（正=向右，负=向左）
/// - 水平端口 (Left/Right)：返回垂直位移（正=向下，负=向上）
///
/// stub 段是锚点沿端口外延方向的短线段（长度 PORT_CLEARANCE），
/// 需要跳过 stub 才能获得实际路由方向。
fn compute_effective_exit_dir(
    edges: &[EdgeLayout],
    edge_index: usize,
    is_from: bool,
    side: Port,
) -> f64 {
    let points: Vec<Point> = edges[edge_index].path_points().into_owned();
    if points.len() < 3 {
        return 0.0;
    }

    let vertical_side = is_vertical_port(side);

    // 从锚点端开始遍历路径点，跳过 stub 段（沿端口外延方向的段），
    // 找到第一个有切线位移的点
    let start_idx = if is_from { 0 } else { points.len() - 1 };
    let anchor = points[start_idx];

    // stub 方向：端口外延方向
    let (out_dx, out_dy) = port_outward(side);

    // 从锚点出发，沿路径跳过 stub 段
    let iter_range: Box<dyn Iterator<Item = usize>> = if is_from {
        Box::new(1..points.len())
    } else {
        Box::new((0..points.len().saturating_sub(1)).rev())
    };

    for idx in iter_range {
        let p = points[idx];
        let dx = p.x - anchor.x;
        let dy = p.y - anchor.y;

        // 跳过仍在 stub 方向上的点（沿端口外延方向移动）
        let is_stub = if out_dx.abs() > EPS {
            // 水平 stub（Left/Right 端口）：dx 与 out_dx 同号
            dx * out_dx > EPS && dy.abs() < EPS
        } else {
            // 垂直 stub（Top/Bottom 端口）：dy 与 out_dy 同号
            dy * out_dy > EPS && dx.abs() < EPS
        };

        if !is_stub {
            // 找到第一个非 stub 点，返回切线位移
            return if vertical_side { dx } else { dy };
        }
    }

    0.0
}

/// 交换两条边的端点锚点并重路由。
///
/// 交换后两条边的锚点坐标互换，然后分别重路由以生成与新锚点匹配的路径。
/// 旧路径的 routed_segments 会被移除，新路径的 segments 会被追加。
fn swap_endpoint_anchors(
    ei_a: usize,
    ei_b: usize,
    endpoint_map: &mut HashMap<(usize, bool), Endpoint>,
    edges: &mut Vec<EdgeLayout>,
    routed_segments: &mut Vec<RoutedSegment>,
    nodes: &HashMap<String, NodeLayout>,
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
    cfg: &OrthoConfig,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    ortho_stats: &mut crate::layout::OrthoDebugStats,
) {
    // 1. 收集两条边在 endpoint_map 中的所有端点 key
    let keys_a = [(ei_a, true), (ei_a, false)];
    let keys_b = [(ei_b, true), (ei_b, false)];

    // 2. 找到需要交换锚点的端点：同一 (node_id, side) 上的端点
    //    对于每条边，检查 from 和 to 端点，找到属于倒挂侧的那个
    for &(ka, kb) in &[(keys_a[0], keys_b[0]), (keys_a[1], keys_b[1])] {
        let Some(ep_a) = endpoint_map.get(&ka) else {
            continue;
        };
        let Some(ep_b) = endpoint_map.get(&kb) else {
            continue;
        };

        // 只交换同一节点同一侧的端点锚点
        if ep_a.node_id != ep_b.node_id || ep_a.side != ep_b.side {
            continue;
        }

        // 交换锚点
        let anchor_a = ep_a.anchor;
        let anchor_b = ep_b.anchor;

        endpoint_map.entry(ka).and_modify(|e| e.anchor = anchor_b);
        endpoint_map.entry(kb).and_modify(|e| e.anchor = anchor_a);
    }

    // 3. 移除旧路径的 routed_segments
    routed_segments.retain(|seg| seg.edge_index != ei_a && seg.edge_index != ei_b);

    // 4. 重路由两条边
    for &ei in &[ei_a, ei_b] {
        let Some(from_ep) = endpoint_map.get(&(ei, true)) else {
            continue;
        };
        let Some(to_ep) = endpoint_map.get(&(ei, false)) else {
            continue;
        };

        let ctx = RoutingContext {
            nodes,
            group_ctx,
            routed_segments,
            cfg,
        };
        let pair = EndpointPair {
            from: from_ep.clone(),
            to: to_ep.clone(),
        };

        let mut path_stats = PathSelectStats::default();
        let path = select_best_path_with_scorer_stats(
            &ctx,
            &pair,
            &DefaultScorer,
            Some(&mut path_stats),
        );
        ortho_stats.total_candidates += path_stats.candidate_count;
        ortho_stats.hard_filter_reject_count += path_stats.hard_filter_reject_count;
        if path_stats.degraded {
            ortho_stats.degraded_count += 1;
        }

        // 重建标签
        let labels = if path.len() >= 2 {
            match relations.get(ei) {
                Some(rel) => {
                    let middle_t = parse_label_t(rel);
                    build_edge_labels(rel, middle_t, Point::new(0.0, 0.0), |t| point_at_path_t(&path, t))
                }
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };

        append_routed_segments(routed_segments, &path, ei);

        let mut edge = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels,
            from_port: from_side[ei],
            to_port: to_side[ei],
        };
        edge.set_polyline_points(path);

        edges[ei] = edge;
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
            // 垂直重叠强制水平 → 垂直端口不可接受
            if oy > EPS && ox <= EPS {
                return false;
            }
            // 方向检查
            let direction_ok = match side {
                Port::Bottom => dy > EPS,
                Port::Top => dy < -EPS,
                _ => unreachable!(),
            };
            if !direction_ok {
                return false;
            }
            // 阈值检查
            if ox <= EPS && oy <= EPS {
                // 无重叠：垂直可接受需 dy.abs() >= dx.abs() * 0.4
                return dy.abs() >= dx.abs() * 0.4 - EPS;
            }
            if ox > EPS && oy > EPS {
                // 双重叠：主轴判定
                return dy.abs() >= dx.abs() - EPS;
            }
            // ox > EPS && oy <= EPS：水平重叠强制垂直 → 可接受
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
            // oy > EPS && ox <= EPS：垂直重叠强制水平 → 可接受
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
