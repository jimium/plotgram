//! V2 反馈模式：友好性驱动布局调整器
//!
//! 在 V1 诊断评估后、正式边路由前，对预测穿障热点做局部节点位移，
//! 减少直线穿障数，从而降低路由阶段的 `edge_node_crossings`。
//!
//! 设计要点（设计文档 §4.4.2）：
//! - **穿障预测驱动**：Phase 1.5 证明 `predicted_crossings` 是最强单维预测器
//!   （层次类 r=0.62），调整聚焦于此度量。
//! - **法线推送**：沿穿障边段的法线方向推开中间节点（复用 refine.rs 思路）。
//! - **回退机制**：每轮重新评估，若 `predicted_crossings` 未减少则回退（借鉴 refine.rs）。
//! - **momentum**：记录节点历史位移，抑制方向反复振荡（VLSI DAC 2025）。
//! - **重叠守卫**：调整后检测 `node_overlap_pairs`，若新增则回退整轮。
//! - **pinned 节点**：Layout Intent 集成后，pinned 节点不参与位移（当前无 pinned 概念，预留接口）。

use crate::ast::Diagram;
use crate::layout::geometry::{Point, Rect};
use crate::layout::LayoutResult;
use crate::layout::refine::{segment_intersects_aabb, segment_intersects_node};
use super::node_outside_segment_bbox;
use std::collections::HashMap;

/// 低于此预测穿障数时不调整：低穿障数通常被路由器绕行解决，
/// 强行调整反而可能引入新穿障（Phase 2 实测 0→14 enc 恶化案例）。
///
/// Phase 2 调参：从 3 降至 2。路由后验证（post_route_select）会回退无效调整，
/// 因此可以更激进地尝试低穿障场景。
const MIN_CROSSINGS_TO_ADJUST: usize = 2;

/// V2 调整器配置
#[derive(Debug, Clone, Copy)]
pub struct AdjusterConfig {
    /// 是否启用 V2 反馈调整
    pub enabled: bool,
    /// 最大调整轮次
    pub max_passes: usize,
    /// 每轮每个穿障节点的推开距离（像素）
    pub push_distance: f64,
    /// momentum 阻尼系数（0.0 = 无阻尼，1.0 = 完全抑制反向位移）
    pub momentum_damping: f64,
    /// 低于此预测穿障数时不调整（过滤路由器可自行绕行的低穿障场景）
    pub min_crossings_to_adjust: usize,
}

impl Default for AdjusterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_passes: 5,
            push_distance: 80.0,
            momentum_damping: 0.5,
            min_crossings_to_adjust: MIN_CROSSINGS_TO_ADJUST,
        }
    }
}

/// 节点位移历史（momentum 用）
///
/// 记录节点上一轮的位移向量，用于检测方向反转并施加阻尼。
type MomentumHistory = HashMap<String, (f64, f64)>;

/// 单条边的穿障信息
#[derive(Debug, Clone)]
pub(crate) struct EdgeCrossingDetail {
    /// 穿过的节点 ID 列表
    crossed_node_ids: Vec<String>,
    /// 边的起点（from 中心）
    p1: Point,
    /// 边的终点（to 中心）
    p2: Point,
}

/// V2 友好性调整器
#[derive(Debug, Clone)]
pub struct FriendlinessAdjuster {
    config: AdjusterConfig,
}

impl FriendlinessAdjuster {
    pub fn new(config: AdjusterConfig) -> Self {
        Self { config }
    }

    pub fn with_default() -> Self {
        Self::new(AdjusterConfig::default())
    }

