//! Orthogonal path normalization (dedup, collinear merge, snap, min-segment).
//!
//! [`normalize_orthogonal`] cleans a routed polyline via the six-step
//! pipeline of reference 15 §3.1: dedup → collinear merge → spike removal →
//! axis snap → collinear merge again → minimum-segment absorption. Endpoints
//! are authoritative and never move; geometry only, no topology invention.

pub use crate::orientation::Point;

/// Options for [`normalize_orthogonal`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizeOptions {
    /// Tolerance for duplicate points, collinearity, and axis snapping (> 0).
    pub eps: f64,
    /// Minimum interior segment length; `0.0` disables step 6.
    ///
    /// Absorption matches bend coordinates by *exact* equality, so it is
    /// only effective on strictly orthogonal polylines — pair it with
    /// `snap_to_axis: true` (or feed pre-snapped input); on jittered
    /// coordinates step 6 degrades to a silent no-op.
    pub min_segment: f64,
    /// Whether to snap near-axis-aligned segments to strict orthogonality.
    pub snap_to_axis: bool,
}

impl Default for NormalizeOptions {
    fn default() -> Self {
        Self {
            eps: 1e-6,
            min_segment: 0.0,
            snap_to_axis: true,
        }
    }
}

/// Normalize a routed polyline (reference 15 §3.1, order matters):
///
/// 1. drop duplicate points (Chebyshev distance ≤ `eps`);
/// 2. merge same-direction collinear triples;
/// 3. remove degenerate A→B→A spikes;
/// 4. snap near-horizontal/vertical segments to strict axis alignment
///    (optional; endpoints pinned);
/// 5. merge collinear again (snapping creates new collinearity);
/// 6. absorb interior segments shorter than `min_segment` (Z-jogs only;
///    endpoints never move — unabsorbable short segments are kept).
///
/// The first and last input points are preserved bit-exactly. Paths with
/// fewer than 2 points are returned unchanged.
///
/// # Panics
///
/// Panics on contract violations: non-finite coordinates, `eps <= 0`,
/// or `min_segment < 0` (non-finite options included).
pub fn normalize_orthogonal(points: &[Point], opts: &NormalizeOptions) -> Vec<Point> {
    assert!(
        opts.eps.is_finite() && opts.eps > 0.0,
        "eps must be finite and > 0, got {}",
        opts.eps
    );
    assert!(
        opts.min_segment.is_finite() && opts.min_segment >= 0.0,
        "min_segment must be finite and >= 0, got {}",
        opts.min_segment
    );
    for (i, p) in points.iter().enumerate() {
        assert!(
            p.x.is_finite() && p.y.is_finite(),
            "point {i} has non-finite coordinate: {p:?}"
        );
    }
    if points.len() < 2 {
        return points.to_vec();
    }

    let eps = opts.eps;
    let mut pts = points.to_vec();
    pts = dedup_points(pts, eps); // 1
    pts = merge_collinear(pts, eps); // 2
    pts = remove_spikes(pts, eps); // 3
    if opts.snap_to_axis {
        snap_to_axis(&mut pts, eps); // 4
        pts = merge_collinear(pts, eps); // 5
    }
    if opts.min_segment > 0.0 {
        pts = enforce_min_segment(pts, eps, opts.min_segment); // 6
    }
    pts
}

fn cheb(a: Point, b: Point) -> f64 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// Step 1: drop consecutive near-duplicates. Endpoints are authoritative:
/// the exact first and last input points always survive; interior points
/// duplicating the last point are dropped instead.
fn dedup_points(pts: Vec<Point>, eps: f64) -> Vec<Point> {
    let last = *pts.last().expect("len >= 2 upheld by caller");
    let mut out = vec![pts[0]];
    for &p in &pts[1..pts.len() - 1] {
        if cheb(*out.last().expect("non-empty"), p) > eps {
            out.push(p);
        }
    }
    while out.len() > 1 && cheb(*out.last().expect("non-empty"), last) <= eps {
        out.pop();
    }
    out.push(last);
    out
}

