//! Drawify 布局模块
//!
//! 提供可插拔的布局算法框架。每种布局算法实现 `LayoutStrategy` trait，
//! 通过 `compute_layout` 统一调度。
//!
//! ## 布局管线层级（Group Frame 三层模型）
//!
//! 详见 `docs/architecture/布局优化/group-frame-spec.md`（v0.2）。
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │ 拓扑主布局（Sugiyama / two_phase / group_divide）              │
//! │   L2 Intra Frame：组内节点排列（group_layout_hint）            │
//! ├──────────────────────────────────────────────────────────────┤
//! │ apply_geometric_refinement（Intent Pin / Align*）→ PinSet     │
//! ├──────────────────────────────────────────────────────────────┤
//! │ L3 Node Frame（grid_snap）：rank/layer 对齐 + 节点 8px 量化    │
//! ├──────────────────────────────────────────────────────────────┤
//! │ recompute group bounds from nodes（L3→L1 数据流桥梁）         │
//! ├──────────────────────────────────────────────────────────────┤
//! │ L1 Group Frame（group_frame）：组间排列/尺寸/对齐/间距/量化    │
//! │   apply_group_frame：Equal / border_align / quantize groups   │
//! ├──────────────────────────────────────────────────────────────┤
//! │ friendliness + route + refine（LayoutRouteFeedback）        │
//! ├──────────────────────────────────────────────────────────────┤
//! │ L1 Group Frame（幂等恢复）+ L3 waypoint snap                  │
//! └──────────────────────────────────────────────────────────────┘
//! ```

use crate::types::standard_attr_keys::diagram;
use crate::types::DiagramType;
use crate::ast::{AttributeValue, Diagram, Entity};
use crate::error::DiagnosticError;
use crate::profile::profile_for;
use self::geometry::Point;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;

pub mod algorithm_config;
pub mod catalog;
pub mod constants;
pub mod edge;
pub mod edge_postprocess;
pub mod friendliness;
pub mod geometry;
pub mod grid_snap;
pub mod group;
pub mod group_frame;
pub mod intent;
pub mod lint;
pub mod node;
pub mod plan;
pub mod perf;
pub mod pipeline;
pub mod postprocess;
pub mod refine;
pub mod registry;
pub mod route_feedback;

pub use intent::{
    GeometricIntent, IntentResult, IntentStatus, LayoutIntentOverlay, PinAxis,
    RefinementReport, TopologyIntent,
};

pub use algorithm_config::{
    AlgorithmOptionSpec, ArchitectureV2LayoutConfig, CircularLayoutConfig, ForceDirectedLayoutConfig,
    MindmapLayoutConfig, OptionKind, SequenceLayoutConfig, SugiyamaLayoutConfig,
};
pub use catalog::{
    layout_catalog, AlgorithmOptionInfo, DiagramTypeCatalog, EdgeRoutingAlgoInfo, LayoutAlgoInfo,
    LayoutCatalog,
};
pub use plan::{validate_layout_plan_warnings, FriendlinessMode, LayoutPlan, ResolvedAlgoOptions};
pub use lint::{
    lint_layout, parse_lint_profile, parse_lint_rule, parse_lint_rules_list, LayoutLinter,
    LayoutViolation, LintConfig, LintProfile, LintReport, LintRuleId, LintSeverity, RuleConfig,
};
pub use registry::{EDGE_ROUTING_NAMES, LAYOUT_ALGORITHM_NAMES};

// 向后兼容：保持 `crate::layout::sugiyama` 等路径可用
pub use edge::{
    edge_routing, edge_routing_bezier, edge_routing_circular,
    edge_routing_organic, edge_routing_orthogonal, edge_routing_spline, visibility,
};
pub use node::{
    architecture_v2, backup, circular, er, flowchart, force_directed, mindmap, sequence,
    sugiyama_v2,
};
pub use node::backup::sugiyama;

// ─── Layout 数据结构 ─────────────────────────────────────

/// 节点的布局信息
#[derive(Debug, Clone, Serialize)]
pub struct NodeLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for NodeLayout {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

/// 分组的布局信息（包围框）
#[derive(Debug, Clone, Serialize)]
pub struct GroupLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for GroupLayout {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

/// 连接端口方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize)]
pub enum Port {
    Top,
    Bottom,
    Left,
    Right,
}

/// 边的路径几何表达
///
/// 分离"路径几何"和"渲染采样结果"：算法输出几何表达，渲染层按需采样。
/// spline 障碍场景不再丢失贝塞尔控制点信息。
#[derive(Debug, Clone, Serialize)]
pub enum PathGeometry {
    Straight {
        start: Point,
        end: Point,
    },
    Bezier {
        start: Point,
        end: Point,
        controls: [Point; 2],
    },
    Polyline {
        points: Vec<Point>,
    },
}

impl PathGeometry {
    pub fn start(&self) -> Point {
        match self {
            PathGeometry::Straight { start, .. } => *start,
            PathGeometry::Bezier { start, .. } => *start,
            PathGeometry::Polyline { points } => points[0],
        }
    }

    pub fn end(&self) -> Point {
        match self {
            PathGeometry::Straight { end, .. } => *end,
            PathGeometry::Bezier { end, .. } => *end,
            PathGeometry::Polyline { points } => points[points.len() - 1],
        }
    }

    pub fn len(&self) -> usize {
        match self {
            PathGeometry::Straight { .. } => 2,
            PathGeometry::Bezier { .. } => 2,
            PathGeometry::Polyline { points } => points.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, PathGeometry::Polyline { points } if points.is_empty())
    }

    pub fn anchor_points(&self) -> Cow<'_, [Point]> {
        match self {
            PathGeometry::Straight { start, end } => Cow::Owned(vec![*start, *end]),
            PathGeometry::Bezier { start, end, .. } => Cow::Owned(vec![*start, *end]),
            PathGeometry::Polyline { points } => Cow::Borrowed(points),
        }
    }

    pub fn bezier_controls(&self) -> Option<[Point; 2]> {
        match self {
            PathGeometry::Bezier { controls, .. } => Some(*controls),
            _ => None,
        }
    }

    pub fn polyline_points(&self) -> Option<&[Point]> {
        match self {
            PathGeometry::Polyline { points } => Some(points),
            _ => None,
        }
    }

    pub fn polyline_points_mut(&mut self) -> Option<&mut Vec<Point>> {
        match self {
            PathGeometry::Polyline { points } => Some(points),
            _ => None,
        }
    }

    pub fn is_bezier(&self) -> bool {
        matches!(self, PathGeometry::Bezier { .. })
    }

    pub fn is_polyline(&self) -> bool {
        matches!(self, PathGeometry::Polyline { .. })
    }

    pub fn is_straight(&self) -> bool {
        matches!(self, PathGeometry::Straight { .. })
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        match self {
            PathGeometry::Straight { start, end } => {
                start.x += dx;
                start.y += dy;
                end.x += dx;
                end.y += dy;
            }
            PathGeometry::Bezier { start, end, controls } => {
                start.x += dx;
                start.y += dy;
                end.x += dx;
                end.y += dy;
                for cp in controls.iter_mut() {
                    cp.x += dx;
                    cp.y += dy;
                }
            }
            PathGeometry::Polyline { points } => {
                for pt in points.iter_mut() {
                    pt.x += dx;
                    pt.y += dy;
                }
            }
        }
    }

    pub fn sample(&self, steps: usize) -> Vec<Point> {
        match self {
            PathGeometry::Bezier { start, end, controls } => {
                let p0 = *start;
                let p3 = *end;
                let p1 = controls[0];
                let p2 = controls[1];
                let steps = steps.max(2);
                (0..=steps)
                    .map(|i| {
                        let t = i as f64 / steps as f64;
                        let u = 1.0 - t;
                        let x = u * u * u * p0.x
                            + 3.0 * u * u * t * p1.x
                            + 3.0 * u * t * t * p2.x
                            + t * t * t * p3.x;
                        let y = u * u * u * p0.y
                            + 3.0 * u * u * t * p1.y
                            + 3.0 * u * t * t * p2.y
                            + t * t * t * p3.y;
                        Point::new(x, y)
                    })
                    .collect()
            }
            _ => self.anchor_points().into_owned(),
        }
    }
}

/// 边标签布局信息。
///
/// 每条边可携带 0 个或多个标签。`center` 统一为标签包围框的几何中心
/// （不再有"基线"与"中心"的歧义）。`size` 是含 padding 的实际包围框尺寸，
/// 由路由阶段预计算，避障与渲染共用。
#[derive(Debug, Clone, Serialize)]
pub struct EdgeLabelLayout {
    pub text: String,
    pub center: Point,
    pub size: (f64, f64),
    pub leader_to: Option<Point>,
    pub rotation: f64,
}

impl EdgeLabelLayout {
    pub fn new(text: impl Into<String>, center: Point) -> Self {
        let text = text.into();
        let size = crate::layout::edge::common::label_avoidance::label_metrics(&text);
        Self {
            text,
            center,
            size,
            leader_to: None,
            rotation: 0.0,
        }
    }

