//! 障碍穿透 proxy：最短正交两段 L 骨架 + 线段×节点 primitive。

use crate::layout::geometry::Point;
use crate::layout::quality::refine::segment_intersects_node;
use crate::layout::NodeLayout;
use std::collections::HashMap;

/// 两条候选 L 骨架（先 H 后 V / 先 V 后 H）。
#[derive(Debug, Clone, Copy)]
pub struct LSkeleton {
    pub mid: Point,
    pub horizontal_first: bool,
}

impl LSkeleton {
    pub fn segments(&self, from: Point, to: Point) -> [(Point, Point); 2] {
        [(from, self.mid), (self.mid, to)]
    }
}

/// 端点中心之间的两条 L 骨架。
pub fn l_skeletons(from: Point, to: Point) -> [LSkeleton; 2] {
    [
        LSkeleton {
            mid: Point::new(to.x, from.y),
            horizontal_first: true,
        },
        LSkeleton {
            mid: Point::new(from.x, to.y),
            horizontal_first: false,
        },
    ]
}

fn node_center(nl: &NodeLayout) -> Point {
    Point::new(nl.x + nl.width * 0.5, nl.y + nl.height * 0.5)
}

/// 单条 L 骨架命中的非端点节点数。
pub fn count_skeleton_node_hits(
    from: Point,
    to: Point,
    sk: &LSkeleton,
    nodes: &HashMap<String, NodeLayout>,
    skip_ids: &[&str],
) -> usize {
    let segs = sk.segments(from, to);
    let mut hits = 0usize;
    let mut seen: Vec<&str> = Vec::new();
    for (id, nl) in nodes {
        if skip_ids.iter().any(|s| *s == id.as_str()) {
            continue;
        }
        let hit = segs
            .iter()
            .any(|(a, b)| segment_intersects_node(*a, *b, nl));
        if hit {
            hits += 1;
            seen.push(id.as_str());
        }
    }
    let _ = seen;
    hits
}

/// `obstacle_hits = max(两候选 L 的 hits)`（方案定死）。
pub fn obstacle_hits_for_edge(
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
) -> usize {
    let (Some(fnl), Some(tnl)) = (nodes.get(from_id), nodes.get(to_id)) else {
        return 0;
    };
    let from = node_center(fnl);
    let to = node_center(tnl);
    if (from.x - to.x).abs() < 1e-6 && (from.y - to.y).abs() < 1e-6 {
        return 0;
    }
    let skip = [from_id, to_id];
    let sks = l_skeletons(from, to);
    let h0 = count_skeleton_node_hits(from, to, &sks[0], nodes, &skip);
    let h1 = count_skeleton_node_hits(from, to, &sks[1], nodes, &skip);
    h0.max(h1)
}

/// 选 hits 更少的 L（平局取 HV）；供网格累加单次贡献。
pub fn preferred_l_skeleton(
    from_id: &str,
    to_id: &str,
    nodes: &HashMap<String, NodeLayout>,
) -> Option<(Point, Point, LSkeleton)> {
    let fnl = nodes.get(from_id)?;
    let tnl = nodes.get(to_id)?;
    let from = node_center(fnl);
    let to = node_center(tnl);
    let skip = [from_id, to_id];
    let sks = l_skeletons(from, to);
    let h0 = count_skeleton_node_hits(from, to, &sks[0], nodes, &skip);
    let h1 = count_skeleton_node_hits(from, to, &sks[1], nodes, &skip);
    let sk = if h1 < h0 { sks[1] } else { sks[0] };
    Some((from, to, sk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_when_middle_node_blocks_l() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
            },
        );
        nodes.insert(
            "mid".into(),
            NodeLayout {
                x: 80.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
            },
        );
        nodes.insert(
            "b".into(),
            NodeLayout {
                x: 160.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
            },
        );
        // a→b 共线，HV/VH 水平段都会穿过 mid
        let hits = obstacle_hits_for_edge("a", "b", &nodes);
        assert!(hits >= 1, "hits={hits}");
    }

    #[test]
    fn zero_hits_when_clear() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
            },
        );
        nodes.insert(
            "b".into(),
            NodeLayout {
                x: 0.0,
                y: 200.0,
                width: 40.0,
                height: 40.0,
            },
        );
        nodes.insert(
            "side".into(),
            NodeLayout {
                x: 200.0,
                y: 100.0,
                width: 40.0,
                height: 40.0,
            },
        );
        assert_eq!(obstacle_hits_for_edge("a", "b", &nodes), 0);
    }
}
