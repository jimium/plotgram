//! Fiedler vector: second-smallest Laplacian eigenvector.
//!
//! Power iteration on the deflated operator `I - L/λ_max` with a fixed
//! iteration count and a deterministic orientation (largest-`|v_i|` component
//! made positive, ties by smallest index). Unweighted undirected simple graph:
//! self-loops ignored, parallel edges treated as one.
//!
//! Does not read Graph / DSL / profile.

use std::fmt;

/// Fixed power-iteration steps (architecture: no data-dependent stopping).
pub const FIEDLER_ITERS: u32 = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiedlerError {
    Invalid(String),
    NonFinite,
}

impl fmt::Display for FiedlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(m) => f.write_str(m),
            Self::NonFinite => f.write_str("fiedler: non-finite vector"),
        }
    }
}

impl std::error::Error for FiedlerError {}

/// Second-smallest Laplacian eigenvector, Euclidean-normalized.
///
/// `edges[i] = (u, v)` with `u, v` in `0..n`.
pub fn fiedler_vector(n: usize, edges: &[(usize, usize)]) -> Result<Vec<f64>, FiedlerError> {
    for (i, &(u, v)) in edges.iter().enumerate() {
        if u >= n || v >= n {
            return Err(FiedlerError::Invalid(format!(
                "fiedler: edge {i} = ({u}, {v}) out of range for n={n}"
            )));
        }
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    if n == 1 {
        return Ok(vec![0.0]);
    }

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(u, v) in edges {
        if u == v {
            continue;
        }
        adj[u].push(v);
        adj[v].push(u);
    }
    for list in adj.iter_mut() {
        list.sort_unstable();
        list.dedup();
    }
    let degree: Vec<usize> = adj.iter().map(|a| a.len()).collect();
    let max_deg = degree.iter().copied().max().unwrap_or(0);
    if max_deg == 0 {
        return Ok(vec![0.0; n]);
    }
    let lambda_max = 2.0 * max_deg as f64 + 1.0;

    let mut v: Vec<f64> = (0..n).map(|i| i as f64 + 1.0).collect();
    deflate(&mut v);
    normalize(&mut v)?;
    let mut y = vec![0.0; n];
    for _ in 0..FIEDLER_ITERS {
        deflate(&mut v);
        for i in 0..n {
            let mut sum = 0.0;
            for &j in &adj[i] {
                sum += v[j];
            }
            y[i] = degree[i] as f64 * v[i] - sum;
        }
        for i in 0..n {
            v[i] -= y[i] / lambda_max;
        }
        normalize(&mut v)?;
    }
    deflate(&mut v);
    normalize(&mut v)?;
    orient(&mut v);
    if v.iter().any(|x| !x.is_finite()) {
        return Err(FiedlerError::NonFinite);
    }
    Ok(v)
}

fn deflate(v: &mut [f64]) {
    let n = v.len() as f64;
    if n == 0.0 {
        return;
    }
    let mean = v.iter().sum::<f64>() / n;
    for x in v.iter_mut() {
        *x -= mean;
    }
}

fn normalize(v: &mut [f64]) -> Result<(), FiedlerError> {
    let mut ss = 0.0;
    for x in v.iter() {
        if !x.is_finite() {
            return Err(FiedlerError::NonFinite);
        }
        ss += *x * *x;
    }
    let norm = ss.sqrt();
    if !norm.is_finite() || norm < 1e-15 {
        return Err(FiedlerError::NonFinite);
    }
    for x in v.iter_mut() {
        *x /= norm;
    }
    Ok(())
}

fn orient(v: &mut [f64]) {
    let mut best = 0usize;
    let mut best_abs = -1.0;
    for (i, x) in v.iter().enumerate() {
        let a = x.abs();
        if a > best_abs + 1e-15 || ((a - best_abs).abs() <= 1e-15 && i < best) {
            best_abs = a;
            best = i;
        }
    }
    if v[best] < 0.0 {
        for x in v.iter_mut() {
            *x = -*x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_ends_have_opposite_signs() {
        let edges = [(0, 1), (1, 2), (2, 3), (3, 4)];
        let v = fiedler_vector(5, &edges).unwrap();
        assert!(
            v[0] * v[4] < 0.0,
            "path ends should have opposite Fiedler signs, got v[0]={} v[4]={}",
            v[0],
            v[4]
        );
        let mut best = 0;
        for i in 1..v.len() {
            if v[i].abs() > v[best].abs() {
                best = i;
            }
        }
        assert!(v[best] > 0.0, "oriented max-abs component should be > 0");
    }

    #[test]
    fn two_runs_identical() {
        let edges = [(0, 1), (1, 2), (2, 0), (2, 3)];
        let a = fiedler_vector(4, &edges).unwrap();
        let b = fiedler_vector(4, &edges).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn small_graphs() {
        assert!(fiedler_vector(0, &[]).unwrap().is_empty());
        assert_eq!(fiedler_vector(1, &[]).unwrap(), vec![0.0]);
        let v = fiedler_vector(2, &[(0, 1)]).unwrap();
        assert_eq!(v.len(), 2);
        assert!(v[0] * v[1] < 0.0);
        assert!(v.iter().any(|x| *x > 0.0));
    }

    #[test]
    fn out_of_range_is_invalid() {
        assert!(matches!(
            fiedler_vector(1, &[(0, 1)]),
            Err(FiedlerError::Invalid(_))
        ));
    }
}