    pub fn with_size(text: impl Into<String>, center: Point, size: (f64, f64)) -> Self {
        Self {
            text: text.into(),
            center,
            size,
            leader_to: None,
            rotation: 0.0,
        }
    }

    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        let (w, h) = self.size;
        (
            self.center.x - w / 2.0,
            self.center.y - h / 2.0,
            self.center.x + w / 2.0,
            self.center.y + h / 2.0,
        )
    }
}

/// 边的布局信息
#[derive(Debug, Clone, Serialize)]
pub struct EdgeLayout {
    /// 路径几何表达
    pub geometry: PathGeometry,
    /// 边的标签列表（通常 0 或 1 个；P2 支持多 label）
    pub labels: Vec<EdgeLabelLayout>,
    /// 起点端口
    pub from_port: Port,
    /// 终点端口
    pub to_port: Port,
}

impl EdgeLayout {
    pub fn empty() -> Self {
        Self {
            geometry: PathGeometry::Polyline { points: vec![] },
            labels: Vec::new(),
            from_port: Port::Bottom,
            to_port: Port::Top,
        }
    }

    pub fn label_pos(&self) -> Point {
        self.labels.first().map(|l| l.center).unwrap_or(Point::zero())
    }

    pub fn set_label_pos(&mut self, pos: Point) {
        if let Some(l) = self.labels.first_mut() {
            l.center = pos;
        }
    }

    pub fn label_bbox(&self) -> (f64, f64, f64, f64) {
        self.labels.first().map(|l| l.bbox()).unwrap_or((0.0, 0.0, 0.0, 0.0))
    }

    pub fn has_label(&self) -> bool {
        !self.labels.is_empty()
    }

    pub fn label_count(&self) -> usize {
        self.labels.len()
    }

    pub fn label_pos_at(&self, idx: usize) -> Option<Point> {
        self.labels.get(idx).map(|l| l.center)
    }

    pub fn set_label_pos_at(&mut self, idx: usize, pos: Point) {
        if let Some(l) = self.labels.get_mut(idx) {
            l.center = pos;
        }
    }

    pub fn label_bbox_at(&self, idx: usize) -> Option<(f64, f64, f64, f64)> {
        self.labels.get(idx).map(|l| l.bbox())
    }

    pub fn path_len(&self) -> usize {
        self.geometry.len()
    }

    pub fn path_is_empty(&self) -> bool {
        self.geometry.is_empty()
    }

    pub fn path_start(&self) -> Option<Point> {
        if self.path_is_empty() {
            None
        } else {
            Some(self.geometry.start())
        }
    }

    pub fn path_end(&self) -> Option<Point> {
        if self.path_is_empty() {
            None
        } else {
            Some(self.geometry.end())
        }
    }

    pub fn path_points(&self) -> Cow<'_, [Point]> {
        self.geometry.anchor_points()
    }

    pub fn sampled_path(&self, steps: usize) -> Vec<Point> {
        self.geometry.sample(steps)
    }

    pub fn is_bezier(&self) -> bool {
        self.geometry.is_bezier()
    }

    pub fn is_polyline(&self) -> bool {
        self.geometry.is_polyline()
    }

    pub fn is_straight(&self) -> bool {
        self.geometry.is_straight()
    }

    pub fn bezier_controls(&self) -> Option<[Point; 2]> {
        self.geometry.bezier_controls()
    }

    pub fn polyline_points(&self) -> Option<&[Point]> {
        self.geometry.polyline_points()
    }

    pub fn polyline_points_mut(&mut self) -> Option<&mut Vec<Point>> {
        self.geometry.polyline_points_mut()
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.geometry.translate(dx, dy);
        for label in &mut self.labels {
            label.center.x += dx;
            label.center.y += dy;
            if let Some(p) = &mut label.leader_to {
                p.x += dx;
                p.y += dy;
            }
        }
    }

    pub fn set_polyline_points(&mut self, points: Vec<Point>) {
        self.geometry = if points.len() <= 2 {
            PathGeometry::Straight {
                start: points[0],
                end: points[1],
            }
        } else {
            PathGeometry::Polyline { points }
        };
    }
}

/// 布局算法对边路由风格的推荐
///
/// 布局算法在 `compute` 阶段写入此 hint，供下游（plan 解析器、渲染器、调试工具）
/// 读取。用户显式配置 `edge_routing:` 时覆盖此推荐；未配置时可作为默认回退。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeRoutingStyle {
    /// 未指定，由用户配置或 profile 默认决定
    #[default]
    Unspecified,
    /// 正交折线（Sugiyama 分层、architecture 等结构化布局）
    Orthogonal,
    /// 直线（force_directed 等自由布局）
    Straight,
    /// 平滑曲线（circular 环边、mindmap 等弧形布局）
    Curved,
    /// 自环 + 折线（sequence 自调用消息）
    SelfLoop,
}

/// 布局阶段产出的提示信息，供边路由读取
#[derive(Debug, Clone, Default)]
pub struct LayoutHints {
    pub circular: Option<node::circular::CircularLayoutHints>,
    pub sequence: Option<node::sequence::SequenceLayoutHints>,
    /// 布局算法对边路由风格的推荐（用户显式配置时覆盖此值）
    pub edge_routing_style: EdgeRoutingStyle,
    /// Sugiyama 系列布局产出的节点 rank 映射（entity_id → rank）。
    ///
    /// 供路由友好性评估的"长边跨层度"度量使用（见 `friendliness::long_edge`）。
    /// 非 Sugiyama 布局为 `None`。
    pub sugiyama_ranks: Option<HashMap<String, usize>>,
    /// MindMap 布局产出的节点深度映射（entity_id → depth）。
    ///
    /// 根节点 depth = 0，一级分支 depth = 1，依此类推。
    /// 供 organic 边路由根据层级动态调整曲线弧度，深层级更平缓。
    /// 非 MindMap 布局为 `None`。
    pub mindmap_depths: Option<HashMap<String, usize>>,
    /// 被布局策略跳过的拓扑意图索引列表（如 Architecture V2 跳过跨组意图）。
    ///
    /// 调度层据此将这些意图标记为 `Partial`，而非依赖 rank 比对给出误导性消息。
    pub skipped_topology_intents: Vec<usize>,
    /// 路由友好性评估报告（V1 诊断模式输出）。
    ///
    /// 在 `compute_layout_with_plan` 中、`router.route` 之前由
    /// `friendliness::RoutingFriendlinessEvaluator` 填充。
    pub friendliness_report: Option<friendliness::FriendlinessReport>,
    /// 分组布局警告：group 包围框互相重叠或框内含非组节点。
    ///
    /// 由 Sugiyama 系列布局在 `compute_group_bounds` 后检测填充。
    /// 流程图布局 group 不参与布局（仅事后画框），此类警告用于诊断
    /// "group 框拉得很长/互相压住"的视觉问题。
    pub group_layout_warnings: Vec<GroupLayoutWarning>,
    /// refine 调试统计（P2-1 可观测性）。
    ///
    /// 由 `refine::run_refine` 在执行后填充；未启用 refine 时为 `None`。
    /// 供 bench 工具和诊断使用，不影响布局结果。
    pub refine_debug: Option<RefineDebugStats>,
    /// orthogonal 路由调试统计（P2-1 可观测性）。
    ///
    /// 由 `route_edges_orthogonal` 在执行后填充；非 orthogonal 路由为 `None`。
    /// 供 bench 工具和诊断使用，不影响布局结果。
    pub orthogonal_debug: Option<OrthoDebugStats>,
    /// 分组路由提示：组间走廊 + 边框壳层厚度（architecture 等含 group 的图）。
    pub group_routing: Option<group::GroupRoutingHints>,
    /// Edge Bundling 结果与调试统计（§7.2）。
    ///
    /// 由 `EdgeBundler::apply` 在执行后填充；未启用 bundling 时为 `None`。
    /// 包含 bundle 分组、路径区段分解（edge_roles）、主干禁放区（trunk_keepouts），
    /// 供后置 label 流水线与渲染层查询。
    pub edge_bundling: Option<edge::edge_bundling::EdgeBundlingHints>,
}

