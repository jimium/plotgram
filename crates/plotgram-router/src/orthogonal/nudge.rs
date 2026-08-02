//! L4 corridor nudging: exact track coordinates via VPSC.
//!
//! Given a backbone line and an ordered set of tracks (from L3 interval
//! coloring), solve for positions that:
//! - stay as close as possible to the backbone (`desired = backbone`);
//! - keep adjacent tracks at least `gap` apart (`x[i+1] − x[i] ≥ gap`).
//!
//! Uses [`plotgram_algo::vpsc`] (architecture.md §5; yFiles 03 §Nudging).
//! L4 must not reorder tracks — that is L3's job.

use plotgram_algo::vpsc::{solve, Constraint, Variable};

/// Solve exact coordinates for `track_count` ordered tracks on a corridor.
///
/// Returns one coordinate per track index `0..track_count`. Equal weights
/// centre the bundle on `backbone` (minimal total squared displacement).
///
/// Falls back to a uniform centred spread if VPSC reports infeasible
/// (should not happen for a pure chain of positive gaps).
pub fn nudge_track_coords(backbone: f64, track_count: usize, gap: f64) -> Vec<f64> {
    if track_count == 0 {
        return Vec::new();
    }
    if track_count == 1 {
        return vec![backbone];
    }
    let vars: Vec<Variable> = (0..track_count).map(|_| Variable::new(backbone)).collect();
    let cons: Vec<Constraint> = (0..track_count - 1)
        .map(|i| Constraint::new(i, i + 1, gap))
        .collect();
    match solve(&vars, &cons) {
        Ok(xs) => xs,
        Err(_) => {
            // Deterministic centred uniform fallback (same geometry family).
            let mid = (track_count - 1) as f64 / 2.0;
            (0..track_count)
                .map(|i| backbone + (i as f64 - mid) * gap)
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tracks_split_symmetrically() {
        let xs = nudge_track_coords(70.0, 2, 20.0);
        assert_eq!(xs.len(), 2);
        assert!((xs[0] - 60.0).abs() < 1e-9, "got {:?}", xs);
        assert!((xs[1] - 80.0).abs() < 1e-9, "got {:?}", xs);
        assert!((xs[1] - xs[0] - 20.0).abs() < 1e-9);
    }

    #[test]
    fn three_tracks_keep_middle_on_backbone() {
        let xs = nudge_track_coords(100.0, 3, 10.0);
        assert!((xs[0] - 90.0).abs() < 1e-9, "got {:?}", xs);
        assert!((xs[1] - 100.0).abs() < 1e-9, "got {:?}", xs);
        assert!((xs[2] - 110.0).abs() < 1e-9, "got {:?}", xs);
    }

    #[test]
    fn single_track_stays() {
        assert_eq!(nudge_track_coords(42.0, 1, 20.0), vec![42.0]);
    }
}