    /// 对布局施加友好性驱动调整
    ///
    /// 流程：
    /// 1. 检测直线穿障（predicted crossings）
    /// 2. 对穿障节点沿法线推送
    /// 3. 重新评估；若穿障减少且无新重叠则接受，否则回退
    /// 4. 重复至 max_passes 或穿障为 0
    ///
    /// 阈值：predicted_crossings < MIN_CROSSINGS_TO_ADJUST 时不调整。
    /// 低穿障数（1-2）通常被路由器绕行解决，强行调整反而可能引入新穿障。
    pub fn apply(&self, diagram: &Diagram, mut result: LayoutResult) -> LayoutResult {
        if !self.config.enabled || self.config.max_passes == 0 {
            return result;
        }

        // 复用 compute_crossing_details 的结果推导 count，避免重复 O(|E|*|V|) 遍历
        let mut details = self.compute_crossing_details(diagram, &result);
        let mut best_crossings: usize = details.iter().map(|d| d.crossed_node_ids.len()).sum();
        if best_crossings < self.config.min_crossings_to_adjust {
            return result;
        }

        let mut best_result = result.clone();
        let mut momentum: MomentumHistory = HashMap::new();

        for _ in 0..self.config.max_passes {
            if details.is_empty() {
                break;
            }

            // 1. 推送穿障节点
            let mut new_result = result.clone();
            self.push_crossed_nodes(&mut new_result, &details, &mut momentum);

            // 2. 评估新穿障（复用 details 推导 count，避免额外 O(|E|*|V|) 扫描）
            let new_details = self.compute_crossing_details(diagram, &new_result);
            let new_crossings: usize = new_details.iter().map(|d| d.crossed_node_ids.len()).sum();

            // 3. 重叠守卫：若引入任何新重叠对则回退
            let old_pairs = overlap_pairs(&result.nodes);
            let new_pairs = overlap_pairs(&new_result.nodes);
            let has_new_overlap = new_pairs.iter().any(|p| !old_pairs.contains(p));

            if new_crossings < best_crossings && !has_new_overlap {
                // 改善且无新重叠：接受，details 复用于下一轮
                best_result = new_result.clone();
                best_crossings = new_crossings;
                result = new_result;
                details = new_details;
            } else {
                // 未改善或引入重叠：回退，停止迭代
                break;
            }
        }

        best_result
    }

    /// 计算每条边的穿障详情（哪些节点被穿过）
    pub(crate) fn compute_crossing_details(
        &self,
        diagram: &Diagram,
        result: &LayoutResult,
    ) -> Vec<EdgeCrossingDetail> {
        let mut details = Vec::new();

        for rel in &diagram.relations {
            let (Some(from), Some(to)) =
                (result.nodes.get(rel.from.as_str()), result.nodes.get(rel.to.as_str()))
            else {
                continue;
            };
            let p1 = Point::new(from.x + from.width / 2.0, from.y + from.height / 2.0);
            let p2 = Point::new(to.x + to.width / 2.0, to.y + to.height / 2.0);

            let mut crossed_node_ids = Vec::new();
            for (node_id, nl) in &result.nodes {
                if node_id == rel.from.as_str() || node_id == rel.to.as_str() {
                    continue;
                }
                // AABB 预过滤：快速跳过远距离节点
                if node_outside_segment_bbox(p1, p2, nl) {
                    continue;
                }
                if segment_intersects_aabb(p1, p2, Rect::from(nl)) {
                    crossed_node_ids.push(node_id.clone());
                }
            }

            if !crossed_node_ids.is_empty() {
                details.push(EdgeCrossingDetail {
                    crossed_node_ids,
                    p1,
                    p2,
                });
            }
        }

        details
    }

    /// 对穿障节点沿边法线方向推送
    ///
    /// 每个被穿障的节点累积所有穿障边的法线推力，归一化后乘以 push_distance。
    /// momentum：若当前推送方向与历史位移方向相反，按 damping 系数衰减。
    fn push_crossed_nodes(
        &self,
        result: &mut LayoutResult,
        details: &[EdgeCrossingDetail],
        momentum: &mut MomentumHistory,
    ) {
        // 累积每个节点的推送力
        let mut push_forces: HashMap<String, (f64, f64)> = HashMap::new();

        for detail in details {
            for node_id in &detail.crossed_node_ids {
                let Some(nl) = result.nodes.get(node_id.as_str()) else {
                    continue;
                };
                let force = compute_push_force(detail.p1, detail.p2, nl);
                let entry = push_forces.entry(node_id.clone()).or_default();
                entry.0 += force.0;
                entry.1 += force.1;
            }
        }

        // 应用推送（含 momentum 阻尼）
        for (node_id, (fx, fy)) in &push_forces {
            let len = (fx * fx + fy * fy).sqrt();
            if len < f64::EPSILON {
                continue;
            }
            let mut dx = fx / len * self.config.push_distance;
            let mut dy = fy / len * self.config.push_distance;

            // momentum：检测方向反转
            if let Some(&(prev_dx, prev_dy)) = momentum.get(node_id) {
                let dot = dx * prev_dx + dy * prev_dy;
                if dot < 0.0 {
                    // 方向反转：施加阻尼
                    let damping = 1.0 - self.config.momentum_damping;
                    dx *= damping;
                    dy *= damping;
                }
            }

            if let Some(nl) = result.nodes.get_mut(node_id.as_str()) {
                nl.x += dx;
                nl.y += dy;
                momentum.insert(node_id.clone(), (dx, dy));
            }
        }
    }
}

