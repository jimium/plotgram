//! 可见性图模块
//!
//! 实现基于可见性图的最短路径算法，用于障碍避让样条路由。
//!
//! 算法流程：
//! 1. 将所有节点视为矩形障碍物，膨胀一定间距后提取角点
//! 2. 构建角点之间的可见性图（两角点连线不穿过任何障碍物）
//! 3. 在可见性图上用 Dijkstra 求最短路径
//! 4. 输出折线路径，供样条拟合使用
//!
//! 参考：Graphviz Spline-o-Matic 的 pathplan 库

use crate::layout::geometry::{Point, Rect};
use crate::layout::{constants, NodeLayout};
use std::collections::HashMap;

/// 坐标比较容差
const EPS: f64 = 0.1;

/// 障碍物均匀网格 cell 边长（像素）。
const OBSTACLE_GRID_CELL: f64 = 64.0;

fn obstacle_cell_coord(v: f64) -> i32 {
    (v / OBSTACLE_GRID_CELL).floor() as i32
}

/// 障碍物 bbox 均匀网格空间索引。
///
/// `ObstacleIndex::build` 原本对每对角点（(4N)² 对）线性扫描全部 N 个障碍物，
/// 复杂度 O(N³)；`is_visible_point`（逐边 start/end→角点）也是 O(N)。
/// 本索引将障碍物按 bbox 插入网格，查询段 bbox 时只返回可能相交的障碍物，
/// 将两处内层循环降为 O(k)（k 为邻近障碍物数，通常 ≤ 几个）。
///
/// 段-障碍物相交的交点必然同时落在段 bbox 与障碍物 bbox 内，故 bbox 预筛选
/// 不会漏检（安全）。
struct ObstacleGrid {
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl ObstacleGrid {
    fn build(obstacles: &[Obstacle]) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (oi, obs) in obstacles.iter().enumerate() {
            let cx0 = obstacle_cell_coord(obs.rect.left());
            let cx1 = obstacle_cell_coord(obs.rect.right());
            let cy0 = obstacle_cell_coord(obs.rect.top());
            let cy1 = obstacle_cell_coord(obs.rect.bottom());
            for cx in cx0..=cx1 {
                for cy in cy0..=cy1 {
                    cells.entry((cx, cy)).or_default().push(oi);
                }
            }
        }
        // 每个 cell 内按障碍物索引排序去重，保证查询返回顺序确定（AGENTS.md §2）
        for list in cells.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        Self { cells }
    }

    /// 返回 bbox 与查询段 bbox（含 EPS 余量）相交的障碍物索引，按索引升序去重。
    fn query_segment(&self, a: Point, b: Point) -> Vec<usize> {
        let xmin = a.x.min(b.x) - EPS;
        let xmax = a.x.max(b.x) + EPS;
        let ymin = a.y.min(b.y) - EPS;
        let ymax = a.y.max(b.y) + EPS;
        let cx0 = obstacle_cell_coord(xmin);
        let cx1 = obstacle_cell_coord(xmax);
        let cy0 = obstacle_cell_coord(ymin);
        let cy1 = obstacle_cell_coord(ymax);
        // 命中数通常很小（< 邻近障碍物数），用 Vec 线性去重比 HashSet 快
        let mut seen: Vec<usize> = Vec::new();
        for cx in cx0..=cx1 {
            for cy in cy0..=cy1 {
                if let Some(list) = self.cells.get(&(cx, cy)) {
                    for &oi in list {
                        if !seen.contains(&oi) {
                            seen.push(oi);
                        }
                    }
                }
            }
        }
        seen.sort_unstable();
        seen
    }
}

/// 膨胀后的矩形障碍物
#[derive(Debug, Clone)]
pub struct Obstacle {
    /// 关联的节点 ID（用于排除起止节点）
    pub node_id: Option<usize>,
    /// 膨胀后的矩形区域
    pub rect: Rect,
}

