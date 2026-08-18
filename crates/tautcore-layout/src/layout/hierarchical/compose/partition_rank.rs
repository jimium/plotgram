//! Main-axis partition rank intervals (partition-grid.md PG-3).
//!
//! Network Simplex is unchanged. After NS, assigned nodes are packed into
//! disjoint contiguous rank intervals in **axis declaration order**.
//! Interleaving or a clamp that would reverse a working edge fails hard.

use std::collections::BTreeMap;

use tautcore_engine_api::LayoutError;
use tautcore_model::partition::PartitionCell;

use super::partition_axes::{cell_on, ConsumedAxes};
use super::rank::RankMap;
use crate::layout::hierarchical::model::{ElemKey, PlanGraph, RealGraph};

/// Packed main-axis rank intervals after clamp (declaration order).
#[derive(Debug, Clone)]
pub struct PartitionMainRankPlan {
    pub ids: Vec<String>,
    /// Inclusive `[lo, hi]` per id. Empty axes occupy a dedicated dummy rank
    /// (`lo == hi`) so trailing empty bands still exist after properify.
    pub intervals: Vec<(u32, u32)>,
}

/// Clamp `ranks` into disjoint main-axis intervals. `None` when the consumed
/// main axis is empty (column-only TB, or no grid) — ranks are untouched.
pub fn clamp_partition_main_ranks(
    graph: &RealGraph,
    ranks: &mut RankMap,
    axes: &ConsumedAxes,
) -> Result<Option<PartitionMainRankPlan>, LayoutError> {
    if axes.main_ids.is_empty() {
        return Ok(None);
    }
    let n = graph.ids.len();
    if n == 0 {
        let intervals = (0..axes.main_ids.len() as u32).map(|i| (i, i)).collect();
        return Ok(Some(PartitionMainRankPlan {
            ids: axes.main_ids.clone(),
            intervals,
        }));
    }

    let index_of: BTreeMap<&str, usize> = axes
        .main_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let node_axis: Vec<Option<usize>> = (0..n)
        .map(|i| {
            graph.partition_cell.get(i).and_then(|c| {
                c.as_ref().and_then(|cell| {
                    cell_on(cell, axes.main).and_then(|id| index_of.get(id).copied())
                })
            })
        })
        .collect();

    let mut assigned: Vec<usize> = (0..n).filter(|&i| node_axis[i].is_some()).collect();
    assigned.sort_by_key(|&i| (ranks[i], i));

    let n_axes = axes.main_ids.len();
    let mut first_pos = vec![None; n_axes];
    let mut last_pos = vec![None; n_axes];
    for (pos, &i) in assigned.iter().enumerate() {
        let ai = node_axis[i].expect("assigned");
        if first_pos[ai].is_none() {
            first_pos[ai] = Some(pos);
        }
        last_pos[ai] = Some(pos);
    }
    for i in 0..n_axes {
        let (Some(fi), Some(li)) = (first_pos[i], last_pos[i]) else {
            continue;
        };
        for j in (i + 1)..n_axes {
            let (Some(fj), Some(lj)) = (first_pos[j], last_pos[j]) else {
                continue;
            };
            if !(li < fj || lj < fi) {
                return Err(LayoutError::message(format!(
                    "hierarchical: partition {} `{}` and `{}` interleave in rank order — \
                     cannot form disjoint rank intervals (partition-grid.md PG-3)",
                    axes.main.as_noun(),
                    axes.main_ids[i],
                    axes.main_ids[j]
                )));
            }
        }
    }

    let mut new_rank = vec![0u32; n];
    let mut next = 0u32;
    let mut intervals = vec![(0u32, 0u32); n_axes];
    for ai in 0..n_axes {
        let mut members: Vec<usize> = (0..n).filter(|&i| node_axis[i] == Some(ai)).collect();
        if members.is_empty() {
            intervals[ai] = (next, next);
            next += 1;
            continue;
        }
        members.sort_by_key(|&i| (ranks[i], i));
        let mut unique_old: Vec<u32> = members.iter().map(|&i| ranks[i]).collect();
        unique_old.sort_unstable();
        unique_old.dedup();
        let lo = next;
        for &i in &members {
            let offset = unique_old.binary_search(&ranks[i]).unwrap() as u32;
            new_rank[i] = lo + offset;
        }
        let hi = lo + unique_old.len() as u32 - 1;
        intervals[ai] = (lo, hi);
        next = hi + 1;
    }

    let mut old_to_new: BTreeMap<u32, u32> = BTreeMap::new();
    for i in 0..n {
        if node_axis[i].is_some() {
            old_to_new
                .entry(ranks[i])
                .and_modify(|v| *v = (*v).min(new_rank[i]))
                .or_insert(new_rank[i]);
        }
    }
    for i in 0..n {
        if node_axis[i].is_some() {
            continue;
        }
        let old = ranks[i];
        new_rank[i] = if let Some((_, &nr)) = old_to_new.range(..=old).next_back() {
            nr
        } else if let Some((_, &nr)) = old_to_new.iter().next() {
            nr
        } else {
            0
        };
    }

    for e in &graph.edges {
        if e.undirected {
            continue;
        }
        if new_rank[e.working_target] <= new_rank[e.working_source] {
            return Err(LayoutError::message(format!(
                "hierarchical: partition {} clamp would reverse working edge `{}` \
                 ({} -> {}) (partition-grid.md PG-3)",
                axes.main.as_noun(),
                e.edge_id,
                graph.ids[e.working_source],
                graph.ids[e.working_target]
            )));
        }
    }

    *ranks = new_rank;
    Ok(Some(PartitionMainRankPlan {
        ids: axes.main_ids.clone(),
        intervals,
    }))
}

