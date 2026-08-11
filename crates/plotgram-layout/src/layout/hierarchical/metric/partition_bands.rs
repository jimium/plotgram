//! Cross-axis band plan for consumed partition columns (partition-grid.md
//! PG-1).
//!
//! The cross solve reads this plan DIRECTLY (hard constraints inside
//! [`super::symmetry_objective`]); DemandBoard only resolves main-axis
//! LayerGap seams, so band facts never route through demand solving —
//! [`crate::layout::hierarchical::demand::DemandKey::PartitionBandMinSize`]
//! is published for observability only.
//!
//! PG-2: [`band_coords`] reads the SOLVED clamp positions back out — the
//! metric stage stays the band coordinate source of truth; everything
//! downstream (HierarchicalObs, debug trace, measure) is a projection.

use crate::layout::hierarchical::model::{BoundarySide, ElemKey, PlanGraph};

/// Minimum band width for a column with no assigned members anywhere — an
/// empty swimlane keeps a visible strip for its title. Local to PG-1; PG-2
/// revisits once bands gain first-class coordinates.
pub const PARTITION_EMPTY_BAND_MIN: f64 = 96.0;

/// Cross-axis facts for the consumed partition columns.
#[derive(Debug, Clone)]
pub struct PartitionBandPlan {
    /// Consumed columns, declaration order (= [`PlanGraph::partition_columns`]).
    pub columns: Vec<String>,
    /// Inter-column separation AND band pad. PG-1 ruling: both are the
    /// author `node_gap` — no new parameters (partition-grid.md §7 PG-1).
    pub gap: f64,
    /// column index -> the column has NO assigned real member anywhere.
    pub empty: Vec<bool>,
}

impl PartitionBandPlan {
    /// Derive from a consumed plan; `None` when partition is not consumed
    /// (the §10 single gate: no fields on the plan → no band behavior).
    pub fn build(plan: &PlanGraph, node_gap: f64) -> Option<Self> {
        if plan.partition_columns.is_empty() {
            return None;
        }
        let mut empty = vec![true; plan.partition_columns.len()];
        for (ei, col) in plan.partition_elem_col.iter().enumerate() {
            let Some(ci) = col else { continue };
            if !matches!(plan.elems[ei].key, ElemKey::Real(_)) {
                continue;
            }
            if let Some(slot) = empty.get_mut(*ci) {
                *slot = false;
            }
        }
        Some(Self {
            columns: plan.partition_columns.clone(),
            gap: node_gap,
            empty,
        })
    }

    /// Clamp elem for `(column, rank, side)`, if the plan carries it.
    pub fn clamp(
        &self,
        plan: &PlanGraph,
        column: &str,
        rank: u32,
        side: BoundarySide,
    ) -> Option<usize> {
        plan.index_of
            .get(&ElemKey::PartitionBoundary {
                column: column.to_string(),
                rank,
                side,
            })
            .copied()
    }
}

/// Solved cross-axis band interval for one consumed column (PG-2),
/// canonical coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionBandCoords {
    pub column: String,
    pub start: f64,
    pub end: f64,
    pub empty: bool,
}