/// refine 调试统计（P2-1 可观测性）
#[derive(Debug, Clone, Default)]
pub struct RefineDebugStats {
    /// refine 推动节点的总次数（所有轮次累计）
    pub push_count: usize,
    /// momentum 方向反转次数（所有轮次累计）
    pub momentum_reversals: usize,
    /// refine 实际执行的轮次
    pub passes_executed: usize,
}

/// orthogonal 路由调试统计（P2-1 可观测性）
#[derive(Debug, Clone, Default)]
pub struct OrthoDebugStats {
    /// 硬过滤拒绝的候选路径总数（穿障候选被丢弃）
    pub hard_filter_reject_count: usize,
    /// 生成的候选路径总数（含被拒绝的）
    pub total_candidates: usize,
    /// 退化边数：所有干净候选均被硬过滤拒绝，退化为最低惩罚脏候选
    pub degraded_count: usize,
    /// 路由的边总数
    pub edge_count: usize,
}

impl OrthoDebugStats {
    /// 硬过滤拒绝率（0.0-1.0）
    pub fn hard_filter_reject_rate(&self) -> f64 {
        if self.total_candidates == 0 {
            0.0
        } else {
            self.hard_filter_reject_count as f64 / self.total_candidates as f64
        }
    }

    /// 每条边平均候选数
    pub fn avg_candidates_per_edge(&self) -> f64 {
        if self.edge_count == 0 {
            0.0
        } else {
            self.total_candidates as f64 / self.edge_count as f64
        }
    }
}

/// 分组布局警告
#[derive(Debug, Clone, PartialEq)]
pub struct GroupLayoutWarning {
    /// 警告类型
    pub kind: GroupLayoutWarningKind,
    /// 主分组 ID
    pub group_id: String,
    /// 关联实体 ID（其他分组 ID 或节点 ID）
    pub other_id: String,
    /// 重叠面积（像素²），用于评估严重程度
    pub overlap_area: f64,
}

/// 分组布局警告类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GroupLayoutWarningKind {
    /// 两个非嵌套分组的包围框互相重叠
    GroupOverlap,
    /// 非组成员节点落在分组包围框内
    ForeignNodeInside,
}

/// 布局计算结果
#[derive(Debug, Clone)]
pub struct LayoutResult {
    pub nodes: HashMap<String, NodeLayout>,
    pub groups: HashMap<String, GroupLayout>,
    pub edges: Vec<EdgeLayout>,
    pub total_width: f64,
    pub total_height: f64,
    pub hints: LayoutHints,
}

/// 分组包含性违规：节点或子组超出所属分组的边界
#[derive(Debug, Clone, PartialEq)]
pub struct GroupContainmentViolation {
    /// 分组 ID
    pub group_id: String,
    /// 违规实体 ID（节点或子组）
    pub entity_id: String,
    /// 违规类型
    pub kind: ContainmentViolationKind,
    /// 超出距离（像素）
    pub excess: f64,
}

/// 违规方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainmentViolationKind {
    /// 节点/子组顶部在分组上方
    TopOverflow,
    /// 节点/子组底部在分组下方
    BottomOverflow,
    /// 节点/子组左侧在分组左方
    LeftOverflow,
    /// 节点/子组右侧在分组右方
    RightOverflow,
}

impl LayoutResult {
    /// 检查所有节点和子组是否在所属分组的边界内。
    ///
    /// 返回违规列表。空列表表示所有实体都在分组内。
    /// 容差 1.0px，用于浮点精度补偿。
    pub fn validate_group_containment(&self, diagram: &Diagram) -> Vec<GroupContainmentViolation> {
        const TOLERANCE: f64 = 1.0;
        let mut violations = Vec::new();

        for group in &diagram.groups {
            let Some(gl) = self.groups.get(group.id.as_str()) else {
                continue;
            };
            let g_left = gl.x;
            let g_top = gl.y;
            let g_right = gl.x + gl.width;
            let g_bottom = gl.y + gl.height;

            // 检查直接实体节点
            for eid in &group.entity_ids {
                let Some(nl) = self.nodes.get(eid.as_str()) else {
                    continue;
                };
                let n_left = nl.x;
                let n_top = nl.y;
                let n_right = nl.x + nl.width;
                let n_bottom = nl.y + nl.height;

                if n_top < g_top - TOLERANCE {
                    violations.push(GroupContainmentViolation {
                        group_id: group.id.as_str().to_string(),
                        entity_id: eid.as_str().to_string(),
                        kind: ContainmentViolationKind::TopOverflow,
                        excess: g_top - n_top,
                    });
                }
                if n_bottom > g_bottom + TOLERANCE {
                    violations.push(GroupContainmentViolation {
                        group_id: group.id.as_str().to_string(),
                        entity_id: eid.as_str().to_string(),
                        kind: ContainmentViolationKind::BottomOverflow,
                        excess: n_bottom - g_bottom,
                    });
                }
                if n_left < g_left - TOLERANCE {
                    violations.push(GroupContainmentViolation {
                        group_id: group.id.as_str().to_string(),
                        entity_id: eid.as_str().to_string(),
                        kind: ContainmentViolationKind::LeftOverflow,
                        excess: g_left - n_left,
                    });
                }
                if n_right > g_right + TOLERANCE {
                    violations.push(GroupContainmentViolation {
                        group_id: group.id.as_str().to_string(),
                        entity_id: eid.as_str().to_string(),
                        kind: ContainmentViolationKind::RightOverflow,
                        excess: n_right - g_right,
                    });
                }
            }

            // 检查子组是否在父组内
            for child_gid in &group.child_group_ids {
                let Some(child_gl) = self.groups.get(child_gid.as_str()) else {
                    continue;
                };
                let c_left = child_gl.x;
                let c_top = child_gl.y;
                let c_right = child_gl.x + child_gl.width;
                let c_bottom = child_gl.y + child_gl.height;

                if c_top < g_top - TOLERANCE {
                    violations.push(GroupContainmentViolation {
                        group_id: group.id.as_str().to_string(),
                        entity_id: child_gid.as_str().to_string(),
                        kind: ContainmentViolationKind::TopOverflow,
                        excess: g_top - c_top,
                    });
                }
                if c_bottom > g_bottom + TOLERANCE {
                    violations.push(GroupContainmentViolation {
                        group_id: group.id.as_str().to_string(),
                        entity_id: child_gid.as_str().to_string(),
                        kind: ContainmentViolationKind::BottomOverflow,
                        excess: c_bottom - g_bottom,
                    });
                }
                if c_left < g_left - TOLERANCE {
                    violations.push(GroupContainmentViolation {
                        group_id: group.id.as_str().to_string(),
                        entity_id: child_gid.as_str().to_string(),
                        kind: ContainmentViolationKind::LeftOverflow,
                        excess: g_left - c_left,
                    });
                }
                if c_right > g_right + TOLERANCE {
                    violations.push(GroupContainmentViolation {
                        group_id: group.id.as_str().to_string(),
                        entity_id: child_gid.as_str().to_string(),
                        kind: ContainmentViolationKind::RightOverflow,
                        excess: c_right - g_right,
                    });
                }
            }
        }

        violations
    }
}

// ─── Layout Trait ────────────────────────────────────────

/// 布局策略 trait
///
/// 所有布局算法都需要实现此 trait。
pub trait LayoutStrategy {
    /// 算法名称
    fn name(&self) -> &'static str;

    /// 根据 Diagram 计算布局（节点 + 分组）
    fn compute(&self, diagram: &Diagram) -> LayoutResult;

