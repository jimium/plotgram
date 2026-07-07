//! 平行边分组与偏移计算
//!
//! 同一对节点间的多条边需要分组并分配偏移量，避免视觉重叠。

use crate::layout::constants::DEFAULT_EDGE_OFFSET;
use super::edge_geometry::{canonical_pair, undirected_pair_key};

/// 平行边分组结果
///
/// 仅保留每条边的偏移量；分组内部信息不对外暴露
/// （orthogonal / circular 路由器各自维护分组逻辑，需求不同）。
pub struct ParallelGroups {
    /// 每条边的垂直偏移量
    pub offsets: Vec<f64>,
}

/// 对平行边进行分组并计算偏移量
///
/// 返回每条边的偏移量。同一对节点间的多条边围绕基线对称分布；
/// 正反向边分别落在法线两侧。
pub fn group_parallel_edges(
    relations: &[crate::ast::Relation],
    edge_offset: f64,
) -> ParallelGroups {
    let n = relations.len();
    let mut pair_groups: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }

    let mut offsets = vec![0.0; n];
    for indices in pair_groups.values() {
        if indices.len() == 1 {
            continue;
        }

        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());

        let mut forward = Vec::new();
        let mut backward = Vec::new();
        for &i in indices {
            let rel = &relations[i];
            if rel.from.as_str() == can_from && rel.to.as_str() == can_to {
                forward.push(i);
            } else {
                backward.push(i);
            }
        }

        if !forward.is_empty() && !backward.is_empty() {
            distribute_offsets(&mut offsets, &forward, edge_offset / 2.0);
            distribute_offsets(&mut offsets, &backward, -edge_offset / 2.0);
        } else {
            distribute_offsets(&mut offsets, indices, 0.0);
        }
    }

    ParallelGroups { offsets }
}

/// 基于一组边索引分配偏移量，围绕 base 值居中展开
pub fn distribute_offsets(offsets: &mut [f64], indices: &[usize], base: f64) {
    let n = indices.len();
    for (j, &i) in indices.iter().enumerate() {
        let centered = j as f64 - (n - 1) as f64 / 2.0;
        offsets[i] = base + centered * DEFAULT_EDGE_OFFSET;
    }
}
