//! Interval coloring / track assignment within a channel.
//!
//! [`color_intervals`] assigns each 1-D interval a track index so overlapping
//! intervals never share a track, via the left-edge greedy algorithm
//! (Yoshimura–Kuh 1982). Greedy by left endpoint is *optimal* for interval
//! graphs: the number of tracks equals the maximum overlap after extending
//! each interval by `gap`. Deterministic: sort by `(lo, id)` with `total_cmp`,
//! reuse the smallest-index free track, no `HashMap` anywhere.

/// Closed 1-D interval `[lo, hi]`; `lo == hi` (zero length) is allowed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    pub lo: f64,
    pub hi: f64,
}

impl Interval {
    pub fn new(lo: f64, hi: f64) -> Self {
        Self { lo, hi }
    }
}

/// Assign a track index to every interval (input index = interval id).
///
/// Returns `res` with `res[i]` = track of `intervals[i]`, tracks numbered
/// contiguously from 0. Two intervals may share a track only if they are
/// separated by at least `gap` (`earlier.hi + gap <= later.lo`); with
/// `gap == 0` touching endpoints may share a track.
///
/// The reuse test compares `end + gap <= lo` *exactly* — no tolerance.
/// Callers with floating-point noise in interval endpoints should fold
/// their tolerance into `gap` (slightly reduce it to keep noisy touching
/// intervals sharing a track, or increase it to force separation).
///
/// The greedy left-edge scheme is optimal: the produced track count equals
/// the maximum number of pairwise-conflicting intervals.
///
/// # Panics
///
/// Panics on contract violations: non-finite `lo`/`hi`/`gap`, `lo > hi`,
/// or `gap < 0`.
pub fn color_intervals(intervals: &[Interval], gap: f64) -> Vec<usize> {
    assert!(
        gap.is_finite() && gap >= 0.0,
        "gap must be finite and >= 0, got {gap}"
    );
    for (i, iv) in intervals.iter().enumerate() {
        assert!(
            iv.lo.is_finite() && iv.hi.is_finite(),
            "interval {i} has non-finite endpoint: {iv:?}"
        );
        assert!(iv.lo <= iv.hi, "interval {i} has lo > hi: {iv:?}");
    }

    // Left-edge order: by lo, then input index for a fully deterministic
    // tie-break.
    let mut order: Vec<usize> = (0..intervals.len()).collect();
    order.sort_by(|&a, &b| intervals[a].lo.total_cmp(&intervals[b].lo).then(a.cmp(&b)));

    // track_end[t] = right endpoint of the last interval placed on track t.
    // Linear scan for the smallest-index reusable track: O(m·k), explicit
    // tie-break, no float-ordering containers involved.
    let mut track_end: Vec<f64> = Vec::new();
    let mut result = vec![0usize; intervals.len()];
    for &i in &order {
        let iv = intervals[i];
        let track = track_end
            .iter()
            .position(|&end| end + gap <= iv.lo)
            .unwrap_or_else(|| {
                track_end.push(f64::NEG_INFINITY);
                track_end.len() - 1
            });
        track_end[track] = iv.hi;
        result[i] = track;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hard assertions shared by manual and random cases:
    /// - no two intervals on the same track violate the gap separation;
    /// - track ids are contiguous 0..k.
    fn check_valid(intervals: &[Interval], gap: f64, tracks: &[usize]) {
        assert_eq!(intervals.len(), tracks.len());
        for i in 0..intervals.len() {
            for j in i + 1..intervals.len() {
                if tracks[i] != tracks[j] {
                    continue;
                }
                let (a, b) = (intervals[i], intervals[j]);
                let separated = a.hi + gap <= b.lo || b.hi + gap <= a.lo;
                assert!(
                    separated,
                    "track {} holds conflicting intervals {i}:{a:?} and {j}:{b:?} (gap {gap})",
                    tracks[i]
                );
            }
        }
        if let Some(&max) = tracks.iter().max() {
            for t in 0..=max {
                assert!(tracks.contains(&t), "track ids not contiguous: {t} missing");
            }
        }
    }

    /// Sweep-line oracle: maximum overlap after extending each interval's
    /// right end by `gap` (clique number = optimal track count).
    fn max_overlap(intervals: &[Interval], gap: f64) -> usize {
        // Event: (position, +1 at lo inclusive / -1 just after hi+gap).
        // Closed intervals with `end + gap <= lo` reusable means intervals
        // [lo, hi] and [lo', hi'] conflict iff lo' < hi + gap OR touch with
        // lo' == hi + gap excluded. So the extended interval is [lo, hi+gap)
        // against the point lo'. Sort events with ends before starts at the
        // same coordinate.
        let mut events: Vec<(f64, i32)> = Vec::new();
        for iv in intervals {
            events.push((iv.lo, 1));
            events.push((iv.hi + gap, -1));
        }
        events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut cur = 0i32;
        let mut best = 0i32;
        for (_, d) in events {
            cur += d;
            best = best.max(cur);
        }
        // A zero-length interval with gap == 0 is an empty half-open range
        // and never registers in the sweep, yet still occupies one track.
        if intervals.is_empty() {
            0
        } else {
            (best as usize).max(1)
        }
    }

    #[test]
    fn manual_cases() {
        struct Case {
            name: &'static str,
            intervals: Vec<Interval>,
            gap: f64,
            expect: Vec<usize>,
        }
        let iv = Interval::new;
        let cases = vec![
            Case {
                name: "empty",
                intervals: vec![],
                gap: 0.0,
                expect: vec![],
            },
            Case {
                name: "single",
                intervals: vec![iv(0.0, 5.0)],
                gap: 0.0,
                expect: vec![0],
            },
            Case {
                name: "disjoint chain shares track 0",
                intervals: vec![iv(0.0, 1.0), iv(2.0, 3.0), iv(4.0, 5.0)],
                gap: 0.5,
                expect: vec![0, 0, 0],
            },
            Case {
                name: "nested 3 deep needs 3 tracks",
                intervals: vec![iv(0.0, 10.0), iv(1.0, 9.0), iv(2.0, 8.0)],
                gap: 0.0,
                expect: vec![0, 1, 2],
            },
            Case {
                name: "staircase overlap needs 2 tracks",
                intervals: vec![iv(0.0, 2.0), iv(1.0, 3.0), iv(2.5, 4.0)],
                gap: 0.0,
                expect: vec![0, 1, 0],
            },
            Case {
                name: "touching endpoints share with gap 0",
                intervals: vec![iv(0.0, 1.0), iv(1.0, 2.0)],
                gap: 0.0,
                expect: vec![0, 0],
            },
            Case {
                name: "touching endpoints split with gap > 0",
                intervals: vec![iv(0.0, 1.0), iv(1.0, 2.0)],
                gap: 0.25,
                expect: vec![0, 1],
            },
            Case {
                // With gap == 0 the reuse rule `end + gap <= lo` lets
                // coincident zero-length intervals share a track.
                name: "zero-length intervals",
                intervals: vec![iv(1.0, 1.0), iv(1.0, 1.0), iv(3.0, 3.0)],
                gap: 0.0,
                expect: vec![0, 0, 0],
            },
            Case {
                name: "zero-length intervals split by gap",
                intervals: vec![iv(1.0, 1.0), iv(1.0, 1.0), iv(3.0, 3.0)],
                gap: 0.5,
                expect: vec![0, 1, 0],
            },
            Case {
                name: "unsorted input keeps positional mapping",
                intervals: vec![iv(4.0, 6.0), iv(0.0, 5.0), iv(7.0, 8.0)],
                gap: 0.0,
                expect: vec![1, 0, 0],
            },
        ];
        for c in cases {
            let got = color_intervals(&c.intervals, c.gap);
            assert_eq!(got, c.expect, "case `{}`", c.name);
            check_valid(&c.intervals, c.gap, &got);
            let k = got.iter().max().map_or(0, |&m| m + 1);
            assert_eq!(
                k,
                max_overlap(&c.intervals, c.gap),
                "case `{}` optimality",
                c.name
            );
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

    #[test]
    fn random_instances_valid_optimal_deterministic() {
        let mut rng = Lcg(0xC0FFEE);
        for round in 0..30 {
            let m = rng.usize_below(81); // 0..=80
            let gap = if round % 3 == 0 {
                0.0
            } else {
                rng.f64_unit() * 2.0
            };
            let intervals: Vec<Interval> = (0..m)
                .map(|_| {
                    let lo = rng.f64_unit() * 100.0;
                    let len = rng.f64_unit() * 20.0;
                    Interval::new(lo, lo + len)
                })
                .collect();
            let tracks = color_intervals(&intervals, gap);
            check_valid(&intervals, gap, &tracks);
            let k = tracks.iter().max().map_or(0, |&t| t + 1);
            assert_eq!(
                k,
                max_overlap(&intervals, gap),
                "round {round}: greedy not optimal"
            );
            // Bit-identical double run.
            assert_eq!(tracks, color_intervals(&intervals, gap), "round {round}");
        }
    }

    #[test]
    #[should_panic(expected = "non-finite endpoint")]
    fn panics_on_nan_endpoint() {
        color_intervals(&[Interval::new(f64::NAN, 1.0)], 0.0);
    }

    #[test]
    #[should_panic(expected = "lo > hi")]
    fn panics_on_inverted_interval() {
        color_intervals(&[Interval::new(2.0, 1.0)], 0.0);
    }

    #[test]
    #[should_panic(expected = "gap must be finite")]
    fn panics_on_negative_gap() {
        color_intervals(&[Interval::new(0.0, 1.0)], -0.5);
    }
}
