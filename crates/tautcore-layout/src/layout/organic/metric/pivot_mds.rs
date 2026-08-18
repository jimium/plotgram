//! PivotMDS initialization (Brandes–Pich 2007).
//!
//! Deterministic: farthest-first pivots (max-degree start, ties by smallest
//! index), unweighted BFS distances, fixed power-iteration count, fixed
//! eigenvector orientation (largest-|component| positive, ties by smallest
//! index — same convention as `tautcore_algo::fiedler`).

use std::f64::consts::TAU;

/// Pivot count (ebook §6: 50–200 for sparse stress; MDS init needs fewer).
pub const PIVOT_COUNT: usize = 32;
/// Fixed power-iteration steps (no data-dependent stopping).
pub const POWER_ITERS: u32 = 128;

/// BFS hop distances from `start` over `adjacency`. Unreached nodes stay
/// `None` (cannot happen inside a weak component; caller guarantees).
pub fn bfs_hops(adjacency: &[Vec<usize>], start: usize) -> Vec<Option<u32>> {
    let n = adjacency.len();
    let mut dist = vec![None; n];
    let mut next = vec![start];
    dist[start] = Some(0);
    let mut d = 0;
    while !next.is_empty() {
        let frontier = std::mem::take(&mut next);
        d += 1;
        for u in frontier {
            for &v in &adjacency[u] {
                if dist[v].is_none() {
                    dist[v] = Some(d);
                    next.push(v);
                }
            }
        }
    }
    dist
}

/// Farthest-first pivot selection with deterministic tie-breaks.
pub fn select_pivots(adjacency: &[Vec<usize>]) -> Vec<usize> {
    let n = adjacency.len();
    let k = n.min(PIVOT_COUNT);
    if k == 0 {
        return Vec::new();
    }

    let mut pivots = Vec::with_capacity(k);
    // Start: highest degree, ties by smallest index.
    let first = (0..n)
        .max_by(|&a, &b| {
            adjacency[a]
                .len()
                .cmp(&adjacency[b].len())
                .then_with(|| b.cmp(&a))
        })
        .unwrap_or(0);
    pivots.push(first);

    let mut nearest = bfs_hops(adjacency, first)
        .into_iter()
        .map(|d| d.unwrap_or(0) as f64)
        .collect::<Vec<f64>>();

    while pivots.len() < k {
        // Farthest from the pivot set; ties by smallest index.
        let next = (0..n)
            .filter(|&v| !pivots.contains(&v))
            .max_by(|&a, &b| nearest[a].partial_cmp(&nearest[b]).unwrap().then_with(|| b.cmp(&a)))
            .unwrap_or(0);
        let d = bfs_hops(adjacency, next);
        pivots.push(next);
        for (v, dv) in d.into_iter().enumerate() {
            let dv = dv.unwrap_or(0) as f64;
            if dv < nearest[v] {
                nearest[v] = dv;
            }
        }
    }
    pivots
}

/// Two-dimensional PivotMDS coordinates. `pivot_dists[j][i]` = BFS hops from
/// pivot `j` to node `i`. Falls back to a deterministic ring when the spectrum
/// is degenerate.
pub fn pivot_mds_2d(n: usize, pivot_dists: &[Vec<f64>]) -> Vec<(f64, f64)> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![(0.0, 0.0)];
    }
    let k = pivot_dists.len();
    debug_assert!(k >= 1);

    // Double centering on the squared distance matrix.
    let d2: Vec<Vec<f64>> = pivot_dists.iter().map(|row| row.iter().map(|d| d * d).collect()).collect();
    let row_mean: Vec<f64> = d2.iter().map(|r| r.iter().sum::<f64>() / n as f64).collect();
    let col_mean: Vec<f64> = (0..n)
        .map(|i| d2.iter().map(|r| r[i]).sum::<f64>() / k as f64)
        .collect();
    let grand: f64 = d2.iter().map(|r| r.iter().sum::<f64>()).sum::<f64>() / (n * k) as f64;

    // B[j][i]
    let b = |j: usize, i: usize| -> f64 {
        -0.5 * (d2[j][i] - row_mean[j] - col_mean[i] + grand)
    };

    // M = B Bᵀ (k×k).
    let mut m = vec![vec![0.0; k]; k];
    for a in 0..k {
        for c in 0..k {
            m[a][c] = (0..n).map(|i| b(a, i) * b(c, i)).sum();
        }
    }

    let (lambda1, u1) = power_eigen(&m);
    // Deflate for the second eigenvector.
    let mut m2 = m.clone();
    for a in 0..k {
        for c in 0..k {
            m2[a][c] -= lambda1 * u1[a] * u1[c];
        }
    }
    let (lambda2, u2) = power_eigen(&m2);

    let sigma1 = lambda1.max(0.0).sqrt();
    let sigma2 = lambda2.max(0.0).sqrt();
    let scale_ref = row_mean.iter().cloned().fold(0.0_f64, f64::max).sqrt().max(1e-6);
    let eps = scale_ref * 1e-6;
    let ring = ring_fallback(n);

    // X column j = Bᵀ u_j / sqrt(σ_j). Per-axis fallback: a degenerate axis
    // (e.g. the second axis of a path graph, whose MDS embedding is 1D)
    // borrows the ring coordinate instead of collapsing the whole output.
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x1: f64 = if sigma1 > eps {
            let x = (0..k).map(|j| b(j, i) * u1[j]).sum::<f64>() / sigma1.sqrt();
            if x.is_finite() { x } else { ring[i].0 }
        } else {
            ring[i].0
        };
        let x2: f64 = if sigma2 > eps {
            let x = (0..k).map(|j| b(j, i) * u2[j]).sum::<f64>() / sigma2.sqrt();
            if x.is_finite() { x } else { ring[i].1 }
        } else {
            ring[i].1
        };
        out.push((x1, x2));
    }
    orient_deterministic(&mut out);
    out
}