/// Same-direction collinearity: `b` lies within `eps` of the line `a`–`c`
/// and the turn does not reverse direction (reversals are spikes, step 3).
fn collinear_same_dir(a: Point, b: Point, c: Point, eps: f64) -> bool {
    let (v1x, v1y) = (b.x - a.x, b.y - a.y);
    let (v2x, v2y) = (c.x - b.x, c.y - b.y);
    if v1x * v2x + v1y * v2y <= 0.0 {
        return false;
    }
    let (acx, acy) = (c.x - a.x, c.y - a.y);
    let ac_len = (acx * acx + acy * acy).sqrt();
    if ac_len <= eps {
        return false;
    }
    // Distance from b to the line a–c.
    (acx * v1y - acy * v1x).abs() / ac_len <= eps
}

/// Steps 2 and 5: stack-based merge — cascades resolve in one pass and the
/// first/last points can never be popped.
fn merge_collinear(pts: Vec<Point>, eps: f64) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for p in pts {
        while out.len() >= 2 && collinear_same_dir(out[out.len() - 2], out[out.len() - 1], p, eps) {
            out.pop();
        }
        out.push(p);
    }
    out
}

/// Step 3: remove degenerate A→B→A′ round trips (`|A − A′| ≤ eps`).
/// Iterates to a fixpoint with an explicit bound; keeps path endpoints.
fn remove_spikes(mut pts: Vec<Point>, eps: f64) -> Vec<Point> {
    let bound = pts.len() + 1;
    for _ in 0..bound {
        let n = pts.len();
        let Some(i) = (1..n.saturating_sub(1)).find(|&i| cheb(pts[i - 1], pts[i + 1]) <= eps)
        else {
            break;
        };
        if i + 1 < n - 1 {
            // Keep the earlier A, drop B and the duplicate A′.
            pts.drain(i..=i + 1);
        } else if i - 1 > 0 {
            // A′ is the exact path end: keep it, drop A and B instead.
            pts.drain(i - 1..=i);
        } else {
            // 3-point path whose start ≈ end: only the spike tip goes.
            pts.remove(i);
        }
    }
    pts
}

#[derive(Clone, Copy, PartialEq)]
enum SegClass {
    H,
    V,
    Other,
}

fn classify(a: Point, b: Point, eps: f64) -> SegClass {
    let (dx, dy) = ((b.x - a.x).abs(), (b.y - a.y).abs());
    if dy <= eps && dx > eps {
        SegClass::H
    } else if dx <= eps && dy > eps {
        SegClass::V
    } else {
        SegClass::Other
    }
}

/// Step 4: run-based snapping. Maximal runs of near-horizontal segments
/// share one exact `y` (near-vertical: one exact `x`). A run touching a path
/// endpoint takes that endpoint's coordinate (endpoints never move); an
/// interior run takes its first point's coordinate (deterministic). A run
/// pinned by *both* endpoints with differing coordinates is unresolvable
/// without moving an endpoint and is left as-is.
fn snap_to_axis(pts: &mut [Point], eps: f64) {
    let n = pts.len();
    let classes: Vec<SegClass> = (0..n - 1)
        .map(|i| classify(pts[i], pts[i + 1], eps))
        .collect();
    let mut s = 0;
    while s < classes.len() {
        let class = classes[s];
        if class == SegClass::Other {
            s += 1;
            continue;
        }
        let mut e = s;
        while e + 1 < classes.len() && classes[e + 1] == class {
            e += 1;
        }
        // Run covers points s..=e+1.
        let start_pinned = s == 0;
        let end_pinned = e + 1 == n - 1;
        let value = match class {
            SegClass::H => match (start_pinned, end_pinned) {
                (true, true) if pts[0].y != pts[n - 1].y => None,
                (_, true) if !start_pinned => Some(pts[n - 1].y),
                _ => Some(pts[s].y),
            },
            SegClass::V => match (start_pinned, end_pinned) {
                (true, true) if pts[0].x != pts[n - 1].x => None,
                (_, true) if !start_pinned => Some(pts[n - 1].x),
                _ => Some(pts[s].x),
            },
            SegClass::Other => unreachable!(),
        };
        if let Some(v) = value {
            for p in &mut pts[s..=e + 1] {
                match class {
                    SegClass::H => p.y = v,
                    SegClass::V => p.x = v,
                    SegClass::Other => unreachable!(),
                }
            }
        }
        s = e + 1;
    }
}

