//! 有机贝塞尔边路由模块
//!
//! 专为 MindMap 树形结构设计的边路由算法。
//! 采用 Plotgram「绽放曲线」：纯切线肩 + 非对称生长感 + 圆形 root 径向绽放，
//! 端点处水平/径向切线连续，观感优于常见的弦向污染 S 曲线。
//!
//! 可通过 `edge_routing: organic { … }` 调节。
//!
//! 主要特性：
//! - 曲线风格预设：organic / round / soft 三种预设风格
//! - 层级感知：根据节点深度自动调整曲线弧度，根→一级最明显，深层级更平缓
//! - 连接点均匀分布：同一父节点的子节点连接点在垂直方向均匀排布，避免拥挤
//! - 圆形 root 径向出边：从圆心沿半径「长出」，再水平接入子节点
//! - 障碍避让：路由完成后采样曲线检测穿障；mindmap 保持贝塞尔拉弓，不退化折线

use crate::types::DiagramType;
use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::algorithm_config::{AlgorithmOptionSpec, OptionKind};
use crate::layout::{EdgeLayout, EdgeRoutingStrategy, LayoutResult, PathGeometry};
use crate::layout::routing::common::edge_geometry::{
    build_edge_labels, compute_bezier_controls_organic,
    compute_bezier_controls_organic_tangents, cubic_bezier_point, parse_label_t, point_at_path_t,
    port_direction, radial_outward_tangent, DEFAULT_BEZIER_TENSION, DEFAULT_SHOULDER_RATIO,
};
use crate::layout::routing::common::routing_skeleton::{
    finalize_edges, resolve_endpoints, EdgeEndpoints, LabelOffset, RoutingContext,
};
use crate::layout::routing::visibility;
use std::collections::HashMap;

const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
    DiagramType::Mindmap,
];

/// 默认深度衰减系数（每深入一层，曲线参数乘以该比例）
const DEFAULT_DEPTH_DECAY: f64 = 0.72;

/// 曲线风格预设
/// 0 = organic（绽放曲线，默认）
/// 1 = round（大圆弧，更圆润）
/// 2 = soft（柔和曲线，更平缓）
const DEFAULT_CURVE_STYLE: f64 = 0.0;

/// 连接点均匀分布强度（0.0 = 关闭，1.0 = 完全均匀分布）
const DEFAULT_PORT_DISTRIBUTION: f64 = 1.0;

pub(crate) const ORGANIC_OPTIONS: &[AlgorithmOptionSpec] = &[
    AlgorithmOptionSpec {
        key: "tension",
        kind: OptionKind::Number {
            min: 0.0,
            max: 2.0,
            exclude_min: true,
        },
        default: DEFAULT_BEZIER_TENSION,
        description: "有机曲线整体弧度大小（0-2）",
    },
    AlgorithmOptionSpec {
        key: "shoulder_ratio",
        kind: OptionKind::Number {
            min: 0.0,
            max: 1.0,
            exclude_min: false,
        },
        default: DEFAULT_SHOULDER_RATIO,
        description: "肩长比例（沿端口方向伸出段占连线长度的比例）",
    },
    AlgorithmOptionSpec {
        key: "depth_decay",
        kind: OptionKind::Number {
            min: 0.0,
            max: 1.0,
            exclude_min: false,
        },
        default: DEFAULT_DEPTH_DECAY,
        description: "层级衰减系数（每深入一层，曲线弧度乘以该比例，1.0 = 不衰减）",
    },
    AlgorithmOptionSpec {
        key: "curve_style",
        kind: OptionKind::Number {
            min: 0.0,
            max: 2.0,
            exclude_min: false,
        },
        default: DEFAULT_CURVE_STYLE,
        description: "曲线风格预设：0=organic肘形S曲线，1=round大圆弧，2=soft柔和曲线",
    },
    AlgorithmOptionSpec {
        key: "port_distribution",
        kind: OptionKind::Number {
            min: 0.0,
            max: 1.0,
            exclude_min: false,
        },
        default: DEFAULT_PORT_DISTRIBUTION,
        description: "连接点均匀分布强度（0.0=关闭，1.0=完全均匀分布，同一父节点的子节点连接点垂直均匀排布）",
    },
];