/// Read the solved band intervals back from the clamp positions (PG-2).
///
/// Canonical cross-axis coordinates; the caller owns the physical shift.
/// Non-empty columns carry the GLOBAL snapped values
/// ([`super::symmetry_objective`] snap writes every rank's clamps to the
/// same member extremes ± pad), so min/max over ranks is a no-op there;
/// empty columns keep their solved positions and the extremes defend
/// against sub-pixel rank drift. Empty Vec when partition is not consumed
/// (the §10 single gate).
pub fn band_coords(plan: &PlanGraph, cross: &[f64]) -> Vec<PartitionBandCoords> {
    let Some(bands) = PartitionBandPlan::build(plan, 0.0) else {
        return Vec::new();
    };
    let mut out: Vec<PartitionBandCoords> = Vec::with_capacity(bands.columns.len());
    for (ci, column) in bands.columns.iter().enumerate() {
        let mut start: Option<f64> = None;
        let mut end: Option<f64> = None;
        for elem in &plan.elems {
            let ElemKey::PartitionBoundary {
                column: col, side, ..
            } = &elem.key
            else {
                continue;
            };
            if col != column {
                continue;
            }
            let Some(&ei) = plan.index_of.get(&elem.key) else {
                continue;
            };
            let x = cross.get(ei).copied().unwrap_or(0.0);
            match side {
                BoundarySide::Left => start = Some(start.map_or(x, |s: f64| s.min(x))),
                BoundarySide::Right => end = Some(end.map_or(x, |e: f64| e.max(x))),
            }
        }
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        out.push(PartitionBandCoords {
            column: column.clone(),
            start,
            end,
            empty: bands.empty.get(ci).copied().unwrap_or(false),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::hierarchical::compose::partition_boundary::insert_partition_boundaries;
    use crate::layout::hierarchical::model::{Elem, RealGraph};
    use plotgram_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

    fn two_col_plan(second_col_assigned: bool) -> (PlanGraph, RealGraph) {
        let mut real = RealGraph::default();
        real.partition = Some(PartitionGrid {
            columns: vec![PartitionAxis::new("a"), PartitionAxis::new("b")],
            rows: vec![],
        });
        let specs: [(&str, Option<&str>); 2] = [
            ("n1", Some("a")),
            ("n2", if second_col_assigned { Some("b") } else { None }),
        ];
        let mut elems = Vec::new();
        for (i, (id, col)) in specs.iter().enumerate() {
            real.ids.push((*id).to_string());
            real.index_of.insert((*id).to_string(), i);
            real.partition_cell.push(col.map(PartitionCell::col));
            elems.push(Elem {
                key: ElemKey::Real((*id).into()),
                group_path: vec![],
                rank: 0,
            });
        }
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1],
            segments: vec![],
            layers: vec![vec![0, 1]],
            ..Default::default()
        };
        insert_partition_boundaries(&mut plan, &real).unwrap();
        (plan, real)
    }

    #[test]
    fn build_flags_empty_columns_only() {
        let (plan, _) = two_col_plan(true);
        let bands = PartitionBandPlan::build(&plan, 24.0).unwrap();
        assert_eq!(bands.columns, vec!["a", "b"]);
        assert_eq!(bands.gap, 24.0);
        assert_eq!(bands.empty, vec![false, false]);

        let (plan2, _) = two_col_plan(false);
        let bands2 = PartitionBandPlan::build(&plan2, 24.0).unwrap();
        assert_eq!(bands2.empty, vec![false, true]);
    }

    #[test]
    fn build_is_none_when_partition_not_consumed() {
        let (mut plan, _) = two_col_plan(true);
        plan.partition_columns.clear();
        assert!(PartitionBandPlan::build(&plan, 24.0).is_none());
    }

    fn set_clamp(
        plan: &PlanGraph,
        cross: &mut [f64],
        col: &str,
        rank: u32,
        side: BoundarySide,
        x: f64,
    ) {
        let key = ElemKey::PartitionBoundary {
            column: col.to_string(),
            rank,
            side,
        };
        if let Some(&ei) = plan.index_of.get(&key) {
            cross[ei] = x;
        }
    }

    #[test]
    fn band_coords_reads_solved_clamp_positions() {
        let (plan, _) = two_col_plan(true);
        let mut cross = vec![0.0; plan.elems.len()];
        set_clamp(&plan, &mut cross, "a", 0, BoundarySide::Left, 10.0);
        set_clamp(&plan, &mut cross, "a", 0, BoundarySide::Right, 90.0);
        set_clamp(&plan, &mut cross, "b", 0, BoundarySide::Left, 130.0);
        set_clamp(&plan, &mut cross, "b", 0, BoundarySide::Right, 226.0);

        let bands = band_coords(&plan, &cross);
        assert_eq!(bands.len(), 2);
        assert_eq!(
            bands[0],
            PartitionBandCoords { column: "a".into(), start: 10.0, end: 90.0, empty: false }
        );
        assert_eq!(
            bands[1],
            PartitionBandCoords { column: "b".into(), start: 130.0, end: 226.0, empty: false }
        );
    }

    #[test]
    fn band_coords_takes_extremes_across_ranks_and_flags_empty() {
        // n1 (col a) on rank 0, n2 unassigned on rank 1 → column b is empty.
        let mut real = RealGraph::default();
        real.partition = Some(PartitionGrid {
            columns: vec![PartitionAxis::new("a"), PartitionAxis::new("b")],
            rows: vec![],
        });
        let specs: [(&str, u32, Option<&str>); 2] = [("n1", 0, Some("a")), ("n2", 1, None)];
        let mut elems = Vec::new();
        for (i, (id, rank, col)) in specs.iter().enumerate() {
            real.ids.push((*id).to_string());
            real.index_of.insert((*id).to_string(), i);
            real.partition_cell.push(col.map(PartitionCell::col));
            elems.push(Elem {
                key: ElemKey::Real((*id).into()),
                group_path: vec![],
                rank: *rank,
            });
        }
        let index_of = elems
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut plan = PlanGraph {
            elems,
            index_of,
            decl_index: vec![0, 1],
            segments: vec![],
            layers: vec![vec![0], vec![1]],
            ..Default::default()
        };
        insert_partition_boundaries(&mut plan, &real).unwrap();

        let mut cross = vec![0.0; plan.elems.len()];
        // a snaps to identical globals on both ranks; b keeps solved
        // positions with sub-pixel rank drift to exercise the extremes.
        for rank in [0u32, 1] {
            set_clamp(&plan, &mut cross, "a", rank, BoundarySide::Left, 20.0);
            set_clamp(&plan, &mut cross, "a", rank, BoundarySide::Right, 100.0);
        }
        set_clamp(&plan, &mut cross, "b", 0, BoundarySide::Left, 160.0);
        set_clamp(&plan, &mut cross, "b", 0, BoundarySide::Right, 256.0);
        set_clamp(&plan, &mut cross, "b", 1, BoundarySide::Left, 161.0);
        set_clamp(&plan, &mut cross, "b", 1, BoundarySide::Right, 258.0);

        let bands = band_coords(&plan, &cross);
        assert_eq!(bands.len(), 2);
        assert_eq!(
            bands[0],
            PartitionBandCoords { column: "a".into(), start: 20.0, end: 100.0, empty: false }
        );
        assert_eq!(
            bands[1],
            PartitionBandCoords { column: "b".into(), start: 160.0, end: 258.0, empty: true }
        );
        assert!(bands[1].end - bands[1].start >= PARTITION_EMPTY_BAND_MIN);
    }

    #[test]
    fn band_coords_is_empty_when_partition_not_consumed() {
        let (mut plan, _) = two_col_plan(true);
        plan.partition_columns.clear();
        let cross = vec![0.0; plan.elems.len()];
        assert!(band_coords(&plan, &cross).is_empty());
    }
}
