//! Edge Bundling 核心数据结构
//!
//! 定义边捆绑（Edge Bundling）所需的配置、束（bundle）与结果类型。
//! 详见 `docs/architecture/布局优化/edge-bundling-research.md` §7.2。
//!
//! ## 确定性约定
//!
//! 按 [AGENTS.md](../../../../../AGENTS.md) 要求，所有映射使用 `Vec` 按 `edge_index`
//! 排序存储，**不使用 `HashMap`**，避免任何迭代顺序依赖。

use crate::layout::geometry::Point;
use crate::layout::Port;

/// 一维坐标轴（主干方向）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// 水平轴（x 方向）——主干为水平段
    Horizontal,
    /// 垂直轴（y 方向）——主干为垂直段
    Vertical,
}

/// 沿轴的方向（用于 PathSegment 的 direction 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentDirection {
    /// 沿轴正方向（x 增大 / y 增大）
    Positive,
    /// 沿轴负方向（x 减小 / y 减小）
    Negative,
}

/// 路径分段（Step 1 输出）：将 Polyline 分解为有向段。
#[derive(Debug, Clone)]
pub struct PathSegment {
    pub edge_index: usize,
    pub axis: Axis,
    pub start: Point,
    pub end: Point,
    pub direction: SegmentDirection,
    pub length: f64,
    /// 所属通道层（y 坐标 for H-seg, x for V-seg）
    pub layer: f64,
}

/// 边路径上的区段角色（§4.10.4 SegmentAware）。
///
/// bundling 重写后的路径可分解为五类区段，label 关联语义依角色而定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentRole {
    /// 近 from 端口的短腿（每边独立）
    FromStub,
    /// 合入段：从 stub 终点到主干 entry 点（近源侧独占）
    MergeLeg,
    /// 主干共享段（bundle 内重合，默认禁止放 label）
    Trunk,
    /// 分叉段：从主干 exit 点到 to 端 stub 起点（近目标侧独占）
    ForkLeg,
    /// 近 to 端口的短腿（每边独立）
    ToStub,
}

/// 边路径上的半开区间 [t_start, t_end) 及折线点索引（§4.10.4）。
#[derive(Debug, Clone)]
pub struct SegmentSpan {
    pub role: SegmentRole,
    /// 折线点起始索引（含）
    pub point_start: usize,
    /// 折线点结束索引（不含）
    pub point_end: usize,
    /// 在整条 path 上的参数区间 [0,1]
    pub t_start: f64,
    pub t_end: f64,
    pub length: f64,
}

/// 每条边的路径区段分解（含未捆绑边：仅 Trunk 为空或 Trunk=全路径）。
#[derive(Debug, Clone)]
pub struct EdgePathRoles {
    pub edge_index: usize,
    /// 按 path 顺序排列的区段，确定性排序
    pub spans: Vec<SegmentSpan>,
}

/// 主干禁放区（Trunk Keep-out Zone，§4.10.5）。
///
/// 主干段外扩 `label_trunk_pad` 后的避让带，供后置 label 与渲染查询。
#[derive(Debug, Clone)]
pub struct TrunkKeepout {
    pub bundle_id: usize,
    /// 外扩后的轴对齐条带（水平主干 → 薄矩形；垂直主干 → 薄矩形）
    /// 格式：(x_min, y_min, x_max, y_max)
    pub zones: Vec<(f64, f64, f64, f64)>,
}

/// label 与 bundling 的协同策略（§4.10.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelBundlePolicy {
    /// 中段 label 锚在合入/分叉等独占段（默认，§4.10.4）
    SegmentAware,
    /// 双方有不同 label 时不捆绑
    Conservative,
    /// 允许捆绑，主干上错开 t
    Stagger,
    /// 有 label 的边永不进 bundle
    ForkOnly,
}

impl Default for LabelBundlePolicy {
    fn default() -> Self {
        Self::SegmentAware
    }
}