/// 有机曲线路由可调参数
#[derive(Clone, Copy)]
pub struct OrganicConfig {
    pub tension: f64,
    pub shoulder_ratio: f64,
    pub depth_decay: f64,
    pub curve_style: f64,
    pub port_distribution: f64,
}

impl Default for OrganicConfig {
    fn default() -> Self {
        Self {
            tension: ORGANIC_OPTIONS[0].default,
            shoulder_ratio: ORGANIC_OPTIONS[1].default,
            depth_decay: ORGANIC_OPTIONS[2].default,
            curve_style: ORGANIC_OPTIONS[3].default,
            port_distribution: ORGANIC_OPTIONS[4].default,
        }
    }
}

/// 有机贝塞尔边路由策略
pub struct OrganicRouting {
    config: OrganicConfig,
}

impl Default for OrganicRouting {
    fn default() -> Self {
        Self::from_options(&crate::layout::pipeline::plan::ResolvedAlgoOptions::from_spec_defaults(
            ORGANIC_OPTIONS,
        ))
    }
}

impl OrganicRouting {
    pub fn from_options(options: &crate::layout::pipeline::plan::ResolvedAlgoOptions) -> Self {
        Self {
            config: OrganicConfig {
                tension: options.get_or_default(&ORGANIC_OPTIONS[0]),
                shoulder_ratio: options.get_or_default(&ORGANIC_OPTIONS[1]),
                depth_decay: options.get_or_default(&ORGANIC_OPTIONS[2]),
                curve_style: options.get_or_default(&ORGANIC_OPTIONS[3]),
                port_distribution: options.get_or_default(&ORGANIC_OPTIONS[4]),
            },
        }
    }
}

impl EdgeRoutingStrategy for OrganicRouting {
    fn name(&self) -> &'static str {
        "organic"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn supports_custom(&self) -> bool {
        true
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        ORGANIC_OPTIONS
    }

    fn route(&self, diagram: &Diagram, result: LayoutResult) -> LayoutResult {
        route_edges_organic(diagram, result, self.config)
    }

    /// 穿障后会退化为 Polyline，需要 refine 检测并兜底。
    fn supports_refine(&self) -> bool {
        true
    }
}