/// Stamp main-axis ownership onto the properified plan and extend layers so
/// trailing empty-axis dummy ranks survive (properify sizes layers by max
/// real/virtual rank).
pub fn apply_main_rank_plan(
    plan: &mut PlanGraph,
    real_graph: &RealGraph,
    axes: &ConsumedAxes,
    rp: &PartitionMainRankPlan,
) {
    plan.partition_rows = rp.ids.clone();
    plan.partition_row_intervals = rp.intervals.clone();
    plan.partition_main_kind = axes.main;
    let index_of: BTreeMap<&str, usize> = rp
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    plan.partition_elem_row = vec![None; plan.elems.len()];
    for (ei, elem) in plan.elems.iter().enumerate() {
        let ElemKey::Real(id) = &elem.key else {
            continue;
        };
        let Some(&di) = real_graph.index_of.get(id) else {
            continue;
        };
        let Some(cell) = real_graph.partition_cell.get(di).and_then(|c| c.as_ref()) else {
            continue;
        };
        if let Some(axis_id) = cell_on(cell, axes.main) {
            plan.partition_elem_row[ei] = index_of.get(axis_id).copied();
        }
    }
    let need = rp
        .intervals
        .iter()
        .map(|&(_, hi)| hi as usize + 1)
        .max()
        .unwrap_or(0);
    if plan.layers.len() < need {
        plan.layers.resize(need, Vec::new());
    }
}

