//! X-3: Lane Assignment 车道分配后处理。
//!
//! 对 bundling 无法合并的残余平行段，通过平移整段 cross-axis 坐标分配车道偏移，
//! 分离 anti-parallel 重合段。不插入 Z 字弯，保持正交性。
//!
//! 设计原理：
//! - 正交路径中相邻段交替 H/V。平移段 si 的 cross-axis 坐标（H 段移 y，V 段移 x）时，
//!   被移段仍为 H/V，相邻段仅长度变化，自动保持正交性。
//! - 与 nudge 的区别：nudge 修改 main-axis 需 Z 字弯补偿；lane shift 修改 cross-axis，
//!   相邻段自动适配。
//! - 仅偏移 interior 段（1 ≤ si ≤ n_segs-2），保护端口锚点。
//!
//! 确定性（AGENTS.md §2）：所有分组/排序使用 BTreeMap + 显式 sort key。

use super::*;
use crate::ast::Relation;
use crate::layout::edge::common::edge_geometry::{build_edge_labels, parse_label_t, point_at_path_t};
use crate::layout::geometry::{Point, Rect};
use crate::layout::{EdgeLayout, NodeLayout, PathGeometry, Port};
use std::collections::{BTreeMap, HashMap};

/// X-3: lane assignment 统计结果
#[derive(Default, Debug, Clone, Copy)]
pub struct LaneAssignmentStats {
    /// 检测到的车道组数（冲突段连通分量，≥2 成员）
    pub lane_groups: usize,
    /// 成功偏移的段数
    pub segments_shifted: usize,
    /// 偏移失败的段数（验证不通过）
    pub shifts_failed: usize,
}

/// 邻段最小长度——偏移后邻段不得退化至此值以下
const MIN_ADJACENT_LEN: f64 = 4.0;

/// 可偏移段的元信息
struct SegmentInfo {
    ei: usize,
    si: usize,
    is_horizontal: bool,
    /// H 段 = y, V 段 = x
    layer: f64,
    /// 方向：H 段 dx>0 为 positive, V 段 dy>0 为 positive
    is_positive: bool,
    p1: Point,
    p2: Point,
}

/// Union-Find（并查集）用于冲突段分组
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut cur = x;
        while self.parent[cur] != root {
            let next = self.parent[cur];
            self.parent[cur] = root;
            cur = next;
        }
        root
    }

    fn union(&mut self, x: usize, y: usize) {
        let (rx, ry) = (self.find(x), self.find(y));
        if rx == ry {
            return;
        }
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }
    }
}

/// 检查线段是否穿越任何节点（用于偏移验证，参考 nudge.rs:30 模式）
fn segment_hits_node(
    a: Point,
    b: Point,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
) -> bool {
    let seg_xmin = a.x.min(b.x) - NODE_OBSTACLE_PAD;
    let seg_xmax = a.x.max(b.x) + NODE_OBSTACLE_PAD;
    let seg_ymin = a.y.min(b.y) - NODE_OBSTACLE_PAD;
    let seg_ymax = a.y.max(b.y) + NODE_OBSTACLE_PAD;
    for node_id in sorted_node_ids {
        if let Some(nl) = nodes.get(node_id.as_str()) {
            if nl.x + nl.width < seg_xmin
                || nl.x > seg_xmax
                || nl.y + nl.height < seg_ymin
                || nl.y > seg_ymax
            {
                continue;
            }
            if Rect::from(nl)
                .expanded(NODE_OBSTACLE_PAD)
                .segment_crosses_interior(a, b, EPS)
            {
                return true;
            }
        }
    }
    false
}

/// 返回段的主导方向符号（+1 / -1 / 0），用于反转检测。
/// H 段返回 sign(dx)，V 段返回 sign(dy)。
fn direction_sign(a: Point, b: Point) -> i32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    if dx.abs() > dy.abs() {
        dx.signum() as i32
    } else {
        dy.signum() as i32
    }
}

