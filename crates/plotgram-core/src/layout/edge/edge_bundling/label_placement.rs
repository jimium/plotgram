//! §4.10: bundling 后置 label 流水线
//!
//! bundling 重写 `PathGeometry` 后，旧 label 几何失效（悬空或堆叠）。
//! 本模块基于新 path + `EdgePathRoles` 重算全部 label。
//!
//! ## 流水线（§4.10.2）
//!
//! ```text
//! Step A: 清除旧 label 几何（保留 text，重置 center/leader_to/rotation）
//!     ↓
//! Step B: 按 label 类型 + 策略初定位（SegmentAware / Conservative / Stagger）
//!     ↓
//! Step C: resolve_label_overlaps（复用现有 label_avoidance）
//!     ↓
//! Step D: assign_leader_lines（由 resolve_label_overlaps 自动调用）
//!     ↓
//! Step E: bundle 主干禁放区二次校验（label 不得压在 trunk 带内）
//! ```
//!
//! ## SegmentAware 策略（默认，§4.10.4）
//!
//! 中段 `label` 锚定在该边**独占段**（MergeLeg/ForkLeg/stub），不占用共享主干。
//! `head_label`/`tail_label` 同理优先近端独占段。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, GroupLayout, NodeLayout};

use super::types::{
    BundlingConfig, BundlingResult, EdgePathRoles, LabelBundlePolicy, SegmentRole, SegmentSpan,
};

/// 数值微分步长（计算切线角度用）
const TANGENT_DT: f64 = 0.01;

/// 坐标比较容差
const EPS: f64 = 0.1;

/// §4.10.7: bundling 后置 label 重算入口。
///
/// 流程：
/// 1. 对每条边，基于新 path + edge_roles 重算 label 位置
/// 2. 调用 `resolve_label_overlaps` 处理标签间/标签-节点/标签-路径重叠
/// 3. trunk keepout 二次校验
///
/// **不变量**：只修改 `EdgeLayout.labels`，不修改 `PathGeometry` 与端口。
pub fn relayout_edge_labels_after_bundling(
    diagram: &Diagram,
    edges: &mut [EdgeLayout],
    bundling: &BundlingResult,
    config: &BundlingConfig,
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
) {
    // ── Step A + B: 重算每条边的 label ──
    for (i, edge) in edges.iter_mut().enumerate() {
        let rel = &diagram.relations[i];
        let path: Vec<Point> = edge.path_points().into_owned();

        if path.len() < 2 {
            continue;
        }

        let is_bundled = bundling.edge_to_bundle.get(i).and_then(|b| *b).is_some();

        if is_bundled && config.label_bundle_policy == LabelBundlePolicy::SegmentAware {
            // SegmentAware: label 锚定在独占段
            let roles = &bundling.edge_roles[i];
            edge.labels = place_labels_segment_aware(rel, &path, roles, config);
        } else {
            // Conservative / Stagger / 未捆绑: 标准放置（全路径 t）
            let middle_t = crate::layout::edge::common::edge_geometry::parse_label_t(rel);
            edge.labels = crate::layout::edge::common::edge_geometry::build_edge_labels(
                rel,
                middle_t,
                Point::new(0.0, 0.0),
                |t| {
                    crate::layout::edge::common::edge_geometry::point_at_path_t(&path, t)
                },
            );
        }
    }

    // ── Step C + D: resolve_label_overlaps（自动调用 assign_leader_lines）──
    crate::layout::edge::common::label_avoidance::resolve_label_overlaps(edges, nodes, groups);

    // ── Step E: trunk keepout 二次校验 ──
    enforce_trunk_keepouts(edges, bundling, config);
}

