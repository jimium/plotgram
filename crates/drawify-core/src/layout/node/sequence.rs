//! 时序图布局 (Sequence Layout)
//!
//! 专为时序图设计：参与者按声明顺序水平铺开，消息按声明顺序在垂直方向
//! 依次排列（每条消息占据一个时间步），并直接把边路径计算为水平直线。
//!
//! 与通用 [`crate::layout::edge_routing`] 不同，这里在布局阶段就生成边的几何
//! 信息，因此 [`crate::layout::compute_layout`] 会识别出"已带边"的布局结果，
//! 避免被通用边路由覆盖。
//!
//! 几何布局：
//!
//! ```text
//!      ┌───────┐   ┌───────┐   ┌───────┐
//!      │  P1   │   │  P2   │   │  P3   │   <- 参与者
//!      └───┬───┘   └───┬───┘   └───┬───┘
//!          │           │           │       <- 生命线
//!          │  msg 1 ──▶│           │
//!          │           │  msg 2 ──▶│
//!          │◀── msg 3 ─│           │
//!          │           │           │
//! ```
//!
//! # 边情况
//!
//! - **自调用**（`a -> a`）：消息源与目标相同，绘制为 U 形：从生命线右探出
//!   一小段后返回到生命线。
//! - **返回消息**（`-->`）：与主动消息共享同一时间步逻辑，箭头样式（虚线 +
//!   空心箭头）由渲染器根据 `Relation.arrow` 决定，布局阶段不区分。

use crate::types::DiagramType;
use crate::ast::{ArrowType, Diagram};
use crate::layout::algorithm_config::{SequenceLayoutConfig, SEQUENCE_LAYOUT_OPTIONS};
use crate::layout::geometry::Point;
use crate::layout::node::common::group_bounds::{self, GroupPadding};
use crate::layout::edge::common::label_avoidance::resolve_label_overlaps;
use crate::layout::plan::ResolvedAlgoOptions;
use crate::layout::constants;
use crate::layout::{AlgorithmOptionSpec, EdgeLabelLayout, EdgeLayout, LayoutResult, LayoutStrategy, NodeLayout, PathGeometry, Port};
use std::collections::HashMap;

// ─── 布局常量 ────────────────────────────────────────────

const APPLICABLE_TYPES: &[DiagramType] = &[DiagramType::Sequence];

/// 第一条消息距离参与者底部的距离
const FIRST_MESSAGE_OFFSET: f64 = 30.0;
/// 画布底部留白
const BOTTOM_PADDING: f64 = 30.0;
/// 自调用消息水平探出长度
const SELF_MESSAGE_WIDTH: f64 = 40.0;
/// 消息线端点相对生命线的内缩距离（避免与虚线竖线重叠）
pub const MESSAGE_LIFELINE_INSET: f64 = 4.0;
/// 生命线在消息交叉处单侧留空（总缺口 ≈ 2 × 此值）
pub const LIFELINE_MESSAGE_GAP_HALF: f64 = 5.0;

/// 时序图布局阶段写入、渲染阶段读取的提示信息
#[derive(Debug, Clone)]
pub struct SequenceLayoutHints {
    /// 参与者 id → 需留缺口的消息 y 坐标列表（已排序去重）
    pub lifeline_gaps: HashMap<String, Vec<f64>>,
}

impl SequenceLayoutHints {
    pub fn into_layout_hints(self) -> crate::layout::LayoutHints {
        crate::layout::LayoutHints {
            sequence: Some(self),
            edge_routing_style: crate::layout::EdgeRoutingStyle::SelfLoop,
            ..Default::default()
        }
    }
}

/// 时序图布局
pub struct SequenceLayout {
    config: SequenceLayoutConfig,
}

impl SequenceLayout {
    pub fn new(config: SequenceLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(SequenceLayoutConfig::from_options(options))
    }
}

impl Default for SequenceLayout {
    fn default() -> Self {
        Self::new(SequenceLayoutConfig::default())
    }
}