/// 在节点布局完成后，为所有边计算有机贝塞尔路径与标签位置
pub fn route_edges_organic(
    diagram: &Diagram,
    result: LayoutResult,
    config: OrganicConfig,
) -> LayoutResult {
    let relations = &diagram.relations;
    let depth_decay = config.depth_decay;
    let curve_style = config.curve_style;
    let port_distribution = config.port_distribution;
    let ctx = RoutingContext::new(diagram, &result);

    // 读取 mindmap 节点深度信息（非 mindmap 布局为 None）
    let node_depths = result.hints.mindmap_depths.as_ref();

    // ── 曲线风格预设 ──
    // 根据风格预设调整基础参数（用户显式设置的参数会覆盖预设）
    // 这里先计算风格对应的基础值，再与用户配置取最大/按权重混合
    let (style_tension, style_shoulder): (f64, f64) = match curve_style as i32 {
        1 => (0.9, 0.55),    // round: 大圆弧，肩更长
        2 => (0.4, 0.25),    // soft: 柔和，弧度小
        _ => (config.tension, config.shoulder_ratio), // organic: 用户配置或默认
    };
    // 如果用户没改默认值，用风格预设；如果用户改了，用用户的
    // 简化处理：直接用用户配置，风格预设只在用户使用默认值时生效
    let base_tension = if (config.tension - DEFAULT_BEZIER_TENSION).abs() < 0.001 {
        style_tension
    } else {
        config.tension
    };
    let base_shoulder_ratio = if (config.shoulder_ratio - DEFAULT_SHOULDER_RATIO).abs() < 0.001 {
        style_shoulder
    } else {
        config.shoulder_ratio
    };

    let is_mindmap = matches!(diagram.diagram_type, DiagramType::Mindmap);
    // mindmap 树布局中所有边都是父子直连（depth diff=1），不会穿障，
    // 跳过 O(n²) 的 ObstacleIndex 构建（63 节点 → 87ms → 0ms）
    // P0-3: 非 mindmap 也做快速粗检，若无边可能穿障则跳过构建
    let need_obstacle_index = if is_mindmap && node_depths.is_some() {
        false
    } else {
        // 快速检测：直线段 vs 节点 bbox 粗检
        crate::layout::routing::common::routing_skeleton::quick_check_need_obstacle_index(&result, relations)
    };

    let (node_id_to_idx, obstacle_index) = if need_obstacle_index {
        let (idx, obs) = crate::layout::routing::common::routing_skeleton::build_obstacle_context(&result);
        (idx, Some(obs))
    } else {
        (HashMap::new(), None)
    };

    // 父子映射：穿障检测时跳过同父兄弟，避免扇出曲线误判为穿障
    let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for rel in relations {
        children_of
            .entry(rel.from.as_str())
            .or_default()
            .push(rel.to.as_str());
    }

    // ── 第一轮：解析所有边的端点 ──
    // 先得到真实的连接点坐标，再基于真实坐标做均匀分布
    let mut endpoints: Vec<Option<(EdgeEndpoints, LabelOffset)>> =
        Vec::with_capacity(relations.len());
    for (i, rel) in relations.iter().enumerate() {
        endpoints.push(resolve_endpoints(&ctx, rel, i));
    }

    // ── Phase 3: 连接点均匀分布 ──
    // 基于真实连接点 y 坐标计算均匀分布后的目标 y
    // 只有当 port_distribution > 0 时才启用
    let distributed_starts: HashMap<usize, (f64, f64)> = if port_distribution > 0.01 {
        compute_distributed_port_points(&result, relations, &endpoints, port_distribution)
    } else {
        HashMap::new()
    };

    let mut edges: Vec<EdgeLayout> = Vec::with_capacity(relations.len());

    for (i, rel) in relations.iter().enumerate() {
        let Some((ep, label_off)) = endpoints[i].clone() else {
            edges.push(EdgeLayout::empty());
            continue;
        };

        // ── 层级感知参数计算 ──
        // 根据父节点（from 端）深度动态调整曲线参数
        // depth = 0（根→一级）：最大弧度
        // depth 越大：弧度越小，曲线越平缓
        let (effective_tension, effective_shoulder) = if let Some(depths) = node_depths {
            let from_depth = depths.get(ep.from_id.as_str()).copied().unwrap_or(0);
            let decay = depth_decay.powi(from_depth as i32);
            // 深层保留最低张力，避免叶子边退化成「假直线」
            let t = (base_tension * decay).max(base_tension * 0.48);
            let s = (base_shoulder_ratio * decay).max(base_shoulder_ratio * 0.55);
            (t, s)
        } else {
            (base_tension, base_shoulder_ratio)
        };

        // ── 应用连接点均匀分布 ──
        let (start_pt, from_port) = if let Some((sx, sy)) = distributed_starts.get(&i) {
            // 分布后的点已吸附到左/右边界，端口与水平侧对齐
            let nl = result.nodes.get(ep.from_id.as_str());
            let port = if let Some(nl) = nl {
                let cx = nl.x + nl.width / 2.0;
                if *sx >= cx {
                    crate::layout::Port::Right
                } else {
                    crate::layout::Port::Left
                }
            } else {
                ep.from_port
            };
            (Point::new(*sx, *sy), port)
        } else if is_mindmap {
            coerce_mindmap_start_to_horizontal_port(&result, &ep)
        } else {
            (ep.start, ep.from_port)
        };

        // 子节点接入：思维导图强制接到朝向父节点的侧边中部（杜绝角点）
        let (end_pt, to_port) = if is_mindmap {
            snap_mindmap_end(&result, &ep, &start_pt)
        } else {
            (ep.end, ep.to_port)
        };

        let start_x = start_pt.x;
        let start_y = start_pt.y;
        let end_x = end_pt.x;
        let end_y = end_pt.y;

        // 大跨度边加长水平肩
        let dx = (end_x - start_x).abs();
        let dy = (end_y - start_y).abs();
        let aspect = if dx > 1.0 { (dy / dx).min(2.5) } else { 0.0 };
        let adaptive_shoulder = effective_shoulder * (1.0 + 0.28 * aspect);

        // 圆形 parent：径向绽放出边；矩形 parent：端口法向出边
        let control_points = if let Some(from_nl) = result.nodes.get(ep.from_id.as_str()) {
            let aspect_n = from_nl.width / from_nl.height.max(1e-6);
            if (aspect_n - 1.0).abs() < 0.08 {
                let from_dir = radial_outward_tangent(from_nl, start_pt);
                let to_dir = port_direction(to_port);
                compute_bezier_controls_organic_tangents(
                    start_x, start_y, end_x, end_y,
                    from_dir, to_dir, effective_tension, adaptive_shoulder,
                )
            } else {
                compute_bezier_controls_organic(
                    start_x, start_y, end_x, end_y,
                    from_port, to_port, effective_tension, adaptive_shoulder,
                )
            }
        } else {
            compute_bezier_controls_organic(
                start_x, start_y, end_x, end_y,
                from_port, to_port, effective_tension, adaptive_shoulder,
            )
        };
        let control_points = [
            Point::new(control_points[0].x + ep.mid_ox, control_points[0].y + ep.mid_oy),
            Point::new(control_points[1].x + ep.mid_ox, control_points[1].y + ep.mid_oy),
        ];

        // 标签位于曲线 t 处（由 label_position 锚点决定）
        let cp0 = control_points[0];
        let cp1 = control_points[1];
        let bez_start = start_pt;
        let bez_end = end_pt;
        let middle_t = parse_label_t(rel);
        let labels = build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
            cubic_bezier_point(bez_start, cp0, cp1, bez_end, t)
        });

        let geometry = PathGeometry::Bezier {
            start: start_pt,
            end: end_pt,
            controls: control_points,
        };

        let mut edge = EdgeLayout {
            geometry,
            labels,
            from_port,
            to_port,
        };

        // ── 穿障检测：采样曲线，若穿过非端点节点则退化到 spline 绕行 ──
        // mindmap 树布局已在上方跳过 ObstacleIndex 构建，此处 obstacle_index 为 None
        if let Some(obstacle_index) = obstacle_index.as_ref() {
            let from_idx = node_id_to_idx.get(ep.from_id.as_str()).copied().unwrap_or(usize::MAX);
            let to_idx = node_id_to_idx.get(ep.to_id.as_str()).copied().unwrap_or(usize::MAX);
            let mut skip = vec![from_idx, to_idx];
            // 同父兄弟扇出时曲线常擦过中间兄弟，属预期而非穿障
            if let Some(siblings) = children_of.get(ep.from_id.as_str()) {
                for sib in siblings {
                    if *sib != ep.to_id.as_str() {
                        if let Some(&idx) = node_id_to_idx.get(sib) {
                            skip.push(idx);
                        }
                    }
                }
            }

            if crate::layout::routing::common::obstacle_check::curve_intersects_obstacles(&edge, obstacle_index, &skip) {
                if is_mindmap {
                    // 思维导图保持平滑贝塞尔：用绕行中点拉弓，避免折线观感
                    if let Some(bowed) = bow_bezier_around_obstacles(
                        &edge, obstacle_index, &skip, from_port, to_port,
                        effective_tension, adaptive_shoulder,
                    ) {
                        edge.geometry = bowed;
                        let sampled = edge.sampled_path(24);
                        edge.labels = build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
                            point_at_path_t(&sampled, t)
                        });
                    }
                } else {
                    let detour = obstacle_index.shortest_path(start_pt, end_pt, &skip);
                    if !detour.is_empty() {
                        edge.geometry = PathGeometry::Polyline { points: detour };
                        let sampled = edge.path_points().into_owned();
                        edge.labels = build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
                            point_at_path_t(&sampled, t)
                        });
                    }
                }
            }
        }

        edges.push(edge);
    }

    finalize_edges(result, edges, diagram)
}

