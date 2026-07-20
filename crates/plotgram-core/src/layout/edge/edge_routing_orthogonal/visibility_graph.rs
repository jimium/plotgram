//! 正交 Visibility Graph (OVG) 路径搜索
//!
//! Phase B 核心：基于 Wybrow 2009 的正交可见性图，在障碍物空间中搜索全局最优正交路径。
//! 替代候选枚举的 fallback 方案，仅对 degraded 边启用。
//!
//! 算法：
//! 1. 收集障碍物膨胀后的 bbox 四角作为候选顶点
//! 2. 添加起点/终点及其在各轴上的投影点
//! 3. 对每对顶点，检查水平/垂直可见性（线段不穿任何障碍物）
//! 4. 可见则添加边，权重 = 线段长度
//! 5. Dijkstra 最短路径（边权 = 长度 + bend_penalty * 转弯）

use crate::layout::geometry::{Point, Rect, EPS};
use crate::layout::Port;
use std::collections::BinaryHeap;
use std::cmp::Ordering;

/// f64 包装器，实现 Ord 用于 BinaryHeap
#[derive(Clone, Copy, PartialEq)]
struct Cost(f64);

impl Eq for Cost {}

impl PartialOrd for Cost {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cost {
    fn cmp(&self, other: &Self) -> Ordering {
        // 反向排序用于最小堆
        other.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
    }
}

/// OVG 顶点类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexKind {
    /// 障碍物角落（膨胀后 bbox 的四角）
    ObstacleCorner,
    /// 路径端点（起点/终点）
    Endpoint,
    /// 通道交汇点（端点在各轴上的投影）
    ChannelJunction,
}

/// OVG 顶点
#[derive(Clone, Copy, Debug)]
pub struct OvgVertex {
    pub point: Point,
    pub kind: VertexKind,
}

/// 邻接边
#[derive(Clone, Copy, Debug)]
struct OvgEdge {
    to: usize,
    weight: f64,
    is_horizontal: bool,
}

/// 正交可见性图
pub struct OrthogonalVisibilityGraph {
    vertices: Vec<OvgVertex>,
    /// 邻接表：adjacency[from] = Vec<OvgEdge>
    adjacency: Vec<Vec<OvgEdge>>,
    /// 节点障碍物（用于可见性阻断 + 端点连接）
    obstacles: Vec<Rect>,
    /// 前 N 个为节点障碍物，其余为组障碍物。
    /// 端点连接仅检查节点障碍物（允许从组内出发/到达）。
    node_obstacle_count: usize,
    /// 组矩形（软惩罚：Dijkstra 穿越时加权，不阻断可见性）
    group_rects: Vec<Rect>,
}

/// OVG 顶点数上限（超出则降级为候选枚举）
const MAX_VERTICES: usize = 500;

/// 可见性检查的额外余量（与 geometry::EPS 对齐，确保 OVG 路径通过 path_is_clean）
const VISIBILITY_EPS: f64 = 0.1;

/// 组穿越软惩罚：Dijkstra 中边段穿越组内部时叠加的权重。
/// 使 OVG 路径自然偏好绕行组，从而产出 strict（避节点+避组）候选。
const GROUP_CROSSING_PENALTY: f64 = 500.0;

/// 已路由边重叠惩罚：每像素共线重叠的惩罚权重。
/// 使 OVG 路径偏好与已有边分离，减少视觉重叠。
const OVERLAP_PENALTY_PER_PX: f64 = 3.0;

/// 重叠检测的垂直/水平容差（px）：两条段在此距离内视为“共线”。
const OVERLAP_PROXIMITY: f64 = 4.0;

impl OrthogonalVisibilityGraph {
    /// 从障碍物集合构建 OVG（所有障碍物均视为节点障碍物，无组惩罚）
    pub fn build(obstacles: &[Rect]) -> Self {
        Self::build_with_groups(obstacles, obstacles.len(), &[])
    }