impl LayoutStrategy for SequenceLayout {
    fn name(&self) -> &'static str {
        "sequence"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        SEQUENCE_LAYOUT_OPTIONS
    }

    fn produces_edge_geometry(&self) -> bool {
        true
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        let config = self.config;
        let node_count = diagram.entities.len();

        // 1) 节点布局：按声明顺序水平铺开
        let mut nodes: HashMap<String, NodeLayout> = HashMap::new();
        let mut current_x = constants::DEFAULT_PADDING;
        for entity in &diagram.entities {
            let (w, h) = crate::layout::styled_node_size(entity, constants::DEFAULT_NODE_WIDTH, constants::DEFAULT_NODE_HEIGHT);
            nodes.insert(
                entity.id.as_str().to_string(),
                NodeLayout {
                    x: current_x,
                    y: constants::DEFAULT_PADDING,
                    width: w,
                    height: h,
                    ..Default::default()
                },
            );
            current_x += constants::DEFAULT_NODE_WIDTH + config.node_spacing;
        }
        let total_width = if node_count == 0 {
            constants::DEFAULT_PADDING * 2.0
        } else {
            current_x - config.node_spacing + constants::DEFAULT_PADDING
        };

        // 2) 分组包围框
        let groups = group_bounds::compute_group_bounds(
            diagram,
            &nodes,
            GroupPadding::uniform(config.group_padding, 16.0),
        );

        // 3) 边布局：按声明顺序分配时间步（y 坐标）
        let message_start_y = constants::DEFAULT_PADDING + constants::DEFAULT_NODE_HEIGHT + FIRST_MESSAGE_OFFSET;
        let mut edges: Vec<EdgeLayout> = Vec::with_capacity(diagram.relations.len());

        for (idx, rel) in diagram.relations.iter().enumerate() {
            let y = message_start_y + idx as f64 * config.message_spacing;

            // 找不到节点时输出空边，让上层 fallback
            let (from_nl, to_nl) = match (nodes.get(rel.from.as_str()), nodes.get(rel.to.as_str())) {
                (Some(f), Some(t)) => (f, t),
                _ => {
                    edges.push(EdgeLayout::empty());
                    continue;
                }
            };

            let from_cx = from_nl.x + from_nl.width / 2.0;
            let to_cx = to_nl.x + to_nl.width / 2.0;

            let geometry = if rel.from == rel.to {
                build_self_message_path(from_nl, y)
            } else {
                let from_port = if to_cx >= from_cx { Port::Right } else { Port::Left };
                let to_port = if to_cx >= from_cx { Port::Left } else { Port::Right };

                // 消息锚定在生命线上，端点内缩以与虚线竖线留出视觉间隙
                let (x1, x2) = inset_message_endpoints(from_cx, to_cx);
                let path = vec![Point::new(x1, y), Point::new(x2, y)];
                // 双向箭头用 Polyline 以便渲染层区分样式；其余用 Straight
                let geometry = if matches!(rel.arrow, ArrowType::Bidirectional) {
                    PathGeometry::Polyline { points: path }
                } else {
                    PathGeometry::Straight { start: Point::new(x1, y), end: Point::new(x2, y) }
                };
                EdgeGeometry::new(geometry, from_port, to_port)
            };

            let label_pos = label_position(&geometry.path_slice(), rel.arrow == ArrowType::Bidirectional);

            let labels = rel.label.as_ref()
                .map(|text| vec![EdgeLabelLayout::new(text, label_pos)])
                .unwrap_or_default();

            edges.push(EdgeLayout {
                geometry: geometry.geometry,
                labels,
                from_port: geometry.from_port,
                to_port: geometry.to_port,
            });
        }

        // 4) 标签统一避障（标签↔节点/分组/边路径/标签↔标签）
        resolve_label_overlaps(&mut edges, &nodes, &groups);

        // 5) 生命线缺口：收集每个参与者需避让的消息 y
        let lifeline_gaps = collect_lifeline_gaps(diagram, &edges);

        // 6) 画布高度 = 顶部到消息区底部 + 底部留白
        let message_area_height = if diagram.relations.is_empty() {
            FIRST_MESSAGE_OFFSET
        } else {
            FIRST_MESSAGE_OFFSET + (diagram.relations.len() as f64 - 1.0) * config.message_spacing
        };
        let total_height = constants::DEFAULT_PADDING + constants::DEFAULT_NODE_HEIGHT + message_area_height + BOTTOM_PADDING;

        LayoutResult {
            nodes,
            groups,
            edges,
            total_width,
            total_height,
            hints: SequenceLayoutHints { lifeline_gaps }.into_layout_hints(),
        }
    }
}

/// 边几何信息：路径几何 + 端口
struct EdgeGeometry {
    geometry: PathGeometry,
    from_port: Port,
    to_port: Port,
}

impl EdgeGeometry {
    fn new(geometry: PathGeometry, from_port: Port, to_port: Port) -> Self {
        Self {
            geometry,
            from_port,
            to_port,
        }
    }

    /// 返回路径锚点切片，供标签位置计算使用
    fn path_slice(&self) -> Vec<Point> {
        self.geometry.anchor_points().into_owned()
    }
}

