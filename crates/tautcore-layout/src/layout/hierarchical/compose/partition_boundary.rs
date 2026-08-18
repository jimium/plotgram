//! Partition column boundary clamps (partition-grid.md PG-1).
//!
//! After [`super::boundary::insert_group_boundaries`] and before
//! [`super::order::order_layers`], give every declared column a zero-width
//! Left/Right clamp on **every** rank (columns are global full-height bands,
//! unlike groups which span only their member ranks), rebuild each layer as
//! `[free… col₁_L …col₁… col₁_R col₂_L …col₂… col₂_R …free…]` in column
//! declaration order, and wire high-weight `pb:{column}:L/R` cross-rank
//! segments (same mechanism as `gb:` in [`super::boundary`]).
//!
//! Group coexistence (PG-1 boundary clause): a group whose members all sit
//! in one column nests its block inside that column block; a group spanning
//! ≥2 columns is a hard error — no joint group×partition precedence yet
//! (PG-4 territory). Unassigned nodes and virtual dummies stay in the free
//! zones outside the blocks; ordering sweeps decide their side, and
//! [`super::order`] restores enclosure after each sweep.

use std::collections::BTreeMap;

use super::super::model::{BoundarySide, Elem, ElemKey, PlanGraph, RealGraph, Segment};
use super::partition_axes::{cell_on, ConsumedAxes};
use tautcore_engine_api::LayoutError;

/// TB convenience for tests: consume author columns as the cross axis.
#[cfg(test)]
pub fn insert_partition_boundaries(
    plan: &mut PlanGraph,
    real_graph: &RealGraph,
) -> Result<(), LayoutError> {
    use tautcore_algo::orientation::Orientation;
    let Some(grid) = &real_graph.partition else {
        return Ok(());
    };
    insert_partition_cross_boundaries(
        plan,
        real_graph,
        &ConsumedAxes::from_grid(grid, Orientation::Tb),
    )
}