/// 计算节点相对边段的法线推送方向
///
/// 推送方向沿边段法线，远离边段（与 refine.rs accumulate_push 逻辑一致）。
fn compute_push_force(
    p1: Point,
    p2: Point,
    nl: &crate::layout::NodeLayout,
) -> (f64, f64) {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < f64::EPSILON {
        return (0.0, 0.0);
    }
    // 法线方向（两个候选：(-dy, dx) 和 (dy, -dx)）
    let nx = -dy / len;
    let ny = dx / len;
    // 节点中心相对 p1 的向量，判断在法线哪一侧
    let cx = nl.x + nl.width / 2.0;
    let cy = nl.y + nl.height / 2.0;
    let vx = cx - p1.x;
    let vy = cy - p1.y;
    let dot = vx * nx + vy * ny;
    let sign = if dot >= 0.0 { 1.0 } else { -1.0 };
    (sign * nx, sign * ny)
}

/// 计算节点重叠对集合（AABB 检测，含 0.5px 容差）
///
/// 返回所有重叠的节点 ID 对（已排序，(a,b) 中 a < b 按字典序）。
pub fn overlap_pairs(nodes: &HashMap<String, crate::layout::NodeLayout>) -> std::collections::HashSet<(String, String)> {
    let node_list: Vec<(&String, &crate::layout::NodeLayout)> = nodes.iter().collect();
    let mut pairs = std::collections::HashSet::new();
    for i in 0..node_list.len() {
        for j in (i + 1)..node_list.len() {
            let (id_a, a) = node_list[i];
            let (id_b, b) = node_list[j];
            let eps = 0.5;
            let overlaps = a.x + a.width > b.x + eps
                && b.x + b.width > a.x + eps
                && a.y + a.height > b.y + eps
                && b.y + b.height > a.y + eps;
            if overlaps {
                let first = std::cmp::min(id_a, id_b).clone();
                let second = std::cmp::max(id_a, id_b).clone();
                pairs.insert((first, second));
            }
        }
    }
    pairs
}

/// 计算实际边-节点穿障数（路由后，基于折线路径）
///
/// 与 drawify-eval::metrics::count_edge_node_crossings 算法一致：
/// 对每条边的每段折线，检测是否穿过非端点节点的矩形（含 0.5px 容差）。
///
/// 注意：使用 `segment_intersects_node`（与评估器一致），而非 `segment_intersects_aabb`
/// （slab method，无容差）。两者在边界情况下判定不同，必须与评估器使用相同标准。
pub fn count_actual_edge_node_crossings(diagram: &Diagram, result: &LayoutResult) -> usize {
    let mut count = 0;
    for (i, edge) in result.edges.iter().enumerate() {
        if edge.path_len() < 2 {
            continue;
        }
        let rel = &diagram.relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();

        let path = edge.path_points();
        for window in path.windows(2) {
            let a = window[0];
            let b = window[1];
            for (node_id, nl) in &result.nodes {
                if node_id == from_id || node_id == to_id {
                    continue;
                }
                if segment_intersects_node(a, b, nl) {
                    count += 1;
                }
            }
        }
    }
    count
}

/// 检测 V2 调整是否实际改变了节点位置
pub fn layout_changed(
    a: &HashMap<String, crate::layout::NodeLayout>,
    b: &HashMap<String, crate::layout::NodeLayout>,
) -> bool {
    if a.len() != b.len() {
        return true;
    }
    for (id, nl_a) in a {
        match b.get(id) {
            None => return true,
            Some(nl_b) => {
                if (nl_a.x - nl_b.x).abs() > 0.01 || (nl_a.y - nl_b.y).abs() > 0.01 {
                    return true;
                }
            }
        }
    }
    false
}

