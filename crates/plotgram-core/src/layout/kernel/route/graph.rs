//! `ResourceGraph`：唯一的「边可以走哪里」模型（Phase 2 可搜索图）。
//!
//! 顶点：PortAnchor / ChannelJunction / GroupGate。
//! 边：轴向可见段；障碍不入图 → H1/H2 由构造保证。
//! 全部用 [`BTreeMap`] / 显式排序键（AGENTS.md §2）。

use crate::layout::geometry::{Point, Rect, EPS};
use crate::layout::group::GroupRoutingContext;
use crate::layout::types::{NodeLayout, Port};
use std::collections::{BTreeMap, HashMap};

/// 资源 ID（通道段 / 端口侧占用槽）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(pub u64);

/// 顶点 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceVertexId(pub u32);

/// 资源图顶点种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceVertexKind {
    /// 端口锚点。
    PortAnchor,
    /// 通道交汇点（障碍角点 / 投影）。
    ChannelJunction,
    /// 分组门。
    GroupGate,
}

/// 资源图顶点。
#[derive(Debug, Clone)]
pub struct ResourceVertex {
    pub id: ResourceVertexId,
    pub kind: ResourceVertexKind,
    pub position: Point,
    pub node_id: Option<String>,
    pub port: Option<Port>,
}

/// 资源图有向边（轴向可行段）。
#[derive(Debug, Clone)]
pub struct ResourceEdge {
    pub from: ResourceVertexId,
    pub to: ResourceVertexId,
    pub resource: ResourceId,
    pub capacity: u32,
    pub length: f64,
    pub is_horizontal: bool,
}

/// 资源图（确定性容器）。
#[derive(Debug, Clone, Default)]
pub struct ResourceGraph {
    vertices: BTreeMap<ResourceVertexId, ResourceVertex>,
    adjacency: BTreeMap<ResourceVertexId, Vec<ResourceEdge>>,
    next_vertex: u32,
    next_resource: u64,
}

const VIS_EPS: f64 = 0.1;
/// 障碍外扩量：角点/投影落在通道上，避免边界对边连成穿障段。
const CHANNEL_PAD: f64 = 8.0;
const DEFAULT_CAPACITY: u32 = 8;
const MAX_JUNCTIONS: usize = 400;