    /// 从节点障碍物 + 组障碍物构建 OVG
    ///
    /// - `node_obstacles`: 节点 bbox（膨胀后）——用于可见性阻断
    /// - `group_obstacles`: 组 bbox——仅作 Dijkstra 软惩罚，不阻断可见性
    ///
    /// 组矩形不参与图边可见性检查（避免碎片化），而是在 Dijkstra 中作为穿越惩罚。
    pub fn build_with_group_obstacles(
        node_obstacles: &[Rect],
        group_obstacles: &[Rect],
    ) -> Self {
        Self::build_with_groups(node_obstacles, node_obstacles.len(), group_obstacles)
    }

    fn build_with_groups(obstacles: &[Rect], node_obstacle_count: usize, group_rects: &[Rect]) -> Self {
        let mut vertices = Vec::new();
        let mut adjacency: Vec<Vec<OvgEdge>> = Vec::new();

        // 1. 收集障碍物四角作为候选顶点
        for rect in obstacles {
            let corners = [
                rect.top_left(),
                rect.top_right(),
                rect.bottom_left(),
                rect.bottom_right(),
            ];
            for corner in corners {
                vertices.push(OvgVertex {
                    point: corner,
                    kind: VertexKind::ObstacleCorner,
                });
            }
        }

        // 顶点数检查
        if vertices.len() > MAX_VERTICES {
            return Self {
                vertices: Vec::new(),
                adjacency: Vec::new(),
                obstacles: obstacles.to_vec(),
                node_obstacle_count,
                group_rects: group_rects.to_vec(),
            };
        }

        // 2. 构建可见性边（检查全部障碍物，包括组）
        let n = vertices.len();
        adjacency.resize(n, Vec::new());

        for i in 0..n {
            for j in (i + 1)..n {
                let p1 = vertices[i].point;
                let p2 = vertices[j].point;

                let is_horizontal = (p1.y - p2.y).abs() < EPS;
                let is_vertical = (p1.x - p2.x).abs() < EPS;

                if !is_horizontal && !is_vertical {
                    continue;
                }

                if Self::is_visible(p1, p2, obstacles) {
                    let weight = p1.distance_to(p2);
                    adjacency[i].push(OvgEdge {
                        to: j,
                        weight,
                        is_horizontal,
                    });
                    adjacency[j].push(OvgEdge {
                        to: i,
                        weight,
                        is_horizontal,
                    });
                }
            }
        }

        Self {
            vertices,
            adjacency,
            obstacles: obstacles.to_vec(),
            node_obstacle_count,
            group_rects: group_rects.to_vec(),
        }
    }

    /// 检查两点之间的水平/垂直线段是否可见
    fn is_visible(p1: Point, p2: Point, obstacles: &[Rect]) -> bool {
        for rect in obstacles {
            if rect.segment_crosses_interior(p1, p2, VISIBILITY_EPS) {
                return false;
            }
        }
        true
    }

    /// 搜索最短路径
    ///
    /// `from_obstacle_idx` / `to_obstacle_idx`: 端点所属节点的障碍物索引（连接时跳过）。
    /// 传 None 表示不跳过任何障碍物。
    pub fn shortest_path(
        &self,
        start: Point,
        end: Point,
        from_port: Port,
        to_port: Port,
        bend_penalty: f64,
    ) -> Option<Vec<Point>> {
        self.shortest_path_excluding(start, end, from_port, to_port, bend_penalty, None, None, &[])
    }

    /// 搜索最短路径（可排除端点所属障碍物）
    ///
    /// 性能优化：零克隆 + 轻量级投影（仅 bbox 内障碍物）+ 空间过滤。
    /// `occupied`: 已路由段列表 (x1, y1, x2, y2)，用于重叠惩罚。
    pub fn shortest_path_excluding(
        &self,
        start: Point,
        end: Point,
        _from_port: Port,
        _to_port: Port,
        bend_penalty: f64,
        from_exclude: Option<usize>,
        to_exclude: Option<usize>,
        occupied: &[(f64, f64, f64, f64)],
    ) -> Option<Vec<Point>> {
        if self.vertices.is_empty() {
            return None;
        }

        let n = self.vertices.len();
        let start_idx = n;
        let end_idx = n + 1;

        // 空间过滤：仅考虑边 bbox + margin 内的顶点
        let margin = 80.0;
        let x_lo = start.x.min(end.x) - margin;
        let x_hi = start.x.max(end.x) + margin;
        let y_lo = start.y.min(end.y) - margin;
        let y_hi = start.y.max(end.y) + margin;

        // 有效障碍物（排除端点自己的节点）
        let effective_obs: Vec<Rect> = self.obstacles[..self.node_obstacle_count]
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != from_exclude && Some(*i) != to_exclude)
            .map(|(_, r)| *r)
            .collect();