/// §4.10.4 SegmentAware: 将 label 锚定在独占段（非 Trunk 段）。
///
/// 对每个 label（middle / tail / head）：
/// 1. 找到候选独占段（role != Trunk 且 length ≥ min_label_segment_len）
/// 2. 按 DSL 偏好选择最佳段
/// 3. 在段中点放置 label
fn place_labels_segment_aware(
    rel: &crate::ast::Relation,
    path: &[Point],
    roles: &EdgePathRoles,
    config: &BundlingConfig,
) -> Vec<crate::layout::EdgeLabelLayout> {
    let mut labels = Vec::new();
    let min_seg_len = config.min_exclusive_segment_for_label;

    // 收集独占段（role != Trunk）
    let exclusive_spans: Vec<&SegmentSpan> = roles
        .spans
        .iter()
        .filter(|s| s.role != SegmentRole::Trunk && s.length >= min_seg_len)
        .collect();

    // 中段 label
    if let Some(text) = &rel.label {
        if let Some(t) = find_best_segment_t(&exclusive_spans, LabelKind::Middle, path) {
            let center = crate::layout::edge::common::edge_geometry::point_at_path_t(path, t);
            let angle = tangent_angle_at_t(path, t);
            let mut lbl = crate::layout::EdgeLabelLayout::new(text, center);
            lbl.rotation = angle;
            labels.push(lbl);
        } else {
            // 无可用独占段 → 回退到全路径中点
            let center = crate::layout::edge::common::edge_geometry::point_at_path_t(path, 0.5);
            let mut lbl = crate::layout::EdgeLabelLayout::new(text, center);
            lbl.rotation = tangent_angle_at_t(path, 0.5);
            labels.push(lbl);
        }
    }

    // 尾部 label（靠近 from 端）→ 优先 FromStub / MergeLeg
    if let Some(text) = &rel.tail_label {
        let t = find_best_segment_t(&exclusive_spans, LabelKind::Tail, path)
            .unwrap_or(0.15);
        let center = crate::layout::edge::common::edge_geometry::point_at_path_t(path, t);
        let angle = tangent_angle_at_t(path, t);
        let mut lbl = crate::layout::EdgeLabelLayout::new(text, center);
        lbl.rotation = angle;
        labels.push(lbl);
    }

    // 头部 label（靠近 to 端）→ 优先 ToStub / ForkLeg
    if let Some(text) = &rel.head_label {
        let t = find_best_segment_t(&exclusive_spans, LabelKind::Head, path)
            .unwrap_or(0.85);
        let center = crate::layout::edge::common::edge_geometry::point_at_path_t(path, t);
        let angle = tangent_angle_at_t(path, t);
        let mut lbl = crate::layout::EdgeLabelLayout::new(text, center);
        lbl.rotation = angle;
        labels.push(lbl);
    }

    labels
}

/// label 类型（用于段偏好选择）
#[derive(Clone, Copy, PartialEq, Eq)]
enum LabelKind {
    Middle,
    Tail,
    Head,
}