impl Obstacle {
    /// 从节点布局创建膨胀后的障碍物
    ///
    /// 膨胀间距取自 [`constants::DEFAULT_NODE_MARGIN`]。
    pub fn from_node(node_id: usize, nl: &NodeLayout) -> Self {
        let padding = constants::DEFAULT_NODE_MARGIN;
        Self {
            node_id: Some(node_id),
            rect: Rect::from(nl).expanded(padding),
        }
    }

    /// 提取四个角点
    pub fn corners(&self) -> [Point; 4] {
        [
            self.rect.top_left(),
            self.rect.top_right(),
            self.rect.bottom_right(),
            self.rect.bottom_left(),
        ]
    }

    /// 点是否在障碍物内部（严格内部，不含边界）
    pub fn contains_point(&self, p: &Point) -> bool {
        self.rect.contains_point_strict(*p, EPS)
    }
}

/// 图级障碍物索引：预计算角点两两阻挡关系，支持按 skip 集合快速查询可见性。
///
/// 构建一次，可被同一张图的所有边复用。核心优化：
/// - 障碍物列表只构建一次（全图节点 + padding）
/// - 角点两两阻挡关系预计算：`blockers[i][j]` = 阻挡 corner_i → corner_j 的障碍物索引列表
/// - 每条边查询时，corner_i → corner_j 可见 ⟺ 所有阻挡者都在 skip 集合中
/// - start/end 到角点的可见性逐边计算（不可缓存）
///
/// 相比原 `VisibilityGraph::build` 逐边建图（O(E·V²)），本结构将角点两两可见性
/// 预计算分摊到全图一次（O(V²·N)），每条边只需 O(V·N) 计算 start/end 到角点的可见性
/// + O(V²) 查表 + O(V² log V) Dijkstra。
pub struct ObstacleIndex {
    /// 全图障碍物（含 padding），按节点索引顺序
    obstacles: Vec<Obstacle>,
    /// 所有角点 + 所属障碍物索引，按 obstacles 顺序 × 4 角点展开
    corners: Vec<(Point, usize)>,
    /// 角点两两阻挡列表（对称矩阵，扁平存储）。
    /// `blockers[i * num_corners + j]` = 阻挡 corner_i → corner_j 的障碍物索引列表。
    /// 空列表表示无阻挡（对全部障碍物可见）。
    blockers: Vec<Vec<usize>>,
    num_corners: usize,
    /// 障碍物 bbox 空间索引，加速逐边 `is_visible_point` 的段-障碍物查询（方案 5）。
    grid: ObstacleGrid,
}

impl ObstacleIndex {
    /// 全图只建一次。预计算所有角点两两的阻挡障碍物列表。
    ///
    /// 障碍物膨胀间距取自 [`constants::DEFAULT_NODE_MARGIN`]。
    pub fn build(nodes: &[(usize, &NodeLayout)]) -> Self {
        let obstacles: Vec<Obstacle> = nodes
            .iter()
            .map(|&(idx, nl)| Obstacle::from_node(idx, nl))
            .collect();

        let grid = ObstacleGrid::build(&obstacles);

        let corners: Vec<(Point, usize)> = obstacles
            .iter()
            .enumerate()
            .flat_map(|(oi, obs)| obs.corners().into_iter().map(move |c| (c, oi)))
            .collect();

        let num_corners = corners.len();
        let mut blockers = vec![Vec::new(); num_corners * num_corners];

        for i in 0..num_corners {
            for j in (i + 1)..num_corners {
                let (pi, _) = corners[i];
                let (pj, _) = corners[j];
                let mut bl = Vec::new();
                // 方案 4：用障碍物网格只检查 bbox 与该角点对段相交的障碍物，
                // 将内层 O(N) 降为 O(k)。返回值已按索引升序去重，故 bl 顺序确定。
                for oi in grid.query_segment(pi, pj) {
                    if segment_intersects_obstacle(&pi, &pj, &obstacles[oi]) {
                        bl.push(oi);
                    }
                }
                // 对称存储
                blockers[i * num_corners + j] = bl.clone();
                blockers[j * num_corners + i] = bl;
            }
        }

        Self {
            obstacles,
            corners,
            blockers,
            num_corners,
            grid,
        }
    }