        // 收集 bbox 内障碍物边界坐标（用于轻量级投影）
        let mut proj_y_coords: Vec<f64> = Vec::new();
        let mut proj_x_coords: Vec<f64> = Vec::new();
        for rect in &effective_obs {
            if rect.right() >= x_lo && rect.left() <= x_hi
                && rect.bottom() >= y_lo && rect.top() <= y_hi {
                proj_y_coords.push(rect.top());
                proj_y_coords.push(rect.bottom());
                proj_x_coords.push(rect.left());
                proj_x_coords.push(rect.right());
            }
        }
        proj_y_coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        proj_y_coords.dedup_by(|a, b| (*a - *b).abs() < EPS);
        proj_x_coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        proj_x_coords.dedup_by(|a, b| (*a - *b).abs() < EPS);

        // 虚拟投影节点池：(point, adj_list)
        // 索引从 n+2 开始
        let mut virt_points: Vec<Point> = Vec::new();
        let mut virt_adj: Vec<Vec<OvgEdge>> = Vec::new();
        // 原图顶点的额外 overlay 边（到虚拟节点 / start / end）
        let mut overlay_map: Vec<(usize, OvgEdge)> = Vec::new();
        let mut start_adj: Vec<OvgEdge> = Vec::new();
        let mut end_adj: Vec<OvgEdge> = Vec::new();

        // 连接端点到原图顶点（直接连接）
        for (i, v) in self.vertices.iter().enumerate() {
            if v.point.x < x_lo || v.point.x > x_hi || v.point.y < y_lo || v.point.y > y_hi {
                continue;
            }
            let h_s = (start.y - v.point.y).abs() < EPS;
            let v_s = (start.x - v.point.x).abs() < EPS;
            if (h_s || v_s) && Self::is_visible(start, v.point, &effective_obs) {
                let w = start.distance_to(v.point);
                start_adj.push(OvgEdge { to: i, weight: w, is_horizontal: h_s });
                overlay_map.push((i, OvgEdge { to: start_idx, weight: w, is_horizontal: h_s }));
            }
            let h_e = (end.y - v.point.y).abs() < EPS;
            let v_e = (end.x - v.point.x).abs() < EPS;
            if (h_e || v_e) && Self::is_visible(end, v.point, &effective_obs) {
                let w = end.distance_to(v.point);
                end_adj.push(OvgEdge { to: i, weight: w, is_horizontal: h_e });
                overlay_map.push((i, OvgEdge { to: end_idx, weight: w, is_horizontal: h_e }));
            }
        }

        // 直接连接 start→end
        if Self::is_visible(start, end, &effective_obs) {
            let h = (start.y - end.y).abs() < EPS;
            let v = (start.x - end.x).abs() < EPS;
            if h || v {
                start_adj.push(OvgEdge { to: end_idx, weight: start.distance_to(end), is_horizontal: h });
            }
        }

        // 轻量级投影：为 start 创建虚拟节点
        self.build_projections(
            start, start_idx, &effective_obs, &proj_y_coords, &proj_x_coords,
            x_lo, x_hi, y_lo, y_hi,
            &mut virt_points, &mut virt_adj, &mut overlay_map, &mut start_adj,
        );
        // 为 end 创建虚拟节点
        self.build_projections(
            end, end_idx, &effective_obs, &proj_y_coords, &proj_x_coords,
            x_lo, x_hi, y_lo, y_hi,
            &mut virt_points, &mut virt_adj, &mut overlay_map, &mut end_adj,
        );

        overlay_map.sort_by_key(|(src, _)| *src);

        // Dijkstra
        let virt_base = n + 2;
        let total = virt_base + virt_points.len();
        let mut dist = vec![f64::INFINITY; total];
        let mut prev: Vec<Option<(usize, bool)>> = vec![None; total];
        let mut heap = BinaryHeap::new();

        dist[start_idx] = 0.0;
        heap.push((Cost(0.0), start_idx, false));