/// 水平消息端点内缩：从发送方生命线略向外、在接收方生命线略向内结束。
fn inset_message_endpoints(from_cx: f64, to_cx: f64) -> (f64, f64) {
    if (to_cx - from_cx).abs() <= MESSAGE_LIFELINE_INSET * 2.0 {
        return (from_cx, to_cx);
    }
    if to_cx >= from_cx {
        (from_cx + MESSAGE_LIFELINE_INSET, to_cx - MESSAGE_LIFELINE_INSET)
    } else {
        (from_cx - MESSAGE_LIFELINE_INSET, to_cx + MESSAGE_LIFELINE_INSET)
    }
}

/// 收集每个参与者生命线上需留缺口的消息 y 坐标。
fn collect_lifeline_gaps(
    diagram: &Diagram,
    edges: &[EdgeLayout],
) -> HashMap<String, Vec<f64>> {
    let mut gaps: HashMap<String, Vec<f64>> = HashMap::new();

    for (idx, rel) in diagram.relations.iter().enumerate() {
        let Some(edge) = edges.get(idx) else { continue };
        if edge.path_is_empty() {
            continue;
        }

        let path = edge.path_points();
        let path_len = edge.path_len();
        let ys: Vec<f64> = if rel.from == rel.to && path_len == 4 {
            vec![path[0].y, path[3].y]
        } else {
            vec![path[0].y]
        };

        let ids: Vec<&str> = if rel.from == rel.to {
            vec![rel.from.as_str()]
        } else {
            vec![rel.from.as_str(), rel.to.as_str()]
        };

        for id in ids {
            let entry = gaps.entry(id.to_string()).or_default();
            for &y in &ys {
                if !entry.iter().any(|existing| (existing - y).abs() < 0.5) {
                    entry.push(y);
                }
            }
        }
    }

    for ys in gaps.values_mut() {
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    }
    gaps
}

/// 自调用消息：从生命线向右探出再返回到生命线，整体为 U 形。
fn build_self_message_path(nl: &NodeLayout, y: f64) -> EdgeGeometry {
    let cx = nl.x + nl.width / 2.0;
    let left = cx;
    let right = cx + SELF_MESSAGE_WIDTH;
    // 路径：起点 (生命线, y-6) → 右上 (right, y-6) → 终点 (生命线, y)
    let points = vec![Point::new(left, y - 6.0), Point::new(right, y - 6.0), Point::new(right, y), Point::new(left, y)];
    EdgeGeometry::new(
        PathGeometry::Polyline { points },
        Port::Top,
        Port::Top,
    )
}

