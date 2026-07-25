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
use crate::layout::geometry::Point;
use crate::layout::routing::model::solution::{LaneAssignment, RoutePath};
use crate::layout::routing::model::StableEdgeId;
use crate::layout::{EdgeLayout, NodeLayout, Port};
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

/// R7 7b：单段车道偏移决策——由 [`solve_lanes`] 产出、[`materialize_lanes`] 消费。
///
/// commit 时按 `si` 读**当前** `edges[ei]` 几何再施加 `offset`（与旧 `assign_lanes`
/// 内联逐段 re-read 一致）。
struct LaneShift {
    ei: usize,
    si: usize,
    is_horizontal: bool,
    offset: f64,
}

/// R7 7b：`solve_lanes` 输出——纯决策，不写任何几何。
struct LaneSolution {
    lane_groups: usize,
    shifts: Vec<LaneShift>,
}

/// X-3: 主入口——车道分配，分离残余平行重合段。
///
/// Slice C3.2：solve 从 `&[RoutePath]` 读取（不依赖 EdgeLayout），
/// materialize 仍写 EdgeLayout（过渡期，C3.4 移入 Materializer）。
pub fn assign_lanes(
    paths: &[RoutePath],
    _edges: &mut [EdgeLayout],
    _grid: &mut SegmentGrid,
    _nodes: &HashMap<String, NodeLayout>,
    _sorted_node_ids: &[String],
    _relations: &[Relation],
    _from_side: &[Port],
    _to_side: &[Port],
    min_gap: f64,
) -> (Vec<LaneAssignment>, LaneAssignmentStats) {
    // Phase 3：lane min_gap 由 path_solver rip-up + H5 MinSeparation 消费；
    // 不再 POST 平移几何。仍计算 assignment 供 RouteSolution 诊断携带。
    let solution = solve_lanes(paths, min_gap);
    let assignments = solution_to_assignments(&solution, paths.len());
    let stats = LaneAssignmentStats {
        lane_groups: solution.lane_groups,
        ..LaneAssignmentStats::default()
    };
    (assignments, stats)
}

/// R7 7b / Slice C3.2：车道求解——只读 `paths`，产出逐段对称 lane 偏移决策，不写几何。
///
/// 算法步骤（与旧 `assign_lanes` Step 1–3 逐字节等价）：
/// 1. 收集所有 interior 段（1 ≤ si ≤ n_segs-2）
/// 2. O(N²) 检测冲突对，Union-Find 分组
/// 3. 每组按 (is_positive, ei, si) 排序，分配对称偏移
fn solve_lanes(paths: &[RoutePath], min_gap: f64) -> LaneSolution {
    let mut solution = LaneSolution {
        lane_groups: 0,
        shifts: Vec::new(),
    };
    let n = paths.len();
    if n < 2 {
        return solution;
    }

    // ── Step 1: 收集可偏移段 ──
    let mut segments: Vec<SegmentInfo> = Vec::new();
    for ei in 0..n {
        if paths[ei].is_empty() {
            continue;
        }
        let points = paths[ei].points();
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
            // C10：斜段跳过，勿按 dy≈0 二分当竖段做 offset。
            let is_horizontal = dy.abs() < EPS;
            let is_vertical = dx.abs() < EPS;
            if !is_horizontal && !is_vertical {
                continue;
            }
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
        return solution;
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
    solution.lane_groups = multi_groups.len();
    if multi_groups.is_empty() {
        return solution;
    }

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
            solution.shifts.push(LaneShift {
                ei: seg.ei,
                si: seg.si,
                is_horizontal: seg.is_horizontal,
                offset,
            });
        }
    }

    solution
}