        while let Some((Cost(cost), u, last_horizontal)) = heap.pop() {
            if cost > dist[u] + EPS {
                continue;
            }
            if u == end_idx {
                break;
            }

            let u_point = if u == start_idx { start }
                else if u == end_idx { end }
                else if u >= virt_base { virt_points[u - virt_base] }
                else { self.vertices[u].point };

            if u == start_idx {
                for edge in &start_adj {
                    self.dijkstra_relax(edge, u, u_point, last_horizontal, cost,
                        bend_penalty, start, end, virt_base, &virt_points,
                        occupied, &mut dist, &mut prev, &mut heap);
                }
            } else if u == end_idx {
                for edge in &end_adj {
                    self.dijkstra_relax(edge, u, u_point, last_horizontal, cost,
                        bend_penalty, start, end, virt_base, &virt_points,
                        occupied, &mut dist, &mut prev, &mut heap);
                }
            } else if u >= virt_base {
                let vi = u - virt_base;
                for edge in &virt_adj[vi] {
                    self.dijkstra_relax(edge, u, u_point, last_horizontal, cost,
                        bend_penalty, start, end, virt_base, &virt_points,
                        occupied, &mut dist, &mut prev, &mut heap);
                }
            } else {
                // 原图顶点
                for edge in &self.adjacency[u] {
                    let vp = self.vertices[edge.to].point;
                    if vp.x < x_lo || vp.x > x_hi || vp.y < y_lo || vp.y > y_hi {
                        continue;
                    }
                    self.dijkstra_relax(edge, u, u_point, last_horizontal, cost,
                        bend_penalty, start, end, virt_base, &virt_points,
                        occupied, &mut dist, &mut prev, &mut heap);
                }
                // overlay 反向边
                for &(_, ref edge) in overlay_map.iter().filter(|(src, _)| *src == u) {
                    self.dijkstra_relax(edge, u, u_point, last_horizontal, cost,
                        bend_penalty, start, end, virt_base, &virt_points,
                        occupied, &mut dist, &mut prev, &mut heap);
                }
            }
        }

        if dist[end_idx].is_infinite() {
            return None;
        }

        // 回溯路径
        let mut path = Vec::new();
        let mut cur = end_idx;
        while cur != start_idx {
            let p = if cur == end_idx { end }
                else if cur >= virt_base { virt_points[cur - virt_base] }
                else { self.vertices[cur].point };
            path.push(p);
            match prev[cur] {
                Some((p_idx, _)) => cur = p_idx,
                None => return None,
            }
        }
        path.push(start);
        path.reverse();

