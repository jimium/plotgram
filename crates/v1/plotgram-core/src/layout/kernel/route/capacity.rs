//! H4 端口侧容量 / H5 边间距：ResourceId 与约束编译（Phase 3）。

use super::graph::ResourceId;
use super::model::{
    ConstraintSource, ConstraintSourceKind, EdgeId, RouteHardConstraint,
};
use crate::layout::geometry::Point;
use crate::layout::types::{EdgeLayout, Port};
use std::collections::BTreeMap;

/// 同节点同侧默认容量（对齐旧 `PORT_CAPACITY_PER_SIDE`）。
pub const PORT_SIDE_CAPACITY: u32 = 4;

/// FNV-1a 64：跨平台确定性（不用 DefaultHasher）。
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// 稳定：同一 `(node_id, port)` → 同一 `ResourceId`。
pub fn port_side_resource_id(node_id: &str, port: Port) -> ResourceId {
    let mut buf = Vec::with_capacity(4 + node_id.len() + 1);
    buf.extend_from_slice(b"PORT");
    buf.extend_from_slice(node_id.as_bytes());
    buf.push(port_to_id(port));
    ResourceId(fnv1a64(&buf))
}

pub fn port_to_id(p: Port) -> u8 {
    match p {
        Port::Top => 0,
        Port::Right => 1,
        Port::Bottom => 2,
        Port::Left => 3,
    }
}

pub fn id_to_port(id: u8) -> Port {
    match id % 4 {
        0 => Port::Top,
        1 => Port::Right,
        2 => Port::Bottom,
        _ => Port::Left,
    }
}

/// 四向候选（决策域全集）。
pub fn all_port_candidates() -> Vec<u8> {
    vec![0, 1, 2, 3]
}

/// 为图中出现的每个 `(node, side)` 编译 H4 `ResourceCapacity`。
pub fn compile_port_capacity_constraints(
    edge_ports: &[(EdgeId, &str, Port, &str, Port)],
) -> Vec<RouteHardConstraint> {
    let mut seen: BTreeMap<(String, u8), ()> = BTreeMap::new();
    let mut hard = Vec::new();
    for &(_, from_n, from_p, to_n, to_p) in edge_ports {
        for (nid, port) in [(from_n, from_p), (to_n, to_p)] {
            let key = (nid.to_string(), port_to_id(port));
            if seen.insert(key.clone(), ()).is_some() {
                continue;
            }
            let resource = port_side_resource_id(nid, port);
            hard.push(RouteHardConstraint::ResourceCapacity {
                resource,
                capacity: PORT_SIDE_CAPACITY,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::ResourceCapacity,
                    entities: vec![nid.to_string()],
                    note: "H4 port-side capacity",
                },
            });
        }
    }
    hard
}

/// 平行/反向边对 → H5 `MinSeparation`。
pub fn compile_min_separation_constraints(
    pairs: &[(EdgeId, EdgeId)],
    distance: f64,
) -> Vec<RouteHardConstraint> {
    let mut hard = Vec::with_capacity(pairs.len());
    for &(a, b) in pairs {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        hard.push(RouteHardConstraint::MinSeparation {
            a: lo,
            b: hi,
            distance,
            source: ConstraintSource {
                kind: ConstraintSourceKind::EdgeSeparation,
                entities: vec![format!("e{lo}"), format!("e{hi}")],
                note: "H5 parallel/reverse min_gap",
            },
        });
    }
    hard
}

/// 统计每端口侧占用；返回超额边索引列表 `(edge_index, node, port, count)`。
pub fn port_capacity_overloads(
    edges: &[EdgeLayout],
    from_nodes: &[&str],
    to_nodes: &[&str],
    capacity: u32,
) -> Vec<(usize, String, Port, u32)> {
    // (node, port_id) → 边索引列表
    let mut occupancy: BTreeMap<(String, u8), Vec<usize>> = BTreeMap::new();
    let n = edges.len().min(from_nodes.len()).min(to_nodes.len());
    for i in 0..n {
        if edges[i].path_is_empty() {
            continue;
        }
        let fp = edges[i].from_port;
        let tp = edges[i].to_port;
        occupancy
            .entry((from_nodes[i].to_string(), port_to_id(fp)))
            .or_default()
            .push(i);
        occupancy
            .entry((to_nodes[i].to_string(), port_to_id(tp)))
            .or_default()
            .push(i);
    }
    let mut over = Vec::new();
    for ((node, pid), members) in occupancy {
        let count = members.len() as u32;
        if count > capacity {
            let port = id_to_port(pid);
            for &ei in &members {
                over.push((ei, node.clone(), port, count));
            }
        }
    }
    over
}

/// 两折线是否存在平行段间距 < `min_gap`（粗检，供 H5 审计）。
pub fn paths_violate_min_separation(a: &[Point], b: &[Point], min_gap: f64) -> bool {
    const EPS: f64 = 0.5;
    if a.len() < 2 || b.len() < 2 {
        return false;
    }
    for i in 0..a.len() - 1 {
        let (a0, a1) = (a[i], a[i + 1]);
        let a_h = (a0.y - a1.y).abs() < EPS;
        let a_v = (a0.x - a1.x).abs() < EPS;
        if !a_h && !a_v {
            continue;
        }
        for j in 0..b.len() - 1 {
            let (b0, b1) = (b[j], b[j + 1]);
            let b_h = (b0.y - b1.y).abs() < EPS;
            let b_v = (b0.x - b1.x).abs() < EPS;
            if a_h && b_h {
                let gap = (a0.y - b0.y).abs();
                if gap + EPS < min_gap {
                    let t0 = a0.x.min(a1.x);
                    let t1 = a0.x.max(a1.x);
                    let u0 = b0.x.min(b1.x);
                    let u1 = b0.x.max(b1.x);
                    if t1.min(u1) - t0.max(u0) > EPS {
                        return true;
                    }
                }
            } else if a_v && b_v {
                let gap = (a0.x - b0.x).abs();
                if gap + EPS < min_gap {
                    let t0 = a0.y.min(a1.y);
                    let t1 = a0.y.max(a1.y);
                    let u0 = b0.y.min(b1.y);
                    let u1 = b0.y.max(b1.y);
                    if t1.min(u1) - t0.max(u0) > EPS {
                        return true;
                    }
                }
            }
        }
    }
    false
}
