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

use plotgram_algo::orientation::Size;
use plotgram_model::geometry::Rect;

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
                axis: column.to_string(),
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
                axis: col, side, ..
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

/// Solved main-axis band interval for one consumed row/column (PG-3),
/// canonical coordinates (y in TB).
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionRowBandCoords {
    pub row: String,
    pub start: f64,
    pub end: f64,
    pub empty: bool,
}

/// Row bands from the stacked main axis (partition-grid.md PG-3).
///
/// Non-empty: bounding `[min layer top, max layer bottom]` of the row's rank
/// interval. Empty: the LayerGap strip attached to the dummy rank.
pub fn row_band_coords(
    plan: &PlanGraph,
    main: &[f64],
    size_of: &dyn Fn(usize) -> Size,
    layer_gaps: &[f64],
) -> Vec<PartitionRowBandCoords> {
    if plan.partition_rows.is_empty() {
        return Vec::new();
    }
    let n_layers = plan.layers.len();
    let mut layer_top = vec![0.0; n_layers];
    let mut layer_bot = vec![0.0; n_layers];
    for (r, layer) in plan.layers.iter().enumerate() {
        let mut top = f64::INFINITY;
        let mut bot = f64::NEG_INFINITY;
        for &ei in layer {
            let y = main.get(ei).copied().unwrap_or(0.0);
            let h = size_of(ei).height;
            top = top.min(y);
            bot = bot.max(y + h);
        }
        if !top.is_finite() {
            top = 0.0;
            bot = 0.0;
        }
        layer_top[r] = top;
        layer_bot[r] = bot;
    }
    let mut out = Vec::with_capacity(plan.partition_rows.len());
    for (ri, row) in plan.partition_rows.iter().enumerate() {
        let empty = !plan
            .partition_elem_row
            .iter()
            .zip(plan.elems.iter())
            .any(|(slot, elem)| *slot == Some(ri) && matches!(elem.key, ElemKey::Real(_)));
        let (lo, hi) = plan
            .partition_row_intervals
            .get(ri)
            .copied()
            .unwrap_or((0, 0));
        let (start, end) = if empty {
            empty_row_strip(lo as usize, n_layers, &layer_top, layer_gaps)
        } else {
            // Ruling: y-union of the packed rank interval after main-axis
            // stacking. Member Fit is unioned so a frame that extends past
            // zero-height clamp extrema on that rank is still covered.
            let mut start = f64::INFINITY;
            let mut end = f64::NEG_INFINITY;
            for r in lo as usize..=hi as usize {
                if r < n_layers {
                    start = start.min(layer_top[r]);
                    end = end.max(layer_bot[r]);
                }
            }
            for (ei, elem) in plan.elems.iter().enumerate() {
                if plan.partition_elem_row.get(ei).copied().flatten() != Some(ri) {
                    continue;
                }
                if !matches!(elem.key, ElemKey::Real(_)) {
                    continue;
                }
                let y = main.get(ei).copied().unwrap_or(0.0);
                let h = size_of(ei).height;
                start = start.min(y);
                end = end.max(y + h);
            }
            if start.is_finite() {
                (start, end)
            } else {
                empty_row_strip(lo as usize, n_layers, &layer_top, layer_gaps)
            }
        };
        out.push(PartitionRowBandCoords {
            row: row.clone(),
            start,
            end,
            empty,
        });
    }
    out
}

fn empty_row_strip(
    r: usize,
    n_layers: usize,
    layer_top: &[f64],
    layer_gaps: &[f64],
) -> (f64, f64) {
    let y = layer_top.get(r).copied().unwrap_or(0.0);
    if r + 1 < n_layers {
        let gap = layer_gaps
            .get(r)
            .copied()
            .unwrap_or(0.0)
            .max(PARTITION_EMPTY_BAND_MIN);
        (y, y + gap)
    } else if r > 0 {
        let gap = layer_gaps
            .get(r - 1)
            .copied()
            .unwrap_or(0.0)
            .max(PARTITION_EMPTY_BAND_MIN);
        (y - gap, y)
    } else {
        (0.0, PARTITION_EMPTY_BAND_MIN)
    }
}

/// Fit envelopes from already-written frames (StrongMacro: no clamp elems).
/// `pad` is applied on the cross axis (same as Weak snap pad = node_gap).
pub fn band_coords_from_member_frames(
    plan: &PlanGraph,
    frames: &[Rect],
    pad: f64,
) -> (Vec<PartitionBandCoords>, Vec<PartitionRowBandCoords>) {
    let n_cross = plan.partition_columns.len();
    let mut col_start = vec![f64::INFINITY; n_cross];
    let mut col_end = vec![f64::NEG_INFINITY; n_cross];
    let mut col_empty = vec![true; n_cross];
    let n_main = plan.partition_rows.len();
    let mut row_start = vec![f64::INFINITY; n_main];
    let mut row_end = vec![f64::NEG_INFINITY; n_main];
    let mut row_empty = vec![true; n_main];
    for (ei, elem) in plan.elems.iter().enumerate() {
        if !matches!(elem.key, ElemKey::Real(_)) {
            continue;
        }
        let Some(f) = frames.get(ei) else {
            continue;
        };
        if let Some(&ci) = plan.partition_elem_col.get(ei).and_then(|c| c.as_ref()) {
            if ci < n_cross {
                col_empty[ci] = false;
                col_start[ci] = col_start[ci].min(f.x);
                col_end[ci] = col_end[ci].max(f.right());
            }
        }
        if let Some(&ri) = plan.partition_elem_row.get(ei).and_then(|c| c.as_ref()) {
            if ri < n_main {
                row_empty[ri] = false;
                row_start[ri] = row_start[ri].min(f.y);
                row_end[ri] = row_end[ri].max(f.bottom());
            }
        }
    }
    let mut cols = Vec::with_capacity(n_cross);
    let mut cursor = 0.0;
    for (ci, column) in plan.partition_columns.iter().enumerate() {
        let (start, end) = if col_empty[ci] {
            let start = cursor;
            (start, start + PARTITION_EMPTY_BAND_MIN)
        } else {
            (col_start[ci] - pad, col_end[ci] + pad)
        };
        cursor = end + pad.max(0.0);
        cols.push(PartitionBandCoords {
            column: column.clone(),
            start,
            end,
            empty: col_empty[ci],
        });
    }
    let mut rows = Vec::with_capacity(n_main);
    let mut y_cursor = 0.0;
    for (ri, row) in plan.partition_rows.iter().enumerate() {
        let (start, end) = if row_empty[ri] {
            let start = y_cursor;
            (start, start + PARTITION_EMPTY_BAND_MIN)
        } else {
            (row_start[ri], row_end[ri])
        };
        y_cursor = end;
        rows.push(PartitionRowBandCoords {
            row: row.clone(),
            start,
            end,
            empty: row_empty[ri],
        });
    }
    (cols, rows)
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
            axis: col.to_string(),
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
            PartitionBandCoords {
                column: "a".into(),
                start: 10.0,
                end: 90.0,
                empty: false
            }
        );
        assert_eq!(
            bands[1],
            PartitionBandCoords {
                column: "b".into(),
                start: 130.0,
                end: 226.0,
                empty: false
            }
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
            PartitionBandCoords {
                column: "a".into(),
                start: 20.0,
                end: 100.0,
                empty: false
            }
        );
        assert_eq!(
            bands[1],
            PartitionBandCoords {
                column: "b".into(),
                start: 160.0,
                end: 258.0,
                empty: true
            }
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