        Some(simplify_orthogonal_path(&path))
    }

    /// Dijkstra 松弛步骤（内联优化）
    #[inline]
    fn dijkstra_relax(
        &self,
        edge: &OvgEdge,
        u: usize,
        u_point: Point,
        last_horizontal: bool,
        cost: f64,
        bend_penalty: f64,
        start: Point,
        end: Point,
        virt_base: usize,
        virt_points: &[Point],
        occupied: &[(f64, f64, f64, f64)],
        dist: &mut [f64],
        prev: &mut [Option<(usize, bool)>],
        heap: &mut BinaryHeap<(Cost, usize, bool)>,
    ) {
        let v = edge.to;
        let bend_cost = if last_horizontal != edge.is_horizontal && cost > EPS {
            bend_penalty
        } else {
            0.0
        };
        let v_point = if v == self.vertices.len() { start }
            else if v == self.vertices.len() + 1 { end }
            else if v >= virt_base { virt_points[v - virt_base] }
            else { self.vertices[v].point };
        let group_cost = self.group_crossing_cost(u_point, v_point);
        let overlap_cost = Self::segment_overlap_penalty(u_point, v_point, occupied);
        let new_dist = cost + edge.weight + bend_cost + group_cost + overlap_cost;
        if new_dist < dist[v] - EPS {
            dist[v] = new_dist;
            prev[v] = Some((u, edge.is_horizontal));
            heap.push((Cost(new_dist), v, edge.is_horizontal));
        }
    }

    /// 计算一条边与已路由段的共线重叠惩罚
    #[inline]
    fn segment_overlap_penalty(p1: Point, p2: Point, occupied: &[(f64, f64, f64, f64)]) -> f64 {
        if occupied.is_empty() {
            return 0.0;
        }
        let is_h = (p1.y - p2.y).abs() < EPS;
        let is_v = (p1.x - p2.x).abs() < EPS;
        if !is_h && !is_v {
            return 0.0;
        }
        let mut penalty = 0.0;
        if is_h {
            let y = p1.y;
            let (lo, hi) = if p1.x < p2.x { (p1.x, p2.x) } else { (p2.x, p1.x) };
            for &(x1, y1, x2, y2) in occupied {
                // 已路由段也是水平？
                if (y1 - y2).abs() < EPS && (y1 - y).abs() < OVERLAP_PROXIMITY {
                    let (o_lo, o_hi) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
                    let overlap = (hi.min(o_hi) - lo.max(o_lo)).max(0.0);
                    penalty += overlap;
                }
            }
        } else {
            let x = p1.x;
            let (lo, hi) = if p1.y < p2.y { (p1.y, p2.y) } else { (p2.y, p1.y) };
            for &(x1, y1, x2, y2) in occupied {
                // 已路由段也是垂直？
                if (x1 - x2).abs() < EPS && (x1 - x).abs() < OVERLAP_PROXIMITY {
                    let (o_lo, o_hi) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
                    let overlap = (hi.min(o_hi) - lo.max(o_lo)).max(0.0);
                    penalty += overlap;
                }
            }
        }
        penalty * OVERLAP_PENALTY_PER_PX
    }

    /// 轻量级投影：为端点创建虚拟节点（仅 bbox 内障碍物边界坐标）
    fn build_projections(
        &self,
        endpoint: Point,
        endpoint_idx: usize,
        effective_obs: &[Rect],
        y_coords: &[f64],
        x_coords: &[f64],
        x_lo: f64, x_hi: f64, y_lo: f64, y_hi: f64,
        virt_points: &mut Vec<Point>,
        virt_adj: &mut Vec<Vec<OvgEdge>>,
        overlay_map: &mut Vec<(usize, OvgEdge)>,
        endpoint_adj: &mut Vec<OvgEdge>,
    ) {
        let virt_base = self.vertices.len() + 2;

        // 垂直投影：(endpoint.x, y_coord)
        for &yc in y_coords {
            if yc < y_lo || yc > y_hi || (yc - endpoint.y).abs() < EPS {
                continue;
            }
            let proj = Point::new(endpoint.x, yc);
            if !Self::is_visible(endpoint, proj, effective_obs) {
                continue;
            }
            let vi = virt_points.len();
            let proj_idx = virt_base + vi;
            virt_points.push(proj);
            virt_adj.push(Vec::new());

            let w = endpoint.distance_to(proj);
            endpoint_adj.push(OvgEdge { to: proj_idx, weight: w, is_horizontal: false });
            virt_adj[vi].push(OvgEdge { to: endpoint_idx, weight: w, is_horizontal: false });

            // 连接到同 y 的原图顶点
            for (i, v) in self.vertices.iter().enumerate() {
                if (v.point.y - proj.y).abs() < EPS
                    && v.point.x >= x_lo && v.point.x <= x_hi
                    && Self::is_visible(proj, v.point, &self.obstacles)
                {
                    let w2 = proj.distance_to(v.point);
                    virt_adj[vi].push(OvgEdge { to: i, weight: w2, is_horizontal: true });
                    overlay_map.push((i, OvgEdge { to: proj_idx, weight: w2, is_horizontal: true }));
                }
            }
        }

        // 水平投影：(x_coord, endpoint.y)
        for &xc in x_coords {
            if xc < x_lo || xc > x_hi || (xc - endpoint.x).abs() < EPS {
                continue;
            }
            let proj = Point::new(xc, endpoint.y);
            if !Self::is_visible(endpoint, proj, effective_obs) {
                continue;
            }
            let vi = virt_points.len();
            let proj_idx = virt_base + vi;
            virt_points.push(proj);
            virt_adj.push(Vec::new());

            let w = endpoint.distance_to(proj);
            endpoint_adj.push(OvgEdge { to: proj_idx, weight: w, is_horizontal: true });
            virt_adj[vi].push(OvgEdge { to: endpoint_idx, weight: w, is_horizontal: true });

            // 连接到同 x 的原图顶点
            for (i, v) in self.vertices.iter().enumerate() {
                if (v.point.x - proj.x).abs() < EPS
                    && v.point.y >= y_lo && v.point.y <= y_hi
                    && Self::is_visible(proj, v.point, &self.obstacles)
                {
                    let w2 = proj.distance_to(v.point);
                    virt_adj[vi].push(OvgEdge { to: i, weight: w2, is_horizontal: false });
                    overlay_map.push((i, OvgEdge { to: proj_idx, weight: w2, is_horizontal: false }));
                }
            }
        }
    }

    /// 组穿越惩罚计算
    #[inline]
    fn group_crossing_cost(&self, p1: Point, p2: Point) -> f64 {
        if self.group_rects.is_empty() {
            return 0.0;
        }
        self.group_rects.iter()
            .filter(|r| r.segment_crosses_interior(p1, p2, VISIBILITY_EPS))
            .count() as f64 * GROUP_CROSSING_PENALTY
    }

    /// 图是否为空
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// 顶点数
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
}