/// 路由后验证：比较 V2 调整结果与基线，选择更优的
///
/// 接受 V2 当且仅当：
/// - 实际 `edge_node_crossings` 严格减少，且
/// - 无新增 `node_overlap_pairs`（V2 的重叠对是基线的子集）
///
/// 否则回退到基线，确保 V2 永远不会让布局变差。
pub fn post_route_select(
    diagram: &Diagram,
    v2_result: LayoutResult,
    baseline_result: LayoutResult,
) -> LayoutResult {
    let enc_v2 = count_actual_edge_node_crossings(diagram, &v2_result);
    let enc_baseline = count_actual_edge_node_crossings(diagram, &baseline_result);

    let overlaps_v2 = overlap_pairs(&v2_result.nodes);
    let overlaps_baseline = overlap_pairs(&baseline_result.nodes);
    let has_new_overlap = overlaps_v2.iter().any(|p| !overlaps_baseline.contains(p));

    if enc_v2 < enc_baseline && !has_new_overlap {
        v2_result
    } else {
        baseline_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, Span};
    use crate::layout::{LayoutHints, NodeLayout};
    use crate::types::DiagramType;
    use std::collections::HashMap;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    /// 构造 a→b 直线穿过 c 的布局
    fn make_crossing_diagram() -> (Diagram, LayoutResult) {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: dummy_span(),
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: dummy_span(),
                },
                Entity {
                    id: Identifier::new_unchecked("c"),
                    label: "C".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: dummy_span(),
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: dummy_span(),
            }],
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 1,
            },
        };

        // a 在左，b 在右，c 在中间（a→b 直线穿过 c）
        let result = LayoutResult {
            nodes: HashMap::from([
                (
                    "a".to_string(),
                    NodeLayout {
                        x: 0.0,
                        y: 10.0,
                        width: 80.0,
                        height: 30.0,
                        ..Default::default()
                    },
                ),
                (
                    "b".to_string(),
                    NodeLayout {
                        x: 300.0,
                        y: 10.0,
                        width: 80.0,
                        height: 30.0,
                        ..Default::default()
                    },
                ),
                (
                    "c".to_string(),
                    NodeLayout {
                        x: 140.0,
                        y: 10.0,
                        width: 80.0,
                        height: 30.0,
                        ..Default::default()
                    },
                ),
            ]),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 400.0,
            total_height: 50.0,
            hints: LayoutHints::default(),
        };

        (diagram, result)
    }

    fn count_crossings_from_details(details: &[EdgeCrossingDetail]) -> usize {
        details.iter().map(|d| d.crossed_node_ids.len()).sum()
    }

    #[test]
    fn test_adjuster_reduces_predicted_crossings() {
        let (diagram, result) = make_crossing_diagram();
        let adjuster = FriendlinessAdjuster::new(AdjusterConfig {
            min_crossings_to_adjust: 1,
            ..AdjusterConfig::default()
        });

        let before = count_crossings_from_details(&adjuster.compute_crossing_details(&diagram, &result));
        assert_eq!(before, 1, "should detect 1 predicted crossing before adjust");

        let adjusted = adjuster.apply(&diagram, result);
        let after = count_crossings_from_details(&adjuster.compute_crossing_details(&diagram, &adjusted));
        assert_eq!(
            after, 0,
            "predicted crossing should be eliminated after adjust"
        );
    }

    #[test]
    fn test_adjuster_no_crossings_noop() {
        // 构造无穿障布局
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: dummy_span(),
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: dummy_span(),
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: dummy_span(),
            }],
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 1,
            },
        };
        let result = LayoutResult {
            nodes: HashMap::from([
                (
                    "a".to_string(),
                    NodeLayout {
                        x: 0.0,
                        y: 0.0,
                        width: 80.0,
                        height: 30.0,
                        ..Default::default()
                    },
                ),
                (
                    "b".to_string(),
                    NodeLayout {
                        x: 200.0,
                        y: 0.0,
                        width: 80.0,
                        height: 30.0,
                        ..Default::default()
                    },
                ),
            ]),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 300.0,
            total_height: 50.0,
            hints: LayoutHints::default(),
        };

        let adjuster = FriendlinessAdjuster::with_default();
        let adjusted = adjuster.apply(&diagram, result.clone());

        // 无穿障时应原样返回（节点位置不变）
        let a_before = result.nodes.get("a").unwrap();
        let a_after = adjusted.nodes.get("a").unwrap();
        assert_eq!(a_before.x, a_after.x);
        assert_eq!(a_before.y, a_after.y);
    }

    #[test]
    fn test_adjuster_disabled_noop() {
        let (diagram, result) = make_crossing_diagram();
        let adjuster = FriendlinessAdjuster::new(AdjusterConfig {
            enabled: false,
            ..AdjusterConfig::default()
        });

        let adjusted = adjuster.apply(&diagram, result.clone());
        let c_before = result.nodes.get("c").unwrap();
        let c_after = adjusted.nodes.get("c").unwrap();
        assert_eq!(c_before.x, c_after.x, "disabled adjuster should not move nodes");
    }

    #[test]
    fn test_adjuster_does_not_introduce_overlaps() {
        let (diagram, result) = make_crossing_diagram();
        let adjuster = FriendlinessAdjuster::new(AdjusterConfig {
            min_crossings_to_adjust: 1,
            ..AdjusterConfig::default()
        });

        let before_overlaps = overlap_pairs(&result.nodes);
        let adjusted = adjuster.apply(&diagram, result);
        let after_overlaps = overlap_pairs(&adjusted.nodes);

        assert!(
            after_overlaps.len() <= before_overlaps.len(),
            "adjuster should not introduce new overlaps: before={}, after={}",
            before_overlaps.len(),
            after_overlaps.len()
        );
    }
}
