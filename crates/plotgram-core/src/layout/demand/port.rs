//! 端口侧压力：按几何偏好出边侧聚合度数（不跑 stub 去冲突）。

use crate::ast::Relation;
use crate::layout::{NodeLayout, Port};
use std::collections::{BTreeMap, HashMap};

use super::types::PortPressure;

/// 由端点相对位置推断 from 端偏好出边侧。
pub fn preferred_exit_side(from: &NodeLayout, to: &NodeLayout) -> Port {
    let fx = from.x + from.width * 0.5;
    let fy = from.y + from.height * 0.5;
    let tx = to.x + to.width * 0.5;
    let ty = to.y + to.height * 0.5;
    let dx = tx - fx;
    let dy = ty - fy;
    if dy.abs() >= dx.abs() {
        if dy >= 0.0 {
            Port::Bottom
        } else {
            Port::Top
        }
    } else if dx >= 0.0 {
        Port::Right
    } else {
        Port::Left
    }
}

fn preferred_entry_side(from: &NodeLayout, to: &NodeLayout) -> Port {
    match preferred_exit_side(from, to) {
        Port::Top => Port::Bottom,
        Port::Bottom => Port::Top,
        Port::Left => Port::Right,
        Port::Right => Port::Left,
    }
}

/// 按 `(node, side)` 聚合出入边偏好侧计数（确定性：BTreeMap）。
pub fn aggregate_port_pressure(
    nodes: &HashMap<String, NodeLayout>,
    relations: &[Relation],
) -> Vec<PortPressure> {
    let mut counts: BTreeMap<(String, Port), usize> = BTreeMap::new();
    for rel in relations {
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        if from_id == to_id {
            continue;
        }
        let (Some(fnl), Some(tnl)) = (nodes.get(from_id), nodes.get(to_id)) else {
            continue;
        };
        let out_side = preferred_exit_side(fnl, tnl);
        *counts
            .entry((from_id.to_string(), out_side))
            .or_insert(0) += 1;
        let in_side = preferred_entry_side(fnl, tnl);
        *counts.entry((to_id.to_string(), in_side)).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|((node_id, side), count)| PortPressure {
            node_id,
            side,
            count,
        })
        .collect()
}