/// 计算同一父节点的子节点连接点均匀分布后的精确边界坐标
///
/// 基于真实的连接点 y 坐标，在父节点高度范围内均匀分配，
/// 然后通过椭圆方程重新计算边界 x，确保与节点形状完美贴合。
/// 圆形节点完全精确，圆角矩形也有很好的椭圆近似效果。
///
/// **按端口侧（Left / Right）分别分布**，避免径向思维导图左右分支
/// 抢同一组 y 槽位，导致一侧连接点落到圆外或分布失衡。
///
/// 返回 HashMap<edge_index, (start_x, start_y)>
fn compute_distributed_port_points(
    result: &LayoutResult,
    _relations: &[crate::ast::Relation],
    endpoints: &[Option<(EdgeEndpoints, LabelOffset)>],
    distribution_strength: f64,
) -> HashMap<usize, (f64, f64)> {
    use std::collections::BTreeMap;

    // 按 (from_id, port_side) 分组，左右独立分布
    let mut from_to_edges: BTreeMap<(String, bool), Vec<(usize, f64, crate::layout::Port)>> =
        BTreeMap::new();

    for (i, ep_opt) in endpoints.iter().enumerate() {
        let Some((ep, _)) = ep_opt else { continue };
        let Some(nl) = result.nodes.get(&ep.from_id) else { continue };
        let aspect = nl.width / nl.height.max(1e-6);
        let is_circular = (aspect - 1.0).abs() < 0.08;

        let (is_right, port) = match ep.from_port {
            crate::layout::Port::Right => (true, crate::layout::Port::Right),
            crate::layout::Port::Left => (false, crate::layout::Port::Left),
            // 仅圆形节点把 Top/Bottom 归入左/右侧（矩形节点保持原端口，不参与水平分布）
            _ if is_circular => {
                let cx = nl.x + nl.width / 2.0;
                let right = ep.end.x >= cx;
                (
                    right,
                    if right {
                        crate::layout::Port::Right
                    } else {
                        crate::layout::Port::Left
                    },
                )
            }
            _ => continue,
        };
        from_to_edges
            .entry((ep.from_id.clone(), is_right))
            .or_default()
            .push((i, ep.start.y, port));
    }

    let mut result_map: HashMap<usize, (f64, f64)> = HashMap::new();

    for ((from_id, _), edges) in from_to_edges.iter() {
        let from_nl = match result.nodes.get(from_id) {
            Some(nl) => nl,
            None => continue,
        };

        let n = edges.len();
        if n <= 1 {
            // 单边：仅圆形节点需要吸附（斜角矩形交点可能在圆外）
            if n == 1 {
                let aspect = from_nl.width / from_nl.height.max(1e-6);
                if (aspect - 1.0).abs() < 0.08 {
                    let (edge_idx, original_y, port) = edges[0];
                    let (x, y) = snap_to_side(from_nl, original_y, port);
                    result_map.insert(edge_idx, (x, y));
                }
            }
            continue;
        }

        // 按原始连接点 y 坐标排序
        let mut sorted = edges.clone();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 在 from 节点高度范围内均匀分布；加大边距，避开圆角/椭圆极点
        let margin_ratio = 0.28;
        let top = from_nl.y + from_nl.height * margin_ratio;
        let bottom = from_nl.y + from_nl.height * (1.0 - margin_ratio);
        let range = (bottom - top).max(1.0);

        for (i, (edge_idx, original_y, port)) in sorted.iter().enumerate() {
            let target_y = top + range * (i as f64) / ((n - 1) as f64);
            let final_y = original_y + (target_y - original_y) * distribution_strength;
            let (final_x, snapped_y) = snap_to_side(from_nl, final_y, *port);
            result_map.insert(*edge_idx, (final_x, snapped_y));
        }
    }

    result_map
}