/// 边捆绑配置（§7.2）。
#[derive(Debug, Clone, PartialEq)]
pub struct BundlingConfig {
    /// 是否启用边捆绑（默认 false，渐进式发布）
    pub enabled: bool,
    /// 兼容性阈值（0.0~1.0，越高捆绑越保守）
    pub compatibility_threshold: f64,
    /// 同束最大边数（默认 8，超过子分）
    pub max_bundle_size: usize,
    /// 束间最小间距（像素，默认 12px）——不同 bundle 的主干之间
    pub bundle_gap: f64,
    /// 分叉点距节点的最小距离（像素，默认 16px）
    pub fork_distance: f64,
    /// 分叉点之间的最小间距（像素，默认 8px）——同 bundle 内相邻分叉点
    pub fork_spacing: f64,
    /// 最小 Ink 节省比例，低于则不合并（默认 0.1）
    pub min_ink_saving: f64,
    /// bundle 内最多允许多少条带中段 label 的边（默认 2，见 §4.10.3）
    pub max_labeled_edges_per_bundle: usize,
    /// 主干禁放区外扩（像素，默认 8）
    pub label_trunk_pad: f64,
    /// bundle 内中段 label 的 t 间距（默认 0.08，仅 Stagger 策略）
    pub label_t_spacing: f64,
    /// 独占段最小长度（像素）；低于此值的段不可锚 label
    pub min_exclusive_segment_for_label: f64,
    /// label 与 bundling 的协同策略（默认 SegmentAware）
    pub label_bundle_policy: LabelBundlePolicy,
}

impl Default for BundlingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            compatibility_threshold: 0.5,
            max_bundle_size: 8,
            bundle_gap: 12.0,
            fork_distance: 16.0,
            fork_spacing: 8.0,
            min_ink_saving: 0.1,
            max_labeled_edges_per_bundle: 2,
            label_trunk_pad: 8.0,
            label_t_spacing: 0.08,
            min_exclusive_segment_for_label: 40.0,
            label_bundle_policy: LabelBundlePolicy::default(),
        }
    }
}

/// 一个 bundle（一束边）。
#[derive(Debug, Clone)]
pub struct EdgeBundle {
    /// 束 ID（确定性，按组内最小 edge_index 编号）
    pub id: usize,
    /// 包含的边索引列表（已排序，升序）
    pub edges: Vec<usize>,
    /// 主干段方向
    pub trunk_axis: Axis,
    pub trunk_start: Point,
    pub trunk_end: Point,
    /// 每条边的合入点（按 edge_index 升序排序，与 edges 对齐）
    pub entry_points: Vec<Point>,
    /// 每条边的分叉点（按 edge_index 升序排序，与 edges 对齐）
    pub exit_points: Vec<Point>,
}

/// Bundling 结果（附加在 LayoutHints 中供渲染与 label 后置使用）。
#[derive(Debug, Clone, Default)]
pub struct BundlingResult {
    pub bundles: Vec<EdgeBundle>,
    /// edge_index → 所属 bundle_id（None 表示未捆绑）
    /// 按 edge_index 索引（vec[edge_index] = Option<bundle_id>），无需 HashMap
    pub edge_to_bundle: Vec<Option<usize>>,
    /// 总 Ink 节省量（像素）
    pub total_ink_saved: f64,
    /// 每条边的路径区段分解（§4.10.4 SegmentAware）
    pub edge_roles: Vec<EdgePathRoles>,
    /// 主干禁放区，供 §4.10.5 后置 label 与渲染查询
    pub trunk_keepouts: Vec<TrunkKeepout>,
    /// 箭头被抑制的边索引集合（同 bundle 内多条边指向同一节点时，只保留第一条边的箭头）
    pub arrow_suppressed: std::collections::HashSet<usize>,
}