    /// 支持意图叠加层的布局入口。
    ///
    /// 默认实现：忽略 `valid_topology`，直接委托 [`compute`](Self::compute)。
    /// 需要原生消费拓扑意图的算法（SugiyamaV2 / Flowchart / Er / ArchitectureV2）
    /// 覆写此方法，在 `build_graph` 阶段注入约束边并保护意图边不被 FAS 反转。
    ///
    /// `valid_topology` 为 `None` 时必须与 `compute` 行为完全一致（既有测试不变）。
    /// `valid_topology` 为 `Some` 时，其中的意图已通过 `validate_topology_intents`
    /// 校验（节点存在、无环、无矛盾），可直接注入。
    fn compute_with_overlay(
        &self,
        diagram: &Diagram,
        valid_topology: Option<&[intent::topology::ValidTopologyIntent]>,
    ) -> LayoutResult {
        let _ = valid_topology;
        self.compute(diagram)
    }

    /// 该布局算法是否在 `compute` 阶段自行产出边几何信息。
    ///
    /// 返回 `true` 时，`compute_layout` 将跳过通用边路由后处理，
    /// 避免覆盖已经精心计算好的路径。
    ///
    /// 当前返回 `true` 的布局：`sequence`。
    /// 其他布局返回 `false`（默认），由 `EdgeRoutingStrategy` 统一计算边路径。
    fn produces_edge_geometry(&self) -> bool {
        false
    }

    /// 该算法适用的内置图表类型列表。
    ///
    /// 算法是自身适用范围的权威：由算法声明自己适合哪些图类型，
    /// 而非由图类型集中罗列适用算法。
    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[]
    }

    /// 是否支持 Custom 图表类型（默认 false）。
    ///
    /// Custom 类型无法放入 `applicable_diagram_types` 的静态数组，
    /// 因此单独声明。
    fn supports_custom(&self) -> bool {
        false
    }

    /// 判断是否支持指定的图表类型。
    ///
    /// 默认实现：Custom 类型走 `supports_custom()`，其余走 `applicable_diagram_types()` 包含判断。
    fn supports_diagram_type(&self, diagram_type: &DiagramType) -> bool {
        match diagram_type {
            DiagramType::Custom(_) => self.supports_custom(),
            other => self.applicable_diagram_types().contains(other),
        }
    }

    /// 该算法支持的 DSL 配置块 option 列表。
    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        &[]
    }

    /// 该布局算法支持的方向列表。
    ///
    /// 空切片表示不消费 diagram 级 `direction`（如 sequence、circular）。
    /// 非空时，`validate_layout_config` 会校验 effective direction 是否在支持列表中。
    fn supported_directions(&self) -> &'static [&'static str] {
        &[]
    }
}

// ─── EdgeRouting Trait ───────────────────────────────────

/// 边路由策略 trait
///
/// 所有边路由算法都需要实现此 trait。
/// 在节点布局完成后，为每条边计算几何路径与标签位置。
pub trait EdgeRoutingStrategy {
    /// 算法名称
    fn name(&self) -> &'static str;

    /// 在节点布局完成后，为所有边计算几何路径
    fn route(&self, diagram: &Diagram, result: LayoutResult) -> LayoutResult;

    /// 该路由算法适用的内置图表类型列表。
    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[]
    }

    /// 是否支持 Custom 图表类型（默认 false）。
    fn supports_custom(&self) -> bool {
        false
    }

    /// 判断是否支持指定的图表类型。
    fn supports_diagram_type(&self, diagram_type: &DiagramType) -> bool {
        match diagram_type {
            DiagramType::Custom(_) => self.supports_custom(),
            other => self.applicable_diagram_types().contains(other),
        }
    }

    /// 该算法支持的 DSL 配置块 option 列表。
    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        &[]
    }

    /// 是否输出可被 refine 后处理消费的 Polyline 路径。
    ///
    /// refine（`refine::run_refine`）只检测 `PathGeometry::Polyline` 的穿障情况，
    /// 对直线 / 贝塞尔路径是空跑。返回 `false` 的 router 不会进入 refine 循环。
    /// 默认 `false`，需要 refine 的 router（如 spline / orthogonal）覆写为 `true`。
    fn supports_refine(&self) -> bool {
        false
    }

    /// 是否需要避障索引（用于调度层决定是否预建全图障碍索引）。
    ///
    /// 当前仅 spline 路由会用到可见性图避障；其他 router 返回 `false`，
    /// 调度层据此跳过 `ObstacleIndex` 的构建开销。S3 阶段接入。
    fn needs_obstacle_index(&self) -> bool {
        false
    }

    /// 节点位移后的增量重路由（默认回退为全图重路由）。
    ///
    /// 正交路由覆写为仅重路由端点落在 `moved_node_ids` 上的边，
    /// 其余边保留已有路径并作为已路由段参与避让。
    fn route_after_node_moves(
        &self,
        diagram: &Diagram,
        result: LayoutResult,
        moved_node_ids: &std::collections::HashSet<String>,
    ) -> LayoutResult {
        let _ = moved_node_ids;
        self.route(diagram, result)
    }

    /// refine 增量重路由：保留 `preserve_edges` 中的已有路径，仅重算其余边。
    ///
    /// 默认实现回退为全图重路由；orthogonal 覆写为 `preserve_edges` 增量模式。
    fn route_preserve(
        &self,
        diagram: &Diagram,
        result: LayoutResult,
        preserve_edges: &std::collections::HashSet<usize>,
    ) -> LayoutResult {
        let _ = preserve_edges;
        self.route(diagram, result)
    }
}

// ─── Layout 调度 ─────────────────────────────────────────

/// 根据 diagram 配置计算布局（统一入口）
///
/// 支持多种布局算法：
/// - "flowchart": 流程图专属分层布局（共享 sugiyama-v2 引擎）
/// - "er": ER 图专属分层布局（共享 sugiyama-v2 引擎）
/// - "state": 状态图专属布局（共享 circular 引擎）
/// - "architecture": 架构图专属布局（分组感知两阶段）
/// - "mindmap": 思维导图专属布局（中心辐射 / 单向树）
/// - "sequence": 时序图专属布局
/// - "sugiyama-v2": 通用 Sugiyama 分层布局（高级选项）
/// - "circular": 通用自适应圆形布局（高级选项）
/// - "force-directed": 通用力导向布局（高级选项）
/// - "sugiyama": Sugiyama 分层算法（旧版）
///
/// 可通过 diagram 属性 `layout_algo: 算法名` 切换。
///
/// 支持多种边路由算法：
/// - "orthogonal": 正交路由（折线路由）（默认）
/// - "straight": 直线路由
/// - "bezier": 贝塞尔曲线路由
/// - "spline": 障碍避让多段样条路由（可见性图 + Catmull-Rom → 多段贝塞尔，C1 连续）
/// - "circular": 弧形边路由（圆形布局专用）
/// 节点布局完成后自动执行边路由。
///
/// 解析 diagram 的有效布局方向。
///
/// - 若 AST 显式声明 `direction` → 返回该显式值
/// - 否则若 profile.default_direction 为 `Some` → 返回 profile 默认
/// - 否则 → `None`（该图不参与 direction 体系）
///
/// 所有布局代码统一通过此函数获取方向，禁止自行判断 AST 里有没有 direction 属性。
pub fn resolve_effective_direction<'a>(diagram: &'a Diagram) -> Option<&'a str> {
    if diagram.direction_attr().is_some() {
        return diagram.direction_attr();
    }
    profile_for(&diagram.diagram_type).default_direction
}

/// 布局配置的语义校验（算法是否存在、是否适用于当前图表类型）在此执行。
pub fn compute_layout(
    diagram: &Diagram,
) -> std::result::Result<LayoutResult, DiagnosticError> {
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(diagram, profile);
    compute_layout_with_plan(diagram, &plan)
}

/// 使用已解析的 [`LayoutPlan`] 计算布局（`PreparedDiagram` 在 prepare 阶段已解析 plan 时走此路径）。
///
/// 等价于 `compute_layout_with_plan_and_overlay(diagram, plan, None)` 的布局部分（丢弃空报告）。
pub fn compute_layout_with_plan(
    diagram: &Diagram,
    plan: &LayoutPlan,
) -> std::result::Result<LayoutResult, DiagnosticError> {
    compute_layout_with_plan_and_overlay(diagram, plan, None).map(|(r, _)| r)
}

