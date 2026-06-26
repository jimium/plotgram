//! Path scoring and obstacle avoidance for orthogonal edge routing

use super::*;
use crate::layout::geometry::{Point, Rect};
use crate::layout::group::{
    corridor_misalignment_penalty, segment_near_misses_group_shell, GroupRoutingContext,
    GROUP_BORDER_SHELL_PAD, PORT_STUB_CLEARANCE,
};
use crate::layout::{GroupLayout, NodeLayout};
use std::collections::HashMap;

/// 节点障碍物膨胀间距（边路由时节点障碍物膨胀的固定间距）。
pub const NODE_OBSTACLE_PAD: f64 = 18.0;
/// P1-3 / Border Shell：分组边框壳层厚度。
pub const GROUP_OBSTACLE_PAD: f64 = GROUP_BORDER_SHELL_PAD;
/// 近距擦过检测的额外余量（在节点 margin 基础上额外增加的距离）
const NODE_NEAR_MISS_EXTRA: f64 = 10.0;
const NODE_NEAR_MISS_PENALTY: f64 = 2_500.0;
/// 分组边框近距擦过惩罚
const GROUP_NEAR_MISS_PENALTY: f64 = 2_000.0;
/// 分组边框近距擦过检测额外余量
const GROUP_NEAR_MISS_EXTRA: f64 = 8.0;
/// 分组穿越（Transit/Interior/Crossing）软惩罚。
/// 低于 NODE_CROSSING_PENALTY（穿节点更严重），高于 GROUP_NEAR_MISS_PENALTY（穿越比擦过更严重）。
/// 用于 scorer 软约束，不作为硬过滤——当分组铺满画布时允许穿越以避免穿节点。
const GROUP_TRANSIT_PENALTY: f64 = 3_000.0;

/// P2-1: edge-overlap bbox 预筛选扩张量（含 EDGE_PARALLEL_GAP + 余量）。
/// 用于 `edge_overlap_penalty` 中快速跳过 bbox 不相交的已路由段。
const BBOX_EXPAND: f64 = 10.0;

/// Scores a candidate path in the context of a routing request.
///
/// Extracting the scoring policy behind a trait lets future optimizations
/// (A* / multi-objective / learned weights) swap the scorer without touching
/// `select_best_path`'s structure.
pub trait CandidateScorer {
    fn score(&self, path: &[Point], ctx: &RoutingContext, pair: &EndpointPair) -> f64;
}

/// Default scorer: the original 4-term weighted sum
/// (length + bend penalty + obstacle penalty + edge-overlap penalty).
pub struct DefaultScorer;

impl CandidateScorer for DefaultScorer {
    fn score(&self, path: &[Point], ctx: &RoutingContext, pair: &EndpointPair) -> f64 {
        let mut score = path_length(path);
        score += path.len().saturating_sub(2) as f64 * BEND_PENALTY;
        score += obstacle_penalty(
            path,
            pair.from_id(),
            pair.to_id(),
            ctx.nodes,
            ctx.group_ctx,
        );
        score += edge_overlap_penalty(
            path,
            ctx.routed_segments,
        );
        if !ctx.group_ctx.corridors.is_empty() {
            score += corridor_misalignment_penalty(
                path,
                &ctx.group_ctx.corridors,
                ctx.group_ctx.corridor_misalignment_penalty,
            );
        }
        score
    }
}