impl ResourceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn edge_count(&self) -> usize {
        self.adjacency.values().map(|v| v.len()).sum()
    }

    pub fn vertices(&self) -> impl Iterator<Item = &ResourceVertex> {
        self.vertices.values()
    }

    pub fn vertex(&self, id: ResourceVertexId) -> Option<&ResourceVertex> {
        self.vertices.get(&id)
    }

    pub fn neighbors(&self, id: ResourceVertexId) -> &[ResourceEdge] {
        self.adjacency
            .get(&id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn alloc_vertex(
        &mut self,
        kind: ResourceVertexKind,
        position: Point,
        node_id: Option<String>,
        port: Option<Port>,
    ) -> ResourceVertexId {
        // 合并近重合顶点（确定性：按插入序，先到先得）
        for v in self.vertices.values() {
            if (v.position.x - position.x).abs() < EPS && (v.position.y - position.y).abs() < EPS {
                return v.id;
            }
        }
        let id = ResourceVertexId(self.next_vertex);
        self.next_vertex = self.next_vertex.saturating_add(1);
        self.vertices.insert(
            id,
            ResourceVertex {
                id,
                kind,
                position,
                node_id,
                port,
            },
        );
        self.adjacency.entry(id).or_default();
        id
    }

    /// 为每个节点的四侧端口中点各加一个 PortAnchor（Phase 1 骨架 / 测试用）。
    pub fn add_port_anchors_from_nodes(&mut self, nodes: &BTreeMap<String, NodeLayout>) {
        for (nid, nl) in nodes {
            for port in [Port::Top, Port::Bottom, Port::Left, Port::Right] {
                let position = port_midpoint(nl, port);
                self.alloc_vertex(
                    ResourceVertexKind::PortAnchor,
                    position,
                    Some(nid.clone()),
                    Some(port),
                );
            }
        }
    }

    pub fn try_add_axis_edge(
        &mut self,
        from: ResourceVertexId,
        to: ResourceVertexId,
        capacity: u32,
    ) -> bool {
        let (Some(a), Some(b)) = (self.vertices.get(&from), self.vertices.get(&to)) else {
            return false;
        };
        let dx = (a.position.x - b.position.x).abs();
        let dy = (a.position.y - b.position.y).abs();
        if dx > EPS && dy > EPS {
            return false;
        }
        if dx <= EPS && dy <= EPS {
            return false;
        }
        let length = dx.max(dy);
        let is_horizontal = dy <= EPS;
        let resource = ResourceId(self.next_resource);
        self.next_resource = self.next_resource.saturating_add(1);
        let edge = ResourceEdge {
            from,
            to,
            resource,
            capacity,
            length,
            is_horizontal,
        };
        let list = self.adjacency.entry(from).or_default();
        let pos = list
            .binary_search_by(|e| e.to.cmp(&to).then(e.resource.cmp(&resource)))
            .unwrap_or_else(|i| i);
        list.insert(pos, edge);
        true
    }

    /// 为单条边查询构建可搜索图：端点 PortAnchor + 障碍角点 ChannelJunction + 可选 GroupGate。
    ///
    /// `node_obstacles`：已膨胀的节点障碍矩形（不含 from/to 自身亦可，由调用方决定）。
    /// 返回 `(graph, start_id, end_id)`。
    pub fn build_for_query(
        from: Point,
        to: Point,
        from_port: Port,
        to_port: Port,
        from_id: &str,
        to_id: &str,
        node_obstacles: &[Rect],
        group_rects: &[(String, Rect)],
        group_ctx: Option<&GroupRoutingContext>,
    ) -> (Self, ResourceVertexId, ResourceVertexId) {
        let mut g = Self::new();
        let start = g.alloc_vertex(
            ResourceVertexKind::PortAnchor,
            from,
            Some(from_id.to_string()),
            Some(from_port),
        );
        let end = g.alloc_vertex(
            ResourceVertexKind::PortAnchor,
            to,
            Some(to_id.to_string()),
            Some(to_port),
        );

        // 障碍外扩后的角点 → ChannelJunction（落在通道上，不贴障边界）
        let mut junction_count = 0usize;
        for rect in node_obstacles {
            let shell = rect.expanded(CHANNEL_PAD);
            for corner in [
                shell.top_left(),
                shell.top_right(),
                shell.bottom_left(),
                shell.bottom_right(),
            ] {
                if junction_count >= MAX_JUNCTIONS {
                    break;
                }
                g.alloc_vertex(ResourceVertexKind::ChannelJunction, corner, None, None);
                junction_count += 1;
            }
        }

        // 跨组：组边界中点 → GroupGate（同样外扩，避免贴边穿障）
        let cross_group = group_ctx
            .map(|ctx| !ctx.is_same_leaf_group(from_id, to_id))
            .unwrap_or(false);
        if cross_group {
            for (gid, rect) in group_rects {
                let shell = rect.expanded(CHANNEL_PAD);
                let mid_top = Point::new(shell.x + shell.width * 0.5, shell.y);
                let mid_bot = Point::new(shell.x + shell.width * 0.5, shell.bottom());
                let mid_left = Point::new(shell.x, shell.y + shell.height * 0.5);
                let mid_right = Point::new(shell.right(), shell.y + shell.height * 0.5);
                for p in [mid_top, mid_bot, mid_left, mid_right] {
                    g.alloc_vertex(
                        ResourceVertexKind::GroupGate,
                        p,
                        Some(gid.clone()),
                        None,
                    );
                }
            }
        }

        // 端点对障碍外扩壳的正交投影（提升绕障连通性）
        let margin = 120.0;
        let x_lo = from.x.min(to.x) - margin;
        let x_hi = from.x.max(to.x) + margin;
        let y_lo = from.y.min(to.y) - margin;
        let y_hi = from.y.max(to.y) + margin;
        for rect in node_obstacles {
            let shell = rect.expanded(CHANNEL_PAD);
            if shell.right() < x_lo
                || shell.left() > x_hi
                || shell.bottom() < y_lo
                || shell.top() > y_hi
            {
                continue;
            }
            for (sx, sy) in [(from.x, from.y), (to.x, to.y)] {
                let proj_h = Point::new(sx, shell.top());
                let proj_h2 = Point::new(sx, shell.bottom());
                let proj_v = Point::new(shell.left(), sy);
                let proj_v2 = Point::new(shell.right(), sy);
                for p in [proj_h, proj_h2, proj_v, proj_v2] {
                    if p.x >= x_lo && p.x <= x_hi && p.y >= y_lo && p.y <= y_hi {
                        g.alloc_vertex(ResourceVertexKind::ChannelJunction, p, None, None);
                    }
                }
            }
        }

        // 轴向可见连边
        let ids: Vec<ResourceVertexId> = g.vertices.keys().copied().collect();
        let positions: HashMap<ResourceVertexId, Point> = g
            .vertices
            .iter()
            .map(|(id, v)| (*id, v.position))
            .collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let a = ids[i];
                let b = ids[j];
                let pa = positions[&a];
                let pb = positions[&b];
                let horiz = (pa.y - pb.y).abs() < EPS;
                let vert = (pa.x - pb.x).abs() < EPS;
                if !horiz && !vert {
                    continue;
                }
                if !segment_clear(pa, pb, node_obstacles) {
                    continue;
                }
                // 组内部：非成员组硬阻断（H2）
                if !group_segment_clear(pa, pb, group_rects, from_id, to_id, group_ctx) {
                    continue;
                }
                g.try_add_axis_edge(a, b, DEFAULT_CAPACITY);
                g.try_add_axis_edge(b, a, DEFAULT_CAPACITY);
            }
        }

        (g, start, end)
    }
}