/// 带意图叠加层的布局入口。
///
/// 与 [`compute_layout_with_plan`] 的差异：
/// - `overlay` 为 `None` 时行为与 [`compute_layout_with_plan`] 完全一致（既有测试不变）。
/// - `overlay` 为 `Some` 时：
///   - 拓扑意图由 `strategy.compute_with_overlay` 在布局内部消费（P1）。
///   - 几何意图由 `apply_geometric_refinement` 在 grid snap 前消费（P1.5）。
/// - 返回值额外携带 [`RefinementReport`]，汇总每条意图的满足状态。
///   `overlay` 为 `None` 时报告为 `None`。
///
/// `diagram` 不会被变异，`relations[i] ↔ edges[i]` 索引契约保持不变。
pub fn compute_layout_with_plan_and_overlay(
    diagram: &Diagram,
    plan: &LayoutPlan,
    overlay: Option<&LayoutIntentOverlay>,
) -> std::result::Result<(LayoutResult, Option<RefinementReport>), DiagnosticError> {
    validate_layout_config(diagram)?;
    pipeline::LayoutPipeline::new(diagram, plan, overlay).run()
}

fn layout_strategy_for(algo: &str) -> Option<Box<dyn LayoutStrategy>> {
    registry::build_layout_strategy(algo, &LayoutPlan::default_for_catalog())
}

fn edge_routing_strategy_for(algo: &str) -> Option<Box<dyn EdgeRoutingStrategy>> {
    registry::build_edge_routing_strategy(algo, &LayoutPlan::catalog_edge_plan(algo))
}

// ─── 算法注册表查询 ─────────────────────────────────────

/// 所有内置图表类型（不含 Custom，Custom 需单独通过 `supports_custom` 判断）
pub const BUILTIN_DIAGRAM_TYPES: &[DiagramType] = &[
    DiagramType::Flowchart,
    DiagramType::Sequence,
    DiagramType::Architecture,
    DiagramType::State,
    DiagramType::Er,
    DiagramType::Mindmap,
];

/// 返回所有已注册的布局策略实例（catalog / 元数据查询）。
pub(super) fn all_layout_strategies() -> Vec<Box<dyn LayoutStrategy>> {
    registry::all_layout_strategies()
}

/// 返回所有已注册的边路由策略实例（catalog / 元数据查询，使用 spec 默认值）。
pub(super) fn all_routing_strategies() -> Vec<Box<dyn EdgeRoutingStrategy>> {
    registry::all_routing_strategies()
}

/// 查询布局算法的 option 元数据。
pub fn layout_option_specs(algo: &str) -> &'static [AlgorithmOptionSpec] {
    layout_strategy_for(algo)
        .map(|s| s.option_specs())
        .unwrap_or(&[])
}

/// 查询边路由算法的 option 元数据。
pub fn edge_routing_option_specs(algo: &str) -> &'static [AlgorithmOptionSpec] {
    edge_routing_strategy_for(algo)
        .map(|s| s.option_specs())
        .unwrap_or(&[])
}

/// 正向查询：指定图表类型适用的布局算法名称列表。
pub fn applicable_layouts_for_type(diagram_type: &DiagramType) -> Vec<&'static str> {
    all_layout_strategies()
        .into_iter()
        .filter(|s| s.supports_diagram_type(diagram_type))
        .map(|s| s.name())
        .collect()
}

/// 正向查询：指定图表类型适用的边路由算法名称列表。
pub fn applicable_routings_for_type(diagram_type: &DiagramType) -> Vec<&'static str> {
    all_routing_strategies()
        .into_iter()
        .filter(|s| s.supports_diagram_type(diagram_type))
        .map(|s| s.name())
        .collect()
}

/// 反向查询：指定布局算法适用的图表类型列表。
pub fn diagram_types_for_layout(algo: &str) -> Vec<DiagramType> {
    let mut types: Vec<DiagramType> = BUILTIN_DIAGRAM_TYPES
        .iter()
        .filter(|dt| {
            all_layout_strategies()
                .into_iter()
                .any(|s| s.name() == algo && s.supports_diagram_type(dt))
        })
        .cloned()
        .collect();

    // 检查是否支持 Custom
    if all_layout_strategies()
        .into_iter()
        .any(|s| s.name() == algo && s.supports_custom())
    {
        types.push(DiagramType::Custom(String::new()));
    }

    types
}

/// 反向查询：指定边路由算法适用的图表类型列表。
pub fn diagram_types_for_routing(algo: &str) -> Vec<DiagramType> {
    let mut types: Vec<DiagramType> = BUILTIN_DIAGRAM_TYPES
        .iter()
        .filter(|dt| {
            all_routing_strategies()
                .into_iter()
                .any(|s| s.name() == algo && s.supports_diagram_type(dt))
        })
        .cloned()
        .collect();

    if all_routing_strategies()
        .into_iter()
        .any(|s| s.name() == algo && s.supports_custom())
    {
        types.push(DiagramType::Custom(String::new()));
    }

    types
}

pub(crate) fn known_layout_algo_names() -> Vec<&'static str> {
    registry::LAYOUT_ALGORITHM_NAMES.to_vec()
}

pub(crate) fn known_edge_routing_names() -> Vec<&'static str> {
    registry::EDGE_ROUTING_NAMES.to_vec()
}

fn layout_attr_span(diagram: &Diagram, key: &str) -> crate::ast::Span {
    diagram
        .attributes
        .iter()
        .find(|a| a.key == key)
        .map(|a| a.span)
        .unwrap_or_else(crate::ast::Span::dummy)
}

pub(crate) fn layout_config_error(
    diagram: &Diagram,
    key: &str,
    value: &str,
    known: &[&str],
) -> DiagnosticError {
    DiagnosticError::invalid_enum_value(layout_attr_span(diagram, key), key, value, known)
}

const VALID_DIRECTION_ATOMS: &[&str] = &[
    crate::types::attr_constants::direction::TOP_TO_BOTTOM,
    crate::types::attr_constants::direction::LEFT_TO_RIGHT,
    crate::types::attr_constants::direction::RADIAL,
];

/// 校验 diagram 级布局配置（算法名、路由名、方向、方向×布局交叉校验）。
///
/// 在 `compute_layout` 执行前调用；DSL 层只保证 atom 类型。
fn validate_layout_config(diagram: &Diagram) -> std::result::Result<(), DiagnosticError> {
    // 1. 逐属性基础校验
    for attr in &diagram.attributes {
        let Some(name) = attr.value.algorithm_name() else {
            continue;
        };

        match attr.key.as_str() {
            diagram::DIRECTION => {
                if !VALID_DIRECTION_ATOMS.contains(&name) {
                    return Err(DiagnosticError::invalid_enum_value(
                        attr.span,
                        diagram::DIRECTION,
                        name,
                        VALID_DIRECTION_ATOMS,
                    ));
                }
            }
            diagram::LAYOUT => validate_registered_layout_algo(diagram, attr.span, name)?,
            diagram::EDGE_ROUTING => {
                if applicable_routings_for_type(&diagram.diagram_type).is_empty() {
                    return Err(DiagnosticError::structure_violation(
                        attr.span,
                        format!(
                            "diagram type '{}' does not support edge_routing; \
                             message paths are computed by layout: sequence",
                            profile_for(&diagram.diagram_type).name,
                        ),
                    ));
                }
                validate_registered_edge_routing(diagram, attr.span, name)?;
            }
            _ => {}
        }
    }

    // 2. direction × layout 交叉校验
    validate_direction_layout_compat(diagram)?;

    Ok(())
}

