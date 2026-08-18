//! Node overlap removal via 1-D separation constraints (Dwyer–Marriott–
//! Stuckey VPSC), x/y alternating passes. Independent post-stage — never
//! mixed into the solver loop (ebook 04 §4 architecture note).

use tautcore_algo::vpsc::{self, Constraint, Variable};
use tautcore_model::geometry::Size;

/// Fixed pass count (x/y alternation). Deterministic; no data-dependent stop.
pub const OVERLAP_PASSES: usize = 4;

/// Push axis-aligned boxes (centers + sizes) apart until they clear `gap`.
/// Boxes already clear of each other keep their centers (mental map).
pub fn remove_overlaps(
    centers: &mut [(f64, f64)],
    sizes: &[Size],
    gap: f64,
) -> Result<(), String> {
    let n = centers.len();
    if n < 2 {
        return Ok(());
    }
    for pass in 0..OVERLAP_PASSES {
        let axis_x = pass % 2 == 0;
        build_and_solve_axis(centers, sizes, gap, axis_x)?;
    }
    Ok(())
}

fn build_and_solve_axis(
    centers: &mut [(f64, f64)],
    sizes: &[Size],
    gap: f64,
    axis_x: bool,
) -> Result<(), String> {
    let n = centers.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let ka = axis_key(centers[a], axis_x);
        let kb = axis_key(centers[b], axis_x);
        ka.partial_cmp(&kb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(&b))
    });

    let mut constraints: Vec<Constraint> = Vec::new();
    for w in 0..n {
        let i = order[w];
        let ki = axis_key(centers[i], axis_x);
        let ext_i = axis_extent(sizes[i], axis_x);
        for &j in order.iter().skip(w + 1) {
            let kj = axis_key(centers[j], axis_x);
            let need = ext_i + axis_extent(sizes[j], axis_x) + gap;
            if kj - ki >= need {
                break; // sorted on the axis → no later j can overlap i
            }
            // Cross-axis projection must overlap too.
            let ci = cross_key(centers[i], axis_x);
            let cj = cross_key(centers[j], axis_x);
            let cross_need = axis_extent(sizes[i], !axis_x) + axis_extent(sizes[j], !axis_x);
            if (cj - ci).abs() < cross_need {
                let (left, right) = if ki <= kj { (i, j) } else { (j, i) };
                constraints.push(Constraint::new(left, right, need));
            }
        }
    }
    if constraints.is_empty() {
        return Ok(());
    }

    let vars: Vec<Variable> = (0..n)
        .map(|i| Variable::new(axis_key(centers[i], axis_x)))
        .collect();
    let solved = vpsc::solve(&vars, &constraints).map_err(|e| format!("vpsc: {e}"))?;
    for (i, s) in solved.into_iter().enumerate() {
        if axis_x {
            centers[i].0 = s;
        } else {
            centers[i].1 = s;
        }
    }
    Ok(())
}

fn axis_key(p: (f64, f64), axis_x: bool) -> f64 {
    if axis_x {
        p.0
    } else {
        p.1
    }
}

fn cross_key(p: (f64, f64), axis_x: bool) -> f64 {
    if axis_x {
        p.1
    } else {
        p.0
    }
}

fn axis_extent(s: Size, axis_x: bool) -> f64 {
    if axis_x {
        s.width / 2.0
    } else {
        s.height / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fully_overlapping_boxes_separate() {
        let mut centers = vec![(0.0, 0.0); 4];
        let sizes: Vec<Size> = (0..4).map(|_| Size::new(40.0, 20.0)).collect();
        remove_overlaps(&mut centers, &sizes, 10.0).unwrap();
        for i in 0..4 {
            for j in (i + 1)..4 {
                let dx = (centers[i].0 - centers[j].0).abs();
                let dy = (centers[i].1 - centers[j].1).abs();
                assert!(
                    dx >= 50.0 - 1e-6 || dy >= 30.0 - 1e-6,
                    "boxes {i}/{j} still overlap: dx={dx} dy={dy}"
                );
            }
        }
    }

    #[test]
    fn clear_boxes_keep_centers() {
        let mut centers = vec![(0.0, 0.0), (200.0, 0.0)];
        let sizes: Vec<Size> = (0..2).map(|_| Size::new(40.0, 20.0)).collect();
        remove_overlaps(&mut centers, &sizes, 10.0).unwrap();
        assert_eq!(centers, vec![(0.0, 0.0), (200.0, 0.0)]);
    }
}