/// Edge Bundling 调试统计（供 LayoutHints 可观测性）。
#[derive(Debug, Clone, Default)]
pub struct EdgeBundlingDebugStats {
    /// 参与兼容性评估的边数
    pub edge_count: usize,
    /// 评估的兼容边对数
    pub compatibility_pairs_evaluated: usize,
    /// 通过兼容性阈值的边对数
    pub compatible_pairs: usize,
    /// 形成的 bundle 数
    pub bundle_count: usize,
    /// 因 Ink 节省不足回退的 bundle 数
    pub ink_fallback_count: usize,
    /// 因穿障回退的 bundle 数
    pub obstacle_fallback_count: usize,
    /// 总 Ink 节省量（像素）
    pub total_ink_saved: f64,
    /// bundling 阶段耗时（微秒）
    pub elapsed_us: u64,
}

/// Edge Bundling 提示信息（附加在 LayoutHints 中）。
#[derive(Debug, Clone, Default)]
pub struct EdgeBundlingHints {
    /// 完整的 bundling 结果（含 bundle / edge_roles / trunk_keepouts）
    pub result: BundlingResult,
    /// 调试统计
    pub debug: EdgeBundlingDebugStats,
}

// ─── 辅助函数 ─────────────────────────────────────────────

/// 端口对应的外法线方向（沿轴）。
///
/// 返回 (axis, direction)：端口所在边的法线方向。
/// - Top → 垂直负方向（向上）
/// - Bottom → 垂直正方向（向下）
/// - Left → 水平负方向（向左）
/// - Right → 水平正方向（向右）
pub fn port_outward_axis(side: Port) -> (Axis, SegmentDirection) {
    match side {
        Port::Top => (Axis::Vertical, SegmentDirection::Negative),
        Port::Bottom => (Axis::Vertical, SegmentDirection::Positive),
        Port::Left => (Axis::Horizontal, SegmentDirection::Negative),
        Port::Right => (Axis::Horizontal, SegmentDirection::Positive),
    }
}

/// 判断端口是否在节点的垂直边（Left/Right）上。
pub fn is_vertical_port(side: Port) -> bool {
    matches!(side, Port::Left | Port::Right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundling_config_default() {
        let cfg = BundlingConfig::default();
        assert!(!cfg.enabled);
        assert!((cfg.compatibility_threshold - 0.5).abs() < 1e-9);
        assert_eq!(cfg.max_bundle_size, 8);
        assert!((cfg.bundle_gap - 12.0).abs() < 1e-9);
        assert!((cfg.fork_distance - 16.0).abs() < 1e-9);
        assert!((cfg.fork_spacing - 8.0).abs() < 1e-9);
        assert!((cfg.min_ink_saving - 0.1).abs() < 1e-9);
        assert_eq!(cfg.max_labeled_edges_per_bundle, 2);
        assert!((cfg.label_trunk_pad - 8.0).abs() < 1e-9);
        assert!((cfg.label_t_spacing - 0.08).abs() < 1e-9);
        assert_eq!(cfg.label_bundle_policy, LabelBundlePolicy::SegmentAware);
    }

    #[test]
    fn label_bundle_policy_default_is_segment_aware() {
        assert_eq!(LabelBundlePolicy::default(), LabelBundlePolicy::SegmentAware);
    }

    #[test]
    fn port_outward_axis_top_is_vertical_negative() {
        let (axis, dir) = port_outward_axis(Port::Top);
        assert_eq!(axis, Axis::Vertical);
        assert_eq!(dir, SegmentDirection::Negative);
    }

    #[test]
    fn port_outward_axis_right_is_horizontal_positive() {
        let (axis, dir) = port_outward_axis(Port::Right);
        assert_eq!(axis, Axis::Horizontal);
        assert_eq!(dir, SegmentDirection::Positive);
    }

    #[test]
    fn bundling_result_default_is_empty() {
        let result = BundlingResult::default();
        assert!(result.bundles.is_empty());
        assert!(result.edge_to_bundle.is_empty());
        assert!((result.total_ink_saved).abs() < 1e-9);
        assert!(result.edge_roles.is_empty());
        assert!(result.trunk_keepouts.is_empty());
    }
}
