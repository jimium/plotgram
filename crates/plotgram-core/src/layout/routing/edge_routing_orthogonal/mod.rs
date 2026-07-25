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
use crate::layout::LayoutResult;
use crate::ast::Diagram;

// 子模块 / 测试经 `use super::*` 共享的类型与几何
pub(super) use crate::layout::geometry::Point;
pub(super) use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
pub(super) use crate::types::DiagramType;

pub(super) mod profile;
pub(super) mod channel_load;
pub(super) mod context;
pub(super) mod corridor_route;
pub(super) mod feedback_side;
pub(super) mod lane_assignment;
pub(super) mod layer_order;
pub(super) mod path;
pub(super) mod path_kernel;
pub(crate) use path_kernel::repair_group_interior_crossings;
pub(super) mod path_legacy;
pub(super) mod scoring;
pub(super) mod simplify;
pub(super) mod shape_boundary;
pub(super) mod slot;
pub(super) mod contract;
pub(super) mod sanitize;
pub(super) mod stub_occupancy;
pub(super) mod semantic_trunk_merge;
pub(super) mod draft;
pub(super) mod run;
pub(super) mod phases;
pub(super) mod visibility_graph;
pub(super) mod port_solver;
pub(super) mod channel_planner;
pub(super) mod path_solver;

// Re-exports for cross-submodule access via `use super::*;`
pub(super) use profile::OrthoRoutingProfile;
pub(super) use channel_load::{channel_load_penalty, corridor_overflow_penalty, ChannelLoadMap};
pub(super) use context::{EndpointPair, PreparedObstacles, OrthoRoutingContext, SegmentGrid};
pub(crate) use context::SegmentGrid as OrthoSegmentGrid;
pub(super) use lane_assignment::{
    apply_corridor_planned_offsets, assign_lanes, separate_unrelated_trunk_overlaps,
};
pub(super) use path::{select_best_path_with_scorer_stats, PathSelectStats, RoutedSegment};
#[allow(unused_imports)] // SpacingViolationKind/segments_violate_spacing/path_edge_spacing_violations used in X-1
pub(super) use scoring::{CandidateScorer, DefaultScorer, GROUP_OBSTACLE_PAD, NODE_OBSTACLE_PAD, path_is_clean_from_edges, path_length, SpacingViolationKind, segments_violate_spacing, path_edge_spacing_violations, count_all_edge_spacing_violations};
/// Phase 1：供 `kernel::route::feasibility` 薄封装复用（生产调用点仍在本模块内）。
pub(crate) use scoring::{path_avoids_group_interiors, path_is_clean};
pub(super) use simplify::simplify_path;
#[allow(unused_imports)] // choose_pair_sides is used by tests
pub(super) use slot::{
    choose_docking_strategy, choose_pair_sides, choose_pair_sides_with_group, is_vertical_port, slot_anchor, slot_fraction,
    slot_fraction_around, DockingStrategy, Endpoint,
};
// Slice D3：sanitize 内核不再对外再导出——canonicalize 归 materializer，
// 调用方经 `GeometryMaterializer::canonicalize_orthogonal_edges` 进入。
pub use lane_assignment::{
    enforce_reverse_pair_dock_separation, enforce_reverse_pair_min_gap,
};
pub use stub_occupancy::{
    collect_stub_occupancy, estimate_layer_band_demands, find_stub_occupancy_conflicts,
    resolve_exact_stub_occupancy_post_route, resolve_stub_occupancy_conflicts, LayerBandDemand,
    StubOccupancyConflict, StubOccupancyRecord, StubOccupancyStats,
};

// run.rs 内被 path_solver 等兄弟模块调用的共享辅助
pub(super) use run::{
    endpoint_bundling_key, should_strict_group_transit, validated_corridor_path,
};

// run 总控各 phase 实现（A4 从 run.rs 拆出）；供总控调用
pub(super) use phases::{
    phase_lane, phase_layer_order, phase_port_slot, phase_route_edges, phase_sanitize,
};