/// 验证偏移后的段及其邻段是否合法。
///
/// 检查三项：
/// a. 邻段不反转：si-1/si+1 方向符号不反号
/// b. 邻段不退化：长度 ≥ MIN_ADJACENT_LEN
/// c. 无节点穿透：si-1/si/si+1 均不穿节点
fn validate_shift(
    original: &[Point],
    new: &[Point],
    si: usize,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
) -> bool {
    if si == 0 || si + 1 >= new.len() {
        return false;
    }

    // 检查邻段 si-1（偏移前 vs 偏移后方向不反号 + 不退化 + 不穿节点）
    if si >= 1 {
        let orig_dir = direction_sign(original[si - 1], original[si]);
        let new_dir = direction_sign(new[si - 1], new[si]);
        if orig_dir != 0 && new_dir != 0 && orig_dir != new_dir {
            return false;
        }
        let adj_len = new[si - 1].distance_to(new[si]);
        if adj_len < MIN_ADJACENT_LEN {
            return false;
        }
        if segment_hits_node(new[si - 1], new[si], nodes, sorted_node_ids) {
            return false;
        }
    }

    // 检查被偏移段 si 本身不穿节点
    if segment_hits_node(new[si], new[si + 1], nodes, sorted_node_ids) {
        return false;
    }

    // 检查邻段 si+1
    if si + 2 < new.len() {
        let orig_dir = direction_sign(original[si + 1], original[si + 2]);
        let new_dir = direction_sign(new[si + 1], new[si + 2]);
        if orig_dir != 0 && new_dir != 0 && orig_dir != new_dir {
            return false;
        }
        let adj_len = new[si + 1].distance_to(new[si + 2]);
        if adj_len < MIN_ADJACENT_LEN {
            return false;
        }
        if segment_hits_node(new[si + 1], new[si + 2], nodes, sorted_node_ids) {
            return false;
        }
    }

    true
}

