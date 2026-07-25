//! Plotgram 布局模块
//!
//! 提供可插拔的布局算法框架。每种布局算法实现 `LayoutStrategy` trait，
//! 通过 `compute_layout` 统一调度。
//!
//! ## 模块组织（doc 34 结构收敛后）
//!
//! | 模块 | 职责 |
//! |------|------|
//! | [`pipeline`] | 管线编排：入口调度、算法注册、计划解析 |
//! | [`recipes`] | 布局配方：compile → LayoutContract / CoordinateProblem |
//! | [`kernel`] | 求解内核：坐标 / Group IR / layered / common |
//! | [`routing`] | 边路由 + post_route + group_ctx |
//! | [`quality`] | lint / metrics / refine |
//! | [`group`] | 走廊 / 写权 / 常量（收缩后） |
//! | [`demand`] | 空间需求与走廊模型 |
//! | [`snap`] | 对齐与画布最终化 |
//!
//! 过渡：[`engines`] 仅保留遗留 `coordinate` builder；common/layered 已在 kernel。

pub mod algorithm_config;
pub mod catalog;
pub mod snap;
pub mod constants;
pub mod decl_order;
pub mod demand;
pub mod routing;
pub mod engines;
pub mod geometry;
pub mod geometry_helpers;
pub mod group;
pub mod kernel;
pub mod quality;
pub mod recipes;
pub mod perf;
pub mod pipeline;
pub mod route_feedback;
pub mod traits;
pub mod types;

pub use algorithm_config::{
    AlgorithmOptionSpec, ArchitectureV2LayoutConfig, CircularLayoutConfig,
    MindmapLayoutConfig, OptionKind, SequenceLayoutConfig, SugiyamaLayoutConfig,
};
pub use catalog::{
    layout_catalog, AlgorithmOptionInfo, DiagramTypeCatalog, EdgeRoutingAlgoInfo, LayoutAlgoInfo,
    LayoutCatalog,
};
pub use pipeline::plan::{validate_layout_plan_warnings, LayoutPlan, ResolvedAlgoOptions};
pub use quality::lint::{
    compute_lint_metrics, count_unrelated_parallel_overlaps, lint_layout, parse_lint_profile,
    parse_lint_rule, parse_lint_rules_list, AdviceConfidence, LayoutKnob, LintAdvice,
    LayoutLinter, LayoutViolation, LintConfig, LintMetricsSummary, LintProfile, LintReport,
    LintRuleId, LintSeverity, RuleConfig,
};
pub use demand::band::{
    demand_extra_over_base, edge_band_demand, layer_gaps_from_demand, EdgeBandDemandBreakdown,
    EdgeBandDemandProfile,
};
pub use demand::{
    collect_edge_features, compute_corridor_model, score_edge, score_edges,
    DifficultyProfile, EdgeFeatures, PressureSnapshot,
};
pub use quality::metrics::{
    compute_collinear_sample_metrics, compute_congestion_sample_metrics, node_fingerprint,
    CollinearBaselineSnapshot, CollinearOrthoStats, CollinearSampleMetrics,
    CongestionBaselineSnapshot, CongestionSampleMetrics,
};
pub use pipeline::registry::{EDGE_ROUTING_NAMES, LAYOUT_ALGORITHM_NAMES};
pub use snap::grid_snap::{DiagramAlignOverride, EdgeSnapConfig, LayerAxisAlign, NodeAlignConfig};
pub use routing::segment_pair::{
    classify_segment_pair, find_needs_separation_edge_pairs, measure_segment_pair,
    ClassifyPairContext, ClassifyResult, ConflictDisposition, OrthoSegment, SegmentPairMeasure,
    SeparationReason, SpacingClass,
};

pub use routing::{
    edge_routing, edge_routing_bezier, edge_routing_circular,
    edge_routing_organic, edge_routing_orthogonal, edge_routing_spline, visibility,
};
pub use recipes::{
    architecture, circular, er, flowchart, mindmap, sequence,
};

// Re-exports from split modules (preserve external API)
pub use types::{
    NodeLayout, GroupLayout, GroupTable, Port, PathGeometry, EdgeLabelLayout, EdgeLayout,
    EdgeRoutingStyle, LayoutHints, GutterBudgetDebug, RefineDebugStats,
    OrthoDebugStats, GroupLayoutWarning, GroupLayoutWarningKind,
    LayoutResult, GroupContainmentViolation, ContainmentViolationKind,
};
pub use traits::{LayoutStrategy, RoutingProduct, RoutingRecipeDyn};
pub use geometry_helpers::{styled_node_size, edge_point, ellipse_edge_point};
pub use pipeline::entry::{
    resolve_effective_direction, compute_layout, compute_layout_incremental,
    compute_layout_with_plan,
    layout_option_specs, edge_routing_option_specs, applicable_layouts_for_type,
    applicable_routings_for_type, diagram_types_for_layout, diagram_types_for_routing,
    BUILTIN_DIAGRAM_TYPES,
};
// `pub(in crate::layout)` items from pipeline modules
use pipeline::registry::{all_layout_strategies, all_routing_strategies};

#[cfg(test)]
mod tests;
