use crate::layout::geometry::Point;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;

// Bring layout submodules into scope so `LayoutHints` field references such as
// `node::circular::CircularLayoutHints` resolve from this submodule.
use crate::layout::{group, group_frame, node, space_budget};

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
    /// 障碍避让样条（ER 等关系图）
    Spline,
    /// 自环 + 折线（sequence 自调用消息）
    SelfLoop,
}

/// 布局阶段产出的提示信息，供边路由读取
#[derive(Debug, Clone, Default)]
pub struct LayoutHints {
    // WRITE: layout(circular)  READ: render(edge routing)
    pub circular: Option<node::circular::CircularLayoutHints>,
    // WRITE: layout(sequence)  READ: render(edge routing)
    pub sequence: Option<node::sequence::SequenceLayoutHints>,
    /// 布局算法对边路由风格的推荐（用户显式配置时覆盖此值）
    // WRITE: prepare(user config) / layout(algo default)  READ: render(edge routing)
    pub edge_routing_style: EdgeRoutingStyle,
    /// Sugiyama 系列布局产出的节点 rank 映射（entity_id → rank）。
    ///
    /// 供正交路由层序、flowchart group 重建等使用。
    /// 非 Sugiyama 布局为 `None`。
    // WRITE: layout(sugiyama)  READ: render(orthogonal routing, flowchart group rebuild)
    pub sugiyama_ranks: Option<HashMap<String, usize>>,
    /// MindMap 布局产出的节点深度映射（entity_id → depth）。
    ///
    /// 根节点 depth = 0，一级分支 depth = 1，依此类推。
    /// 供 organic 边路由根据层级动态调整曲线弧度，深层级更平缓。
    /// 非 MindMap 布局为 `None`。
    // WRITE: layout(mindmap)  READ: render(organic routing)
    pub mindmap_depths: Option<HashMap<String, usize>>,
    /// 分组布局警告：group 包围框互相重叠或框内含非组节点。
    ///
    /// 由 Sugiyama 系列布局在 `compute_group_bounds` 后检测填充。
    /// 流程图布局 group 不参与布局（仅事后画框），此类警告用于诊断
    /// "group 框拉得很长/互相压住"的视觉问题。
    // WRITE: layout(after compute_group_bounds)  READ: encode(lint advice)
    pub group_layout_warnings: Vec<GroupLayoutWarning>,
    /// Group Frame pass 的执行报告，供 lint advice 与调试消费。
    // WRITE: layout(group_frame pass)  READ: encode(lint advice) / debug
    pub group_frame_report: Option<group_frame::GroupFrameReport>,
    /// refine 调试统计（P2-1 可观测性）。
    ///
    /// 由 `refine::run_refine` 在执行后填充；未启用 refine 时为 `None`。
    /// 供 bench 工具和诊断使用，不影响布局结果。
    // WRITE: layout(refine::run_refine)  READ: bench / debug
    pub refine_debug: Option<RefineDebugStats>,
    /// orthogonal 路由调试统计（P2-1 可观测性）。
    ///
    /// 由 `route_edges_orthogonal` 在执行后填充；非 orthogonal 路由为 `None`。
    /// 供 bench 工具和诊断使用，不影响布局结果。
    // WRITE: render(route_edges_orthogonal)  READ: bench / debug
    pub orthogonal_debug: Option<OrthoDebugStats>,
    /// 分组路由提示：组间走廊 + 边框壳层厚度（architecture 等含 group 的图）。
    // WRITE: layout(group layout)  READ: render(orthogonal routing)
    pub group_routing: Option<group::GroupRoutingHints>,
    /// EGB / PRS 调试统计（architecture 可选观测）。
    // WRITE: layout(EGB / PRS)  READ: debug
    pub gutter_budget_debug: Option<GutterBudgetDebug>,
    /// 空间契约：同层间距 / 标签缝 / 端口 clearance（布局预留，路由与后处理守约）。
    // WRITE: layout(space_budget)  READ: render(routing + post-process)
    pub space_budget: Option<space_budget::SpaceBudget>,
    /// 正交路由 C 末旁路注解（stub / 受保护 trunk）；不改 `EdgeLayout`。
    // WRITE: render(route_edges_orthogonal @ C end)  READ: sanitize / grid_snap validate
    pub route_annotations: Option<crate::layout::edge::RouteAnnotationSet>,
    /// 同层边：(from_entity_id, to_entity_id)，路由按短横/L 处理而非绕底廊。
    // WRITE: layout(sugiyama same-layer)  READ: render(orthogonal routing)
    pub same_layer_edges: Vec<(String, String)>,
    /// Feedback hub：(hub_entity_id, primary_pred_entity_id)，路由走侧廊。
    // WRITE: layout(sugiyama same-layer)  READ: render(orthogonal routing)
    pub feedback_hubs: Vec<(String, String)>,
}