/// 重建边路径与标签（参考 nudge.rs:292 apply_nudge 模式）
fn commit_shifted_path(
    edges: &mut [EdgeLayout],
    ei: usize,
    new_points: &[Point],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
) {
    let labels = if new_points.len() >= 2 {
        match relations.get(ei) {
            Some(rel) => {
                let middle_t = parse_label_t(rel);
                build_edge_labels(rel, middle_t, Point::new(0.0, 0.0), |t| {
                    point_at_path_t(new_points, t)
                })
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let mut edge = EdgeLayout {
        geometry: PathGeometry::Polyline { points: Vec::new() },
        labels,
        from_port: from_side[ei],
        to_port: to_side[ei],
    };
    edge.set_polyline_points(new_points.to_vec());
    edges[ei] = edge;
}

/// X-3: 主入口——车道分配，分离残余平行重合段。
///
/// 算法步骤：
/// 1. 收集所有 interior 段（1 ≤ si ≤ n_segs-2）
/// 2. O(N²) 检测冲突对，Union-Find 分组
/// 3. 每组按 (is_positive, ei, si) 排序，分配对称偏移
/// 4. 逐段应用偏移 + 验证（反转/退化/穿透）
/// 5. 重建 SegmentGrid
pub fn assign_lanes(
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    min_gap: f64,
) -> LaneAssignmentStats {
    let mut stats = LaneAssignmentStats::default();
    let n = edges.len();
    if n < 2 {
        return stats;
    }

    // ── Step 1: 收集可偏移段 ──
    let mut segments: Vec<SegmentInfo> = Vec::new();
    for ei in 0..n {
        if edges[ei].path_is_empty() {
            continue;
        }
        let points: Vec<Point> = edges[ei].path_points().into_owned();
        if points.len() < 4 {
            continue;
        }
        let n_segs = points.len() - 1;
        for si in 1..n_segs.saturating_sub(1) {
            let p1 = points[si];
            let p2 = points[si + 1];
            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;
            let length = (dx * dx + dy * dy).sqrt();
            if length < EPS {
                continue;
            }
            let is_horizontal = dy.abs() < EPS;
            let (layer, is_positive) = if is_horizontal {
                (p1.y, dx > 0.0)
            } else {
                (p1.x, dy > 0.0)
            };
            segments.push(SegmentInfo {
                ei,
                si,
                is_horizontal,
                layer,
                is_positive,
                p1,
                p2,
            });
        }
    }

    if segments.is_empty() {
        return stats;
    }

    // ── Step 2: 检测冲突对 + Union-Find 分组 ──
    let mut uf = UnionFind::new(segments.len());
    for i in 0..segments.len() {
        for j in (i + 1)..segments.len() {
            let a = &segments[i];
            let b = &segments[j];
            if a.is_horizontal != b.is_horizontal {
                continue;
            }
            if (a.layer - b.layer).abs() >= min_gap {
                continue;
            }
            let seg_a = RoutedSegment {
                x1: a.p1.x,
                y1: a.p1.y,
                x2: a.p2.x,
                y2: a.p2.y,
                edge_index: a.ei,
            };
            let seg_b = RoutedSegment {
                x1: b.p1.x,
                y1: b.p1.y,
                x2: b.p2.x,
                y2: b.p2.y,
                edge_index: b.ei,
            };
            if segments_violate_spacing(&seg_a, &seg_b, min_gap).is_some() {
                uf.union(i, j);
            }
        }
    }

    // ── Step 3: 按组分配偏移 ──
    // 使用 BTreeMap 保证组遍历顺序确定（AGENTS.md §2）
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..segments.len() {
        let root = uf.find(i);
        groups.entry(root).or_default().push(i);
    }

    let multi_groups: Vec<Vec<usize>> = groups.into_values().filter(|g| g.len() >= 2).collect();
    stats.lane_groups = multi_groups.len();
    if multi_groups.is_empty() {
        return stats;
    }

    let mut shifted_edges: Vec<usize> = Vec::new();

    for group in &multi_groups {
        // 按 (is_positive, ei, si) 排序——Negative 在前，Positive 在后
        let mut sorted_group: Vec<usize> = group.clone();
        sorted_group.sort_by(|&a, &b| {
            let sa = &segments[a];
            let sb = &segments[b];
            sa.is_positive
                .cmp(&sb.is_positive)
                .then(sa.ei.cmp(&sb.ei))
                .then(sa.si.cmp(&sb.si))
        });

        let n_group = sorted_group.len();

        for (pos, &seg_idx) in sorted_group.iter().enumerate() {
            let seg = &segments[seg_idx];
            let offset = (pos as f64 - (n_group as f64 - 1.0) / 2.0) * min_gap;
            if offset.abs() < EPS {
                continue;
            }

            if edges[seg.ei].path_is_empty() {
                stats.shifts_failed += 1;
                continue;
            }
            let original: Vec<Point> = edges[seg.ei].path_points().into_owned();
            if seg.si == 0 || seg.si + 1 >= original.len() {
                stats.shifts_failed += 1;
                continue;
            }

            let mut new_points = original.clone();
            if seg.is_horizontal {
                new_points[seg.si].y += offset;
                new_points[seg.si + 1].y += offset;
            } else {
                new_points[seg.si].x += offset;
                new_points[seg.si + 1].x += offset;
            }

            if validate_shift(&original, &new_points, seg.si, nodes, sorted_node_ids) {
                commit_shifted_path(
                    edges,
                    seg.ei,
                    &new_points,
                    relations,
                    from_side,
                    to_side,
                );
                shifted_edges.push(seg.ei);
                stats.segments_shifted += 1;
            } else {
                stats.shifts_failed += 1;
            }
        }
    }

    // ── Step 5: 重建 SegmentGrid ──
    if !shifted_edges.is_empty() {
        shifted_edges.sort();
        shifted_edges.dedup();
        grid.remove_by_edges(&shifted_edges);
        for &ei in &shifted_edges {
            let points = edges[ei].path_points();
            grid.insert_path(&points, ei);
        }
    }

    stats
}

/// Architecture A7：对通用路由回退的走廊边，按 plan 的 cross-axis offset 平移干线 interior 段。
pub fn apply_corridor_planned_offsets(
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    corridor_plan: &super::corridor_route::CorridorRoutePlan,
    group_ctx: &crate::layout::group::GroupRoutingContext,
) -> usize {
    use crate::layout::group::CorridorAxis;

    let mut shifted = 0usize;
    let mut shifted_edges = Vec::new();
    let n = edges.len();

    for ei in 0..n {
        let Some((axis, offset)) =
            super::corridor_route::planned_cross_axis_offset_for_edge(ei, corridor_plan, group_ctx)
        else {
            continue;
        };
        if edges[ei].path_is_empty() {
            continue;
        }
        let original: Vec<Point> = edges[ei].path_points().into_owned();
        if original.len() < 4 {
            continue;
        }

        let n_segs = original.len() - 1;
        let mut best_si = None;
        let mut best_len = 0.0f64;
        for si in 1..n_segs.saturating_sub(1) {
            let p1 = original[si];
            let p2 = original[si + 1];
            let dx = (p2.x - p1.x).abs();
            let dy = (p2.y - p1.y).abs();
            let len = dx.max(dy);
            let is_target = match axis {
                CorridorAxis::Horizontal => dx < EPS && dy >= MIN_SHARED_TRUNK_LEN,
                CorridorAxis::Vertical => dy < EPS && dx >= MIN_SHARED_TRUNK_LEN,
            };
            if is_target && len > best_len {
                best_len = len;
                best_si = Some(si);
            }
        }
        let Some(si) = best_si else { continue };

        let mut new_points = original.clone();
        match axis {
            CorridorAxis::Horizontal => {
                new_points[si].x += offset;
                new_points[si + 1].x += offset;
            }
            CorridorAxis::Vertical => {
                new_points[si].y += offset;
                new_points[si + 1].y += offset;
            }
        }

        if validate_shift(&original, &new_points, si, nodes, sorted_node_ids) {
            commit_shifted_path(edges, ei, &new_points, relations, from_side, to_side);
            shifted_edges.push(ei);
            shifted += 1;
        }
    }

    if !shifted_edges.is_empty() {
        shifted_edges.sort_unstable();
        shifted_edges.dedup();
        grid.remove_by_edges(&shifted_edges);
        for &ei in &shifted_edges {
            grid.insert_path(&edges[ei].path_points(), ei);
        }
    }

    shifted
}

/// Architecture：对 `edges_may_share_trunk == false` 的边对，强制分离仍重合的干线 interior 段。
pub fn separate_unrelated_trunk_overlaps(
    edges: &mut [EdgeLayout],
    grid: Option<&mut SegmentGrid>,
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
    min_gap: f64,
    profile: &super::OrthoRoutingProfile,
) -> usize {
    use crate::layout::edge::edge_merge_policy::{edge_merge_context, edges_may_share_trunk};

    let n = edges.len();
    if n < 2 {
        return 0;
    }

    let mut separated = 0usize;
    let mut shifted_edges = Vec::new();

    for i in 0..n {
        for j in (i + 1)..n {
            let Some(rel_i) = relations.get(i) else { continue };
            let Some(rel_j) = relations.get(j) else { continue };
            let ctx_i = edge_merge_context(rel_i.from.as_str(), rel_i.to.as_str(), i);
            let ctx_j = edge_merge_context(rel_j.from.as_str(), rel_j.to.as_str(), j);
            if edges_may_share_trunk(&ctx_i, &ctx_j, profile.merge_policy_diagram_type()) {
                continue;
            }
            if edges[i].path_is_empty() || edges[j].path_is_empty() {
                continue;
            }
            let path_i: Vec<Point> = edges[i].path_points().into_owned();
            let path_j: Vec<Point> = edges[j].path_points().into_owned();
            if path_i.len() < 4 || path_j.len() < 4 {
                continue;
            }

            let sep = try_separate_edge_pair(
                &path_i,
                edges,
                j,
                min_gap,
                nodes,
                sorted_node_ids,
                relations,
                from_side,
                to_side,
            ) || try_separate_edge_pair(
                &path_j,
                edges,
                i,
                min_gap,
                nodes,
                sorted_node_ids,
                relations,
                from_side,
                to_side,
            );
            if sep {
                shifted_edges.push(j);
                shifted_edges.push(i);
                separated += 1;
            }
        }
    }

    if !shifted_edges.is_empty() {
        shifted_edges.sort_unstable();
        shifted_edges.dedup();
        if let Some(grid) = grid {
            grid.remove_by_edges(&shifted_edges);
            for &ei in &shifted_edges {
                grid.insert_path(&edges[ei].path_points(), ei);
            }
        }
    }

    separated
}

fn try_separate_edge_pair(
    reference_path: &[Point],
    edges: &mut [EdgeLayout],
    target_ei: usize,
    min_gap: f64,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
    relations: &[Relation],
    from_side: &[Port],
    to_side: &[Port],
) -> bool {
    let original: Vec<Point> = edges[target_ei].path_points().into_owned();
    if original.len() < 4 {
        return false;
    }

    let n_segs = original.len() - 1;
    for si in 1..n_segs.saturating_sub(1) {
        let t1 = original[si];
        let t2 = original[si + 1];
        let tdx = (t2.x - t1.x).abs();
        let tdy = (t2.y - t1.y).abs();
        let is_vertical = tdx < EPS && tdy >= MIN_SHARED_TRUNK_LEN;
        let is_horizontal = tdy < EPS && tdx >= MIN_SHARED_TRUNK_LEN;
        if !is_vertical && !is_horizontal {
            continue;
        }

        for ri in 0..reference_path.len().saturating_sub(1) {
            let r1 = reference_path[ri];
            let r2 = reference_path[ri + 1];
            let rdx = (r2.x - r1.x).abs();
            let rdy = (r2.y - r1.y).abs();
            let shares = if is_vertical {
                rdx < EPS
                    && tdx < EPS
                    && (t1.x - r1.x).abs() < EPS
                    && t1.y.max(t2.y).min(r1.y.max(r2.y)) - t1.y.min(t2.y).max(r1.y.min(r2.y))
                        >= MIN_SHARED_TRUNK_LEN - EPS
            } else {
                rdy < EPS
                    && tdy < EPS
                    && (t1.y - r1.y).abs() < EPS
                    && t1.x.max(t2.x).min(r1.x.max(r2.x)) - t1.x.min(t2.x).max(r1.x.min(r2.x))
                        >= MIN_SHARED_TRUNK_LEN - EPS
            };
            if !shares {
                continue;
            }

            for magnitude in [
                min_gap,
                min_gap + 6.0,
                18.0,
                24.0,
                36.0,
                48.0,
            ] {
                for sign in [1.0, -1.0] {
                    let mut new_points = original.clone();
                    let offset = magnitude * sign;
                    if is_vertical {
                        new_points[si].x += offset;
                        new_points[si + 1].x += offset;
                    } else {
                        new_points[si].y += offset;
                        new_points[si + 1].y += offset;
                    }
                    if validate_shift(&original, &new_points, si, nodes, sorted_node_ids) {
                        commit_shifted_path(
                            edges,
                            target_ei,
                            &new_points,
                            relations,
                            from_side,
                            to_side,
                        );
                        return true;
                    }
                }
            }

            let shared_coord = if is_vertical { t1.x } else { t1.y };
            for magnitude in [min_gap, min_gap + 6.0, 18.0, 24.0, 36.0, 48.0] {
                for sign in [1.0, -1.0] {
                    let mut new_points = original.clone();
                    let target = shared_coord + magnitude * sign;
                    if force_shift_trunk_coord(&mut new_points, is_vertical, shared_coord, target) {
                        commit_shifted_path(
                            edges,
                            target_ei,
                            &new_points,
                            relations,
                            from_side,
                            to_side,
                        );
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// 将路径上所有位于 `from_coord` 的干线点强制移至 `to_coord`（保持正交折线）。
fn force_shift_trunk_coord(
    path: &mut [Point],
    is_vertical: bool,
    from_coord: f64,
    to_coord: f64,
) -> bool {
    if (from_coord - to_coord).abs() < EPS || path.len() < 2 {
        return false;
    }
    let mut moved = false;
    for p in path.iter_mut() {
        if is_vertical {
            if (p.x - from_coord).abs() < EPS {
                p.x = to_coord;
                moved = true;
            }
        } else if (p.y - from_coord).abs() < EPS {
            p.y = to_coord;
            moved = true;
        }
    }
    moved
}

const MIN_SHARED_TRUNK_LEN: f64 = 12.0;

// ═══════════════════════════════════════════════════════════
//  单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{GroupLayout, PathGeometry};

    fn mk_edge(points: &[Point]) -> EdgeLayout {
        let mut e = EdgeLayout {
            geometry: PathGeometry::Polyline { points: Vec::new() },
            labels: vec![],
            from_port: Port::Bottom,
            to_port: Port::Top,
        };
        e.set_polyline_points(points.to_vec());
        e
    }

    fn empty_nodes() -> HashMap<String, NodeLayout> {
        HashMap::new()
    }

    fn empty_relations() -> Vec<Relation> {
        Vec::new()
    }

    fn empty_sides(n: usize) -> (Vec<Port>, Vec<Port>) {
        (vec![Port::Bottom; n], vec![Port::Top; n])
    }

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn test_two_anti_parallel_v_segments_separated() {
        // 两条 V 段同 x=200，方向相反（anti-parallel），投影重叠 → 应被分离
        let p0 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let p1 = vec![pt(300.0, 300.0), pt(200.0, 300.0), pt(200.0, 100.0), pt(100.0, 100.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);
        let (from_side, to_side) = empty_sides(2);

        let stats = assign_lanes(
            &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert!(stats.lane_groups >= 1, "应检测到至少 1 个车道组");
        assert!(stats.segments_shifted >= 2, "应偏移至少 2 段，got {}", stats.segments_shifted);

        // 验证两条 V 段的 x 坐标已分离 ≥ 8px
        let pts0: Vec<Point> = edges[0].path_points().into_owned();
        let pts1: Vec<Point> = edges[1].path_points().into_owned();
        let x0 = pts0[1].x;
        let x1 = pts1[1].x;
        let gap = (x0 - x1).abs();
        assert!(gap >= 8.0 - EPS, "V 段 x 坐标应分离 ≥ 8px，实际 gap={}", gap);
    }

    #[test]
    fn test_two_same_direction_v_segments_separated() {
        // 两条同向 V 段同 x=200，投影重叠 → 应被分离
        let p0 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let p1 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(350.0, 300.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);
        let (from_side, to_side) = empty_sides(2);

        let stats = assign_lanes(
            &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert!(stats.segments_shifted >= 2, "同向 V 段也应被分离");
    }

    #[test]
    fn test_three_segments_centered() {
        // 三条同向 V 段同 x=200 → 中间不动，两侧 ±8
        let p0 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let p1 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let p2 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1), mk_edge(&p2)];
        let mut grid = SegmentGrid::new();
        for (i, p) in edges.iter().enumerate() {
            grid.insert_path(&p.path_points(), i);
        }
        let (from_side, to_side) = empty_sides(3);

        let stats = assign_lanes(
            &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert!(stats.segments_shifted >= 2, "三段中至少两侧被偏移");
        // 中间段 offset=0 被跳过，所以 segments_shifted 应为 2
        assert_eq!(stats.segments_shifted, 2, "中间段不应偏移");
    }

    #[test]
    fn test_stub_segment_not_shifted() {
        // 2 点路径（1 段）无 interior 段，不应偏移
        let p0 = vec![pt(100.0, 100.0), pt(300.0, 100.0)];
        let p1 = vec![pt(100.0, 100.0), pt(300.0, 100.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);
        let (from_side, to_side) = empty_sides(2);

        let stats = assign_lanes(
            &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert_eq!(stats.lane_groups, 0, "无 interior 段不应检测到车道组");
        assert_eq!(stats.segments_shifted, 0, "不应偏移任何段");
    }

    #[test]
    fn test_segment_adjacent_to_anchor_not_shifted() {
        // 3 点路径（2 段）：si=0 和 si=1 都是 stub/端点段，无 interior
        let p0 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0)];
        let p1 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);
        let (from_side, to_side) = empty_sides(2);

        let stats = assign_lanes(
            &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert_eq!(stats.lane_groups, 0, "3 点路径无 interior 段");
    }

    #[test]
    fn test_shift_rejected_for_node_penetration() {
        // V 段偏移后会穿过一个节点 → 应被拒绝
        let p0 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let p1 = vec![pt(300.0, 300.0), pt(200.0, 300.0), pt(200.0, 100.0), pt(100.0, 100.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);

        // 在 x=208, y=150..250 放一个节点，偏移 +8 后 V 段穿过它
        let mut nodes = HashMap::new();
        nodes.insert(
            "obstacle".to_string(),
            NodeLayout {
                x: 204.0,
                y: 150.0,
                width: 20.0,
                height: 100.0,
            },
        );
        let sorted_node_ids: Vec<String> = vec!["obstacle".to_string()];
        let (from_side, to_side) = empty_sides(2);

        let stats = assign_lanes(
            &mut edges, &mut grid, &nodes, &sorted_node_ids, &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        // 至少有一些偏移失败（因为穿节点）
        assert!(stats.shifts_failed >= 1, "穿节点的偏移应被拒绝");
    }

    #[test]
    fn test_adjacent_segment_reversal_rejected() {
        // 构造一个场景：偏移后邻段会反转
        // p0: (100,100) → (200,100) → (200,300) → (300,300)
        // 如果 V 段 si=1 向右偏移 8px，则 H 段 si=0 变为 (100,100)→(208,100)，方向不变
        // 但如果 H 段 si=0 原本很短（如 4px），偏移后可能反转
        let p0 = vec![pt(196.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let p1 = vec![pt(300.0, 300.0), pt(200.0, 300.0), pt(200.0, 100.0), pt(196.0, 100.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);
        let (from_side, to_side) = empty_sides(2);

        let stats = assign_lanes(
            &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        // 由于 H 段 si=0 长度仅 4px，偏移 V 段后 H 段会反转或退化
        // 至少应有部分失败
        assert!(
            stats.shifts_failed >= 1 || stats.segments_shifted == 0,
            "邻段反转或退化时应被拒绝 (shifted={}, failed={})",
            stats.segments_shifted, stats.shifts_failed
        );
    }

    #[test]
    fn test_no_conflict_no_shift() {
        // 两条 V 段 x 坐标相差 > 8px，无冲突 → 不偏移
        let p0 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let p1 = vec![pt(100.0, 100.0), pt(250.0, 100.0), pt(250.0, 300.0), pt(300.0, 300.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);
        let (from_side, to_side) = empty_sides(2);

        let stats = assign_lanes(
            &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert_eq!(stats.lane_groups, 0, "无冲突不应检测到车道组");
        assert_eq!(stats.segments_shifted, 0, "无冲突不应偏移");
    }

    #[test]
    fn test_grid_updated_after_shift() {
        // 偏移后 grid 应反映新路径
        let p0 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
        let p1 = vec![pt(300.0, 300.0), pt(200.0, 300.0), pt(200.0, 100.0), pt(100.0, 100.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);
        let (from_side, to_side) = empty_sides(2);

        let stats = assign_lanes(
            &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        if stats.segments_shifted > 0 {
            // grid 中的段应与 edges 的当前路径一致
            let pts0: Vec<Point> = edges[0].path_points().into_owned();
            // 查询 grid 中 edge 0 的段
            let seg = RoutedSegment {
                x1: pts0[1].x,
                y1: pts0[1].y,
                x2: pts0[2].x,
                y2: pts0[2].y,
                edge_index: 0,
            };
            let nearby = grid.query_overlapping(&seg, 4.0);
            // 应该能查到自己
            assert!(
                nearby.iter().any(|s| s.edge_index == 0),
                "grid 应包含偏移后的新路径"
            );
        }
    }

    #[test]
    fn test_deterministic_output() {
        // 同一输入两次运行，输出应完全一致
        let mk = || {
            let p0 = vec![pt(100.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(300.0, 300.0)];
            let p1 = vec![pt(300.0, 300.0), pt(200.0, 300.0), pt(200.0, 100.0), pt(100.0, 100.0)];
            let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
            let mut grid = SegmentGrid::new();
            grid.insert_path(&p0, 0);
            grid.insert_path(&p1, 1);
            let (from_side, to_side) = empty_sides(2);
            (edges, grid, from_side, to_side)
        };

        let (mut edges_a, mut grid_a, fs_a, ts_a) = mk();
        let (mut edges_b, mut grid_b, fs_b, ts_b) = mk();

        let stats_a = assign_lanes(
            &mut edges_a, &mut grid_a, &empty_nodes(), &[], &empty_relations(),
            &fs_a, &ts_a, 8.0,
        );
        let stats_b = assign_lanes(
            &mut edges_b, &mut grid_b, &empty_nodes(), &[], &empty_relations(),
            &fs_b, &ts_b, 8.0,
        );

        assert_eq!(stats_a.lane_groups, stats_b.lane_groups);
        assert_eq!(stats_a.segments_shifted, stats_b.segments_shifted);
        assert_eq!(stats_a.shifts_failed, stats_b.shifts_failed);

        let pa: Vec<Point> = edges_a[0].path_points().into_owned();
        let pb: Vec<Point> = edges_b[0].path_points().into_owned();
        assert_eq!(pa.len(), pb.len());
        for (a, b) in pa.iter().zip(pb.iter()) {
            assert!((a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS, "路径不一致");
        }
    }
}
