//! 标签候选位共享辅助函数。
//!
//! 这些函数原本位于 `label_candidate.rs`，被多个标签放置阶段复用。

use crate::layout::constants::DEFAULT_LABEL_PERP_OFFSET;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, NodeLayout};
use std::collections::HashMap;

type LabelKey = (usize, usize);

pub(super) fn collect_label_keys(edges: &[EdgeLayout]) -> Vec<LabelKey> {
    edges
        .iter()
        .enumerate()
        .flat_map(|(i, e)| {
            if e.path_len() < 2 {
                Vec::new()
            } else {
                (0..e.labels.len()).map(move |li| (i, li)).collect()
            }
        })
        .collect()
}

pub(super) fn sorted_node_obstacles(
    nodes: &HashMap<String, NodeLayout>,
) -> Vec<(f64, f64, f64, f64)> {
    let mut ids: Vec<&String> = nodes.keys().collect();
    ids.sort();
    let m = DEFAULT_LABEL_PERP_OFFSET;
    ids.into_iter()
        .map(|id| {
            let nl = &nodes[id];
            // 外扩法向偏置量：贴边也视为冲突，触发 Phase 1 候选偏置
            (nl.x - m, nl.y - m, nl.x + nl.width + m, nl.y + nl.height + m)
        })
        .collect()
}

pub(super) fn build_edge_segments(edges: &[EdgeLayout]) -> Vec<Vec<(Point, Point)>> {
    edges
        .iter()
        .map(|e| {
            if e.path_len() < 2 {
                return Vec::new();
            }
            let path = e.path_points().into_owned();
            path.windows(2).map(|w| (w[0], w[1])).collect()
        })
        .collect()
}