fn port_midpoint(nl: &NodeLayout, port: Port) -> Point {
    match port {
        Port::Top => Point {
            x: nl.x + nl.width * 0.5,
            y: nl.y,
        },
        Port::Bottom => Point {
            x: nl.x + nl.width * 0.5,
            y: nl.y + nl.height,
        },
        Port::Left => Point {
            x: nl.x,
            y: nl.y + nl.height * 0.5,
        },
        Port::Right => Point {
            x: nl.x + nl.width,
            y: nl.y + nl.height * 0.5,
        },
    }
}

fn segment_clear(a: Point, b: Point, obstacles: &[Rect]) -> bool {
    // 用内部重叠长度：对边端点落在左右/上下边界时，`segment_crosses_interior` 会漏检。
    for rect in obstacles {
        if rect.segment_interior_overlap_length(a, b, VIS_EPS) > 0.0 {
            return false;
        }
    }
    true
}

fn group_segment_clear(
    a: Point,
    b: Point,
    group_rects: &[(String, Rect)],
    from_id: &str,
    to_id: &str,
    group_ctx: Option<&GroupRoutingContext>,
) -> bool {
    let Some(ctx) = group_ctx else {
        return true;
    };
    let endpoint_groups = ctx.endpoint_group_set(from_id, to_id);
    for (gid, rect) in group_rects {
        if endpoint_groups.contains(gid.as_str()) {
            continue;
        }
        // 与 lint `edge_crosses_group_interior` / `path_avoids_group_interiors` 对齐
        if let Some(gl) = ctx.groups.get(gid) {
            if crate::layout::routing::common::geom_obstacle::segment_pierces_group_interior(a, b, gl)
            {
                return false;
            }
        } else if rect.segment_interior_overlap_length(a, b, VIS_EPS) > 0.0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_anchors_are_deterministic() {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "b".into(),
            NodeLayout {
                x: 100.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            },
        );
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            },
        );
        let mut g1 = ResourceGraph::new();
        g1.add_port_anchors_from_nodes(&nodes);
        let mut g2 = ResourceGraph::new();
        g2.add_port_anchors_from_nodes(&nodes);
        assert_eq!(g1.vertex_count(), g2.vertex_count());
    }

    #[test]
    fn build_for_query_connects_clear_l_path() {
        let from = Point::new(20.0, 0.0);
        let to = Point::new(100.0, 80.0);
        let obstacles = [Rect::new(40.0, 20.0, 30.0, 30.0)];
        let (g, s, e) = ResourceGraph::build_for_query(
            from,
            to,
            Port::Bottom,
            Port::Top,
            "a",
            "b",
            &obstacles,
            &[],
            None,
        );
        assert!(g.vertex_count() >= 2);
        assert!(g.vertex(s).is_some());
        assert!(g.vertex(e).is_some());
        assert!(g.edge_count() > 0);
    }
}
