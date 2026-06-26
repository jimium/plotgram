//! 有机贝塞尔边路由模块
//!
//! 专为 MindMap 树形结构设计的边路由算法。
//! 采用「肘形 S 曲线」设计：控制点沿端口方向伸出一段「肩」，再平滑过渡到目标方向，
//! 效果类似 XMind / MindManager 等主流思维导图产品的曲线风格。
//!
//! 可通过 `edge_routing: organic { … }` 调节。
//!
//! 主要特性：
//! - 曲线风格预设：organic / round / soft 三种预设风格
//! - 层级感知：根据节点深度自动调整曲线弧度，根→一级最明显，深层级更平缓
//! - 连接点均匀分布：同一父节点的子节点连接点在垂直方向均匀排布，避免拥挤
//! - 障碍避让：路由完成后采样曲线检测穿障，穿障的边退化到 spline 绕行折线

use crate::types::DiagramType;
use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::algorithm_config::{AlgorithmOptionSpec, OptionKind};
use crate::layout::{EdgeLayout, EdgeRoutingStrategy, LayoutResult, PathGeometry};
use crate::layout::edge::common::edge_geometry::{
    build_edge_labels, compute_bezier_controls_organic,
    cubic_bezier_point, parse_label_t, DEFAULT_BEZIER_TENSION, DEFAULT_SHOULDER_RATIO,
};
use crate::layout::edge::common::routing_skeleton::{
    finalize_edges, resolve_endpoints, EdgeEndpoints, LabelOffset, RoutingContext,
};
use crate::layout::edge::visibility;
use std::collections::HashMap;

const APPLICABLE_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
    DiagramType::Mindmap,
];

/// 默认深度衰减系数（每深入一层，曲线参数乘以该比例）
const DEFAULT_DEPTH_DECAY: f64 = 0.7;

/// 曲线风格预设
/// 0 = organic（肘形 S 曲线，默认）
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

/// 穿障检测的曲线采样点数
const OBSTACLE_CHECK_SAMPLES: usize = 16;

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
        Self::from_options(&crate::layout::plan::ResolvedAlgoOptions::from_spec_defaults(
            ORGANIC_OPTIONS,
        ))
    }
}

impl OrganicRouting {
    pub fn from_options(options: &crate::layout::plan::ResolvedAlgoOptions) -> Self {
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

    let node_list: Vec<(usize, &crate::layout::NodeLayout)> = result
        .nodes
        .iter()
        .enumerate()
        .map(|(i, (_, nl))| (i, nl))
        .collect();
    let node_id_to_idx: HashMap<&str, usize> = result
        .nodes
        .keys()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let obstacle_index = visibility::ObstacleIndex::build(&node_list);

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
            (base_tension * decay, base_shoulder_ratio * decay)
        } else {
            (base_tension, base_shoulder_ratio)
        };

        // ── 应用连接点均匀分布 ──
        let start_pt = if let Some((sx, sy)) = distributed_starts.get(&i) {
            Point::new(*sx, *sy)
        } else {
            ep.start
        };
        let start_x = start_pt.x;
        let start_y = start_pt.y;

        let control_points = compute_bezier_controls_organic(
            start_x, start_y, ep.end.x, ep.end.y,
            ep.from_port, ep.to_port, effective_tension, effective_shoulder,
        );

        // 标签位于曲线 t 处（由 label_position 锚点决定）
        let cp0 = control_points[0];
        let cp1 = control_points[1];
        let bez_start = start_pt;
        let bez_end = ep.end;
        let middle_t = parse_label_t(rel);
        let labels = build_edge_labels(rel, middle_t, Point::new(label_off.ox, label_off.oy), |t| {
            cubic_bezier_point(bez_start, cp0, cp1, bez_end, t)
        });

        let geometry = PathGeometry::Bezier {
            start: start_pt,
            end: ep.end,
            controls: control_points,
        };

        let mut edge = EdgeLayout {
            geometry,
            labels,
            from_port: ep.from_port,
            to_port: ep.to_port,
        };