/// 为指定 label 类型找到最佳独占段的参数 t。
///
/// 返回段中点的 t 值。若无可用段返回 None。
fn find_best_segment_t(
    spans: &[&SegmentSpan],
    kind: LabelKind,
    _path: &[Point],
) -> Option<f64> {
    if spans.is_empty() {
        return None;
    }

    // 按 label 类型确定段角色优先级（§4.10.4 表）
    let preferred_roles: &[SegmentRole] = match kind {
        LabelKind::Tail => &[SegmentRole::FromStub, SegmentRole::MergeLeg],
        LabelKind::Head => &[SegmentRole::ToStub, SegmentRole::ForkLeg],
        LabelKind::Middle => &[SegmentRole::MergeLeg, SegmentRole::ForkLeg, SegmentRole::FromStub, SegmentRole::ToStub],
    };

    // 按优先级顺序查找第一个匹配段
    for &preferred in preferred_roles {
        // 在匹配段中找最长的（确定性：同长度取 role 枚举序小的）
        let best = spans
            .iter()
            .filter(|s| s.role == preferred)
            .max_by(|a, b| {
                a.length
                    .partial_cmp(&b.length)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some(span) = best {
            // 段中点的 t 值
            return Some((span.t_start + span.t_end) / 2.0);
        }
    }

    // 无偏好段匹配 → 取最长的独占段
    let best = spans.iter().max_by(|a, b| {
        a.length
            .partial_cmp(&b.length)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    best.map(|span| (span.t_start + span.t_end) / 2.0)
}

/// 计算路径在参数 t 处的切线角度（度）。
///
/// 通过数值微分求方向向量，再转为角度。
/// 返回值范围 (-180, 180]，0 = 水平向右。
fn tangent_angle_at_t(path: &[Point], t: f64) -> f64 {
    let t_lo = (t - TANGENT_DT).max(0.0);
    let t_hi = (t + TANGENT_DT).min(1.0);
    let p_lo = crate::layout::edge::common::edge_geometry::point_at_path_t(path, t_lo);
    let p_hi = crate::layout::edge::common::edge_geometry::point_at_path_t(path, t_hi);
    let dx = p_hi.x - p_lo.x;
    let dy = p_hi.y - p_lo.y;
    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        return 0.0;
    }
    dy.atan2(dx).to_degrees()
}

/// §4.10.5 Step E: trunk keepout 二次校验。
///
/// 检查每个 label 的 bbox 是否与任一 TrunkKeepout zone 相交，
/// 若相交则沿主干法向推开至条带外。
fn enforce_trunk_keepouts(
    edges: &mut [EdgeLayout],
    bundling: &BundlingResult,
    _config: &BundlingConfig,
) {
    if bundling.trunk_keepouts.is_empty() {
        return;
    }

    for (edge_idx, edge) in edges.iter_mut().enumerate() {
        // 仅检查中段 label（head/tail label 默认允许压在 stub 附近）
        let is_bundled = bundling.edge_to_bundle.get(edge_idx).and_then(|b| *b).is_some();
        if !is_bundled {
            continue;
        }

        for label_idx in 0..edge.labels.len() {
            let bbox = match edge.label_bbox_at(label_idx) {
                Some(b) => b,
                None => continue,
            };

            // 检查与所有 keepout zone 的相交
            for keepout in &bundling.trunk_keepouts {
                for &zone in &keepout.zones {
                    let (zx_min, zy_min, zx_max, zy_max) = zone;
                    let (lx_min, ly_min, lx_max, ly_max) = bbox;

                    // AABB 相交检测
                    if lx_max < zx_min || lx_min > zx_max || ly_max < zy_min || ly_min > zy_max {
                        continue; // 不相交
                    }

                    // 沿最短轴推开（优先上/右）
                    let push_up = zy_min - ly_max; // 向上推
                    let push_down = ly_min - zy_max; // 向下推
                    let push_left = lx_min - zx_max; // 向左推
                    let push_right = zx_min - lx_max; // 向右推

                    // 选最小位移（负值表示不可行方向）
                    let candidates = [
                        (push_up.abs(), 0.0, push_up),    // dy
                        (push_down.abs(), 0.0, -push_down),
                        (push_right.abs(), push_right, 0.0), // dx
                        (push_left.abs(), -push_left, 0.0),
                    ];

                    let best = candidates
                        .iter()
                        .filter(|(dist, _, _)| *dist > EPS)
                        .min_by(|a, b| {
                            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                        });

                    if let Some(&(_, dx, dy)) = best {
                        if let Some(pos) = edge.label_pos_at(label_idx) {
                            edge.set_label_pos_at(label_idx, Point::new(pos.x + dx, pos.y + dy));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Identifier, Relation, Span,
    };
    use crate::layout::{EdgeLayout, Port};

    fn pt(x: f64, y: f64) -> Point { Point::new(x, y) }

    fn make_relation_with_label(from: &str, to: &str, label: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: Some(label.to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn make_node_layout(cx: f64, cy: f64) -> NodeLayout {
        NodeLayout {
            x: cx - 40.0,
            y: cy - 20.0,
            width: 80.0,
            height: 40.0,
            ..Default::default()
        }
    }

    fn make_nodes(ids: &[&str], centers: &[(f64, f64)]) -> HashMap<String, NodeLayout> {
        let mut nodes = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            nodes.insert(id.to_string(), make_node_layout(centers[i].0, centers[i].1));
        }
        nodes
    }

    #[test]
    fn segment_aware_places_label_on_exclusive_segment() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (300.0, 0.0), (0.0, 50.0), (300.0, 50.0)]);
        let rels = vec![
            make_relation_with_label("A", "B", "请求"),
            make_relation_with_label("C", "D", "响应"),
        ];
        let paths: Vec<Vec<Point>> = vec![
            vec![pt(40.0, 0.0), pt(340.0, 0.0)],
            vec![pt(40.0, 50.0), pt(340.0, 50.0)],
        ];

        let mut edges: Vec<EdgeLayout> = paths
            .iter()
            .map(|p| {
                let mut e = EdgeLayout::empty();
                e.set_polyline_points(p.clone());
                e.from_port = Port::Right;
                e.to_port = Port::Left;
                e
            })
            .collect();

        let features: Vec<super::super::compatibility::EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| {
                super::super::compatibility::EdgeFeatures::extract(i, rel, &nodes, None, &paths[i])
                    .unwrap()
            })
            .collect();

        let config = BundlingConfig {
            enabled: true,
            min_ink_saving: 0.0,
            ..Default::default()
        };

        let (bundling_result, _) = super::super::path_rewrite::apply_bundling(
            &mut edges,
            &features,
            &nodes,
            &config,
        );

        let bundled_count = bundling_result.edge_to_bundle.iter().filter(|b| b.is_some()).count();
        assert!(bundled_count >= 2, "应至少有 2 条边被捆绑");

        let diagram = crate::ast::Diagram {
            diagram_type: crate::types::DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![],
            relations: rels,
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: crate::ast::SourceInfo::default(),
        };

        relayout_edge_labels_after_bundling(
            &diagram,
            &mut edges,
            &bundling_result,
            &config,
            &nodes,
            &HashMap::new(),
        );

        for (i, edge) in edges.iter().enumerate() {
            assert_eq!(edge.labels.len(), 1, "edge {} 应有 1 个 label", i);
            let trunk_y = bundling_result.bundles[0].trunk_start.y;
            let label_y = edge.labels[0].center.y;
            assert!(
                (label_y - trunk_y).abs() > 5.0,
                "edge {} label y={} 不应在 trunk y={} 上",
                i,
                label_y,
                trunk_y
            );
        }
    }
}