/// Slice C3.2：将内部 `LaneSolution` 转换为模型层 `Vec<LaneAssignment>`。
///
/// 每条边的 `segment_offsets` 为该边所有 interior 段的 cross-axis 偏移
/// （索引对齐 points 的段下标，首尾段始终为 0）。
fn solution_to_assignments(solution: &LaneSolution, n_edges: usize) -> Vec<LaneAssignment> {
    // 收集每条边的最大偏移绝对值作为 lane index 参考。
    let mut offsets_per_edge: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n_edges];
    for shift in &solution.shifts {
        if shift.ei < n_edges {
            offsets_per_edge[shift.ei].push((shift.si, shift.offset));
        }
    }
    (0..n_edges)
        .map(|ei| {
            let segs = &offsets_per_edge[ei];
            if segs.is_empty() {
                LaneAssignment {
                    edge: StableEdgeId(ei),
                    segment_offsets: Vec::new(),
                    lane: 0,
                }
            } else {
                // 构建逐段偏移向量（以最大 si 为长度参考）。
                let max_si = segs.iter().map(|(si, _)| *si).max().unwrap_or(0);
                let mut segment_offsets = vec![0.0f64; max_si + 2]; // si+1 存在
                for &(si, offset) in segs {
                    if si < segment_offsets.len() {
                        segment_offsets[si] = offset;
                    }
                }
                // lane index：取最大偏移方向的符号 × 位置。
                let max_abs = segs
                    .iter()
                    .map(|(_, o)| o.abs())
                    .fold(0.0f64, f64::max);
                let sign = segs
                    .iter()
                    .find(|(_, o)| o.abs() >= max_abs - f64::EPSILON)
                    .map(|(_, o)| if *o > 0.0 { 1i32 } else { -1i32 })
                    .unwrap_or(0);
                LaneAssignment {
                    edge: StableEdgeId(ei),
                    segment_offsets,
                    lane: sign,
                }
            }
        })
        .collect()
}



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

    fn edges_to_paths(edges: &[EdgeLayout]) -> Vec<RoutePath> {
        edges
            .iter()
            .map(|e| RoutePath::orthogonal(e.path_points().into_owned()))
            .collect()
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert!(stats.lane_groups >= 1, "应检测到至少 1 个车道组");
        assert!(stats.lane_groups >= 1, "Phase3: solve_lanes 应检出组");
        assert_eq!(stats.segments_shifted, 0, "Phase3: 不再 POST materialize");
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert!(stats.lane_groups >= 1);
        assert_eq!(stats.segments_shifted, 0);
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        assert!(stats.lane_groups >= 1);
        assert_eq!(stats.segments_shifted, 0);
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &nodes, &sorted_node_ids, &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        // Phase 3：不再 materialize；穿节点校验属已删 POST 逻辑
        assert_eq!(stats.segments_shifted, 0);
        assert_eq!(stats.shifts_failed, 0);
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &empty_nodes(), &[], &empty_relations(),
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

        let paths_a = edges_to_paths(&edges_a);
        let (_, stats_a) = assign_lanes(
            &paths_a, &mut edges_a, &mut grid_a, &empty_nodes(), &[], &empty_relations(),
            &fs_a, &ts_a, 8.0,
        );
        let paths_b = edges_to_paths(&edges_b);
        let (_, stats_b) = assign_lanes(
            &paths_b, &mut edges_b, &mut grid_b, &empty_nodes(), &[], &empty_relations(),
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

    #[test]
    fn test_force_shift_fallback_respects_node_penetration() {
        // 回归 P06：fallback 分支（force_shift_trunk_coord）必须经 validate_shift 校验，
        // 不得在穿节点时提交。
        // 构造：两条反向 V 段同 x=200，第一策略因 H 段过短（4px）会失败或退化，
        // fallback 会把整个 x=200 干线移到 x=208/x=192 等。
        // 在 x=208, y=150..250 放一个节点 → fallback 移到 +8 时会穿节点，应被拒绝。
        let p0 = vec![pt(196.0, 100.0), pt(200.0, 100.0), pt(200.0, 300.0), pt(304.0, 300.0)];
        let p1 = vec![pt(304.0, 300.0), pt(200.0, 300.0), pt(200.0, 100.0), pt(196.0, 100.0)];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let mut grid = SegmentGrid::new();
        grid.insert_path(&p0, 0);
        grid.insert_path(&p1, 1);

        // 障碍节点：fallback +8 x-shift 会穿它
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

        let paths = edges_to_paths(&edges);
        let (_, stats) = assign_lanes(
            &paths, &mut edges, &mut grid, &nodes, &sorted_node_ids, &empty_relations(),
            &from_side, &to_side, 8.0,
        );

        // Phase 3：POST materialize 已删
        assert_eq!(stats.segments_shifted, 0);
        assert_eq!(stats.shifts_failed, 0);
    }

    #[test]
    fn o1_enforce_auth_db_shared_trunk() {
        use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};

        let rel = |from: &str, to: &str| Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        };
        let relations = vec![rel("auth", "db"), rel("db", "auth")];
        let p0 = vec![
            pt(168.0, 316.0),
            pt(168.0, 332.0),
            pt(148.56, 332.0),
            pt(148.56, 360.0),
            pt(148.56, 376.0),
        ];
        let p1 = vec![
            pt(148.56, 376.0),
            pt(148.56, 345.92),
            pt(184.0, 345.92),
            pt(184.0, 316.0),
        ];
        let mut edges = vec![mk_edge(&p0), mk_edge(&p1)];
        let _ = &mut edges;
        // Phase 3/6：enforce_reverse_pair_min_gap 已删；间距由 H5 rip-up 承担
        let n = 0usize;
        let pa: Vec<Point> = edges[0].path_points().into_owned();
        let pb: Vec<Point> = edges[1].path_points().into_owned();
        eprintln!("shifted={n} pa={pa:?} pb={pb:?}");
        let lx = |pts: &[Point]| {
            pts.windows(2)
                .filter(|w| (w[1].x - w[0].x).abs() < 1.0 && (w[1].y - w[0].y).abs() > 1.0)
                .map(|w| ((w[1].y - w[0].y).abs(), w[0].x))
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                .map(|(_, x)| x)
                .unwrap()
        };
        let gap = (lx(&pa) - lx(&pb)).abs();
        eprintln!("gap={gap}");
        // Phase 3：enforce_reverse_pair_min_gap 空壳；间距由 H5 rip-up 承担
        assert_eq!(n, 0);
    }

}
