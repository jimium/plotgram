//! 平行段测量（Measure）与语义裁决（Classify）。
//!
//! P1：几何只回答 gap / 重叠；是否允许共线由 Classify 统一决定。
//! 不改变路由选路行为——供 lint / 基线 / 后续 X-1 只读消费。

use crate::layout::constants::{ORTHO_PARALLEL_GAP, ORTHO_PARALLEL_GAP_ARCHITECTURE};
use crate::layout::edge::common::edge_geometry::{canonical_pair, undirected_pair_key};
use crate::layout::edge::edge_merge_policy::{
    edge_merge_context, edges_may_share_trunk, requires_semantic_merge, EdgeMergeContext,
};
use crate::layout::geometry::Point;
use crate::types::DiagramType;
use crate::ast::{Diagram, Relation};
use crate::layout::LayoutResult;

/// 与正交 stub 保护区对齐。
pub const STUB_GUARD_LENGTH: f64 = 24.0;
const EPS: f64 = 0.1;
/// lint 历史阈值：共享 trunk 最短长度
pub const MIN_SHARED_TRUNK_LEN: f64 = 24.0;

/// 轴对齐段（不依赖正交模块内部类型）。
#[derive(Debug, Clone, Copy)]
pub struct OrthoSegment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub edge_index: usize,
}

impl OrthoSegment {
    pub fn from_points(a: Point, b: Point, edge_index: usize) -> Self {
        Self {
            x1: a.x,
            y1: a.y,
            x2: b.x,
            y2: b.y,
            edge_index,
        }
    }

    pub fn len(self) -> f64 {
        let dx = self.x2 - self.x1;
        let dy = self.y2 - self.y1;
        (dx * dx + dy * dy).sqrt()
    }
}

/// 纯几何测量结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentPairMeasure {
    pub gap: f64,
    pub overlap_len: f64,
    pub horizontal: bool,
    /// 投影是否重叠（含端点接触）
    pub projection_overlaps: bool,
    /// 仅端点接触（T/L），投影重叠长度 ≈ 0
    pub endpoint_touch: bool,
}

/// 间距几何类别（相对 min_gap）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacingClass {
    ExactOverlap,
    TightSpacing,
    Adequate,
}

impl SegmentPairMeasure {
    pub fn spacing_class(self, min_gap: f64) -> SpacingClass {
        if !self.projection_overlaps || self.endpoint_touch {
            return SpacingClass::Adequate;
        }
        if self.gap < EPS {
            SpacingClass::ExactOverlap
        } else if self.gap + EPS < min_gap {
            SpacingClass::TightSpacing
        } else {
            SpacingClass::Adequate
        }
    }
}

/// 测量两条轴对齐段。非平行或同边返回 None。
pub fn measure_segment_pair(a: &OrthoSegment, b: &OrthoSegment) -> Option<SegmentPairMeasure> {
    if a.edge_index == b.edge_index {
        return None;
    }
    let a_horiz = (a.y1 - a.y2).abs() < EPS;
    let b_horiz = (b.y1 - b.y2).abs() < EPS;
    let a_vert = (a.x1 - a.x2).abs() < EPS;
    let b_vert = (b.x1 - b.x2).abs() < EPS;

    if a_horiz && b_horiz {
        let gap = (a.y1 - b.y1).abs();
        let (overlap, touch) = proj_overlap_detail(a.x1, a.x2, b.x1, b.x2);
        return Some(SegmentPairMeasure {
            gap,
            overlap_len: overlap,
            horizontal: true,
            // 与历史 scoring 一致：仅内部投影重叠（端点相触不算 overlaps）
            projection_overlaps: overlap > EPS,
            endpoint_touch: touch && overlap <= EPS,
        });
    }
    if a_vert && b_vert {
        let gap = (a.x1 - b.x1).abs();
        let (overlap, touch) = proj_overlap_detail(a.y1, a.y2, b.y1, b.y2);
        return Some(SegmentPairMeasure {
            gap,
            overlap_len: overlap,
            horizontal: false,
            projection_overlaps: overlap > EPS,
            endpoint_touch: touch && overlap <= EPS,
        });
    }
    None
}

fn proj_overlap_detail(a0: f64, a1: f64, b0: f64, b1: f64) -> (f64, bool) {
    let a_min = a0.min(a1);
    let a_max = a0.max(a1);
    let b_min = b0.min(b1);
    let b_max = b0.max(b1);
    let overlap = (a_max.min(b_max) - a_min.max(b_min)).max(0.0);
    // 端点接触：区间相触但内部重叠 ≈ 0
    let touch = overlap <= EPS
        && ((a_max - b_min).abs() < EPS
            || (b_max - a_min).abs() < EPS
            || (a_min - b_max).abs() < EPS
            || (b_min - a_max).abs() < EPS);
    // 也覆盖「一端点落在另一段内部」且算投影重叠的情况：此时 overlap>0，不是纯 touch
    let endpoint_on_span = overlap <= EPS
        && (point_on_closed(a_min, a_max, b_min)
            || point_on_closed(a_min, a_max, b_max)
            || point_on_closed(b_min, b_max, a_min)
            || point_on_closed(b_min, b_max, a_max));
    (overlap, touch || endpoint_on_span)
}

