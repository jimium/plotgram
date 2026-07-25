//! 共线 / 重叠基线度量
//!
//! P0：观测基建。P1：`allowed_share_len` / exact/tight 经 Classify 分桶。

use crate::ast::Diagram;
use crate::layout::routing::edge_merge_policy::edge_merge_context;
use crate::layout::routing::segment_pair::{
    classify_segment_pair, is_reverse_pair, measure_segment_pair, parallel_gap_for_diagram,
    segment_is_stub, AllowedReason, ClassifyPairContext, OrthoSegment, SpacingClass,
    STUB_GUARD_LENGTH,
};
use crate::layout::geometry::Point;
use crate::layout::quality::lint::{compute_lint_metrics, LintMetricsSummary};
use crate::layout::quality::metrics::aesthetics::{compute_aesthetics, AestheticsReport};
use crate::layout::{LayoutResult, OrthoDebugStats};
use serde::{Deserialize, Serialize};

const EPS: f64 = 0.1;

/// 单文件共线基线指标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollinearSampleMetrics {
    pub file: String,
    pub diagram_type: String,
    pub nodes: usize,
    pub edges: usize,
    pub groups: usize,
    /// 节点中心指纹（稳定序列的 FNV-1a hex）
    pub node_fp: String,
    /// NeedsSeparation 的 exact：Σ shared_length
    pub exact_sev: f64,
    /// NeedsSeparation 的 tight：Σ (gap_deficit × overlap_len)
    pub tight_sev: f64,
    /// Allowed（语义合流 / stub 等）的 exact 共享长度
    pub allowed_share_len: f64,
    pub exact_pairs: usize,
    pub tight_pairs: usize,
    pub lint: LintMetricsSummary,
    pub ortho: Option<CollinearOrthoStats>,
    pub min_gap: f64,
    /// 由 snapshot 脚本注入（bench-phases）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub median_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub det: Option<bool>,
    /// 美学指标（观测维度）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aesthetics: Option<AestheticsReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollinearOrthoStats {
    pub edge_exact_overlap_pairs: usize,
    pub edge_tight_spacing_pairs: usize,
    pub degraded_count: usize,
    pub reroute_iterations: usize,
    pub rerouted_edges: usize,
    pub lane_groups: usize,
    pub lane_segments_shifted: usize,
    #[serde(default)]
    pub stub_occupancy_conflicts: usize,
    #[serde(default)]
    pub stub_cross_pair_conflicts: usize,
    #[serde(default)]
    pub stub_occupancy_shifted: usize,
    /// A-0 契约诊断：stub 未出组边数
    #[serde(default)]
    pub contract_stub_violations: usize,
    /// A-0 契约诊断：approach 方向违约边数（sanitize 后通常为 0）
    #[serde(default)]
    pub contract_approach_violations: usize,
    /// A-0 契约诊断：目标端口未面向源节点的边数（ISS-002 直接信号）
    #[serde(default)]
    pub contract_unnatural_to_port: usize,
    /// A-0 契约诊断：远离段总数
    #[serde(default)]
    pub contract_away_segments: usize,
    /// A-0 契约诊断：含远离段的边数
    #[serde(default)]
    pub contract_away_edges: usize,
}

impl From<&OrthoDebugStats> for CollinearOrthoStats {
    fn from(o: &OrthoDebugStats) -> Self {
        Self {
            edge_exact_overlap_pairs: o.edge_exact_overlap_pairs,
            edge_tight_spacing_pairs: o.edge_tight_spacing_pairs,
            degraded_count: o.degraded_count,
            reroute_iterations: o.reroute_iterations,
            rerouted_edges: o.rerouted_edges,
            lane_groups: o.lane_groups,
            lane_segments_shifted: o.lane_segments_shifted,
            stub_occupancy_conflicts: o.stub_occupancy_conflicts,
            stub_cross_pair_conflicts: o.stub_cross_pair_conflicts,
            stub_occupancy_shifted: o.stub_occupancy_shifted,
            contract_stub_violations: o.contract_stub_violations,
            contract_approach_violations: o.contract_approach_violations,
            contract_unnatural_to_port: o.contract_unnatural_to_port,
            contract_away_segments: o.contract_away_segments,
            contract_away_edges: o.contract_away_edges,
        }
    }
}

/// 多文件快照根对象。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollinearBaselineSnapshot {
    pub date: String,
    pub note: String,
    pub samples: Vec<CollinearSampleMetrics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perf_runs: Option<usize>,
}