    /// 检查两个任意点之间是否可见（连线不穿过任何非 skip 障碍物）。
    /// 用于 start/end 到角点的可见性（逐边计算，不可缓存）。
    fn is_visible_point(&self, a: &Point, b: &Point, skip: &[usize]) -> bool {
        // 方案 5：用障碍物网格只检查 bbox 与该段相交的障碍物，
        // 将逐边 O(N) 降为 O(k)。skip 集合通常很小（起止节点），线性 contains 足够。
        for oi in self.grid.query_segment(*a, *b) {
            if skip.contains(&oi) {
                continue;
            }
            if segment_intersects_obstacle(a, b, &self.obstacles[oi]) {
                return false;
            }
        }
        true
    }

    /// 检查线段是否与任何非 skip 障碍物相交（公开接口，供 bezier/circular 路由器调用）。
    ///
    /// 返回 `true` 表示线段穿过至少一个障碍物（需要绕行）。
    pub fn segment_hits_any(&self, a: Point, b: Point, skip: &[usize]) -> bool {
        !self.is_visible_point(&a, &b, skip)
    }

    /// 检查两个角点是否可见（使用预计算的阻挡列表）。
    /// 可见 ⟺ 所有阻挡该对的障碍物都在 skip 集合中。
    fn corners_visible(&self, ci: usize, cj: usize, skip: &[usize]) -> bool {
        let bl = &self.blockers[ci * self.num_corners + cj];
        bl.iter().all(|&b| skip.contains(&b))
    }

    /// 为一条边计算障碍避让最短路径。
    ///
    /// 参数：
    /// - `start`/`end`: 起止点坐标
    /// - `skip_obstacles`: 需要跳过的障碍物索引（起止节点对应的障碍物）
    ///
    /// 返回：折线路径点列表（含起终点），如果无障碍（直线可见）则返回空
    pub fn shortest_path(
        &self,
        start: Point,
        end: Point,
        skip_obstacles: &[usize],
    ) -> Vec<Point> {
        // 先检查直线是否可行
        if self.is_visible_point(&start, &end, skip_obstacles) {
            return Vec::new();
        }

        // 构建顶点列表：start + active corners（跳过 skip 障碍物的角点）+ end
        let mut vertices: Vec<Point> = vec![start];
        let mut corner_vertex_idx: Vec<Option<usize>> = vec![None; self.num_corners];

        for (ci, &(p, oi)) in self.corners.iter().enumerate() {
            if skip_obstacles.contains(&oi) {
                continue;
            }
            corner_vertex_idx[ci] = Some(vertices.len());
            vertices.push(p);
        }
        vertices.push(end);

        let n = vertices.len();
        let start_idx = 0;
        let end_idx = n - 1;

        // 构建邻接表
        let mut adjacency: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];

        // start/end → corners（逐边计算可见性，不可缓存）
        for (ci, &(p, _)) in self.corners.iter().enumerate() {
            let Some(vi) = corner_vertex_idx[ci] else {
                continue;
            };
            if self.is_visible_point(&start, &p, skip_obstacles) {
                let d = distance(start, p);
                adjacency[start_idx].push((vi, d));
                adjacency[vi].push((start_idx, d));
            }
            if self.is_visible_point(&end, &p, skip_obstacles) {
                let d = distance(end, p);
                adjacency[end_idx].push((vi, d));
                adjacency[vi].push((end_idx, d));
            }
        }