/// 校验 effective direction 与当前 layout 算法的兼容性。
fn validate_direction_layout_compat(
    diagram: &Diagram,
) -> std::result::Result<(), DiagnosticError> {
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(diagram, profile);
    let algo = plan.layout_algo.as_str();

    let strategy = registry::build_layout_strategy(algo, &plan)
        .ok_or_else(|| layout_config_error(diagram, diagram::LAYOUT, algo, &[algo]))?;

    let supported = strategy.supported_directions();
    let explicit = diagram.direction_attr();

    if supported.is_empty() {
        // layout 不消费 direction
        if explicit.is_some() {
            let span = layout_attr_span(diagram, diagram::DIRECTION);
            return Err(DiagnosticError::structure_violation(
                span,
                format!(
                    "layout '{}' does not support the 'direction' attribute. \
                     Remove 'direction' from the diagram block.",
                    algo
                ),
            ));
        }
        return Ok(()); // 无需校验，不看 effective
    }

    // supported 非空：layout 消费 direction
    let effective = resolve_effective_direction(diagram).ok_or_else(|| {
        let span = layout_attr_span(diagram, diagram::DIRECTION);
        DiagnosticError::structure_violation(
            span,
            format!(
                "diagram type '{}' has no default direction configured",
                profile.name
            ),
        )
    })?;

    if !supported.contains(&effective) {
        let span = layout_attr_span(diagram, diagram::DIRECTION);
        let supported_list = supported.join(", ");
        // 生成 Hint：找到支持当前 direction 的 layout
        let hint = all_layout_strategies()
            .iter()
            .find(|s| {
                s.supports_diagram_type(&diagram.diagram_type)
                    && s.supported_directions().contains(&effective)
            })
            .map(|s| format!("\nHint: set direction: {}, or use layout: {}.", supported[0], s.name()))
            .unwrap_or_default();
        return Err(DiagnosticError::structure_violation(
            span,
            format!(
                "layout '{}' does not support direction '{}'.\n\
                 Supported directions: {}.{hint}",
                algo, effective, supported_list
            ),
        ));
    }
    Ok(())
}

fn validate_registered_layout_algo(
    diagram: &Diagram,
    span: crate::ast::Span,
    value: &str,
) -> std::result::Result<(), DiagnosticError> {
    let strategies = all_layout_strategies();
    let known: Vec<&str> = strategies.iter().map(|s| s.name()).collect();

    if !known.contains(&value) {
        return Err(DiagnosticError::invalid_enum_value(
            span, diagram::LAYOUT, value, &known,
        ));
    }

    let supported = strategies
        .iter()
        .any(|s| s.name() == value && s.supports_diagram_type(&diagram.diagram_type));
    if !supported {
        let applicable = applicable_layouts_for_type(&diagram.diagram_type);
        return Err(DiagnosticError::invalid_enum_value(
            span, diagram::LAYOUT, value, &applicable,
        ));
    }
    Ok(())
}

fn validate_registered_edge_routing(
    diagram: &Diagram,
    span: crate::ast::Span,
    value: &str,
) -> std::result::Result<(), DiagnosticError> {
    let strategies = all_routing_strategies();
    let known: Vec<&str> = strategies.iter().map(|s| s.name()).collect();

    if !known.contains(&value) {
        return Err(DiagnosticError::invalid_enum_value(
            span, diagram::EDGE_ROUTING, value, &known,
        ));
    }

    let supported = strategies
        .iter()
        .any(|s| s.name() == value && s.supports_diagram_type(&diagram.diagram_type));
    if !supported {
        let applicable = applicable_routings_for_type(&diagram.diagram_type);
        return Err(DiagnosticError::invalid_enum_value(
            span, diagram::EDGE_ROUTING, value, &applicable,
        ));
    }
    Ok(())
}

// ─── 样式感知的节点尺寸 ────────────────────────────────────

/// 从 entity 的 `attributes.style` 读取尺寸覆盖，回退到默认值。
///
/// prepare 已将 theme cascade 物化到 `attributes.style`，其中可能包含
/// `width`、`height` 等布局相关属性。布局算法应优先使用这些值，
/// 仅在 style 中未指定时才使用算法自身的默认尺寸。
///
/// # 参数
/// - `entity`: AST 实体
/// - `default_width`: 算法默认宽度
/// - `default_height`: 算法默认高度
///
/// # 返回
/// `(width, height)` — style 覆盖值（如有），否则为默认值
pub fn styled_node_size(entity: &Entity, default_width: f64, default_height: f64) -> (f64, f64) {
    let width = entity
        .attributes
        .style
        .get("width")
        .and_then(|v| match v {
            AttributeValue::Number(n) if *n > 0.0 => Some(*n),
            _ => None,
        })
        .unwrap_or(default_width);

    let height = entity
        .attributes
        .style
        .get("height")
        .and_then(|v| match v {
            AttributeValue::Number(n) if *n > 0.0 => Some(*n),
            _ => None,
        })
        .unwrap_or(default_height);

    let (width, height) = (width, height);
    crate::icons::layout::apply_icon_to_node_size(entity, width, height, &crate::icons::ResolveOptions::default())
}

// ─── 几何工具函数 ────────────────────────────────────────