pub fn path_length(path: &[Point]) -> f64 {
    path.windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            let dy = w[1].y - w[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

pub fn obstacle_penalty(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &GroupRoutingContext,
) -> f64 {
    let mut penalty = 0.0;
    let last_segment_index = path.len().saturating_sub(2);
    let endpoint_groups = group_ctx.endpoint_group_set(from_id, to_id);

    // 预排序 node_ids（确定性，AGENTS.md §2）
    let mut node_ids: Vec<&String> = nodes.keys().collect();
    node_ids.sort();
    let mut group_ids: Vec<&String> = group_ctx.groups.keys().collect();
    group_ids.sort();

    for (segment_index, window) in path.windows(2).enumerate() {
        let a = window[0];
        let b = window[1];

        // 段 bbox（含 near-miss 余量），用于预筛选
        let node_pad_ext = NODE_OBSTACLE_PAD + NODE_NEAR_MISS_EXTRA;
        let seg_nx_min = a.x.min(b.x) - node_pad_ext;
        let seg_nx_max = a.x.max(b.x) + node_pad_ext;
        let seg_ny_min = a.y.min(b.y) - node_pad_ext;
        let seg_ny_max = a.y.max(b.y) + node_pad_ext;

        for nid in &node_ids {
            let is_source_allowed = nid.as_str() == from_id && segment_index == 0;
            let is_target_allowed = nid.as_str() == to_id && segment_index == last_segment_index;
            if is_source_allowed || is_target_allowed {
                continue;
            }
            let Some(nl) = nodes.get(*nid) else { continue };
            // bbox 预筛选——跳过远离段的节点
            if nl.x + nl.width < seg_nx_min || nl.x > seg_nx_max
                || nl.y + nl.height < seg_ny_min || nl.y > seg_ny_max {
                continue;
            }
            let pad = NODE_OBSTACLE_PAD;
            if segment_intersects_node(a, b, nl, pad) {
                penalty += NODE_CROSSING_PENALTY;
            } else if segment_near_misses_node(a, b, nl, pad) {
                penalty += NODE_NEAR_MISS_PENALTY;
            }
        }

        // 段 bbox（含 group near-miss 余量）
        let grp_pad_ext = group_ctx.border_shell_pad + GROUP_NEAR_MISS_EXTRA;
        let seg_gx_min = a.x.min(b.x) - grp_pad_ext;
        let seg_gx_max = a.x.max(b.x) + grp_pad_ext;
        let seg_gy_min = a.y.min(b.y) - grp_pad_ext;
        let seg_gy_max = a.y.max(b.y) + grp_pad_ext;

        for gid in &group_ids {
            let Some(gl) = group_ctx.groups.get(*gid) else {
                continue;
            };
            // bbox 预筛选——跳过远离段的分组
            if gl.x + gl.width < seg_gx_min || gl.x > seg_gx_max
                || gl.y + gl.height < seg_gy_min || gl.y > seg_gy_max {
                continue;
            }
            let endpoint_in_group = endpoint_groups.contains(gid.as_str());
            if group_ctx.segment_violates_border_shell(path, segment_index, gl, endpoint_in_group)
            {
                penalty += GROUP_TRANSIT_PENALTY;
            } else if segment_near_misses_group_shell(
                a,
                b,
                gl,
                group_ctx.border_shell_pad,
                GROUP_NEAR_MISS_EXTRA,
            ) {
                penalty += GROUP_NEAR_MISS_PENALTY;
            }
        }
    }
    penalty
}

/// 硬过滤（节点级）：检查路径是否穿过任何**非端点**节点（含 `NODE_OBSTACLE_PAD` 膨胀）。
///
/// 确定性：节点按 id 排序后检查（AGENTS.md §2）。
/// 返回 `true` 表示路径"干净"（不穿节点），可保留；`false` 表示应丢弃。
///
/// 端点节点（from/to）不是障碍物——路径自然从端点出发/到达，stub 在膨胀范围内
/// 是正常的。端点附近的"回路穿越"由 scorer 的 `obstacle_penalty` 软约束处理。
///
/// 分组内部穿越由 `path_avoids_group_interiors` 额外检查。本函数只检查节点穿越，
/// 作为分组铺满画布时的退化后备——优先使用 `path_is_clean` + `path_avoids_group_interiors`，
/// 仅当无严格干净候选时才接受只避开节点的候选。
pub fn path_is_clean(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
    _group_ctx: &GroupRoutingContext,
) -> bool {
    if path.len() < 2 {
        return true;
    }
    let mut node_ids: Vec<&String> = nodes.keys().collect();
    node_ids.sort();

    for window in path.windows(2) {
        let a = window[0];
        let b = window[1];
        // 段 bbox 预筛选——跳过明显不相交的节点
        let seg_xmin = a.x.min(b.x) - NODE_OBSTACLE_PAD;
        let seg_xmax = a.x.max(b.x) + NODE_OBSTACLE_PAD;
        let seg_ymin = a.y.min(b.y) - NODE_OBSTACLE_PAD;
        let seg_ymax = a.y.max(b.y) + NODE_OBSTACLE_PAD;
        for node_id in &node_ids {
            let nid = node_id.as_str();
            if nid == from_id || nid == to_id {
                continue;
            }
            if let Some(nl) = nodes.get(nid) {
                // bbox 预筛选
                if nl.x + nl.width < seg_xmin || nl.x > seg_xmax
                    || nl.y + nl.height < seg_ymin || nl.y > seg_ymax {
                    continue;
                }
                if segment_intersects_node(a, b, nl, NODE_OBSTACLE_PAD) {
                    return false;
                }
            }
        }
    }
    true
}

/// 检查路径是否穿越非端点分组内部（只检查分组，不检查节点）。
///
/// 调用方应先调用 `path_is_clean` 检查节点，再调用本函数检查分组。
/// 使用段 bbox 预筛选跳过明显不相交的分组，减少 `segment_crosses_rect_interior` 调用。
pub fn path_avoids_group_interiors(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    group_ctx: &GroupRoutingContext,
) -> bool {
    if path.len() < 2 {
        return true;
    }
    let endpoint_groups = group_ctx.endpoint_group_set(from_id, to_id);
    let mut group_ids: Vec<&String> = group_ctx.groups.keys().collect();
    group_ids.sort();

    for window in path.windows(2) {
        let a = window[0];
        let b = window[1];
        // 段 bbox
        let seg_xmin = a.x.min(b.x);
        let seg_xmax = a.x.max(b.x);
        let seg_ymin = a.y.min(b.y);
        let seg_ymax = a.y.max(b.y);
        for gid in &group_ids {
            if endpoint_groups.contains(gid.as_str()) {
                continue;
            }
            let Some(gl) = group_ctx.groups.get(*gid) else {
                continue;
            };
            if gl.width <= 0.0 || gl.height <= 0.0 {
                continue;
            }
            // bbox 预筛选——段 bbox 与分组 bbox 不相交则跳过
            if gl.x + gl.width < seg_xmin || gl.x > seg_xmax
                || gl.y + gl.height < seg_ymin || gl.y > seg_ymax {
                continue;
            }
            if segment_crosses_rect_interior(a, b, gl) {
                return false;
            }
        }
    }
    true
}

/// 严格硬过滤（节点 + 分组内部）：`path_is_clean` + `path_avoids_group_interiors`。
///
/// 用于需要同时检查节点和分组的场景。在候选选择中，建议分别调用两个函数
/// 以实现三阶段选择（strict → nodes_only → dirty），避免对 nodes_only 候选重复检查节点。
pub fn path_is_clean_strict(
    path: &[Point],
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &GroupRoutingContext,
) -> bool {
    path_is_clean(path, from_id, to_id, nodes, group_ctx)
        && path_avoids_group_interiors(path, from_id, to_id, group_ctx)
}

/// 检查线段是否穿越矩形的严格内部（不含边界）。
fn segment_crosses_rect_interior(
    a: Point,
    b: Point,
    gl: &GroupLayout,
) -> bool {
    Rect::from(gl).segment_crosses_interior(a, b, EPS)
}

/// 计算边段重叠惩罚。
///
/// P2-1: bbox 预筛选——对每个段先做 bbox 扩张检查，跳过明显远离的已路由段，
/// 减少不必要的 `segments_conflict` 调用。
pub fn edge_overlap_penalty(
    path: &[Point],
    routed_segments: &[RoutedSegment],
) -> f64 {
    let mut penalty = 0.0;
    for window in path.windows(2) {
        let seg = RoutedSegment {
            x1: window[0].x,
            y1: window[0].y,
            x2: window[1].x,
            y2: window[1].y,
            edge_index: usize::MAX,
        };
        // P2-1: 当前段的 bbox（扩张 BBOX_EXPAND）
        let seg_xmin = seg.x1.min(seg.x2) - BBOX_EXPAND;
        let seg_xmax = seg.x1.max(seg.x2) + BBOX_EXPAND;
        let seg_ymin = seg.y1.min(seg.y2) - BBOX_EXPAND;
        let seg_ymax = seg.y1.max(seg.y2) + BBOX_EXPAND;
        for existing in routed_segments {
            // P2-1: bbox 预筛选——跳过 bbox 不相交的段
            let ex_xmin = existing.x1.min(existing.x2);
            let ex_xmax = existing.x1.max(existing.x2);
            let ex_ymin = existing.y1.min(existing.y2);
            let ex_ymax = existing.y1.max(existing.y2);
            if ex_xmax < seg_xmin || ex_xmin > seg_xmax || ex_ymax < seg_ymin || ex_ymin > seg_ymax {
                continue;
            }
            if segments_conflict(&seg, existing) {
                penalty += EDGE_OVERLAP_PENALTY;
            }
        }
    }
    penalty
}

fn segments_conflict(a: &RoutedSegment, b: &RoutedSegment) -> bool {
    if a.edge_index == b.edge_index {
        return false;
    }

    let a_horiz = (a.y1 - a.y2).abs() < EPS;
    let b_horiz = (b.y1 - b.y2).abs() < EPS;
    let a_vert = (a.x1 - a.x2).abs() < EPS;
    let b_vert = (b.x1 - b.x2).abs() < EPS;

    if a_horiz && b_horiz {
        let gap = (a.y1 - b.y1).abs();
        if gap > EDGE_PARALLEL_GAP {
            return false;
        }
        let a_min = a.x1.min(a.x2);
        let a_max = a.x1.max(a.x2);
        let b_min = b.x1.min(b.x2);
        let b_max = b.x1.max(b.x2);
        return a_max > b_min + EPS && b_max > a_min + EPS;
    }

    if a_vert && b_vert {
        let gap = (a.x1 - b.x1).abs();
        if gap > EDGE_PARALLEL_GAP {
            return false;
        }
        let a_min = a.y1.min(a.y2);
        let a_max = a.y1.max(a.y2);
        let b_min = b.y1.min(b.y2);
        let b_max = b.y1.max(b.y2);
        return a_max > b_min + EPS && b_max > a_min + EPS;
    }

    // P0-2: 检测垂直交叉（水平段与垂直段相交，修复 G2 检测缺失）
    if a_horiz && b_vert {
        return segments_cross_perpendicular(a, b);
    }
    if a_vert && b_horiz {
        return segments_cross_perpendicular(b, a);
    }

    false
}

/// 检测水平段 `h` 与垂直段 `v` 是否严格内部相交（不含端点接触）。
///
/// 端点接触（T-junction / L-junction）不算交叉——两条边在端点处汇合是合法的。
fn segments_cross_perpendicular(h: &RoutedSegment, v: &RoutedSegment) -> bool {
    let h_y = h.y1; // = h.y2
    let v_x = v.x1; // = v.x2
    let h_x_min = h.x1.min(h.x2);
    let h_x_max = h.x1.max(h.x2);
    let v_y_min = v.y1.min(v.y2);
    let v_y_max = v.y1.max(v.y2);
    // 严格内部相交：交点在两段的内部（非端点）
    v_x > h_x_min + EPS
        && v_x < h_x_max - EPS
        && h_y > v_y_min + EPS
        && h_y < v_y_max - EPS
}

fn segment_intersects_node(a: Point, b: Point, nl: &NodeLayout, pad: f64) -> bool {
    Rect::from(nl).expanded(pad).segment_crosses_interior(a, b, EPS)
}

/// 水平线段从节点正下方/正上方近距离擦过（视觉上的「穿节点」）
///
/// `pad` 为节点的 margin（障碍物膨胀间距），近距擦过检测在 margin 之外
/// 额外 `NODE_NEAR_MISS_EXTRA` 像素的范围内触发。
fn segment_near_misses_node(a: Point, b: Point, nl: &NodeLayout, pad: f64) -> bool {
    if (a.y - b.y).abs() >= EPS {
        return false;
    }
    let y = a.y;
    let min_x = a.x.min(b.x);
    let max_x = a.x.max(b.x);
    let left = nl.x - EPS;
    let right = nl.x + nl.width + EPS;
    if max_x <= left || min_x >= right {
        return false;
    }

    let bottom = nl.y + nl.height;
    let top = nl.y;
    let near_miss_pad = pad + NODE_NEAR_MISS_EXTRA;
    let below = y >= bottom && y <= bottom + near_miss_pad;
    let above = y <= top && y >= top - near_miss_pad;
    below || above
}

// ═══════════════════════════════════════════════════════════
//  P0-2: 边-边交叉检测测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::group::GroupRoutingContext;
    use crate::layout::GroupLayout;

    fn test_group_ctx(
        groups: HashMap<String, GroupLayout>,
        node_to_groups: HashMap<String, Vec<String>>,
    ) -> GroupRoutingContext {
        GroupRoutingContext {
            groups,
            node_to_groups,
            border_shell_pad: GROUP_OBSTACLE_PAD,
            stub_clearance: PORT_STUB_CLEARANCE,
            corridor_misalignment_penalty: 120.0,
            repulse_max_rounds: 2,
            corridors: vec![],
        }
    }

    fn seg(x1: f64, y1: f64, x2: f64, y2: f64, edge_index: usize) -> RoutedSegment {
        RoutedSegment {
            x1,
            y1,
            x2,
            y2,
            edge_index,
        }
    }

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn test_perpendicular_cross_detected() {
        // 水平段 (100,200)→(300,200) 与垂直段 (200,100)→(200,300) 十字交叉
        let h = seg(100.0, 200.0, 300.0, 200.0, 0);
        let v = seg(200.0, 100.0, 200.0, 300.0, 1);
        assert!(
            segments_conflict(&h, &v),
            "水平段与垂直段十字交叉应被检测到"
        );
        assert!(
            segments_conflict(&v, &h),
            "垂直段与水平段十字交叉应被检测到（对称）"
        );
    }

    #[test]
    fn test_perpendicular_no_cross_when_disjoint() {
        // 水平段 (100,200)→(300,200) 与垂直段 (400,100)→(400,300) 不相交
        let h = seg(100.0, 200.0, 300.0, 200.0, 0);
        let v = seg(400.0, 100.0, 400.0, 300.0, 1);
        assert!(
            !segments_conflict(&h, &v),
            "不相交的水平段与垂直段不应被判定为冲突"
        );
    }

    #[test]
    fn test_perpendicular_no_cross_at_endpoint_t_junction() {
        // T-junction：水平段端点 (200,200) 在垂直段内部 → 不算交叉（合法汇合）
        let h = seg(200.0, 200.0, 300.0, 200.0, 0);
        let v = seg(200.0, 100.0, 200.0, 300.0, 1);
        assert!(
            !segments_conflict(&h, &v),
            "T-junction（端点接触）不应被判定为交叉"
        );
    }

    #[test]
    fn test_perpendicular_no_cross_at_l_junction() {
        // L-junction：两段共享端点 (200,200) → 不算交叉
        let h = seg(100.0, 200.0, 200.0, 200.0, 0);
        let v = seg(200.0, 200.0, 200.0, 300.0, 1);
        assert!(
            !segments_conflict(&h, &v),
            "L-junction（共享端点）不应被判定为交叉"
        );
    }

    #[test]
    fn test_same_edge_no_conflict() {
        // 同一条边的段不应互相冲突
        let h = seg(100.0, 200.0, 300.0, 200.0, 0);
        let v = seg(200.0, 100.0, 200.0, 300.0, 0);
        assert!(
            !segments_conflict(&h, &v),
            "同一条边的段不应被判定为冲突"
        );
    }

    #[test]
    fn test_parallel_overlap_still_detected() {
        // 回归保护：平行重叠仍被检测
        let a = seg(100.0, 200.0, 300.0, 200.0, 0);
        let b = seg(150.0, 202.0, 280.0, 202.0, 1);
        assert!(
            segments_conflict(&a, &b),
            "平行重叠应被检测到（回归保护）"
        );
    }

    #[test]
    fn test_edge_overlap_penalty_includes_crossings() {
        // 端到端测试：路径包含与已路由段交叉的段时，penalty 应非零
        let path = vec![pt(100.0, 200.0), pt(300.0, 200.0)];
        let routed = vec![seg(200.0, 100.0, 200.0, 300.0, 1)];
        let penalty = edge_overlap_penalty(&path, &routed);
        assert!(
            penalty > 0.0,
            "包含交叉段的路径应有非零 overlap penalty"
        );
    }

    // ── P1-3: 分组边框障碍测试 ──

    #[test]
    fn test_p1_3_group_border_is_obstacle_for_external_edge() {
        // P1-3: 两端点都不在分组内时，分组边框是硬障碍。
        // 分组 G1 的包围框 (200,100)→(360,300)，水平线段 y=200 穿过分组。
        let mut groups: HashMap<String, GroupLayout> = HashMap::new();
        groups.insert(
            "g1".to_string(),
            GroupLayout {
                x: 200.0,
                y: 100.0,
                width: 160.0,
                height: 200.0,
            },
        );
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let node_to_groups: HashMap<String, Vec<String>> = HashMap::new();
        let group_ctx = test_group_ctx(groups, node_to_groups);

        // 水平线段从 (100, 200) 到 (500, 200)，穿过分组 G1
        let path = vec![pt(100.0, 200.0), pt(500.0, 200.0)];
        // 分组穿越改为软惩罚后，path_is_clean 不再硬性拒绝分组穿越
        assert!(
            path_is_clean(&path, "a", "c", &nodes, &group_ctx),
            "分组穿越不再由 path_is_clean 硬过滤拦截，改由 obstacle_penalty 软惩罚"
        );
        // 但 obstacle_penalty 仍应惩罚分组穿越
        let penalty = obstacle_penalty(&path, "a", "c", &nodes, &group_ctx);
        assert!(
            penalty >= GROUP_TRANSIT_PENALTY,
            "穿过分组应触发 GROUP_TRANSIT_PENALTY 软惩罚"
        );
    }

    #[test]
    fn test_p1_3_group_border_not_obstacle_for_endpoint_inside() {
        // P1-3: 端点在分组内时，分组边框不是障碍（边自然出入分组边界）。
        // 节点 a 在分组 G1 内，边 a→c 从 G1 内部出发到外部。
        let mut groups: HashMap<String, GroupLayout> = HashMap::new();
        groups.insert(
            "g1".to_string(),
            GroupLayout {
                x: 200.0,
                y: 100.0,
                width: 160.0,
                height: 200.0,
            },
        );
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let mut node_to_groups: HashMap<String, Vec<String>> = HashMap::new();
        node_to_groups.insert("a".to_string(), vec!["g1".to_string()]);
        let group_ctx = test_group_ctx(groups, node_to_groups);

        // 水平线段从 (250, 200)（G1 内部）到 (500, 200)（G1 外部），穿过 G1 右边界
        let path = vec![pt(250.0, 200.0), pt(500.0, 200.0)];
        assert!(
            path_is_clean(&path, "a", "c", &nodes, &group_ctx),
            "边 a→c 的 a 在 G1 内，路径穿过 G1 边界应被允许"
        );
    }

    #[test]
    fn test_p1_3_group_obstacle_penalty_added() {
        // P1-3: 穿过分组的路径应获得 obstacle_penalty 惩罚。
        let mut groups: HashMap<String, GroupLayout> = HashMap::new();
        groups.insert(
            "g1".to_string(),
            GroupLayout {
                x: 200.0,
                y: 100.0,
                width: 160.0,
                height: 200.0,
            },
        );
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let node_to_groups: HashMap<String, Vec<String>> = HashMap::new();
        let group_ctx = test_group_ctx(groups, node_to_groups);

        let through_path = vec![pt(100.0, 200.0), pt(500.0, 200.0)]; // 穿过 G1
        let around_path = vec![pt(100.0, 50.0), pt(500.0, 50.0)]; // 在 G1 上方
        let penalty_through = obstacle_penalty(&through_path, "a", "c", &nodes, &group_ctx);
        let penalty_around = obstacle_penalty(&around_path, "a", "c", &nodes, &group_ctx);
        assert!(
            penalty_through > penalty_around,
            "穿过分组的路径惩罚 ({}) 应大于绕行路径 ({})",
            penalty_through,
            penalty_around
        );
        assert!(
            penalty_through >= GROUP_TRANSIT_PENALTY,
            "穿过分组应触发 GROUP_TRANSIT_PENALTY"
        );
        assert_eq!(
            penalty_around, 0.0,
            "绕行路径不应有分组惩罚"
        );
    }

    // ── Border Shell Phase A: 贴边平行检测 ──

    #[test]
    fn test_border_shell_parallel_exterior_forbidden_even_with_endpoint_in_group() {
        let mut groups: HashMap<String, GroupLayout> = HashMap::new();
        groups.insert(
            "g1".to_string(),
            GroupLayout {
                x: 200.0,
                y: 100.0,
                width: 160.0,
                height: 200.0,
            },
        );
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let mut node_to_groups: HashMap<String, Vec<String>> = HashMap::new();
        node_to_groups.insert("a".to_string(), vec!["g1".to_string()]);
        node_to_groups.insert("b".to_string(), vec!["g1".to_string()]);
        let group_ctx = test_group_ctx(groups, node_to_groups);

        // 沿 G1 左边界外侧竖直行走（x=189，左边框 x=200，间距 11px < pad）
        let path = vec![pt(189.0, 80.0), pt(189.0, 320.0)];
        // 分组贴边改为软惩罚后，path_is_clean 不再硬性拒绝
        assert!(
            path_is_clean(&path, "a", "b", &nodes, &group_ctx),
            "分组贴边不再由 path_is_clean 硬过滤拦截，改由 obstacle_penalty 软惩罚"
        );
    }

    #[test]
    fn test_border_shell_parallel_interior_forbidden() {
        let mut groups: HashMap<String, GroupLayout> = HashMap::new();
        groups.insert(
            "g1".to_string(),
            GroupLayout {
                x: 200.0,
                y: 100.0,
                width: 160.0,
                height: 200.0,
            },
        );
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let mut node_to_groups: HashMap<String, Vec<String>> = HashMap::new();
        node_to_groups.insert("a".to_string(), vec!["g1".to_string()]);
        let group_ctx = test_group_ctx(groups, node_to_groups);

        // 沿 G1 左边界内侧竖直行走（x=208，左边框 x=200，间距 8px < pad）
        let path = vec![pt(208.0, 120.0), pt(208.0, 280.0)];
        // 分组贴边改为软惩罚后，path_is_clean 不再硬性拒绝
        assert!(
            path_is_clean(&path, "a", "c", &nodes, &group_ctx),
            "组内贴边平行不再由 path_is_clean 硬过滤拦截，改由 obstacle_penalty 软惩罚"
        );
    }

    #[test]
    fn test_border_shell_crossing_exit_still_allowed() {
        let mut groups: HashMap<String, GroupLayout> = HashMap::new();
        groups.insert(
            "g1".to_string(),
            GroupLayout {
                x: 200.0,
                y: 100.0,
                width: 160.0,
                height: 200.0,
            },
        );
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let mut node_to_groups: HashMap<String, Vec<String>> = HashMap::new();
        node_to_groups.insert("a".to_string(), vec!["g1".to_string()]);
        let group_ctx = test_group_ctx(groups, node_to_groups);

        let path = vec![pt(250.0, 200.0), pt(500.0, 200.0)];
        assert!(
            path_is_clean(&path, "a", "c", &nodes, &group_ctx),
            "合法穿出分组边界仍应允许"
        );
    }

    #[test]
    fn test_border_shell_stub_segment_exempt_from_hug_detection() {
        let gl = GroupLayout {
            x: 200.0,
            y: 100.0,
            width: 160.0,
            height: 200.0,
        };
        let path = vec![pt(208.0, 200.0), pt(208.0, 184.0), pt(300.0, 184.0)];
        let ctx = test_group_ctx(HashMap::new(), HashMap::new());
        assert!(
            !ctx.segment_violates_border_shell(&path, 0, &gl, true),
            "stub 区内段应豁免贴边检测"
        );
    }

    #[test]
    fn test_border_shell_stub_exemption_limited_to_port_clearance() {
        let gl = GroupLayout {
            x: 200.0,
            y: 100.0,
            width: 160.0,
            height: 200.0,
        };
        let path = vec![
            pt(208.0, 200.0),
            pt(208.0, 184.0),
            pt(208.0, 168.0),
            pt(208.0, 152.0),
            pt(300.0, 152.0),
        ];
        let ctx = test_group_ctx(HashMap::new(), HashMap::new());
        assert!(
            ctx.segment_violates_border_shell(&path, 2, &gl, true),
            "超出 PORT_CLEARANCE 的贴边段不应豁免"
        );
    }
}