        // corner → corner（使用预计算的阻挡列表查表）
        for ci in 0..self.num_corners {
            let Some(vi) = corner_vertex_idx[ci] else {
                continue;
            };
            for cj in (ci + 1)..self.num_corners {
                let Some(vj) = corner_vertex_idx[cj] else {
                    continue;
                };
                if self.corners_visible(ci, cj, skip_obstacles) {
                    let d = distance(self.corners[ci].0, self.corners[cj].0);
                    adjacency[vi].push((vj, d));
                    adjacency[vj].push((vi, d));
                }
            }
        }

        // Dijkstra 求最短路径
        dijkstra(&vertices, &adjacency, start_idx, end_idx)
    }
}

fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

/// Dijkstra 最短路径（BinaryHeap 实现，O(E log V)）
///
/// 返回从 `start` 到 `end` 的路径点列表（含起终点）。无路径时返回空。
fn dijkstra(
    vertices: &[Point],
    adjacency: &[Vec<(usize, f64)>],
    start: usize,
    end: usize,
) -> Vec<Point> {
    use std::cmp::Ordering;
    use std::collections::BinaryHeap;

    let n = vertices.len();
    if n == 0 {
        return Vec::new();
    }

    let mut dist = vec![f64::MAX; n];
    let mut prev = vec![None::<usize>; n];

    dist[start] = 0.0;

    #[derive(Copy, Clone)]
    struct HeapNode {
        dist: f64,
        node: usize,
    }

    impl PartialEq for HeapNode {
        fn eq(&self, other: &Self) -> bool {
            self.dist == other.dist && self.node == other.node
        }
    }
    impl Eq for HeapNode {}

    impl PartialOrd for HeapNode {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            // 最小堆：反转 dist 比较。等距时用 node 作为稳定 tiebreak（AGENTS.md §2 确定性）
            let dist_cmp = other.dist.partial_cmp(&self.dist)?;
            Some(dist_cmp.then_with(|| other.node.cmp(&self.node)))
        }
    }
    impl Ord for HeapNode {
        fn cmp(&self, other: &Self) -> Ordering {
            // dist 为 NaN 时 fallback 到 node 比较，保证全序
            self.partial_cmp(other).unwrap_or_else(|| other.node.cmp(&self.node))
        }
    }

    let mut heap = BinaryHeap::new();
    heap.push(HeapNode {
        dist: 0.0,
        node: start,
    });

    while let Some(current) = heap.pop() {
        if current.node == end {
            break;
        }
        if current.dist > dist[current.node] {
            continue;
        }
        for &(v, w) in &adjacency[current.node] {
            let alt = current.dist + w;
            if alt < dist[v] {
                dist[v] = alt;
                prev[v] = Some(current.node);
                heap.push(HeapNode {
                    dist: alt,
                    node: v,
                });
            }
        }
    }

    // 回溯路径
    let mut path = Vec::new();
    let mut current = end;
    while current != start {
        path.push(vertices[current]);
        match prev[current] {
            Some(p) => current = p,
            None => return Vec::new(), // 无路径
        }
    }
    path.push(vertices[start]);
    path.reverse();
    path
}

