//! SGD stress solver (Zheng–Pawar–Goodman 2018).
//!
//! Projection update per pair with exponential step schedule; deterministic
//! Fisher–Yates pair shuffle per epoch (explicit PRNG). Fixed epoch count —
//! no data-dependent stopping (workspace determinism rule).

use super::super::geom::Prng;

/// A stress term: target distance `d` (px) and weight `w = d⁻²`.
#[derive(Debug, Clone, Copy)]
pub struct StressPair {
    pub i: usize,
    pub j: usize,
    pub d: f64,
    pub w: f64,
}

impl StressPair {
    pub fn new(i: usize, j: usize, d: f64) -> Self {
        let d = d.max(1e-6);
        Self { i, j, d, w: 1.0 / (d * d) }
    }
}

/// Terminal step-size floor factor (paper: ε = 0.1).
const EPSILON: f64 = 0.1;

pub fn solve(
    pos: &mut [(f64, f64)],
    pairs: &[StressPair],
    epochs: u32,
    prng: &mut Prng,
) {
    if pos.len() < 2 || pairs.is_empty() || epochs == 0 {
        return;
    }

    let w_max = pairs.iter().map(|p| p.w).fold(f64::MIN, f64::max);
    let w_min = pairs.iter().map(|p| p.w).fold(f64::MAX, f64::min);
    let eta_max = 1.0 / w_min;
    let eta_min = EPSILON / w_max;
    let lambda = if epochs > 1 {
        (eta_min / eta_max).ln() / (epochs - 1) as f64
    } else {
        0.0
    };

    let mut order: Vec<usize> = (0..pairs.len()).collect();
    for t in 0..epochs {
        let eta = eta_max * (lambda * t as f64).exp();
        prng.shuffle(&mut order);
        for &pi in &order {
            let p = pairs[pi];
            let (mut dx, mut dy) = (pos[p.j].0 - pos[p.i].0, pos[p.j].1 - pos[p.i].1);
            let mut delta = (dx * dx + dy * dy).sqrt();
            if delta < 1e-9 {
                // Coincident points: deterministic micro-jitter (ebook 04 §6).
                let (jx, jy) = prng.jitter();
                pos[p.i].0 += jx * 1e-3;
                pos[p.i].1 += jy * 1e-3;
                dx = pos[p.j].0 - pos[p.i].0;
                dy = pos[p.j].1 - pos[p.i].1;
                delta = (dx * dx + dy * dy).sqrt();
                if delta < 1e-9 {
                    continue;
                }
            }
            let mu = (eta * p.w).min(1.0);
            let r = (delta - p.d) / 2.0 * mu;
            let ux = dx / delta;
            let uy = dy / delta;
            pos[p.i].0 += r * ux;
            pos[p.i].1 += r * uy;
            pos[p.j].0 -= r * ux;
            pos[p.j].1 -= r * uy;
        }
    }
}

/// Full stress value (for tests only — never a stopping rule in the solver).
#[cfg(test)]
pub fn stress_value(pos: &[(f64, f64)], pairs: &[StressPair]) -> f64 {
    pairs
        .iter()
        .map(|p| {
            let dx = pos[p.j].0 - pos[p.i].0;
            let dy = pos[p.j].1 - pos[p.i].1;
            let delta = (dx * dx + dy * dy).sqrt();
            p.w * (delta - p.d) * (delta - p.d)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stress_decreases_on_square_graph() {
        // 4-cycle: target side 100.
        let pairs: Vec<StressPair> = vec![
            StressPair::new(0, 1, 100.0),
            StressPair::new(1, 2, 100.0),
            StressPair::new(2, 3, 100.0),
            StressPair::new(3, 0, 100.0),
            StressPair::new(0, 2, 141.4),
            StressPair::new(1, 3, 141.4),
        ];
        let mut pos = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let before = stress_value(&pos, &pairs);
        let mut prng = Prng::new(7);
        solve(&mut pos, &pairs, 30, &mut prng);
        let after = stress_value(&pos, &pairs);
        assert!(
            after < before,
            "stress must decrease: {before} -> {after}"
        );
    }

    #[test]
    fn deterministic_for_same_seed() {
        let pairs: Vec<StressPair> = vec![
            StressPair::new(0, 1, 50.0),
            StressPair::new(1, 2, 50.0),
            StressPair::new(0, 2, 86.6),
        ];
        let run = || {
            let mut pos = vec![(0.0, 0.0), (5.0, 5.0), (-3.0, 2.0)];
            let mut prng = Prng::new(42);
            solve(&mut pos, &pairs, 15, &mut prng);
            pos
        };
        assert_eq!(run(), run());
    }
}