/// 计算标签位置：水平消息放到线段中点上方；自调用放到 U 形右上方。
fn label_position(path: &[Point], _is_bidi: bool) -> Point {
    if path.is_empty() {
        return Point::new(0.0, 0.0);
    }
    if path.len() == 4 {
        // 自调用：放在 U 形右上角
        return Point::new(path[1].x + 4.0, path[1].y - 4.0);
    }
    if path.len() >= 2 {
        let p1 = path[0];
        let p2 = path[path.len() - 1];
        return Point::new((p1.x + p2.x) / 2.0, p1.y.min(p2.y) - 6.0);
    }
    Point::new(path[0].x, path[0].y - 6.0)
}

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Diagram, Entity, Identifier, Relation, Span, SourceInfo, AttributeMap};

    fn empty_diagram() -> Diagram {
        Diagram {
            diagram_type: DiagramType::Sequence,
            attributes: Vec::new(),
            entities: Vec::new(),
            relations: Vec::new(),
            groups: Vec::new(),
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    fn entity(id: &str) -> Entity {
        let span = Span::dummy();
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        }
    }

    fn rel(from: &str, to: &str, label: &str) -> Relation {
        let span = Span::dummy();
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: Some(label.to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        }
    }

    #[test]
    fn name_is_sequence() {
        assert_eq!(SequenceLayout::default().name(), "sequence");
    }

    #[test]
    fn empty_diagram_returns_minimal_box() {
        let layout = SequenceLayout::default().compute(&empty_diagram());
        assert!(layout.nodes.is_empty());
        assert!(layout.edges.is_empty());
        assert!(layout.total_height >= constants::DEFAULT_PADDING * 2.0);
    }

    #[test]
    fn participants_laid_out_horizontally() {
        let mut d = empty_diagram();
        d.entities.push(entity("a"));
        d.entities.push(entity("b"));
        d.entities.push(entity("c"));
        let layout = SequenceLayout::default().compute(&d);

        let a = layout.nodes.get("a").unwrap();
        let b = layout.nodes.get("b").unwrap();
        let c = layout.nodes.get("c").unwrap();

        // 都在同一 y
        assert_eq!(a.y, b.y);
        assert_eq!(b.y, c.y);
        // 严格递增 x
        assert!(b.x > a.x);
        assert!(c.x > b.x);
        // 等距
        let d_ab = b.x - a.x;
        let d_bc = c.x - b.x;
        assert!((d_ab - d_bc).abs() < 0.01);
    }

    #[test]
    fn messages_assigned_sequential_y() {
        let mut d = empty_diagram();
        d.entities.push(entity("a"));
        d.entities.push(entity("b"));
        d.relations.push(rel("a", "b", "m1"));
        d.relations.push(rel("a", "b", "m2"));
        d.relations.push(rel("a", "b", "m3"));

        let layout = SequenceLayout::default().compute(&d);
        assert_eq!(layout.edges.len(), 3);
        let ys: Vec<f64> = layout.edges.iter().filter_map(|e| e.path_start()).map(|p| p.y).collect();
        // 三条消息 y 严格递增
        assert!(ys[0] < ys[1]);
        assert!(ys[1] < ys[2]);
        // 等距
        assert!((ys[1] - ys[0] - constants::SEQUENCE_MESSAGE_SPACING).abs() < 0.01);
        assert!((ys[2] - ys[1] - constants::SEQUENCE_MESSAGE_SPACING).abs() < 0.01);
    }

    #[test]
    fn horizontal_message_uses_horizontal_line() {
        let mut d = empty_diagram();
        d.entities.push(entity("a"));
        d.entities.push(entity("b"));
        d.relations.push(rel("a", "b", "hi"));
        let layout = SequenceLayout::default().compute(&d);
        let edge = &layout.edges[0];
        let a = layout.nodes.get("a").unwrap();
        let b = layout.nodes.get("b").unwrap();
        let a_cx = a.x + a.width / 2.0;
        let b_cx = b.x + b.width / 2.0;
        let path = edge.path_points();
        // 端点相对生命线内缩
        assert!((path[0].x - (a_cx + MESSAGE_LIFELINE_INSET)).abs() < 0.01);
        assert!((path[1].x - (b_cx - MESSAGE_LIFELINE_INSET)).abs() < 0.01);
        assert!((path[0].y - path[1].y).abs() < 0.01);
        assert!(edge.is_straight());
    }

    #[test]
    fn lifeline_gaps_recorded_per_participant() {
        let mut d = empty_diagram();
        d.entities.push(entity("a"));
        d.entities.push(entity("b"));
        d.relations.push(rel("a", "b", "m1"));
        d.relations.push(rel("b", "a", "m2"));

        let layout = SequenceLayout::default().compute(&d);
        let hints = layout.hints.sequence.as_ref().unwrap();
        assert_eq!(hints.lifeline_gaps.get("a").map(|v| v.len()), Some(2));
        assert_eq!(hints.lifeline_gaps.get("b").map(|v| v.len()), Some(2));
    }

    #[test]
    fn self_message_uses_u_shape() {
        let mut d = empty_diagram();
        d.entities.push(entity("a"));
        d.relations.push(rel("a", "a", "loop"));
        let layout = SequenceLayout::default().compute(&d);
        let edge = &layout.edges[0];
        assert_eq!(edge.path_len(), 4);
        assert!(edge.is_polyline());
        // 起点和终点的 x 应相同（在生命线上）
        let path = edge.path_points();
        assert!((path[0].x - path[3].x).abs() < 0.01);
    }

    #[test]
    fn missing_endpoints_yield_empty_edge() {
        let mut d = empty_diagram();
        d.entities.push(entity("a"));
        d.relations.push(rel("a", "ghost", "nope"));
        let layout = SequenceLayout::default().compute(&d);
        assert!(layout.edges[0].path_is_empty());
    }

    #[test]
    fn total_height_grows_with_message_count() {
        let mut d = empty_diagram();
        d.entities.push(entity("a"));
        d.entities.push(entity("b"));
        d.relations.push(rel("a", "b", "m1"));
        let h1 = SequenceLayout::default().compute(&d).total_height;
        d.relations.push(rel("a", "b", "m2"));
        let h2 = SequenceLayout::default().compute(&d).total_height;
        d.relations.push(rel("a", "b", "m3"));
        let h3 = SequenceLayout::default().compute(&d).total_height;
        // 每多一条消息，总高度增加 message_spacing
        assert!((h2 - h1 - constants::SEQUENCE_MESSAGE_SPACING).abs() < 0.01);
        assert!((h3 - h2 - constants::SEQUENCE_MESSAGE_SPACING).abs() < 0.01);
    }
}