/// 简化正交路径：移除共线点
fn simplify_orthogonal_path(path: &[Point]) -> Vec<Point> {
    if path.len() <= 2 {
        return path.to_vec();
    }

    let mut result = vec![path[0]];
    for i in 1..path.len() - 1 {
        let prev = result.last().unwrap();
        let curr = &path[i];
        let next = &path[i + 1];

        let collinear_h = (prev.y - curr.y).abs() < EPS && (curr.y - next.y).abs() < EPS;
        let collinear_v = (prev.x - curr.x).abs() < EPS && (curr.x - next.x).abs() < EPS;

        if !collinear_h && !collinear_v {
            result.push(*curr);
        }
    }
    result.push(*path.last().unwrap());
    result
}

/// 检查 OVG 是否启用（默认开启，PLOTGRAM_OVG_ENABLED=0 关闭）
pub fn ovg_enabled() -> bool {
    !std::env::var("PLOTGRAM_OVG_ENABLED")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

/// 从节点布局 + 组布局构建 OVG
///
/// 节点 bbox 膨胀后作为硬障碍物，组 bbox 作为软障碍物（图边不穿组，端点可穿越）。
pub fn build_ovg_from_nodes(
    nodes: &std::collections::HashMap<String, crate::layout::NodeLayout>,
    sorted_node_ids: &[String],
    node_pad: f64,
) -> OrthogonalVisibilityGraph {
    build_ovg_with_groups(nodes, sorted_node_ids, node_pad, &[])
}

/// 从节点 + 组构建 OVG（含组障碍物）
pub fn build_ovg_with_groups(
    nodes: &std::collections::HashMap<String, crate::layout::NodeLayout>,
    sorted_node_ids: &[String],
    node_pad: f64,
    group_rects: &[Rect],
) -> OrthogonalVisibilityGraph {
    let node_obstacles: Vec<Rect> = sorted_node_ids
        .iter()
        .filter_map(|id| nodes.get(id))
        .map(|nl| Rect::new(nl.x, nl.y, nl.width, nl.height).expanded(node_pad))
        .collect();
    OrthogonalVisibilityGraph::build_with_group_obstacles(&node_obstacles, group_rects)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn test_ovg_empty_obstacles() {
        let ovg = OrthogonalVisibilityGraph::build(&[]);
        assert!(ovg.is_empty());
    }

    #[test]
    fn test_ovg_single_obstacle() {
        let obstacles = vec![rect(100.0, 100.0, 50.0, 50.0)];
        let ovg = OrthogonalVisibilityGraph::build(&obstacles);

        assert!(!ovg.is_empty());
        assert_eq!(ovg.vertex_count(), 4);
    }

    #[test]
    fn test_ovg_simple_path() {
        // 两个障碍物，起点在左，终点在右，中间有通道
        let obstacles = vec![
            rect(100.0, 50.0, 40.0, 80.0),   // 上方障碍
            rect(100.0, 200.0, 40.0, 80.0),  // 下方障碍
        ];
        let ovg = OrthogonalVisibilityGraph::build(&obstacles);

        let start = Point::new(50.0, 165.0);  // 在通道中间
        let end = Point::new(200.0, 165.0);

        let path = ovg.shortest_path(start, end, Port::Right, Port::Left, 28.0);

        assert!(path.is_some(), "Should find path through gap");
        let path = path.unwrap();
        assert!(path.len() >= 2);
        assert_eq!(path[0], start);
        assert_eq!(*path.last().unwrap(), end);
    }

    #[test]
    fn test_ovg_path_around_obstacle() {
        // 单个障碍物挡在中间
        let obstacles = vec![rect(100.0, 100.0, 100.0, 100.0)];
        let ovg = OrthogonalVisibilityGraph::build(&obstacles);

        let start = Point::new(50.0, 150.0);
        let end = Point::new(250.0, 150.0);

        let path = ovg.shortest_path(start, end, Port::Right, Port::Left, 28.0);

        assert!(path.is_some(), "Should find path around obstacle");
        let path = path.unwrap();

        // 路径应该绕过障碍物（y <= 100 或 y >= 200，即沿障碍物边界或更远）
        let goes_above = path.iter().any(|p| p.y <= 100.0 + EPS);
        let goes_below = path.iter().any(|p| p.y >= 200.0 - EPS);
        assert!(goes_above || goes_below, "Path should go around obstacle, got: {:?}", path);

        // 路径不应穿过障碍物内部（100 < y < 200 且 100 < x < 200）
        for p in &path {
            let inside_x = p.x > 100.0 + EPS && p.x < 200.0 - EPS;
            let inside_y = p.y > 100.0 + EPS && p.y < 200.0 - EPS;
            assert!(!(inside_x && inside_y), "Path should not go through obstacle interior");
        }
    }

    #[test]
    fn test_simplify_orthogonal_path() {
        let path = vec![
            Point::new(0.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 50.0),
            Point::new(100.0, 100.0),
        ];

        let simplified = simplify_orthogonal_path(&path);
        assert_eq!(simplified.len(), 3);
        assert_eq!(simplified[0], Point::new(0.0, 0.0));
        assert_eq!(simplified[1], Point::new(100.0, 0.0));
        assert_eq!(simplified[2], Point::new(100.0, 100.0));
    }

    #[test]
    fn test_ovg_multiple_obstacles() {
        // 多个障碍物场景：三个障碍物排成一排
        let obstacles = vec![
            rect(80.0, 80.0, 40.0, 40.0),   // 左上
            rect(80.0, 180.0, 40.0, 40.0),  // 左下
            rect(180.0, 130.0, 40.0, 40.0), // 右中
        ];
        let ovg = OrthogonalVisibilityGraph::build(&obstacles);

        assert!(!ovg.is_empty());
        assert_eq!(ovg.vertex_count(), 12); // 3 obstacles * 4 corners

        // 从左侧到右侧，应能找到绕行路径
        let start = Point::new(30.0, 150.0);
        let end = Point::new(270.0, 150.0);

        let path = ovg.shortest_path(start, end, Port::Right, Port::Left, 28.0);
        assert!(path.is_some(), "Should find path through multiple obstacles");
        let path = path.unwrap();
        assert!(path.len() >= 2);
        assert_eq!(path[0], start);
        assert_eq!(*path.last().unwrap(), end);
    }

    #[test]
    fn test_ovg_simple_obstacle() {
        // OVG 单障碍物绕行：起点和终点在障碍物两侧，路径必须绕行
        let obstacles = vec![rect(100.0, 100.0, 100.0, 100.0)];
        let ovg = OrthogonalVisibilityGraph::build(&obstacles);

        // 起点在障碍物左侧，终点在右侧，同一水平线穿过障碍物
        let start = Point::new(50.0, 150.0);
        let end = Point::new(250.0, 150.0);

        let path = ovg.shortest_path(start, end, Port::Right, Port::Left, 28.0);
        assert!(path.is_some(), "OVG should find path around single obstacle");
        let path = path.unwrap();

        // 验证路径不穿过障碍物内部
        for p in &path {
            let inside_x = p.x > 100.0 + EPS && p.x < 200.0 - EPS;
            let inside_y = p.y > 100.0 + EPS && p.y < 200.0 - EPS;
            assert!(!(inside_x && inside_y), "Path must not cross obstacle interior");
        }
    }
}