/// 相邻磁吸点之间的理想间距（像素）；边长不足时自动压缩。
/// 引用共享常量（与 port 容量估算共用）。
pub(super) use crate::layout::constants::ORTHO_SLOT_PITCH as SLOT_PITCH;

/// 紧凑分布模式（2-3 条边）的磁吸点间距
pub const COMPACT_SLOT_PITCH: f64 = 16.0;

/// 侧通道绕行时距障碍节点的留白
/// Phase A 优化：18→24，给回环边和侧通道边更多空间。
pub(super) const CHANNEL_MARGIN: f64 = 24.0;

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
    /// 路由算法正式配置（Slice B：原 PLOTGRAM_* 正式 env 硬切于此）。
    pub routing: crate::layout::routing::config::RoutingConfig,
}

impl OrthoConfig {
    pub fn from_spec_defaults() -> Self {
        Self {
            slot_pitch: ORTHOGONAL_OPTIONS[0].default,
            channel_margin: ORTHOGONAL_OPTIONS[1].default,
            routing: Default::default(),
        }
    }
}

// R10b: RoutingRecipeDyn / OrthogonalRouting 已删除——orthogonal 经 RecipeRouter<OrthogonalRecipe> 驱动。

/// 从节点边界向外延伸的短线段，避免一出线就折回节点内部
pub(super) const PORT_CLEARANCE: f64 = 16.0;

/// slot 在节点边上分布时保留的边界余量（占边长比例）
pub(super) const SLOT_MARGIN_RATIO: f64 = 0.12;

/// 已路由边段重叠惩罚
pub(super) const EDGE_OVERLAP_PENALTY: f64 = 1_200.0;
/// 平行边重叠判定阈值（与 refine/segments_conflict_xy 共享）
pub(super) use crate::layout::constants::ORTHO_PARALLEL_GAP as EDGE_PARALLEL_GAP;

/// X-1: stub 段保护长度——从端点出发的第一段（stub）在此长度内不做硬间距检查，
/// 因为同节点相邻 slot 的 stub 天然平行近距（slot_pitch 可能小于 EDGE_PARALLEL_GAP）。
/// A2：单一来源见 `constants::STUB_GUARD_LENGTH`。
pub(super) use crate::layout::constants::STUB_GUARD_LENGTH;
/// 每个折点的惩罚（鼓励更少拐弯）
/// Phase A 优化：16→28，使 scorer 更强烈偏好少弯折路径。
pub(super) const BEND_PENALTY: f64 = 28.0;

/// 侧通道距障碍节点的最小留白（即便被分组边框挤压也要保留）
pub(super) const MIN_CHANNEL_CLEARANCE: f64 = 10.0;

/// 坐标比较容差
pub(super) const EPS: f64 = 0.1;

/// 在节点布局完成后，为所有边计算正交路径与标签位置
pub fn route_edges_orthogonal(
    diagram: &Diagram,
    result: LayoutResult,
    cfg: OrthoConfig,
) -> LayoutResult {
    run::route_edges_orthogonal_inner(diagram, result, cfg, None)
}

/// 正交路由内核（支持 preserve 增量重路由）——供 recipe/orthogonal.rs 调用。
///
/// Slice F2c：本模块旧的节点位移增量重路由入口已删除，
/// 跨渲染增量统一走 Coordinator 的 `FrozenRoutingSolution` 依赖记录。
pub(crate) fn route_orthogonal_inner(
    diagram: &Diagram,
    result: LayoutResult,
    cfg: OrthoConfig,
    preserve: Option<std::collections::HashSet<usize>>,
) -> LayoutResult {
    run::route_edges_orthogonal_inner(diagram, result, cfg, preserve)
}

#[cfg(test)]
#[path = "orthogonal_tests.rs"]
mod tests;