/// Stamp cross/main ownership without inserting clamps (StrongMacro expand:
/// MacroBlockWriter already wrote geometry; bands are Fit envelopes).
pub fn stamp_partition_ownership(
    plan: &mut PlanGraph,
    real_graph: &RealGraph,
    axes: &ConsumedAxes,
) {
    plan.partition_cross_kind = axes.cross;
    plan.partition_columns = axes.cross_ids.clone();
    plan.partition_main_kind = axes.main;
    plan.partition_rows = axes.main_ids.clone();

    let col_index: BTreeMap<&str, usize> = axes
        .cross_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let row_index: BTreeMap<&str, usize> = axes
        .main_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    plan.partition_elem_col = vec![None; plan.elems.len()];
    plan.partition_elem_row = vec![None; plan.elems.len()];
    let mut row_lo = vec![u32::MAX; axes.main_ids.len()];
    let mut row_hi = vec![0u32; axes.main_ids.len()];
    let mut row_hit = vec![false; axes.main_ids.len()];
    for (ei, elem) in plan.elems.iter().enumerate() {
        let ElemKey::Real(id) = &elem.key else {
            continue;
        };
        let Some(&di) = real_graph.index_of.get(id) else {
            continue;
        };
        let cell: Option<&PartitionCell> =
            real_graph.partition_cell.get(di).and_then(|c| c.as_ref());
        if let Some(cell) = cell {
            if let Some(id) = cell_on(cell, axes.cross) {
                plan.partition_elem_col[ei] = col_index.get(id).copied();
            }
            if let Some(id) = cell_on(cell, axes.main) {
                if let Some(&ri) = row_index.get(id) {
                    plan.partition_elem_row[ei] = Some(ri);
                    row_hit[ri] = true;
                    row_lo[ri] = row_lo[ri].min(elem.rank);
                    row_hi[ri] = row_hi[ri].max(elem.rank);
                }
            }
        }
    }
    plan.partition_row_intervals = (0..axes.main_ids.len())
        .map(|ri| {
            if row_hit[ri] {
                (row_lo[ri], row_hi[ri])
            } else {
                (0, 0)
            }
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::model::RealEdge;
    use tautcore_algo::orientation::Orientation;
    use tautcore_model::partition::{PartitionAxis, PartitionGrid};

    fn graph(n: usize, edges: &[(usize, usize)]) -> RealGraph {
        let ids: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
        let index_of: BTreeMap<String, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let edges = edges
            .iter()
            .enumerate()
            .map(|(i, &(s, t))| RealEdge {
                edge_id: format!("e{i}"),
                original_source: s,
                original_target: t,
                working_source: s,
                working_target: t,
                reversed: false,
                from_port: None,
                to_port: None,
                weight: 1.0,
                ..Default::default()
            })
            .collect();
        RealGraph {
            ids,
            index_of,
            group_path: vec![Vec::new(); n],
            shapes: vec![tautcore_model::NodeShape::DEFAULT; n],
            edges,
            self_loops: Vec::new(),
            partition_cell: vec![None; n],
            ..Default::default()
        }
    }

    fn tb_axes(g: &RealGraph) -> ConsumedAxes {
        ConsumedAxes::from_grid(g.partition.as_ref().unwrap(), Orientation::Tb)
    }

    #[test]
    fn no_rows_is_noop() {
        let g = graph(2, &[(0, 1)]);
        let mut ranks = vec![0, 1];
        let axes = ConsumedAxes {
            cross: crate::layout::hierarchical::model::PartitionAxisKind::Columns,
            main: crate::layout::hierarchical::model::PartitionAxisKind::Rows,
            cross_ids: vec!["c".into()],
            main_ids: vec![],
        };
        let out = clamp_partition_main_ranks(&g, &mut ranks, &axes).unwrap();
        assert!(out.is_none());
        assert_eq!(ranks, vec![0, 1]);
    }

    #[test]
    fn two_row_chain_packs_in_declaration_order() {
        let mut g = graph(3, &[(0, 1), (1, 2)]);
        g.partition = Some(PartitionGrid {
            columns: vec![],
            rows: vec![
                PartitionAxis::new("intake"),
                PartitionAxis::new("build"),
                PartitionAxis::new("release"),
            ],
        });
        g.partition_cell = vec![
            Some(PartitionCell::row("intake")),
            Some(PartitionCell::row("build")),
            Some(PartitionCell::row("release")),
        ];
        let mut ranks = vec![0, 1, 2];
        let axes = tb_axes(&g);
        let plan = clamp_partition_main_ranks(&g, &mut ranks, &axes)
            .unwrap()
            .unwrap();
        assert_eq!(ranks, vec![0, 1, 2]);
        assert_eq!(plan.intervals, vec![(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn empty_middle_row_gets_a_dummy_rank() {
        let mut g = graph(2, &[(0, 1)]);
        g.partition = Some(PartitionGrid {
            columns: vec![],
            rows: vec![
                PartitionAxis::new("intake"),
                PartitionAxis::new("idle"),
                PartitionAxis::new("release"),
            ],
        });
        g.partition_cell = vec![
            Some(PartitionCell::row("intake")),
            Some(PartitionCell::row("release")),
        ];
        let mut ranks = vec![0, 1];
        let axes = tb_axes(&g);
        let plan = clamp_partition_main_ranks(&g, &mut ranks, &axes)
            .unwrap()
            .unwrap();
        assert_eq!(ranks, vec![0, 2]);
        assert_eq!(plan.intervals, vec![(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn interleaved_rows_fail_hard() {
        let mut g = graph(3, &[]);
        g.partition = Some(PartitionGrid {
            columns: vec![],
            rows: vec![PartitionAxis::new("a"), PartitionAxis::new("b")],
        });
        // NS order: a, b, a → interleave.
        g.partition_cell = vec![
            Some(PartitionCell::row("a")),
            Some(PartitionCell::row("b")),
            Some(PartitionCell::row("a")),
        ];
        let mut ranks = vec![0, 1, 2];
        let axes = tb_axes(&g);
        let err = clamp_partition_main_ranks(&g, &mut ranks, &axes).unwrap_err();
        assert!(err.to_string().contains("interleave"), "unexpected: {err}");
    }

    #[test]
    fn clamp_that_would_reverse_a_working_edge_fails() {
        let mut g = graph(2, &[(0, 1)]);
        g.partition = Some(PartitionGrid {
            columns: vec![],
            rows: vec![PartitionAxis::new("a"), PartitionAxis::new("b")],
        });
        // n0 in row b, n1 in row a, edge n0→n1. Packing declaration order
        // puts a before b, reversing the working edge.
        g.partition_cell = vec![Some(PartitionCell::row("b")), Some(PartitionCell::row("a"))];
        let mut ranks = vec![0, 1];
        let axes = tb_axes(&g);
        let err = clamp_partition_main_ranks(&g, &mut ranks, &axes).unwrap_err();
        assert!(
            err.to_string().contains("reverse working edge"),
            "unexpected: {err}"
        );
    }
}
