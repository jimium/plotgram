//! 边路由算法

pub mod common;
pub mod edge_merge_policy;
pub mod edge_routing;
pub mod edge_routing_bezier;
pub mod edge_routing_circular;
pub mod edge_routing_organic;
pub mod edge_routing_orthogonal;
pub mod edge_routing_spline;
pub mod route_annotation;
pub mod segment_pair;
pub mod visibility;

pub use route_annotation::{
    annotate_edge_from_path, freeze_route_annotations_with_merges,
    refresh_route_annotations_preserving_semantics,
    validate_route_edit, EdgeRouteAnnotation, MergeInterval, ProtectedRun,
    RouteAnnotationSet, RouteEditKind, RouteEditObstacleCtx, RouteEditValidateOpts,
    RouteEditViolation,
};
pub use segment_pair::{
    classify_segment_pair, find_needs_separation_edge_pairs, is_reverse_pair,
    measure_segment_pair, parallel_gap_for_diagram, segment_is_stub, AllowedReason,
    ClassifyPairContext, ClassifyResult, ConflictDisposition, OrthoSegment, SegmentPairMeasure,
    SeparationReason, SpacingClass, MIN_SHARED_TRUNK_LEN, STUB_GUARD_LENGTH,
};