fn point_on_closed(min_v: f64, max_v: f64, p: f64) -> bool {
    p >= min_v - EPS && p <= max_v + EPS
}

/// 冲突裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictDisposition {
    Allowed,
    NeedsSeparation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedReason {
    EndpointTouch,
    StubZone,
    SemanticTrunkShare,
    AdequateGap,
    NoOverlap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeparationReason {
    ReversePair,
    NonSemanticTrunk,
    FlowchartTrunkCoincidence,
    TightNonShared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassifyResult {
    pub disposition: ConflictDisposition,
    pub allowed: Option<AllowedReason>,
    pub separation: Option<SeparationReason>,
}

impl ClassifyResult {
    pub fn allowed(reason: AllowedReason) -> Self {
        Self {
            disposition: ConflictDisposition::Allowed,
            allowed: Some(reason),
            separation: None,
        }
    }

    pub fn needs_separation(reason: SeparationReason) -> Self {
        Self {
            disposition: ConflictDisposition::NeedsSeparation,
            allowed: None,
            separation: Some(reason),
        }
    }

    pub fn is_allowed(self) -> bool {
        matches!(self.disposition, ConflictDisposition::Allowed)
    }
}

/// Classify 上下文（边对级）。
#[derive(Debug, Clone)]
pub struct ClassifyPairContext<'a> {
    pub diagram_type: DiagramType,
    pub min_gap: f64,
    pub edge_a: EdgeMergeContext<'a>,
    pub edge_b: EdgeMergeContext<'a>,
    pub a_is_stub: bool,
    pub b_is_stub: bool,
    pub reverse_pair: bool,
}

pub fn parallel_gap_for_diagram(dt: DiagramType) -> f64 {
    match dt {
        DiagramType::Architecture => ORTHO_PARALLEL_GAP_ARCHITECTURE,
        _ => ORTHO_PARALLEL_GAP,
    }
}

/// 语义裁决：先 Measure，再注入 stub / merge / 正反向 / profile。
pub fn classify_segment_pair(
    measure: &SegmentPairMeasure,
    ctx: &ClassifyPairContext<'_>,
) -> ClassifyResult {
    if !measure.projection_overlaps {
        return ClassifyResult::allowed(AllowedReason::NoOverlap);
    }
    if measure.endpoint_touch {
        return ClassifyResult::allowed(AllowedReason::EndpointTouch);
    }

    let spacing = measure.spacing_class(ctx.min_gap);
    if matches!(spacing, SpacingClass::Adequate) {
        return ClassifyResult::allowed(AllowedReason::AdequateGap);
    }

    // 任一侧为短 stub 保护区 → 端口汇聚合法
    if ctx.a_is_stub || ctx.b_is_stub {
        return ClassifyResult::allowed(AllowedReason::StubZone);
    }

    // 正反向对：始终禁止共干 / 紧间距
    if ctx.reverse_pair {
        return ClassifyResult::needs_separation(SeparationReason::ReversePair);
    }

    let may_share = edges_may_share_trunk(&ctx.edge_a, &ctx.edge_b, ctx.diagram_type.clone());

    // Flowchart：semantic_merge 关闭时 edges_may_share_trunk 恒 true，
    // 但产品规则默认禁止 trunk 几何巧合共线。
    if !requires_semantic_merge(ctx.diagram_type.clone()) {
        return match spacing {
            SpacingClass::ExactOverlap | SpacingClass::TightSpacing => {
                ClassifyResult::needs_separation(SeparationReason::FlowchartTrunkCoincidence)
            }
            SpacingClass::Adequate => ClassifyResult::allowed(AllowedReason::AdequateGap),
        };
    }

    // Architecture：仅语义允许共享时，exact 共干合法；tight 仍需分离（同组合流应为 0 gap）
    if may_share {
        return match spacing {
            SpacingClass::ExactOverlap => {
                ClassifyResult::allowed(AllowedReason::SemanticTrunkShare)
            }
            SpacingClass::TightSpacing => {
                ClassifyResult::needs_separation(SeparationReason::TightNonShared)
            }
            SpacingClass::Adequate => ClassifyResult::allowed(AllowedReason::AdequateGap),
        };
    }

    match spacing {
        SpacingClass::ExactOverlap => {
            ClassifyResult::needs_separation(SeparationReason::NonSemanticTrunk)
        }
        SpacingClass::TightSpacing => {
            ClassifyResult::needs_separation(SeparationReason::TightNonShared)
        }
        SpacingClass::Adequate => ClassifyResult::allowed(AllowedReason::AdequateGap),
    }
}

/// 是否为同一无向节点对上的正反向边。
pub fn is_reverse_pair(rel_a: &Relation, rel_b: &Relation) -> bool {
    if undirected_pair_key(rel_a.from.as_str(), rel_a.to.as_str())
        != undirected_pair_key(rel_b.from.as_str(), rel_b.to.as_str())
    {
        return false;
    }
    let (ca, _) = canonical_pair(rel_a.from.as_str(), rel_a.to.as_str());
    let a_forward = rel_a.from.as_str() == ca;
    let b_forward = rel_b.from.as_str() == ca;
    a_forward != b_forward
}

/// 路径上段是否视为 stub（首/末段且长度 ≤ stub_guard）。
pub fn segment_is_stub(path_len_segs: usize, seg_index: usize, seg_len: f64, stub_guard: f64) -> bool {
    if path_len_segs == 0 {
        return false;
    }
    let is_end = seg_index == 0 || seg_index + 1 == path_len_segs;
    is_end && seg_len <= stub_guard + EPS
}

/// 扫描布局结果，收集 `NeedsSeparation` 的边对（用于 lint）。
///
/// 仅统计 `overlap_len >= MIN_SHARED_TRUNK_LEN` 的 exact/tight，与历史
/// `UnrelatedEdgeTrunkMerge` 的长 trunk 语义对齐；architecture 专用门控由调用方决定。
pub fn find_needs_separation_edge_pairs(
    diagram: &Diagram,
    result: &LayoutResult,
) -> Vec<(usize, usize, SeparationReason, f64)> {
    let n = result.edges.len().min(diagram.relations.len());
    let min_gap = parallel_gap_for_diagram(diagram.diagram_type.clone());
    let mut out = Vec::new();

    for i in 0..n {
        for j in (i + 1)..n {
            let Some(rel_i) = diagram.relations.get(i) else {
                continue;
            };
            let Some(rel_j) = diagram.relations.get(j) else {
                continue;
            };
            let pts_i: Vec<Point> = result.edges[i].path_points().into_owned();
            let pts_j: Vec<Point> = result.edges[j].path_points().into_owned();
            if pts_i.len() < 2 || pts_j.len() < 2 {
                continue;
            }

            let ctx_i = edge_merge_context(rel_i.from.as_str(), rel_i.to.as_str(), i);
            let ctx_j = edge_merge_context(rel_j.from.as_str(), rel_j.to.as_str(), j);
            let reverse = is_reverse_pair(rel_i, rel_j);

            let mut best_overlap = 0.0;
            let mut best_reason: Option<SeparationReason> = None;

            let n_i = pts_i.len() - 1;
            let n_j = pts_j.len() - 1;
            for (si, wi) in pts_i.windows(2).enumerate() {
                let seg_i = OrthoSegment::from_points(wi[0], wi[1], i);
                let stub_i = segment_is_stub(n_i, si, seg_i.len(), STUB_GUARD_LENGTH);
                for (sj, wj) in pts_j.windows(2).enumerate() {
                    let seg_j = OrthoSegment::from_points(wj[0], wj[1], j);
                    let stub_j = segment_is_stub(n_j, sj, seg_j.len(), STUB_GUARD_LENGTH);
                    let Some(m) = measure_segment_pair(&seg_i, &seg_j) else {
                        continue;
                    };
                    if m.overlap_len + EPS < MIN_SHARED_TRUNK_LEN {
                        continue;
                    }
                    let pair_ctx = ClassifyPairContext {
                        diagram_type: diagram.diagram_type.clone(),
                        min_gap,
                        edge_a: ctx_i,
                        edge_b: ctx_j,
                        a_is_stub: stub_i,
                        b_is_stub: stub_j,
                        reverse_pair: reverse,
                    };
                    let cls = classify_segment_pair(&m, &pair_ctx);
                    if !cls.is_allowed() {
                        if m.overlap_len > best_overlap {
                            best_overlap = m.overlap_len;
                            best_reason = cls.separation;
                        }
                    }
                }
            }

            if let Some(reason) = best_reason {
                // lint 历史：仅 architecture 的 NonSemanticTrunk（及同类）报 UnrelatedEdgeTrunkMerge
                out.push((i, j, reason, best_overlap));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(ei: usize, x1: f64, y1: f64, x2: f64, y2: f64) -> OrthoSegment {
        OrthoSegment {
            x1,
            y1,
            x2,
            y2,
            edge_index: ei,
        }
    }

    fn ctx_arch<'a>(
        a: EdgeMergeContext<'a>,
        b: EdgeMergeContext<'a>,
        stub: bool,
        reverse: bool,
    ) -> ClassifyPairContext<'a> {
        ClassifyPairContext {
            diagram_type: DiagramType::Architecture,
            min_gap: ORTHO_PARALLEL_GAP_ARCHITECTURE,
            edge_a: a,
            edge_b: b,
            a_is_stub: stub,
            b_is_stub: stub,
            reverse_pair: reverse,
        }
    }

    #[test]
    fn measure_exact_horizontal() {
        let a = seg(0, 0.0, 10.0, 100.0, 10.0);
        let b = seg(1, 20.0, 10.0, 80.0, 10.0);
        let m = measure_segment_pair(&a, &b).unwrap();
        assert!(m.gap < EPS);
        assert!((m.overlap_len - 60.0).abs() < 1.0);
        assert_eq!(m.spacing_class(8.0), SpacingClass::ExactOverlap);
    }

    #[test]
    fn stub_zone_allowed() {
        let a = seg(0, 0.0, 0.0, 100.0, 0.0);
        let b = seg(1, 0.0, 0.0, 100.0, 0.0);
        let m = measure_segment_pair(&a, &b).unwrap();
        let ea = edge_merge_context("a", "b", 0);
        let eb = edge_merge_context("c", "d", 1);
        let cls = classify_segment_pair(
            &m,
            &ctx_arch(ea, eb, true, false),
        );
        assert!(cls.is_allowed());
        assert_eq!(cls.allowed, Some(AllowedReason::StubZone));
    }

    #[test]
    fn reverse_pair_needs_separation() {
        let a = seg(0, 0.0, 0.0, 0.0, 100.0);
        let b = seg(1, 0.0, 0.0, 0.0, 100.0);
        let m = measure_segment_pair(&a, &b).unwrap();
        let ea = edge_merge_context("auth", "db", 0);
        let eb = edge_merge_context("db", "auth", 1);
        let cls = classify_segment_pair(&m, &ctx_arch(ea, eb, false, true));
        assert!(!cls.is_allowed());
        assert_eq!(cls.separation, Some(SeparationReason::ReversePair));
    }

    #[test]
    fn architecture_semantic_share_allowed() {
        let a = seg(0, 0.0, 0.0, 0.0, 100.0);
        let b = seg(1, 0.0, 0.0, 0.0, 100.0);
        let m = measure_segment_pair(&a, &b).unwrap();
        // 同源 fan-out
        let ea = edge_merge_context("hub", "leaf1", 0);
        let eb = edge_merge_context("hub", "leaf2", 1);
        assert!(edges_may_share_trunk(&ea, &eb, DiagramType::Architecture));
        let cls = classify_segment_pair(&m, &ctx_arch(ea, eb, false, false));
        assert!(cls.is_allowed());
        assert_eq!(cls.allowed, Some(AllowedReason::SemanticTrunkShare));
    }

    #[test]
    fn architecture_unrelated_needs_separation() {
        let a = seg(0, 0.0, 0.0, 0.0, 100.0);
        let b = seg(1, 0.0, 0.0, 0.0, 100.0);
        let m = measure_segment_pair(&a, &b).unwrap();
        let ea = edge_merge_context("a", "b", 0);
        let eb = edge_merge_context("c", "d", 1);
        assert!(!edges_may_share_trunk(&ea, &eb, DiagramType::Architecture));
        let cls = classify_segment_pair(&m, &ctx_arch(ea, eb, false, false));
        assert!(!cls.is_allowed());
        assert_eq!(cls.separation, Some(SeparationReason::NonSemanticTrunk));
    }

    #[test]
    fn flowchart_trunk_coincidence_needs_separation() {
        let a = seg(0, 0.0, 0.0, 100.0, 0.0);
        let b = seg(1, 0.0, 0.0, 100.0, 0.0);
        let m = measure_segment_pair(&a, &b).unwrap();
        let ea = edge_merge_context("a", "b", 0);
        let eb = edge_merge_context("a", "b", 1); // 同对但 flowchart 仍禁 trunk 巧合
        let ctx = ClassifyPairContext {
            diagram_type: DiagramType::Flowchart,
            min_gap: ORTHO_PARALLEL_GAP,
            edge_a: ea,
            edge_b: eb,
            a_is_stub: false,
            b_is_stub: false,
            reverse_pair: false,
        };
        let cls = classify_segment_pair(&m, &ctx);
        assert!(!cls.is_allowed());
        assert_eq!(
            cls.separation,
            Some(SeparationReason::FlowchartTrunkCoincidence)
        );
    }
}