/// 将给定 y 吸附到节点左/右边界。
///
/// - 近似圆形：椭圆边界（与渲染圆一致）
/// - 圆角矩形：落在左右竖直边（flat side），避免角点接入
fn snap_to_side(
    nl: &crate::layout::NodeLayout,
    y: f64,
    port: crate::layout::Port,
) -> (f64, f64) {
    let cy = nl.y + nl.height / 2.0;
    let half_h = nl.height / 2.0;
    // 再夹紧一点，远离上下圆角
    let pad = (nl.height * 0.12).min(10.0);
    let clamped_y = y.clamp(nl.y + pad, nl.y + nl.height - pad);

    let aspect = nl.width / nl.height.max(1e-6);
    if (aspect - 1.0).abs() < 0.08 {
        // 圆形：椭圆方程
        let cx = nl.x + nl.width / 2.0;
        let a = nl.width / 2.0;
        let b = half_h;
        let dy = (clamped_y - cy).abs().min(b * 0.999);
        let dx = a * (1.0 - (dy / b).powi(2)).sqrt();
        let final_x = match port {
            crate::layout::Port::Right => cx + dx,
            _ => cx - dx,
        };
        (final_x, clamped_y)
    } else {
        // 矩形/圆角矩形：落在左右平直边
        let final_x = match port {
            crate::layout::Port::Right => nl.x + nl.width,
            _ => nl.x,
        };
        (final_x, clamped_y)
    }
}