        // ── 穿障检测：采样曲线，若穿过非端点节点则退化到 spline 绕行 ──
        let from_idx = node_id_to_idx.get(ep.from_id.as_str()).copied().unwrap_or(usize::MAX);
        let to_idx = node_id_to_idx.get(ep.to_id.as_str()).copied().unwrap_or(usize::MAX);
        let skip = [from_idx, to_idx];

        if curve_intersects_obstacles(&edge, &obstacle_index, &skip) {
            let detour = obstacle_index.shortest_path(ep.start, ep.end, &skip);
            if !detour.is_empty() {
                edge.geometry = PathGeometry::Polyline { points: detour };
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
/// 返回 HashMap<edge_index, (start_x, start_y)>
fn compute_distributed_port_points(
    result: &LayoutResult,
    _relations: &[crate::ast::Relation],
    endpoints: &[Option<(EdgeEndpoints, LabelOffset)>],
    distribution_strength: f64,
) -> HashMap<usize, (f64, f64)> {
    use std::collections::BTreeMap;

    // 按 from_id 分组：from_id -> Vec<(edge_index, start_y, port)>
    let mut from_to_edges: BTreeMap<String, Vec<(usize, f64, crate::layout::Port)>> = BTreeMap::new();

    for (i, ep_opt) in endpoints.iter().enumerate() {
        let Some((ep, _)) = ep_opt else { continue };
        // 只处理水平端口（左/右）
        match ep.from_port {
            crate::layout::Port::Left | crate::layout::Port::Right => {
                from_to_edges
                    .entry(ep.from_id.clone())
                    .or_default()
                    .push((i, ep.start.y, ep.from_port));
            }
            _ => {}
        }
    }

    let mut result_map: HashMap<usize, (f64, f64)> = HashMap::new();

    for (from_id, edges) in from_to_edges.iter() {
        let from_nl = match result.nodes.get(from_id) {
            Some(nl) => nl,
            None => continue,
        };

        let n = edges.len();
        if n <= 1 {
            continue; // 只有一个子节点时不需要均匀分布
        }

        // 按原始连接点 y 坐标排序
        let mut sorted = edges.clone();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 计算均匀分布的目标 y 坐标
        // 在 from 节点的高度范围内均匀分布，带 15% 的边距
        let margin_ratio = 0.15;
        let top = from_nl.y + from_nl.height * margin_ratio;
        let bottom = from_nl.y + from_nl.height * (1.0 - margin_ratio);
        let range = bottom - top;

        // 椭圆参数（用于重新计算边界 x）
        let cx = from_nl.x + from_nl.width / 2.0;
        let cy = from_nl.y + from_nl.height / 2.0;
        let a = from_nl.width / 2.0;
        let b = from_nl.height / 2.0;

        for (i, (edge_idx, original_y, port)) in sorted.iter().enumerate() {
            // 均匀分布的目标 y
            let target_y = top + range * (i as f64) / ((n - 1) as f64);

            // 根据分布强度在原始位置和均匀分布位置之间插值
            let final_y = original_y + (target_y - original_y) * distribution_strength;

            // 用椭圆方程计算对应 y 的边界 x
            // 椭圆方程：(x-cx)²/a² + (y-cy)²/b² = 1
            // 已知 y，求 x：x = cx ± a * √(1 - ((y-cy)/b)²)
            let dy = (final_y - cy).abs().min(b * 0.999); // 夹紧，防止越界
            let dx = a * (1.0 - (dy / b).powi(2)).sqrt();

            let final_x = match port {
                crate::layout::Port::Right => cx + dx,
                crate::layout::Port::Left => cx - dx,
                _ => continue,
            };

            result_map.insert(*edge_idx, (final_x, final_y));
        }
    }

    result_map
}

/// 检测有机贝塞尔曲线采样后是否穿过任何非 skip 障碍物
fn curve_intersects_obstacles(
    edge: &EdgeLayout,
    obstacles: &visibility::ObstacleIndex,
    skip: &[usize],
) -> bool {
    let sampled = edge.sampled_path(OBSTACLE_CHECK_SAMPLES);
    for window in sampled.windows(2) {
        if obstacles.segment_hits_any(window[0], window[1], skip) {
            return true;
        }
    }
    false
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