/// Insert cross-axis clamps + cross-rank `pb:` segments into `plan`.
///
/// No-op when `axes.cross_ids` is empty (the §10 single gate). Errors when a
/// group's members span more than one cross-axis cell.
pub fn insert_partition_cross_boundaries(
    plan: &mut PlanGraph,
    real_graph: &RealGraph,
    axes: &ConsumedAxes,
) -> Result<(), LayoutError> {
    if !plan.partition_columns.is_empty() {
        return Ok(()); // already inserted (retry path re-runs compose)
    }
    if axes.cross_ids.is_empty() {
        return Ok(());
    }
    let columns = axes.cross_ids.clone();
    let col_index: BTreeMap<&str, usize> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), i))
        .collect();

    // dense real index -> cross-axis index (validated at layout entry).
    let dense_col: Vec<Option<usize>> = real_graph
        .partition_cell
        .iter()
        .map(|cell| {
            cell.as_ref()
                .and_then(|c| cell_on(c, axes.cross))
                .map(|c| col_index[c])
        })
        .collect();
    let real_col = |ei: usize| -> Option<usize> {
        match &plan.elems[ei].key {
            ElemKey::Real(id) => dense_col[real_graph.index_of[id]],
            _ => None,
        }
    };

    // Group coexistence: every group's assigned members must agree on one
    // column. Read-only scan first — nothing is allocated before the check.
    let mut group_cols: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for &ei in plan.layers.iter().flatten() {
        if let Some(ci) = real_col(ei) {
            for g in &plan.elems[ei].group_path {
                let entry = group_cols.entry(g.clone()).or_default();
                if !entry.contains(&ci) {
                    entry.push(ci);
                }
            }
        }
    }
    for (g, cis) in &group_cols {
        if cis.len() >= 2 {
            let mut names: Vec<&String> = cis.iter().map(|ci| &columns[*ci]).collect();
            names.sort();
            return Err(LayoutError::message(format!(
                "hierarchical: group `{g}` members span partition {} {names:?} — \
                 partition consumption supports only single-{} groups \
                 (partition-grid.md §7 PG-1)",
                axes.cross.as_noun(),
                axes.cross.as_noun()
            )));
        }
    }
    let group_col_flat: BTreeMap<&str, usize> = group_cols
        .iter()
        .map(|(g, cis)| (g.as_str(), cis[0]))
        .collect();

    // Parse each layer into contiguous pieces BEFORE allocating anything:
    // group blocks are still contiguous here (boundary insert emitted them
    // contiguously and no sweep ran yet). A piece is (owning column, elems).
    let mut pieces_by_rank: Vec<Vec<(Option<usize>, Vec<usize>)>> =
        Vec::with_capacity(plan.layers.len());
    for layer in &plan.layers {
        let mut pieces: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
        // Nested open group blocks; the innermost collects, close pops.
        let mut open: Vec<Vec<usize>> = Vec::new();
        let push_single = |pieces: &mut Vec<_>, ei: usize| {
            let col = real_col(ei);
            pieces.push((col, vec![ei]));
        };
        for &ei in layer {
            match &plan.elems[ei].key {
                ElemKey::GroupBoundary {
                    side: BoundarySide::Left,
                    ..
                } => {
                    open.push(vec![ei]);
                }
                ElemKey::GroupBoundary {
                    group,
                    side: BoundarySide::Right,
                    ..
                } => match open.pop() {
                    Some(mut block) => {
                        block.push(ei);
                        match open.last_mut() {
                            Some(parent) => parent.append(&mut block),
                            None => {
                                let col = group_col_flat.get(group.as_str()).copied();
                                pieces.push((col, block));
                            }
                        }
                    }
                    // Stray Right clamp without an open Left — treat as a
                    // lone free elem (cannot happen after boundary insert).
                    None => push_single(&mut pieces, ei),
                },
                _ => match open.last_mut() {
                    Some(parent) => parent.push(ei),
                    None => push_single(&mut pieces, ei),
                },
            }
        }
        // Unclosed blocks (malformed) flush as pieces, outermost last.
        for block in open.drain(..) {
            pieces.push((None, block));
        }
        pieces_by_rank.push(pieces);
    }

    // Per-elem ownership consumed by `restore_partition_clamps`: assigned
    // reals + every elem of a single-column group block (clamps included).
    let mut partition_elem_col = vec![None; plan.elems.len()];
    for &ei in plan.layers.iter().flatten() {
        partition_elem_col[ei] = match &plan.elems[ei].key {
            ElemKey::Real(_) => real_col(ei),
            ElemKey::GroupBoundary { group, .. } => group_col_flat.get(group.as_str()).copied(),
            _ => None,
        };
    }

    // Alloc clamps: every column × every rank × both sides (full-height
    // bands — unlike groups, columns own clamps on ranks without members).
    let mut boundary_elem: BTreeMap<(usize, u32, BoundarySide), usize> = BTreeMap::new();
    let mut next_decl = plan.decl_index.iter().copied().max().unwrap_or(0) + 1;
    for rank in 0..plan.layers.len() {
        let rank_u = rank as u32;
        for (ci, column) in columns.iter().enumerate() {
            for side in [BoundarySide::Left, BoundarySide::Right] {
                let key = ElemKey::PartitionBoundary {
                    axis: column.clone(),
                    rank: rank_u,
                    side,
                };
                let idx = plan.elems.len();
                plan.elems.push(Elem {
                    key: key.clone(),
                    group_path: Vec::new(),
                    rank: rank_u,
                });
                plan.index_of.insert(key, idx);
                plan.decl_index.push(next_decl);
                next_decl += 1;
                boundary_elem.insert((ci, rank_u, side), idx);
                // Clamps are block markers, never owned content.
                partition_elem_col.push(None);
                plan.partition_elem_row.push(None);
            }
        }
    }

    // Rebuild each layer: declaration-order column blocks, free outside.
    for (rank, pieces) in pieces_by_rank.into_iter().enumerate() {
        let rank_u = rank as u32;
        let mut by_col: Vec<Vec<usize>> = vec![Vec::new(); columns.len()];
        let mut lead_free: Vec<usize> = Vec::new();
        let mut tail_free: Vec<usize> = Vec::new();
        let mut seen_col = false;
        for (col, elems) in pieces {
            match col {
                Some(ci) => {
                    seen_col = true;
                    by_col[ci].extend(elems);
                }
                None if !seen_col => lead_free.extend(elems),
                None => tail_free.extend(elems),
            }
        }
        let mut new_layer = Vec::with_capacity(plan.layers[rank].len() + columns.len() * 2);
        new_layer.extend(lead_free);
        for ci in 0..columns.len() {
            new_layer.push(boundary_elem[&(ci, rank_u, BoundarySide::Left)]);
            new_layer.extend(by_col[ci].iter().copied());
            new_layer.push(boundary_elem[&(ci, rank_u, BoundarySide::Right)]);
        }
        new_layer.extend(tail_free);
        plan.layers[rank] = new_layer;
    }

    // Cross-rank high-weight segments between matching (column, side) clamps
    // — columns exist on every rank, so every adjacent pair is wired.
    for rank in 0..plan.layers.len().saturating_sub(1) {
        let r0 = rank as u32;
        let r1 = (rank + 1) as u32;
        for (ci, column) in columns.iter().enumerate() {
            for side in [BoundarySide::Left, BoundarySide::Right] {
                let side_tag = match side {
                    BoundarySide::Left => "L",
                    BoundarySide::Right => "R",
                };
                plan.segments.push(Segment {
                    edge_id: format!("pb:{column}:{side_tag}"),
                    ordinal: r0,
                    from: boundary_elem[&(ci, r0, side)],
                    to: boundary_elem[&(ci, r1, side)],
                });
            }
        }
    }

    plan.partition_columns = columns;
    plan.partition_cross_kind = axes.cross;
    plan.partition_elem_col = partition_elem_col;
    Ok(())
}