/// 计算从矩形中心到目标点的射线与矩形边界的交点
pub fn edge_point(nl: &NodeLayout, tx: f64, ty: f64) -> (f64, f64) {
    let cx = nl.x + nl.width / 2.0;
    let cy = nl.y + nl.height / 2.0;
    let dx = tx - cx;
    let dy = ty - cy;

    if dx.abs() < 0.01 && dy.abs() < 0.01 {
        return (cx, cy);
    }

    let hw = nl.width / 2.0;
    let hh = nl.height / 2.0;

    let scale_x = if dx.abs() > 0.01 {
        hw / dx.abs()
    } else {
        f64::MAX
    };
    let scale_y = if dy.abs() > 0.01 {
        hh / dy.abs()
    } else {
        f64::MAX
    };
    let scale = scale_x.min(scale_y);

    (cx + dx * scale, cy + dy * scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::geometry::Point;
    use crate::ast::{AttributeValue, Diagram, DiagramAttribute, Position, SourceInfo, Span, TextValue};

    fn sample_diagram(diagram_type: DiagramType) -> Diagram {
        Diagram::new(
            diagram_type,
            SourceInfo {
                file: None,
                line_count: 1,
            },
        )
    }

    fn atom_attr(key: &str, value: &str) -> DiagramAttribute {
        DiagramAttribute {
            key: key.to_string(),
            value: AttributeValue::String(TextValue::unquoted(value.to_string())),
            span: Span::new(Position::new(1, 1), Position::new(1, 1)),
        }
    }

    #[test]
    fn uses_profile_defaults_when_diagram_has_no_override() {
        let diagram = sample_diagram(DiagramType::Sequence);
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        assert_eq!(plan.layout_algo, "sequence");
        assert_eq!(plan.edge_routing, "");
        assert!(compute_layout(&diagram).is_ok());
    }

    #[test]
    fn sequence_rejects_edge_routing_attribute() {
        let mut diagram = sample_diagram(DiagramType::Sequence);
        diagram.attributes.push(atom_attr("edge_routing", "straight"));
        assert!(compute_layout(&diagram).is_err());
    }

    #[test]
    fn explicit_overrides_still_take_priority() {
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("layout", "force-directed"));
        diagram.attributes.push(atom_attr("edge_routing", "bezier"));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        assert_eq!(plan.layout_algo, "force-directed");
        assert_eq!(plan.edge_routing, "bezier");
    }

    #[test]
    fn force_directed_resolves_on_flowchart() {
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram
            .attributes
            .push(atom_attr("layout", "force-directed"));
        let plan = LayoutPlan::resolve(&diagram, profile_for(&diagram.diagram_type));

        assert_eq!(plan.layout_algo, "force-directed");
        assert!(compute_layout(&diagram).is_ok());
    }

    #[test]
    fn sugiyama_v2_resolves() {
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("layout", "sugiyama-v2"));
        let plan = LayoutPlan::resolve(&diagram, profile_for(&diagram.diagram_type));

        assert_eq!(plan.layout_algo, "sugiyama-v2");
    }

    #[test]
    fn string_layout_attrs_are_resolved() {
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("direction", "left-to-right"));
        diagram.attributes.push(atom_attr("layout", "circular"));
        diagram.attributes.push(atom_attr("edge_routing", "bezier"));
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        assert_eq!(diagram.direction(), "left-to-right");
        assert_eq!(plan.layout_algo, "circular");
        assert_eq!(plan.edge_routing, "bezier");
    }

    #[test]
    fn unknown_layout_algo_is_rejected() {
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram
            .attributes
            .push(atom_attr("layout", "not_a_real_algo"));

        match compute_layout(&diagram) {
            Err(err) => assert!(err.message.contains("not_a_real_algo")),
            Ok(_) => panic!("expected layout error"),
        }
    }

    #[test]
    fn unsupported_layout_algo_for_diagram_type_is_rejected() {
        let mut diagram = sample_diagram(DiagramType::Sequence);
        diagram.attributes.push(atom_attr("layout", "mindmap"));

        assert!(compute_layout(&diagram).is_err());
    }

    #[test]
    fn compute_layout_applies_grid_snap_for_sugiyama_v2() {
        use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("layout", "sugiyama-v2"));
        for id in ["a", "b", "c", "d"] {
            diagram.entities.push(Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            });
        }
        for (from, to) in [("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")] {
            diagram.relations.push(Relation {
                from: Identifier::new_unchecked(from),
                to: Identifier::new_unchecked(to),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            });
        }

        let result = compute_layout(&diagram).expect("layout should succeed");
        assert_eq!(result.nodes.len(), 4);

        let b = &result.nodes["b"];
        let c = &result.nodes["c"];
        let b_cy = b.y + b.height / 2.0;
        let c_cy = c.y + c.height / 2.0;
        assert!(
            (b_cy - c_cy).abs() < f64::EPSILON,
            "siblings b and c should share rank-axis center after grid snap"
        );
    }

    #[test]
    fn compute_layout_snaps_orthogonal_edge_waypoints() {
        use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("layout", "sugiyama-v2"));
        for id in ["a", "b"] {
            diagram.entities.push(Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            });
        }
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });

        let result = compute_layout(&diagram).expect("layout should succeed");
        assert_eq!(result.edges.len(), 1);
        let edge = &result.edges[0];
        let path: Vec<Point> = edge.path_points().into_owned();
        if path.len() > 2 {
            for i in 1..path.len() - 1 {
                let prev = path[i - 1];
                let curr = path[i];
                let next = path[i + 1];
                let dx_prev = (curr.x - prev.x).abs();
                let dy_prev = (curr.y - prev.y).abs();
                let dx_next = (next.x - curr.x).abs();
                let dy_next = (next.y - curr.y).abs();
                let prev_vertical = dx_prev < 0.1 && dy_prev >= 0.1;
                let next_vertical = dx_next < 0.1 && dy_next >= 0.1;
                let prev_horizontal = dy_prev < 0.1 && dx_prev >= 0.1;
                let next_horizontal = dy_next < 0.1 && dx_next >= 0.1;

                if prev_vertical && next_vertical {
                    assert!(
                        (curr.x - prev.x).abs() < 0.1,
                        "vertical waypoint x={} must align with x={}",
                        curr.x,
                        prev.x
                    );
                    assert!(
                        (curr.y / 8.0).fract().abs() < 1e-6
                            || (curr.y / 8.0).fract().abs() > 1.0 - 1e-6,
                        "vertical waypoint y={} should be on 8px grid",
                        curr.y
                    );
                } else if prev_horizontal && next_horizontal {
                    assert!(
                        (curr.y - prev.y).abs() < 0.1,
                        "horizontal waypoint y={} must align with y={}",
                        curr.y,
                        prev.y
                    );
                    assert!(
                        (curr.x / 8.0).fract().abs() < 1e-6
                            || (curr.x / 8.0).fract().abs() > 1.0 - 1e-6,
                        "horizontal waypoint x={} should be on 8px grid",
                        curr.x
                    );
                }
            }
        }
        assert!(!edge.is_bezier());
    }

    #[test]
    fn compute_layout_respects_snap_false_attribute() {
        use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("layout", "sugiyama-v2"));
        diagram.attributes.push(DiagramAttribute {
            key: "snap".into(),
            value: AttributeValue::Boolean(false),
            span,
        });
        diagram.entities.push(Entity {
            id: Identifier::new_unchecked("a"),
            label: "a".into(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        });
        diagram.entities.push(Entity {
            id: Identifier::new_unchecked("b"),
            label: "b".into(),
            attributes: AttributeMap::default(),
            group_id: None,
            span,
        });
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });

        assert!(!grid_snap::snap_enabled_for_diagram(&diagram, "sugiyama-v2"));
        assert!(compute_layout(&diagram).is_ok());
    }

    #[test]
    fn group_padding_option_affects_group_bounds() {
        use crate::ast::{AttributeMap, AttributeValue, Entity, Group, Identifier, Relation, ArrowType};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(DiagramAttribute {
            key: "layout".into(),
            value: AttributeValue::Config {
                algo: "flowchart".into(),
                options: [("group_padding".to_string(), AttributeValue::Number(50.0))]
                    .into_iter()
                    .collect(),
            },
            span,
        });
        diagram.entities = vec![
            Entity {
                id: Identifier::new_unchecked("a"),
                label: "a".into(),
                attributes: AttributeMap::default(),
                group_id: Some(Identifier::new_unchecked("g")),
                span,
            },
            Entity {
                id: Identifier::new_unchecked("b"),
                label: "b".into(),
                attributes: AttributeMap::default(),
                group_id: Some(Identifier::new_unchecked("g")),
                span,
            },
        ];
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });
        diagram.groups.push(Group {
            id: Identifier::new_unchecked("g"),
            label: "g".into(),
            attributes: AttributeMap::default(),
            parent_id: None,
            depth: 0,
            entity_ids: vec![
                Identifier::new_unchecked("a"),
                Identifier::new_unchecked("b"),
            ],
            child_group_ids: vec![],
            span,
        });

        let default_layout = compute_layout(&diagram).expect("default layout");
        let default_group = default_layout
            .groups
            .get("g")
            .expect("group bounds");

        diagram.attributes.clear();
        diagram.attributes.push(atom_attr("layout", "flowchart"));
        let baseline_layout = compute_layout(&diagram).expect("baseline layout");
        let baseline_group = baseline_layout.groups.get("g").expect("baseline group");

        assert!(
            default_group.width > baseline_group.width,
            "larger group_padding should expand group width: {} vs {}",
            default_group.width,
            baseline_group.width
        );
    }

    // ── resolve_effective_direction 测试 ──

    #[test]
    fn effective_direction_flowchart_default() {
        let diagram = sample_diagram(DiagramType::Flowchart);
        assert_eq!(resolve_effective_direction(&diagram), Some("top-to-bottom"));
    }

    #[test]
    fn effective_direction_mindmap_default() {
        let diagram = sample_diagram(DiagramType::Mindmap);
        assert_eq!(resolve_effective_direction(&diagram), Some("radial"));
    }

    #[test]
    fn effective_direction_sequence_is_none() {
        let diagram = sample_diagram(DiagramType::Sequence);
        assert_eq!(resolve_effective_direction(&diagram), None);
    }

    #[test]
    fn effective_direction_state_is_none() {
        let diagram = sample_diagram(DiagramType::State);
        assert_eq!(resolve_effective_direction(&diagram), None);
    }

    #[test]
    fn effective_direction_explicit_overrides_default() {
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("direction", "left-to-right"));
        assert_eq!(resolve_effective_direction(&diagram), Some("left-to-right"));
    }

    #[test]
    fn effective_direction_custom_inherits_flowchart() {
        let diagram = sample_diagram(DiagramType::Custom("test".to_string()));
        assert_eq!(resolve_effective_direction(&diagram), Some("top-to-bottom"));
    }

    // ── direction × layout 交叉校验测试 ──

    #[test]
    fn flowchart_with_radial_direction_is_rejected() {
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("direction", "radial"));
        let result = compute_layout(&diagram);
        assert!(result.is_err(), "flowchart + radial should be rejected");
        let err = result.unwrap_err();
        assert!(err.message.contains("does not support direction 'radial'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn mindmap_with_radial_direction_is_accepted() {
        let mut diagram = sample_diagram(DiagramType::Mindmap);
        diagram.attributes.push(atom_attr("direction", "radial"));
        assert!(compute_layout(&diagram).is_ok());
    }

    #[test]
    fn sequence_with_direction_is_rejected() {
        let mut diagram = sample_diagram(DiagramType::Sequence);
        diagram.attributes.push(atom_attr("direction", "top-to-bottom"));
        let result = compute_layout(&diagram);
        assert!(result.is_err(), "sequence + direction should be rejected");
        let err = result.unwrap_err();
        assert!(err.message.contains("does not support the 'direction' attribute"), "unexpected error: {}", err.message);
    }

    #[test]
    fn from_center_is_rejected_as_direction() {
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("direction", "from_center"));
        let result = compute_layout(&diagram);
        assert!(result.is_err(), "from_center should be rejected");
    }

    // ── §6: friendliness 解耦集成测试 ──

    /// §6: `friendliness: off` 时，LayoutResult.hints.friendliness_report 应为 None
    /// （V1 评估被跳过）。
    #[test]
    fn friendliness_off_skips_v1_evaluation() {
        use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        // 设置 friendliness: off
        diagram.attributes.push(DiagramAttribute {
            key: "layout".to_string(),
            value: AttributeValue::Config {
                algo: "flowchart".to_string(),
                options: {
                    let mut m = std::collections::HashMap::new();
                    m.insert(
                        "friendliness".to_string(),
                        AttributeValue::String(TextValue::unquoted("off".to_string())),
                    );
                    m
                },
            },
            span,
        });
        for id in ["a", "b", "c"] {
            diagram.entities.push(Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            });
        }
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("b"),
            to: Identifier::new_unchecked("c"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });

        let (result, _) = compute_layout_with_plan_and_overlay(&diagram, &LayoutPlan::resolve(&diagram, profile_for(&diagram.diagram_type)), None).unwrap();

        // friendliness: off → V1 被跳过 → friendliness_report 应为 None
        assert!(
            result.hints.friendliness_report.is_none(),
            "friendliness: off should skip V1 evaluation (friendliness_report should be None)"
        );
    }

    /// §6: 默认（adjust）时，friendliness_report 应有值（V1 评估执行）。
    #[test]
    fn friendliness_default_adjust_runs_v1_evaluation() {
        use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        for id in ["a", "b", "c"] {
            diagram.entities.push(Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            });
        }
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("b"),
            to: Identifier::new_unchecked("c"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });

        let (result, _) = compute_layout_with_plan_and_overlay(&diagram, &LayoutPlan::resolve(&diagram, profile_for(&diagram.diagram_type)), None).unwrap();

        // 默认 adjust → V1 执行 → friendliness_report 应有值
        assert!(
            result.hints.friendliness_report.is_some(),
            "default friendliness (adjust) should run V1 evaluation (friendliness_report should be Some)"
        );
    }

    // ── Edge Bundling 端到端集成测试（§7.3 / §9 P4e）──

    /// 构造一个含多条平行边的 flowchart，用于 bundling 端到端测试。
    ///
    /// 拓扑：a 与 b 之间有 4 条同向 Active 边（无 label）。
    /// orthogonal 路由会用 lane 机制将它们并排排列（间距 ~16px），
    /// 4 条边共享 trunk 的 Ink 节省率 > 10%（min_ink_saving 阈值）。
    fn make_bundling_test_diagram() -> Diagram {
        use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        diagram.attributes.push(atom_attr("direction", "left-to-right"));
        // edge_routing: orthogonal { bundling: 1.0 }
        diagram.attributes.push(DiagramAttribute {
            key: "edge_routing".into(),
            value: AttributeValue::Config {
                algo: "orthogonal".into(),
                options: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("bundling".to_string(), AttributeValue::Number(1.0));
                    m
                },
            },
            span,
        });

        for id in ["a", "b"] {
            diagram.entities.push(Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            });
        }
        // 4 条同向平行边 a→b
        for _ in 0..4 {
            diagram.relations.push(Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            });
        }
        diagram
    }

    #[test]
    fn bundling_config_resolves_from_dsl() {
        let diagram = make_bundling_test_diagram();
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        assert_eq!(plan.edge_routing, "orthogonal");
        assert!(
            plan.edge_bundling.enabled,
            "bundling should be enabled when edge_routing option bundling=1.0"
        );
    }

    #[test]
    fn bundling_disabled_by_default() {
        use crate::ast::{AttributeMap, Entity, Identifier, Relation, ArrowType};

        let span = Span::new(Position::new(1, 1), Position::new(1, 1));
        let mut diagram = sample_diagram(DiagramType::Flowchart);
        for id in ["a", "b"] {
            diagram.entities.push(Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            });
        }
        diagram.relations.push(Relation {
            from: Identifier::new_unchecked("a"),
            to: Identifier::new_unchecked("b"),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        });

        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);
        assert!(!plan.edge_bundling.enabled, "bundling should be off by default");

        let result = compute_layout_with_plan(&diagram, &plan).expect("layout should succeed");
        assert!(
            result.hints.edge_bundling.is_none(),
            "edge_bundling hints should be absent when bundling is disabled"
        );
    }

    #[test]
    fn bundling_pipeline_runs_and_preserves_edge_endpoints() {
        let diagram = make_bundling_test_diagram();
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        let result = compute_layout_with_plan(&diagram, &plan).expect("layout should succeed");

        // §7.3: bundling 启用后 hints.edge_bundling 必须被填充
        let bundling_hints = result
            .hints
            .edge_bundling
            .as_ref()
            .expect("edge_bundling hints should be populated when bundling is enabled");

        // 每条边都有对应的 edge_to_bundle 条目
        assert_eq!(
            bundling_hints.result.edge_to_bundle.len(),
            result.edges.len(),
            "edge_to_bundle length must match edge count"
        );
        assert_eq!(
            bundling_hints.result.edge_roles.len(),
            result.edges.len(),
            "edge_roles length must match edge count"
        );

        // 所有边的路径仍然有效：≥ 2 个点，首尾点与节点边界对齐
        for (i, edge) in result.edges.iter().enumerate() {
            let path: Vec<Point> = edge.path_points().into_owned();
            assert!(
                path.len() >= 2,
                "edge {} path must have at least 2 points after bundling, got {}",
                i,
                path.len()
            );
            // 首尾点不应为 NaN
            for p in &path {
                assert!(p.x.is_finite(), "edge {} has NaN/inf x in path", i);
                assert!(p.y.is_finite(), "edge {} has NaN/inf y in path", i);
            }
        }
    }

    #[test]
    fn bundling_produces_at_least_one_bundle() {
        let diagram = make_bundling_test_diagram();
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        let result = compute_layout_with_plan(&diagram, &plan).expect("layout should succeed");
        let bundling_hints = result
            .hints
            .edge_bundling
            .as_ref()
            .expect("edge_bundling hints should be populated");

        // 四条同向平行边 → 至少应形成一个 bundle
        let bundle_count = bundling_hints.result.bundles.len();
        let bundled_edge_count = bundling_hints
            .result
            .edge_to_bundle
            .iter()
            .filter(|b| b.is_some())
            .count();

        assert!(
            bundle_count >= 1,
            "expected at least 1 bundle for 4 parallel edges, got {}",
            bundle_count
        );
        assert!(
            bundled_edge_count >= 2,
            "expected at least 2 bundled edges, got {}",
            bundled_edge_count
        );

        // 捆绑后的边路径应包含共享主干
        for bundle in &bundling_hints.result.bundles {
            assert!(
                bundle.edges.len() >= 2,
                "bundle {} should contain at least 2 edges, got {}",
                bundle.id,
                bundle.edges.len()
            );
            assert!(
                bundle.entry_points.len() == bundle.edges.len(),
                "bundle {} entry_points count mismatch",
                bundle.id
            );
            assert!(
                bundle.exit_points.len() == bundle.edges.len(),
                "bundle {} exit_points count mismatch",
                bundle.id
            );
        }
    }

    #[test]
    fn bundling_ink_saved_is_non_negative() {
        let diagram = make_bundling_test_diagram();
        let profile = profile_for(&diagram.diagram_type);
        let plan = LayoutPlan::resolve(&diagram, profile);

        let result = compute_layout_with_plan(&diagram, &plan).expect("layout should succeed");
        let bundling_hints = result
            .hints
            .edge_bundling
            .as_ref()
            .expect("edge_bundling hints should be populated");

        // Ink 节省量必须非负（bundling 只会减少或保持 ink，不会增加）
        assert!(
            bundling_hints.result.total_ink_saved >= 0.0,
            "total_ink_saved should be non-negative, got {}",
            bundling_hints.result.total_ink_saved
        );

        // 如果有 bundle，ink 节省应 > 0（min_ink_saving 默认 0.1，低于此值的 bundle 会被回退）
        if !bundling_hints.result.bundles.is_empty() {
            assert!(
                bundling_hints.result.total_ink_saved > 0.0,
                "total_ink_saved should be positive when bundles exist, got {}",
                bundling_hints.result.total_ink_saved
            );
        }
    }
}