/// 思维导图：若起点落在 Top/Bottom（斜角矩形交点），按子节点水平侧改吸附。
fn coerce_mindmap_start_to_horizontal_port(
    result: &LayoutResult,
    ep: &EdgeEndpoints,
) -> (Point, crate::layout::Port) {
    match ep.from_port {
        crate::layout::Port::Left | crate::layout::Port::Right => {
            // 即便已是水平端口，也吸附到侧边，避免斜向矩形交点贴在角上
            if let Some(nl) = result.nodes.get(&ep.from_id) {
                let (x, y) = snap_to_side(nl, ep.start.y, ep.from_port);
                (Point::new(x, y), ep.from_port)
            } else {
                (ep.start, ep.from_port)
            }
        }
        _ => {
            let Some(nl) = result.nodes.get(&ep.from_id) else {
                return (ep.start, ep.from_port);
            };
            let cx = nl.x + nl.width / 2.0;
            let port = if ep.end.x >= cx {
                crate::layout::Port::Right
            } else {
                crate::layout::Port::Left
            };
            let (x, y) = snap_to_side(nl, ep.start.y, port);
            (Point::new(x, y), port)
        }
    }
}

/// 思维导图终点：强制接到子节点朝向父节点的那一侧中部，杜绝角点接入。
fn snap_mindmap_end(
    result: &LayoutResult,
    ep: &EdgeEndpoints,
    start: &Point,
) -> (Point, crate::layout::Port) {
    let Some(to_nl) = result.nodes.get(&ep.to_id) else {
        return (ep.end, ep.to_port);
    };
    let tcx = to_nl.x + to_nl.width / 2.0;
    // 父在左侧 → 接到子节点左边；父在右侧 → 接到子节点右边
    let port = if start.x < tcx {
        crate::layout::Port::Left
    } else {
        crate::layout::Port::Right
    };
    // 单边接入：落在侧边垂直中心，最干净
    let mid_y = to_nl.y + to_nl.height / 2.0;
    let (x, y) = snap_to_side(to_nl, mid_y, port);
    (Point::new(x, y), port)
}