/// StrongMacro / pre-flight: groups may not span ≥2 cross-axis cells.
pub fn check_groups_single_cross_cell(
    real_graph: &RealGraph,
    axes: &ConsumedAxes,
) -> Result<(), LayoutError> {
    if axes.cross_ids.is_empty() {
        return Ok(());
    }
    let col_index: BTreeMap<&str, usize> = axes
        .cross_ids
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), i))
        .collect();
    let mut group_cols: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, path) in real_graph.group_path.iter().enumerate() {
        let Some(cell) = real_graph.partition_cell.get(i).and_then(|c| c.as_ref()) else {
            continue;
        };
        let Some(id) = cell_on(cell, axes.cross) else {
            continue;
        };
        let Some(&ci) = col_index.get(id) else {
            continue;
        };
        for g in path {
            let entry = group_cols.entry(g.clone()).or_default();
            if !entry.contains(&ci) {
                entry.push(ci);
            }
        }
    }
    for (g, cis) in &group_cols {
        if cis.len() >= 2 {
            let mut names: Vec<&String> = cis.iter().map(|ci| &axes.cross_ids[*ci]).collect();
            names.sort();
            return Err(LayoutError::message(format!(
                "hierarchical: group `{g}` members span partition {} {names:?} — \
                 partition consumption supports only single-{} groups \
                 (partition-grid.md §7 PG-1)",
                axes.cross.as_noun(),
                axes.cross.as_noun()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tautcore_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

    /// One real node: (id, rank, group_path, cell column).
    struct Spec {
        id: &'static str,
        rank: u32,
        path: &'static [&'static str],
        col: Option<&'static str>,
    }

    fn fixture(columns: &[&str], specs: &[Spec]) -> (PlanGraph, RealGraph) {
        let mut real = RealGraph::default();
        real.partition = Some(PartitionGrid {
            columns: columns.iter().map(|c| PartitionAxis::new(*c)).collect(),
            rows: vec![],
        });
        let mut elems = Vec::new();
        for (i, s) in specs.iter().enumerate() {
            real.ids.push(s.id.to_string());
            real.index_of.insert(s.id.to_string(), i);
            real.partition_cell.push(s.col.map(PartitionCell::col));
            elems.push(Elem {
                key: ElemKey::Real(s.id.into()),
                group_path: s.path.iter().map(|g| (*g).to_string()).collect(),
                rank: s.rank,
            });
        }
        let max_rank = specs.iter().map(|s| s.rank).max().unwrap_or(0);
        let mut layers = vec![Vec::new(); max_rank as usize + 1];
        for (i, s) in specs.iter().enumerate() {
            layers[s.rank as usize].push(i);
        }
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let plan = PlanGraph {
            elems,
            index_of,
            decl_index: (0..specs.len()).collect(),
            segments: vec![],
            layers,
            ..Default::default()
        };
        (plan, real)
    }

    fn key_names(plan: &PlanGraph, layer: &[usize]) -> Vec<String> {
        layer
            .iter()
            .map(|&ei| match &plan.elems[ei].key {
                ElemKey::Real(id) => id.clone(),
                ElemKey::PartitionBoundary { axis, side, .. } => format!(
                    "pb:{axis}:{}",
                    match side {
                        BoundarySide::Left => "L",
                        BoundarySide::Right => "R",
                    }
                ),
                ElemKey::GroupBoundary { group, side, .. } => format!(
                    "gb:{group}:{}",
                    match side {
                        BoundarySide::Left => "L",
                        BoundarySide::Right => "R",
                    }
                ),
                _ => "?".to_string(),
            })
            .collect()
    }

    #[test]
    fn rebuilds_layers_in_column_declaration_order_with_free_zones() {
        // Declaration order: warehouse, customer (NOT alphabetical).
        let (mut plan, real) = fixture(
            &["warehouse", "customer"],
            &[
                Spec {
                    id: "pick",
                    rank: 0,
                    path: &[],
                    col: Some("warehouse"),
                },
                Spec {
                    id: "audit",
                    rank: 0,
                    path: &[],
                    col: None,
                },
                Spec {
                    id: "order",
                    rank: 0,
                    path: &[],
                    col: Some("customer"),
                },
                Spec {
                    id: "ship",
                    rank: 1,
                    path: &[],
                    col: Some("warehouse"),
                },
            ],
        );
        insert_partition_boundaries(&mut plan, &real).unwrap();

        assert_eq!(plan.partition_columns, vec!["warehouse", "customer"]);
        assert_eq!(
            key_names(&plan, &plan.layers[0]),
            // `audit` sat AFTER the first assigned elem → trail free zone;
            // blocks follow DECLARATION order, not member order.
            vec![
                "pb:warehouse:L",
                "pick",
                "pb:warehouse:R",
                "pb:customer:L",
                "order",
                "pb:customer:R",
                "audit",
            ]
        );
        // rank1 has no customer member — the band still owns its clamps.
        assert_eq!(
            key_names(&plan, &plan.layers[1]),
            vec![
                "pb:warehouse:L",
                "ship",
                "pb:warehouse:R",
                "pb:customer:L",
                "pb:customer:R",
            ]
        );
    }

    #[test]
    fn wires_cross_rank_pb_segments_for_every_column_and_side() {
        let (mut plan, real) = fixture(
            &["a", "b"],
            &[
                Spec {
                    id: "n1",
                    rank: 0,
                    path: &[],
                    col: Some("a"),
                },
                Spec {
                    id: "n2",
                    rank: 1,
                    path: &[],
                    col: Some("b"),
                },
                Spec {
                    id: "n3",
                    rank: 2,
                    path: &[],
                    col: Some("a"),
                },
            ],
        );
        insert_partition_boundaries(&mut plan, &real).unwrap();

        for col in ["a", "b"] {
            for side in ["L", "R"] {
                let id = format!("pb:{col}:{side}");
                let segs: Vec<&Segment> =
                    plan.segments.iter().filter(|s| s.edge_id == id).collect();
                assert_eq!(segs.len(), 2, "{id} must span both rank gaps");
                for s in segs {
                    assert_eq!(
                        plan.elems[s.to].rank,
                        plan.elems[s.from].rank + 1,
                        "{id} segment must be rank-adjacent"
                    );
                }
            }
        }
    }

    #[test]
    fn single_column_group_block_nests_inside_its_column() {
        let (mut plan, real) = fixture(
            &["sales", "ops"],
            &[
                Spec {
                    id: "m1",
                    rank: 0,
                    path: &["team"],
                    col: Some("sales"),
                },
                Spec {
                    id: "m2",
                    rank: 0,
                    path: &["team"],
                    col: Some("sales"),
                },
                Spec {
                    id: "other",
                    rank: 0,
                    path: &[],
                    col: Some("ops"),
                },
            ],
        );
        super::super::boundary::insert_group_boundaries(&mut plan);
        insert_partition_boundaries(&mut plan, &real).unwrap();

        let names = key_names(&plan, &plan.layers[0]);
        let sales_l = names.iter().position(|n| n == "pb:sales:L").unwrap();
        let sales_r = names.iter().position(|n| n == "pb:sales:R").unwrap();
        let gb_l = names.iter().position(|n| n == "gb:team:L").unwrap();
        let gb_r = names.iter().position(|n| n == "gb:team:R").unwrap();
        assert!(
            sales_l < gb_l && gb_l < gb_r && gb_r < sales_r,
            "group block must nest inside its column block: {names:?}"
        );
        // Ownership: members AND group clamps belong to the column.
        for name in ["gb:team:L", "gb:team:R", "m1", "m2"] {
            let pos = names.iter().position(|n| n == name).unwrap();
            let ei = plan.layers[0][pos];
            assert_eq!(
                plan.partition_elem_col[ei],
                Some(0),
                "{name} must be owned by column `sales`"
            );
        }
    }

    #[test]
    fn cross_column_group_is_a_hard_error() {
        let (mut plan, real) = fixture(
            &["a", "b"],
            &[
                Spec {
                    id: "m1",
                    rank: 0,
                    path: &["span"],
                    col: Some("a"),
                },
                Spec {
                    id: "m2",
                    rank: 1,
                    path: &["span"],
                    col: Some("b"),
                },
            ],
        );
        let err = insert_partition_boundaries(&mut plan, &real).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("`span`") && msg.contains("partition columns"),
            "{msg}"
        );
        // Nothing was allocated before the check failed.
        assert!(plan.partition_columns.is_empty());
        assert!(plan.elems.iter().all(|e| !e.key.is_partition_boundary()));
    }

    #[test]
    fn no_grid_or_no_columns_is_a_noop() {
        let (mut plan, mut real) = fixture(
            &["a"],
            &[Spec {
                id: "n1",
                rank: 0,
                path: &[],
                col: Some("a"),
            }],
        );
        real.partition = None;
        insert_partition_boundaries(&mut plan, &real).unwrap();
        assert!(plan.partition_columns.is_empty());

        let (mut plan2, mut real2) = fixture(
            &[],
            &[Spec {
                id: "n1",
                rank: 0,
                path: &[],
                col: None,
            }],
        );
        real2.partition = Some(PartitionGrid::default());
        insert_partition_boundaries(&mut plan2, &real2).unwrap();
        assert!(plan2.partition_columns.is_empty());
    }
}