/// 检查线段是否与障碍物相交
///
/// 使用 visibility-graph 语义：线段端点在障碍物边上不算相交，
/// 只有穿过严格内部或跨边界才算阻挡。
fn segment_intersects_obstacle(a: &Point, b: &Point, obs: &Obstacle) -> bool {
    obs.rect.segment_crosses_interior(*a, *b, EPS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_obstacle_contains_point() {
        let obs = Obstacle {
            node_id: None,
            rect: Rect::new(10.0, 10.0, 40.0, 40.0),
        };
        assert!(obs.contains_point(&Point::new(30.0, 30.0)));
        assert!(!obs.contains_point(&Point::new(5.0, 30.0)));
        assert!(!obs.contains_point(&Point::new(60.0, 30.0)));
    }

    #[test]
    fn test_obstacle_index_no_obstacles() {
        // 无障碍物：直线可见，返回空
        let nodes: Vec<(usize, &NodeLayout)> = vec![];
        let index = ObstacleIndex::build(&nodes);
        let path = index.shortest_path(Point::new(0.0, 0.0), Point::new(100.0, 100.0), &[]);
        assert!(path.is_empty()); // 直线可见
    }

    #[test]
    fn test_obstacle_index_with_obstacle() {
        // 中间放一个障碍物挡住直线路径
        let nl = NodeLayout {
            x: 40.0,
            y: 10.0,
            width: 20.0,
            height: 30.0,
            ..Default::default()
        };
        let nodes = vec![(0, &nl)];
        let index = ObstacleIndex::build(&nodes);
        // skip=[] 时障碍物阻挡，需要绕行
        let path = index.shortest_path(Point::new(0.0, 25.0), Point::new(100.0, 25.0), &[]);
        // 应该有绕行路径（含起终点 + 角点）
        assert!(path.len() >= 2);
    }

    #[test]
    fn test_obstacle_index_skip_makes_visible() {
        // 障碍物在 skip 集合中时，直线可见
        let nl = NodeLayout {
            x: 40.0,
            y: 10.0,
            width: 20.0,
            height: 30.0,
            ..Default::default()
        };
        let nodes = vec![(0, &nl)];
        let index = ObstacleIndex::build(&nodes);
        // skip=[0] 时障碍物不阻挡，直线可见
        let path = index.shortest_path(Point::new(0.0, 25.0), Point::new(100.0, 25.0), &[0]);
        assert!(path.is_empty()); // 直线可见
    }

    #[test]
    fn test_obstacle_index_multi_obstacle_detour() {
        // 三个节点纵向排列，a→c 被 b 阻挡，需要绕行
        let nls = vec![
            NodeLayout { x: 120.0, y: 40.0, width: 60.0, height: 40.0, ..Default::default() },
            NodeLayout { x: 120.0, y: 150.0, width: 60.0, height: 40.0, ..Default::default() },
            NodeLayout { x: 120.0, y: 300.0, width: 60.0, height: 40.0, ..Default::default() },
        ];
        let nodes: Vec<(usize, &NodeLayout)> = nls.iter().enumerate().map(|(i, nl)| (i, nl)).collect();
        let index = ObstacleIndex::build(&nodes);
        // a=0, c=2 在 skip 中（端点节点），b=1 阻挡
        let path = index.shortest_path(Point::new(150.0, 80.0), Point::new(150.0, 300.0), &[0, 2]);
        // 应该有绕行路径
        assert!(path.len() >= 2);
        // 路径应绕过 b 节点（y 在 150-190 范围外的角点）
        assert!(path.iter().any(|&p| p.y < 142.0 || p.y > 198.0));
    }

    #[test]
    fn test_segment_intersects_obstacle_no_intersection() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 10.0);
        let obs = Obstacle {
            node_id: None,
            rect: Rect::new(50.0, 50.0, 30.0, 30.0),
        };
        assert!(!segment_intersects_obstacle(&a, &b, &obs));
    }

    #[test]
    fn test_segment_intersects_obstacle_crosses() {
        let a = Point::new(0.0, 25.0);
        let b = Point::new(100.0, 25.0);
        let obs = Obstacle {
            node_id: None,
            rect: Rect::new(40.0, 10.0, 20.0, 30.0),
        };
        assert!(segment_intersects_obstacle(&a, &b, &obs));
    }

    #[test]
    fn test_from_node_obstacle() {
        let nl = NodeLayout {
            x: 100.0,
            y: 200.0,
            width: 80.0,
            height: 40.0,
            ..Default::default()
        };
        let obs = Obstacle::from_node(0, &nl);
        let pad = constants::DEFAULT_NODE_MARGIN;
        assert!((obs.rect.left() - (100.0 - pad)).abs() < EPS);
        assert!((obs.rect.top() - (200.0 - pad)).abs() < EPS);
        assert!((obs.rect.right() - (180.0 + pad)).abs() < EPS);
        assert!((obs.rect.bottom() - (240.0 + pad)).abs() < EPS);
    }
}