/// EGB + PRS 性能与效果观测（不影响布局结果）。
#[derive(Debug, Clone, Default)]
pub struct GutterBudgetDebug {
    pub egb_ms: f64,
    pub prs_ms: f64,
    pub prs_grew: bool,
    pub max_side_gutter: f64,
    pub canvas_area_delta_pct: f64,
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
    /// spline 可见性图兜底重路由的边数
    pub spline_fallback_count: usize,
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
    /// 完全重合的平行段对数（间距≈0）
    pub edge_exact_overlap_pairs: usize,
    /// 间距不足的平行段对数（0 < gap < min_gap）
    pub edge_tight_spacing_pairs: usize,
    /// X-1 重路由迭代轮次
    pub reroute_iterations: usize,
    /// X-1 被重路由的边数
    pub rerouted_edges: usize,
    /// X-2 nudge 迭代轮次
    pub nudge_iterations: usize,
    /// X-2 被 nudge 的段数
    pub nudged_segments: usize,
    /// X-2 nudge 失败的段数
    pub nudge_failed: usize,
    /// X-2 反向 stub 端口翻转成功的边数
    pub flipped_stub_edges: usize,
    /// X-3 lane assignment 检测到的车道组数
    pub lane_groups: usize,
    /// X-3 lane assignment 成功偏移的段数
    pub lane_segments_shifted: usize,
    /// X-3 lane assignment 偏移失败的段数
    pub lane_shifts_failed: usize,
    /// Phase 3: reroute 时构建的通道负载图的最大负载值
    pub max_channel_load: usize,
    /// S1：同侧 stub 冲突对数（去冲突前，含跨对）
    pub stub_occupancy_conflicts: usize,
    /// S1：跨无向对的 stub 冲突对数
    pub stub_cross_pair_conflicts: usize,
    /// S1：成功平移的 stub 端数
    pub stub_occupancy_shifted: usize,
    /// S1：未能分离而 degraded 的次数
    pub stub_occupancy_degraded: usize,
    /// S3：成功合流的语义组数（FanIn/FanOut）
    pub semantic_trunk_groups_merged: usize,
    /// S3：合流失败 degraded 的组数
    pub semantic_trunk_degraded: usize,
    /// S4：S3 后为避 FanIn 干线而重路由的 feedback 边数
    pub feedback_rerouted_after_trunk: usize,
    /// A-0 契约诊断：跨组边首个转弯点仍在源组边界内的边数（stub 未出组，ISS-001）
    pub contract_stub_violations: usize,
    /// A-0 契约诊断：末段方向与目标端口方向不一致的边数（approach 违约，ISS-002）
    /// 注：sanitize 已强制末段贴合端口，该计数通常为 0；ISS-002 真正根因由
    /// contract_unnatural_to_port 捕获。
    pub contract_approach_violations: usize,
    /// A-0 契约诊断：目标端口未面向源节点的边数（箭头“倒悬”，ISS-002 直接信号）
    pub contract_unnatural_to_port: usize,
    /// A-0 契约诊断：使到目标曼哈顿距离增大的段的总数（远离段，ISS-008/009c）
    pub contract_away_segments: usize,
    /// A-0 契约诊断：含至少一个远离段的边数
    pub contract_away_edges: usize,
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
    pub fn validate_group_containment(&self, diagram: &crate::ast::Diagram) -> Vec<GroupContainmentViolation> {
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