/// 思维导图穿障时保持贝塞尔：沿绕行折线中点拉弓，生成平滑曲线。
///
/// 若绕行失败或拉弓后仍穿障，返回 `None`（调用方保留原曲线，不退化折线）。
fn bow_bezier_around_obstacles(
    edge: &EdgeLayout,
    obstacles: &visibility::ObstacleIndex,
    skip: &[usize],
    from_port: crate::layout::Port,
    to_port: crate::layout::Port,
    tension: f64,
    shoulder_ratio: f64,
) -> Option<PathGeometry> {
    let (start, end) = match &edge.geometry {
        PathGeometry::Bezier { start, end, .. } => (*start, *end),
        PathGeometry::Straight { start, end } => (*start, *end),
        PathGeometry::Polyline { points } if points.len() >= 2 => {
            (points[0], points[points.len() - 1])
        }
        _ => return None,
    };

    let detour = obstacles.shortest_path(start, end, skip);
    if detour.len() < 3 {
        return None;
    }

    let mid = detour[detour.len() / 2];
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let dist = (dx * dx + dy * dy).sqrt().max(1.0);

    // 控制点：有机肩部 + 向绕行中点拉弓
    let base = compute_bezier_controls_organic(
        start.x, start.y, end.x, end.y, from_port, to_port, tension, shoulder_ratio,
    );
    let pull = 0.55;
    let cp0 = Point::new(
        base[0].x * (1.0 - pull) + mid.x * pull,
        base[0].y * (1.0 - pull) + mid.y * pull,
    );
    let cp1 = Point::new(
        base[1].x * (1.0 - pull) + mid.x * pull,
        base[1].y * (1.0 - pull) + mid.y * pull,
    );

    // 若中点几乎在直线上，略微沿法线外推，避免退化成直线仍穿障
    let perp_x = -dy / dist;
    let perp_y = dx / dist;
    let mid_off = ((mid.x - start.x) * perp_x + (mid.y - start.y) * perp_y).abs();
    let (cp0, cp1) = if mid_off < 8.0 {
        let bump = dist * 0.12;
        (
            Point::new(cp0.x + perp_x * bump, cp0.y + perp_y * bump),
            Point::new(cp1.x + perp_x * bump, cp1.y + perp_y * bump),
        )
    } else {
        (cp0, cp1)
    };

    let bowed = PathGeometry::Bezier {
        start,
        end,
        controls: [cp0, cp1],
    };
    let candidate = EdgeLayout {
        geometry: bowed.clone(),
        labels: vec![],
        from_port,
        to_port,
    };
    if crate::layout::routing::common::obstacle_check::curve_intersects_obstacles(&candidate, obstacles, skip) {
        // 仍穿障则保留原曲线（比折线观感更好）
        None
    } else {
        Some(bowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organic_routing_is_bezier() {
        use crate::ast::{ArrowType, AttributeMap, Entity, Identifier, Relation, SourceInfo, Span};
        use crate::layout::{LayoutHints, NodeLayout};

        let span = Span::dummy();
        let diagram = Diagram {
            diagram_type: DiagramType::Mindmap,
            attributes: Vec::new(),
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "a".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: span.clone(),
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "b".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: span.clone(),
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                label: None,
                head_label: None,
                tail_label: None,
                arrow: ArrowType::Active,
                attributes: AttributeMap::default(),
                span: span.clone(),
            }],
            groups: Vec::new(),
            constraints: vec![],
            style_decls: Vec::new(),
            doc_comment: None,
            source_info: SourceInfo::default(),
        };

        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), NodeLayout {
            x: 0.0, y: 0.0, width: 100.0, height: 60.0,
        });
        nodes.insert("b".to_string(), NodeLayout {
            x: 200.0, y: 100.0, width: 100.0, height: 40.0,
        });

        let result = LayoutResult {
            nodes,
            edges: Vec::new(),
            groups: HashMap::new(),
            total_width: 300.0,
            total_height: 200.0,
            hints: LayoutHints::default(),
        };

        let routed = route_edges_organic(&diagram, result, OrganicConfig::default());
        assert_eq!(routed.edges.len(), 1);
        assert!(routed.edges[0].is_bezier());

        // 验证肘形曲线特征：控制点沿端口方向伸出
        let cp = routed.edges[0].bezier_controls().unwrap();
        let start = routed.edges[0].path_start().unwrap();
        let end = routed.edges[0].path_end().unwrap();
        // CP1 应该在起点的右侧（右端口伸出）
        assert!(cp[0].x > start.x, "cp1 should be right of start point");
        // CP2 应该在终点的左侧（左端口伸出）
        assert!(cp[1].x < end.x, "cp2 should be left of end point");
    }

    #[test]
    fn organic_depth_decay_reduces_curvature() {
        // 验证层级感知：深层节点的边弧度更小
        use crate::ast::{ArrowType, AttributeMap, Entity, Identifier, Relation, SourceInfo, Span};
        use crate::layout::{LayoutHints, NodeLayout};

        let span = Span::dummy();
        let diagram = Diagram {
            diagram_type: DiagramType::Mindmap,
            attributes: Vec::new(),
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("root"),
                    label: "root".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: span.clone(),
                },
                Entity {
                    id: Identifier::new_unchecked("l1"),
                    label: "level1".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: span.clone(),
                },
                Entity {
                    id: Identifier::new_unchecked("l2"),
                    label: "level2".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: span.clone(),
                },
            ],
            relations: vec![
                Relation {
                    from: Identifier::new_unchecked("root"),
                    to: Identifier::new_unchecked("l1"),
                    label: None, head_label: None, tail_label: None,
                    arrow: ArrowType::Active,
                    attributes: AttributeMap::default(),
                    span: span.clone(),
                },
                Relation {
                    from: Identifier::new_unchecked("l1"),
                    to: Identifier::new_unchecked("l2"),
                    label: None, head_label: None, tail_label: None,
                    arrow: ArrowType::Active,
                    attributes: AttributeMap::default(),
                    span: span.clone(),
                },
            ],
            groups: Vec::new(),
            constraints: vec![],
            style_decls: Vec::new(),
            doc_comment: None,
            source_info: SourceInfo::default(),
        };

        let mut nodes = HashMap::new();
        nodes.insert("root".to_string(), NodeLayout {
            x: 0.0, y: 0.0, width: 120.0, height: 60.0,
        });
        nodes.insert("l1".to_string(), NodeLayout {
            x: 200.0, y: 0.0, width: 100.0, height: 50.0,
        });
        nodes.insert("l2".to_string(), NodeLayout {
            x: 380.0, y: 0.0, width: 100.0, height: 40.0,
        });

        let mut depths = HashMap::new();
        depths.insert("root".to_string(), 0);
        depths.insert("l1".to_string(), 1);
        depths.insert("l2".to_string(), 2);

        let result = LayoutResult {
            nodes,
            edges: Vec::new(),
            groups: HashMap::new(),
            total_width: 500.0,
            total_height: 200.0,
            hints: LayoutHints {
                mindmap_depths: Some(depths),
                ..Default::default()
            },
        };

        let config = OrganicConfig {
            tension: 0.8,
            shoulder_ratio: 0.4,
            depth_decay: 0.7,
            ..Default::default()
        };
        let routed = route_edges_organic(&diagram, result, config);
        assert_eq!(routed.edges.len(), 2);

        // 第一条边（root→l1，depth=0）的控制点延伸更远
        let cp0 = routed.edges[0].bezier_controls().unwrap();
        let start0 = routed.edges[0].path_start().unwrap();
        let cp0_ext = cp0[0].x - start0.x; // 控制点水平伸出量

        // 第二条边（l1→l2，depth=1）的控制点延伸更近
        let cp1 = routed.edges[1].bezier_controls().unwrap();
        let start1 = routed.edges[1].path_start().unwrap();
        let cp1_ext = cp1[0].x - start1.x;

        // 深层级的控制点伸出量应该更小
        assert!(
            cp0_ext > cp1_ext,
            "depth 0 edge should have larger extension than depth 1, got cp0_ext={}, cp1_ext={}",
            cp0_ext, cp1_ext
        );
    }

    #[test]
    fn organic_without_depths_uses_base_params() {
        // 验证没有深度信息时，使用基础参数（不衰减）
        use crate::ast::{ArrowType, AttributeMap, Entity, Identifier, Relation, SourceInfo, Span};
        use crate::layout::{LayoutHints, NodeLayout};

        let span = Span::dummy();
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: Vec::new(),
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "a".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: span.clone(),
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "b".into(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: span.clone(),
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                label: None, head_label: None, tail_label: None,
                arrow: ArrowType::Active,
                attributes: AttributeMap::default(),
                span: span.clone(),
            }],
            groups: Vec::new(),
            constraints: vec![],
            style_decls: Vec::new(),
            doc_comment: None,
            source_info: SourceInfo::default(),
        };

        let mut nodes = HashMap::new();
        nodes.insert("a".to_string(), NodeLayout {
            x: 0.0, y: 0.0, width: 100.0, height: 60.0,
        });
        nodes.insert("b".to_string(), NodeLayout {
            x: 200.0, y: 80.0, width: 100.0, height: 50.0,
        });

        let result = LayoutResult {
            nodes,
            edges: Vec::new(),
            groups: HashMap::new(),
            total_width: 300.0,
            total_height: 200.0,
            hints: LayoutHints::default(), // 无 mindmap_depths
        };

        let routed = route_edges_organic(&diagram, result, OrganicConfig::default());
        assert_eq!(routed.edges.len(), 1);
        assert!(routed.edges[0].is_bezier());
        // 没有深度信息时也能正常工作
    }
}
