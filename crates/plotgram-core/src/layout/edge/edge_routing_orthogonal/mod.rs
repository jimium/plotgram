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
use crate::layout::{EdgeRoutingStrategy, EdgeSnapConfig, LayoutResult};
use crate::types::DiagramType;
use crate::ast::Diagram;

// 子模块 / 测试经 `use super::*` 共享的类型与几何
pub(super) use crate::layout::geometry::Point;
pub(super) use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};

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
pub(super) mod run;

// Re-exports for cross-submodule access via `use super::*;`
pub(super) use profile::OrthoRoutingProfile;
pub(super) use channel_load::{channel_load_penalty, ChannelLoadMap};
pub(super) use context::{EndpointPair, PreparedObstacles, OrthoRoutingContext, SegmentGrid};
pub(super) use lane_assignment::{
    apply_corridor_planned_offsets, assign_lanes, separate_unrelated_trunk_overlaps,
};
pub(super) use path::{select_best_path_with_scorer_stats, PathSelectStats, RoutedSegment};
#[allow(unused_imports)] // SpacingViolationKind/segments_violate_spacing/path_edge_spacing_violations used in X-1
pub(super) use scoring::{CandidateScorer, DefaultScorer, GROUP_OBSTACLE_PAD, NODE_OBSTACLE_PAD, path_avoids_group_interiors, path_is_clean, path_is_clean_from_edges, path_length, SpacingViolationKind, segments_violate_spacing, path_edge_spacing_violations, count_all_edge_spacing_violations};
pub(super) use simplify::simplify_path;
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
pub use lane_assignment::enforce_reverse_pair_min_gap;
pub(super) use straighten::straighten_preferred_alignments;
pub(super) use stub_fix::fix_reverse_stub_ports;

// run.rs 内被 slot_replan / conflict_reroute / straighten / stub_fix 等兄弟模块调用的共享辅助
pub(super) use run::{
    endpoint_bundling_key, range_overlap_local, should_strict_group_transit,
    validated_corridor_path,
};

/// 相邻磁吸点之间的理想间距（像素）；边长不足时自动压缩。
/// 引用共享常量（与 port 容量估算共用）。
pub(super) use crate::layout::constants::ORTHO_SLOT_PITCH as SLOT_PITCH;

/// 紧凑分布模式（2-3 条边）的磁吸点间距
pub(super) const COMPACT_SLOT_PITCH: f64 = 16.0;

/// 侧通道绕行时距障碍节点的留白
pub(super) const CHANNEL_MARGIN: f64 = 18.0;

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
pub(super) const PORT_CLEARANCE: f64 = 16.0;

/// slot 在节点边上分布时保留的边界余量（占边长比例）
pub(super) const SLOT_MARGIN_RATIO: f64 = 0.12;

/// 路径穿过节点时的惩罚，确保候选路径优先绕开障碍物
pub(super) const NODE_CROSSING_PENALTY: f64 = 10_000.0;

/// 已路由边段重叠惩罚
pub(super) const EDGE_OVERLAP_PENALTY: f64 = 1_200.0;
/// 平行边重叠判定阈值（与 refine/segments_conflict_xy 共享）
pub(super) use crate::layout::constants::ORTHO_PARALLEL_GAP as EDGE_PARALLEL_GAP;

/// X-1: stub 段保护长度——从端点出发的第一段（stub）在此长度内不做硬间距检查，
/// 因为同节点相邻 slot 的 stub 天然平行近距（slot_pitch 可能小于 EDGE_PARALLEL_GAP）。
pub(super) const STUB_GUARD_LENGTH: f64 = 24.0;
/// 每个折点的惩罚（鼓励更少拐弯）
pub(super) const BEND_PENALTY: f64 = 16.0;

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
    if preserve.is_empty() || (preserve.len() as f64 / n as f64) < crate::layout::post_route::MIN_PRESERVE_RATIO {
        return route_edges_orthogonal(diagram, result, cfg);
    }
    run::route_edges_orthogonal_inner(diagram, result, cfg, Some(preserve))
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
    if (preserve_edges.len() as f64 / n as f64) < crate::layout::post_route::MIN_PRESERVE_RATIO {
        return route_edges_orthogonal(diagram, result, cfg);
    }
    run::route_edges_orthogonal_inner(diagram, result, cfg, Some(preserve_edges.clone()))
}

#[cfg(test)]
#[path = "orthogonal_tests.rs"]
mod tests;