/// Step 6: absorb interior Z-jog segments shorter than `min_segment` by
/// shifting one parallel neighbour segment onto the other's line. The moved
/// bend slides along its other adjacent orthogonal segment, so orthogonality
/// is preserved; a side adjacent to a path endpoint is immovable. After each
/// absorption steps 1–3 re-run; explicit iteration bound.
fn enforce_min_segment(mut pts: Vec<Point>, eps: f64, min_segment: f64) -> Vec<Point> {
    let bound = pts.len() + 1;
    for _ in 0..bound {
        let n = pts.len();
        if n < 4 {
            break;
        }
        let mut absorbed = false;
        // Segment i spans pts[i]→pts[i+1]; interior means neighbour segments
        // exist on both sides.
        for i in 1..n - 2 {
            let (a, b) = (pts[i], pts[i + 1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len >= min_segment {
                continue;
            }
            if a.x == b.x && a.y != b.y {
                // Short vertical between two horizontals (exact post-snap).
                let pre_h = pts[i - 1].y == a.y;
                let post_h = pts[i + 2].y == b.y;
                let same_dir = (a.x - pts[i - 1].x) * (pts[i + 2].x - b.x) > 0.0;
                if !(pre_h && post_h && same_dir) {
                    continue; // U-turn or non-orthogonal context: keep.
                }
                // The moved bend must slide along a *vertical* adjacent
                // segment, or moving it would break orthogonality there.
                if i - 1 > 0 && pts[i - 2].x == pts[i - 1].x {
                    pts[i - 1].y = b.y;
                    pts[i].y = b.y;
                } else if i + 2 < n - 1 && pts[i + 3].x == pts[i + 2].x {
                    pts[i + 1].y = a.y;
                    pts[i + 2].y = a.y;
                } else {
                    continue; // Both sides immovable: keep the short jog.
                }
                absorbed = true;
            } else if a.y == b.y && a.x != b.x {
                let pre_v = pts[i - 1].x == a.x;
                let post_v = pts[i + 2].x == b.x;
                let same_dir = (a.y - pts[i - 1].y) * (pts[i + 2].y - b.y) > 0.0;
                if !(pre_v && post_v && same_dir) {
                    continue;
                }
                if i - 1 > 0 && pts[i - 2].y == pts[i - 1].y {
                    pts[i - 1].x = b.x;
                    pts[i].x = b.x;
                } else if i + 2 < n - 1 && pts[i + 3].y == pts[i + 2].y {
                    pts[i + 1].x = a.x;
                    pts[i + 2].x = a.x;
                } else {
                    continue;
                }
                absorbed = true;
            }
            if absorbed {
                break;
            }
        }
        if !absorbed {
            break;
        }
        pts = dedup_points(pts, eps);
        pts = merge_collinear(pts, eps);
        pts = remove_spikes(pts, eps);
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    /// Hard assertions shared by manual and random cases.
    fn check_normalized(input: &[Point], output: &[Point], eps: f64, require_ortho: bool) {
        // Endpoints bit-exact.
        let (fi, fo) = (input[0], output[0]);
        let (li, lo) = (input[input.len() - 1], output[output.len() - 1]);
        assert_eq!(
            (fi.x.to_bits(), fi.y.to_bits()),
            (fo.x.to_bits(), fo.y.to_bits())
        );
        assert_eq!(
            (li.x.to_bits(), li.y.to_bits()),
            (lo.x.to_bits(), lo.y.to_bits())
        );
        // No adjacent near-duplicates (a fully degenerate 2-point path may
        // legitimately remain within eps).
        if output.len() > 2 {
            for w in output.windows(2) {
                assert!(cheb(w[0], w[1]) > eps, "adjacent duplicates: {w:?}");
            }
        }
        // No same-direction collinear triples.
        for w in output.windows(3) {
            assert!(
                !collinear_same_dir(w[0], w[1], w[2], eps),
                "unmerged collinear triple: {w:?}"
            );
        }
        if require_ortho {
            for w in output.windows(2) {
                assert!(
                    w[0].x == w[1].x || w[0].y == w[1].y,
                    "segment not axis-aligned: {w:?}"
                );
            }
        }
    }

    #[test]
    fn manual_cases() {
        struct Case {
            name: &'static str,
            input: Vec<Point>,
            opts: NormalizeOptions,
            expect: Vec<Point>,
            require_ortho: bool,
        }
        let d = NormalizeOptions::default();
        let cases = vec![
            Case {
                name: "duplicate points removed",
                input: vec![
                    pt(0.0, 0.0),
                    pt(0.0, 0.0),
                    pt(5.0, 0.0),
                    pt(5.0, 0.0),
                    pt(5.0, 3.0),
                ],
                opts: d,
                expect: vec![pt(0.0, 0.0), pt(5.0, 0.0), pt(5.0, 3.0)],
                require_ortho: true,
            },
            Case {
                name: "collinear midpoint removed",
                input: vec![pt(0.0, 0.0), pt(2.0, 0.0), pt(5.0, 0.0), pt(5.0, 4.0)],
                opts: d,
                expect: vec![pt(0.0, 0.0), pt(5.0, 0.0), pt(5.0, 4.0)],
                require_ortho: true,
            },
            Case {
                name: "spike A-B-A removed",
                input: vec![pt(0.0, 0.0), pt(3.0, 0.0), pt(0.0, 0.0), pt(0.0, 4.0)],
                opts: d,
                expect: vec![pt(0.0, 0.0), pt(0.0, 4.0)],
                require_ortho: true,
            },
            Case {
                name: "near-axis segments snapped (within eps)",
                input: vec![pt(0.0, 0.0), pt(10.0, 0.0000005), pt(10.0000004, 6.0)],
                opts: d,
                expect: vec![pt(0.0, 0.0), pt(10.0000004, 0.0), pt(10.0000004, 6.0)],
                require_ortho: true,
            },
            Case {
                name: "beyond eps left untouched",
                input: vec![pt(0.0, 0.0), pt(10.0, 0.5), pt(20.0, 0.0)],
                opts: d,
                expect: vec![pt(0.0, 0.0), pt(10.0, 0.5), pt(20.0, 0.0)],
                require_ortho: false,
            },
            Case {
                name: "second collinear merge after snap",
                input: vec![
                    pt(0.0, 0.0),
                    pt(4.0, 0.0000004),
                    pt(9.0, -0.0000003),
                    pt(9.0, 5.0),
                ],
                opts: d,
                expect: vec![pt(0.0, 0.0), pt(9.0, 0.0), pt(9.0, 5.0)],
                require_ortho: true,
            },
            Case {
                name: "min_segment absorbs Z-jog",
                input: vec![
                    pt(0.0, 0.0),
                    pt(4.0, 0.0),
                    pt(4.0, 1.0),
                    pt(8.0, 1.0),
                    pt(8.0, 5.0),
                ],
                opts: NormalizeOptions {
                    min_segment: 2.0,
                    ..d
                },
                expect: vec![pt(0.0, 0.0), pt(8.0, 0.0), pt(8.0, 5.0)],
                require_ortho: true,
            },
            Case {
                name: "short segment pinned by both endpoints is kept",
                input: vec![pt(0.0, 0.0), pt(4.0, 0.0), pt(4.0, 1.0), pt(8.0, 1.0)],
                opts: NormalizeOptions {
                    min_segment: 2.0,
                    ..d
                },
                expect: vec![pt(0.0, 0.0), pt(4.0, 0.0), pt(4.0, 1.0), pt(8.0, 1.0)],
                require_ortho: true,
            },
            Case {
                name: "single point returned unchanged",
                input: vec![pt(1.0, 2.0)],
                opts: d,
                expect: vec![pt(1.0, 2.0)],
                require_ortho: false,
            },
            Case {
                name: "snap disabled keeps diagonals",
                input: vec![pt(0.0, 0.0), pt(3.0, 4.0), pt(6.0, 0.0)],
                opts: NormalizeOptions {
                    snap_to_axis: false,
                    ..d
                },
                expect: vec![pt(0.0, 0.0), pt(3.0, 4.0), pt(6.0, 0.0)],
                require_ortho: false,
            },
        ];
        for c in cases {
            let got = normalize_orthogonal(&c.input, &c.opts);
            assert_eq!(got, c.expect, "case `{}`", c.name);
            if c.input.len() >= 2 {
                check_normalized(&c.input, &got, c.opts.eps, c.require_ortho);
            }
            // Idempotence, bit-exact.
            let again = normalize_orthogonal(&got, &c.opts);
            assert_eq!(again, got, "case `{}` not idempotent", c.name);
        }
    }

    /// Deterministic LCG (no rand dependency).
    struct Lcg(u64);
    impl Lcg {
        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn usize_below(&mut self, n: usize) -> usize {
            (self.next_u64() % n as u64) as usize
        }
        fn f64_unit(&mut self) -> f64 {
            (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Random orthogonal staircases with injected duplicates, sub-eps jitter,
    /// collinear midpoints, and (some rounds) short jog segments.
    #[test]
    fn random_polylines_normalize_clean() {
        let eps = 1e-3;
        let mut rng = Lcg(0xF10A);
        for round in 0..30 {
            // Strictly alternating H/V walk on an integer-ish grid.
            let steps = 2 + rng.usize_below(14);
            let mut base = vec![pt(0.0, 0.0)];
            let mut horizontal = round % 2 == 0;
            for s in 0..steps {
                let prev = *base.last().expect("non-empty");
                let mut len = 1.0 + rng.usize_below(9) as f64;
                if round % 5 == 0 && s == steps / 2 {
                    len = 0.3; // short jog for min_segment rounds
                }
                let dir = if rng.usize_below(2) == 0 { 1.0 } else { -1.0 };
                let next = if horizontal {
                    pt(prev.x + dir * len, prev.y)
                } else {
                    pt(prev.x, prev.y + dir * len)
                };
                base.push(next);
                horizontal = !horizontal;
            }
            // Corrupt: interior duplicates, midpoints, jitter <= eps/2.
            let mut noisy: Vec<Point> = Vec::new();
            for (i, &p) in base.iter().enumerate() {
                if i > 0 {
                    let prev = base[i - 1];
                    if rng.usize_below(3) == 0 {
                        noisy.push(pt((prev.x + p.x) / 2.0, (prev.y + p.y) / 2.0));
                    }
                }
                noisy.push(p);
                if i > 0 && i < base.len() - 1 {
                    if rng.usize_below(4) == 0 {
                        noisy.push(p); // exact duplicate
                    }
                    let jx = (rng.f64_unit() - 0.5) * eps * 0.8;
                    let jy = (rng.f64_unit() - 0.5) * eps * 0.8;
                    let last = noisy.last_mut().expect("non-empty");
                    last.x += jx;
                    last.y += jy;
                }
            }
            let opts = NormalizeOptions {
                eps,
                min_segment: if round % 5 == 0 { 0.5 } else { 0.0 },
                snap_to_axis: true,
            };
            let got = normalize_orthogonal(&noisy, &opts);
            check_normalized(&noisy, &got, eps, true);
            // Bit-identical double run.
            let again = normalize_orthogonal(&noisy, &opts);
            assert_eq!(got, again, "round {round} not deterministic");
        }
    }

    #[test]
    #[should_panic(expected = "non-finite coordinate")]
    fn panics_on_nan_coordinate() {
        normalize_orthogonal(
            &[pt(f64::NAN, 0.0), pt(1.0, 0.0)],
            &NormalizeOptions::default(),
        );
    }

    #[test]
    #[should_panic(expected = "eps must be finite")]
    fn panics_on_zero_eps() {
        let opts = NormalizeOptions {
            eps: 0.0,
            ..NormalizeOptions::default()
        };
        normalize_orthogonal(&[pt(0.0, 0.0), pt(1.0, 0.0)], &opts);
    }

    #[test]
    #[should_panic(expected = "min_segment must be finite")]
    fn panics_on_negative_min_segment() {
        let opts = NormalizeOptions {
            min_segment: -1.0,
            ..NormalizeOptions::default()
        };
        normalize_orthogonal(&[pt(0.0, 0.0), pt(1.0, 0.0)], &opts);
    }
}