/// Ring fallback (degenerate spectrum / tiny graphs). Ring radius chosen so
/// adjacent ring nodes sit ~1 unit apart.
fn ring_fallback(n: usize) -> Vec<(f64, f64)> {
    let r = if n >= 3 {
        1.0 / (2.0 * (std::f64::consts::PI / n as f64).sin())
    } else {
        0.5
    };
    (0..n)
        .map(|i| {
            let t = -std::f64::consts::FRAC_PI_2 + TAU * i as f64 / n as f64;
            (r * t.cos(), r * t.sin())
        })
        .collect()
}

/// Top eigenpair of a small symmetric matrix via fixed-count power iteration.
/// Deterministic start vector (alternating signs) + deterministic orientation.
fn power_eigen(m: &[Vec<f64>]) -> (f64, Vec<f64>) {
    let k = m.len();
    if k == 0 {
        return (0.0, Vec::new());
    }
    let norm = (k as f64).sqrt();
    let mut v: Vec<f64> = (0..k)
        .map(|i| if i % 2 == 0 { 1.0 } else { -0.5 } / norm)
        .collect();

    for _ in 0..POWER_ITERS {
        let mut next = vec![0.0; k];
        for (a, na) in next.iter_mut().enumerate() {
            *na = m[a].iter().zip(&v).map(|(&m_ac, &c)| m_ac * c).sum();
        }
        let nrm = vector_norm(&next);
        if nrm < 1e-12 {
            break;
        }
        for x in &mut next {
            *x /= nrm;
        }
        v = next;
    }

    let lambda = m
        .iter()
        .zip(&v)
        .map(|(row, &c)| row.iter().zip(&v).map(|(&m_ac, &d)| m_ac * c * d).sum::<f64>())
        .sum::<f64>()
        .max(0.0);

    // Orientation: largest-|component| positive, ties by smallest index.
    let mut lead = 0usize;
    for (i, &x) in v.iter().enumerate() {
        if x.abs() > v[lead].abs() + 1e-12 {
            lead = i;
        }
    }
    if v[lead] < 0.0 {
        for x in &mut v {
            *x = -*x;
        }
    }
    (lambda, v)
}

fn vector_norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Global sign fix per axis: the node with the largest |x| (ties: smallest
/// index) gets a non-negative coordinate.
fn orient_deterministic(points: &mut [(f64, f64)]) {
    for axis in 0..2 {
        let mut lead = 0usize;
        for (i, p) in points.iter().enumerate() {
            let a = if axis == 0 { p.0 } else { p.1 };
            let l = if axis == 0 { points[lead].0 } else { points[lead].1 };
            if a.abs() > l.abs() + 1e-12 {
                lead = i;
            }
        }
        let l = if axis == 0 { points[lead].0 } else { points[lead].1 };
        if l < 0.0 {
            for p in points.iter_mut() {
                if axis == 0 {
                    p.0 = -p.0;
                } else {
                    p.1 = -p.1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_adj(n: usize) -> Vec<Vec<usize>> {
        (0..n)
            .map(|i| {
                let mut v = Vec::new();
                if i > 0 {
                    v.push(i - 1);
                }
                if i + 1 < n {
                    v.push(i + 1);
                }
                v
            })
            .collect()
    }

    #[test]
    fn pivots_are_distinct_and_in_range() {
        let adj = path_adj(10);
        let pivots = select_pivots(&adj);
        assert_eq!(pivots.len(), 10.min(PIVOT_COUNT));
        let mut sorted = pivots.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), pivots.len());
    }

    #[test]
    fn mds_of_path_spreads_extremes() {
        let n = 8;
        let adj = path_adj(n);
        let pivots = select_pivots(&adj);
        let dists: Vec<Vec<f64>> = pivots
            .iter()
            .map(|&p| {
                bfs_hops(&adj, p)
                    .into_iter()
                    .map(|d| d.unwrap_or(0) as f64)
                    .collect()
            })
            .collect();
        let pos = pivot_mds_2d(n, &dists);
        assert_eq!(pos.len(), n);
        assert!(pos.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
        // Path endpoints must land far apart on the leading axis.
        let d0 = (pos[0].0 - pos[n - 1].0).abs();
        assert!(d0 > 1.0, "endpoints too close: {d0}");
    }

    #[test]
    fn deterministic_given_same_input() {
        let adj = path_adj(12);
        let a = pivot_mds_2d(12, &[bfs_hops(&adj, 0).iter().map(|d| d.unwrap() as f64).collect()]);
        let b = pivot_mds_2d(12, &[bfs_hops(&adj, 0).iter().map(|d| d.unwrap() as f64).collect()]);
        assert_eq!(a, b);
    }
}