/// 计算单图共线基线指标。
pub fn compute_collinear_sample_metrics(
    file: &str,
    diagram: &Diagram,
    result: &LayoutResult,
) -> CollinearSampleMetrics {
    let min_gap = parallel_gap_for_diagram(diagram.diagram_type.clone());
    let (exact_sev, tight_sev, allowed_share_len, exact_pairs, tight_pairs) =
        aggregate_spacing_severity(diagram, result, min_gap);
    let lint = compute_lint_metrics(diagram, result);
    let ortho = result
        .hints
        .orthogonal_debug
        .as_ref()
        .map(CollinearOrthoStats::from);

    let aesthetics = compute_aesthetics(diagram, result);

    CollinearSampleMetrics {
        file: file.to_string(),
        diagram_type: format!("{:?}", diagram.diagram_type),
        nodes: result.nodes.len(),
        edges: result.edges.len(),
        groups: result.groups.len(),
        node_fp: node_fingerprint(result),
        exact_sev,
        tight_sev,
        allowed_share_len,
        exact_pairs,
        tight_pairs,
        lint,
        ortho,
        min_gap,
        median_ms: None,
        min_ms: None,
        max_ms: None,
        det: None,
        aesthetics: Some(aesthetics),
    }
}

/// 节点中心指纹：按 id 排序后的 `(id, cx, cy)` 稳定哈希。
pub fn node_fingerprint(result: &LayoutResult) -> String {
    let mut items: Vec<(String, i64, i64)> = result
        .nodes
        .iter()
        .map(|(id, nl)| {
            let cx = nl.x + nl.width / 2.0;
            let cy = nl.y + nl.height / 2.0;
            (
                id.clone(),
                (cx * 100.0).round() as i64,
                (cy * 100.0).round() as i64,
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut payload = String::new();
    for (id, cx, cy) in items {
        payload.push_str(&id);
        payload.push('\t');
        payload.push_str(&cx.to_string());
        payload.push('\t');
        payload.push_str(&cy.to_string());
        payload.push('\n');
    }
    format!("{:016x}", fnv1a64(payload.as_bytes()))
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Clone, Copy)]
struct SegRef {
    edge_index: usize,
    seg_index: usize,
    path_segs: usize,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

fn aggregate_spacing_severity(
    diagram: &Diagram,
    result: &LayoutResult,
    min_gap: f64,
) -> (f64, f64, f64, usize, usize) {
    let n_rel = diagram.relations.len();
    let mut segs: Vec<SegRef> = Vec::new();
    for (ei, edge) in result.edges.iter().enumerate() {
        if edge.path_is_empty() {
            continue;
        }
        let points: Vec<Point> = edge.path_points().into_owned();
        if points.len() < 2 {
            continue;
        }
        let n_segs = points.len() - 1;
        for (si, w) in points.windows(2).enumerate() {
            segs.push(SegRef {
                edge_index: ei,
                seg_index: si,
                path_segs: n_segs,
                x1: w[0].x,
                y1: w[0].y,
                x2: w[1].x,
                y2: w[1].y,
            });
        }
    }

    segs.sort_by(|a, b| {
        a.edge_index
            .cmp(&b.edge_index)
            .then(a.seg_index.cmp(&b.seg_index))
            .then(a.x1.total_cmp(&b.x1))
            .then(a.y1.total_cmp(&b.y1))
    });

    let mut exact_sev = 0.0;
    let mut tight_sev = 0.0;
    let mut allowed_share_len = 0.0;
    let mut exact_pairs = 0usize;
    let mut tight_pairs = 0usize;

    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            if segs[i].edge_index == segs[j].edge_index {
                continue;
            }
            let ei = segs[i].edge_index;
            let ej = segs[j].edge_index;
            if ei >= n_rel || ej >= n_rel {
                continue;
            }
            let oa = OrthoSegment {
                x1: segs[i].x1,
                y1: segs[i].y1,
                x2: segs[i].x2,
                y2: segs[i].y2,
                edge_index: ei,
            };
            let ob = OrthoSegment {
                x1: segs[j].x1,
                y1: segs[j].y1,
                x2: segs[j].x2,
                y2: segs[j].y2,
                edge_index: ej,
            };
            let Some(m) = measure_segment_pair(&oa, &ob) else {
                continue;
            };
            if m.overlap_len <= EPS {
                continue;
            }

            let rel_i = &diagram.relations[ei];
            let rel_j = &diagram.relations[ej];
            let stub_i = segment_is_stub(
                segs[i].path_segs,
                segs[i].seg_index,
                oa.len(),
                STUB_GUARD_LENGTH,
            );
            let stub_j = segment_is_stub(
                segs[j].path_segs,
                segs[j].seg_index,
                ob.len(),
                STUB_GUARD_LENGTH,
            );
            let pair_ctx = ClassifyPairContext {
                diagram_type: diagram.diagram_type.clone(),
                min_gap,
                edge_a: edge_merge_context(rel_i.from.as_str(), rel_i.to.as_str(), ei),
                edge_b: edge_merge_context(rel_j.from.as_str(), rel_j.to.as_str(), ej),
                a_is_stub: stub_i,
                b_is_stub: stub_j,
                reverse_pair: is_reverse_pair(rel_i, rel_j),
            };
            let cls = classify_segment_pair(&m, &pair_ctx);
            let spacing = m.spacing_class(min_gap);

            if cls.is_allowed() {
                if matches!(spacing, SpacingClass::ExactOverlap)
                    && matches!(
                        cls.allowed,
                        Some(AllowedReason::SemanticTrunkShare | AllowedReason::StubZone)
                    )
                {
                    allowed_share_len += m.overlap_len;
                }
                continue;
            }

            match spacing {
                SpacingClass::ExactOverlap => {
                    exact_sev += m.overlap_len;
                    exact_pairs += 1;
                }
                SpacingClass::TightSpacing => {
                    tight_sev += (min_gap - m.gap) * m.overlap_len;
                    tight_pairs += 1;
                }
                SpacingClass::Adequate => {}
            }
        }
    }

    (exact_sev, tight_sev, allowed_share_len, exact_pairs, tight_pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
    };
    use crate::layout::{EdgeLayout, LayoutHints, NodeLayout, PathGeometry};
    use crate::types::DiagramType;
    use std::collections::HashMap;

    fn empty_result() -> LayoutResult {
        LayoutResult {
            nodes: HashMap::new(),
            groups: crate::layout::GroupTable::new(),
            edges: Vec::new(),
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints::default(),
        }
    }

    fn poly(points: Vec<(f64, f64)>) -> EdgeLayout {
        let mut e = EdgeLayout::empty();
        e.geometry = PathGeometry::Polyline { points: Vec::new() };
        e.set_polyline_points(
            points
                .into_iter()
                .map(|(x, y)| Point::new(x, y))
                .collect(),
        );
        e
    }

    fn rel(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn arch_diagram(relations: Vec<Relation>) -> Diagram {
        let mut entities = Vec::new();
        for r in &relations {
            for id in [r.from.as_str(), r.to.as_str()] {
                if !entities.iter().any(|e: &Entity| e.id.as_str() == id) {
                    entities.push(Entity {
                        id: Identifier::new_unchecked(id),
                        label: id.to_string(),
                        attributes: AttributeMap::default(),
                        group_id: None,
                        span: Span::dummy(),
                    });
                }
            }
        }
        Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: Vec::new(),
            entities,
            relations,
            groups: Vec::new(),
            constraints: Vec::new(),
            style_decls: Vec::new(),
            doc_comment: None,
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
        }
    }

    #[test]
    fn unrelated_exact_counts_as_severity() {
        let relations = vec![rel("a", "b"), rel("c", "d")];
        let diagram = arch_diagram(relations);
        let mut result = empty_result();
        result.edges.push(poly(vec![
            (0.0, 0.0),
            (30.0, 0.0),
            (130.0, 0.0),
            (160.0, 0.0),
        ]));
        result.edges.push(poly(vec![
            (0.0, 0.0),
            (30.0, 0.0),
            (130.0, 0.0),
            (160.0, 0.0),
        ]));
        let (exact, _, allowed, pairs, _) = aggregate_spacing_severity(&diagram, &result, 12.0);
        assert!(pairs >= 1);
        assert!(exact >= 99.0, "exact={exact}");
        assert_eq!(allowed, 0.0);
    }

    #[test]
    fn semantic_fanout_goes_to_allowed() {
        let relations = vec![rel("hub", "l1"), rel("hub", "l2")];
        let diagram = arch_diagram(relations);
        let mut result = empty_result();
        result.edges.push(poly(vec![
            (0.0, 0.0),
            (30.0, 0.0),
            (130.0, 0.0),
            (160.0, 0.0),
        ]));
        result.edges.push(poly(vec![
            (0.0, 0.0),
            (30.0, 0.0),
            (130.0, 0.0),
            (160.0, 0.0),
        ]));
        let (exact, _, allowed, _, _) = aggregate_spacing_severity(&diagram, &result, 12.0);
        assert_eq!(exact, 0.0);
        assert!(allowed >= 99.0, "allowed={allowed}");
    }

    #[test]
    fn node_fp_stable_under_hashmap_order() {
        let mut a = empty_result();
        a.nodes.insert(
            "b".into(),
            NodeLayout {
                x: 10.0,
                y: 20.0,
                width: 40.0,
                height: 20.0,
            },
        );
        a.nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            },
        );
        let mut b = empty_result();
        b.nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            },
        );
        b.nodes.insert(
            "b".into(),
            NodeLayout {
                x: 10.0,
                y: 20.0,
                width: 40.0,
                height: 20.0,
            },
        );
        assert_eq!(node_fingerprint(&a), node_fingerprint(&b));
    }
}
