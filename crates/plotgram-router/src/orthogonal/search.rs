//! A* path search over `(vertex, Dir4)` states — the L2 topology writer.
//!
//! Cost = Σ segment length + `bend_penalty` × #bends (architecture.md §5.2).
//! The bend at the goal vertex (between arrival direction and the target
//! stub's approach direction) is folded into the cost of *entering* the goal,
//! so the first goal state popped is globally optimal (consistent heuristic).
//!
//! Determinism (R3): the open list is a binary heap ordered by
//! `(f, h, insertion_seq)` with `f64::total_cmp`; equal-cost ties resolve by
//! insertion order, fixed by the fixed neighbour expansion order
//! ([`Dir4::ALL`]). No `HashMap` iteration anywhere.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use plotgram_model::geometry::Point;

use super::ovg::{Dir4, Grid};

/// One open-list entry. The heap pops the *greatest* element, so `Ord` is
/// inverted below: smaller `(f, h, seq)` compares greater.
struct Open {
    f: f64,
    h: f64,
    g: f64,
    seq: u64,
    state: usize,
}

impl PartialEq for Open {
    fn eq(&self, other: &Self) -> bool {
        self.seq == other.seq // seq is unique per push
    }
}
impl Eq for Open {}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.h.total_cmp(&self.h))
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

/// Least-cost orthogonal path on `grid` from `start` to `goal`.
///
/// - `start_dir`: direction the edge arrives at `start` (source stub
///   direction); turning at `start` costs one bend.
/// - `approach_dir`: direction of travel from `goal` to the target anchor
///   (opposite of the target's outward normal); the bend at `goal` between
///   the arrival direction and `approach_dir` is folded into the entry cost.
/// - `blocked`: closed-collision predicate for one grid step (lazy
///   visibility check with the edge's own-node exemption).
///
/// Returns grid points from `start` to `goal` inclusive, or `None` when no
/// collision-free path exists.
pub fn astar(
    grid: &Grid,
    start: (u32, u32),
    start_dir: Dir4,
    goal: (u32, u32),
    approach_dir: Dir4,
    bend_penalty: f64,
    blocked: impl Fn(Point, Point) -> bool,
) -> Option<Vec<Point>> {
    let (nx, ny) = grid.dims();
    let vertex_count = nx * ny;
    if vertex_count == 0 {
        return None;
    }
    let vidx = |xi: u32, yi: u32| (yi as usize) * nx + (xi as usize);
    let state_of = |v: usize, d: Dir4| v * 4 + d as usize;
    let goal_v = vidx(goal.0, goal.1);
    let goal_p = grid.point(goal.0, goal.1);
    // Manhattan distance on actual coordinates — admissible (ignores bends)
    // and consistent on the grid.
    let h = |v: usize| {
        let p = grid.point((v % nx) as u32, (v / nx) as u32);
        (p.x - goal_p.x).abs() + (p.y - goal_p.y).abs()
    };

    const NONE: u32 = u32::MAX;
    let mut best = vec![f64::INFINITY; vertex_count * 4];
    let mut parent = vec![NONE; vertex_count * 4];
    let mut heap = BinaryHeap::new();
    let mut seq = 0u64;

    let start_state = state_of(vidx(start.0, start.1), start_dir);
    best[start_state] = 0.0;
    let h0 = h(vidx(start.0, start.1));
    heap.push(Open {
        f: h0,
        h: h0,
        g: 0.0,
        seq,
        state: start_state,
    });
    seq += 1;

    while let Some(open) = heap.pop() {
        if open.g > best[open.state] {
            continue; // stale entry superseded by a cheaper push
        }
        let v = open.state / 4;
        let dir = Dir4::from_index(open.state % 4);
        if v == goal_v {
            return Some(reconstruct(grid, nx, &parent, open.state));
        }
        let xi = (v % nx) as u32;
        let yi = (v / nx) as u32;
        let from = grid.point(xi, yi);
        for nd in Dir4::ALL {
            let Some((nxi, nyi)) = grid.step(xi, yi, nd) else {
                continue;
            };
            let to = grid.point(nxi, nyi);
            if blocked(from, to) {
                continue;
            }
            let step_len = (to.x - from.x).abs() + (to.y - from.y).abs();
            let mut ng = open.g + step_len;
            if nd != dir {
                ng += bend_penalty; // bend at `from`
            }
            let nv = vidx(nxi, nyi);
            if nv == goal_v && nd != approach_dir {
                ng += bend_penalty; // bend at `goal` (folded into entry cost)
            }
            let nstate = state_of(nv, nd);
            if ng < best[nstate] {
                best[nstate] = ng;
                parent[nstate] = open.state as u32;
                let nh = h(nv);
                heap.push(Open {
                    f: ng + nh,
                    h: nh,
                    g: ng,
                    seq,
                    state: nstate,
                });
                seq += 1;
            }
        }
    }
    None
}

/// Walk parent pointers from the goal state back to the start state.
fn reconstruct(grid: &Grid, nx: usize, parent: &[u32], goal_state: usize) -> Vec<Point> {
    let mut pts = Vec::new();
    let mut cur = goal_state;
    loop {
        let v = cur / 4;
        pts.push(grid.point((v % nx) as u32, (v / nx) as u32));
        let par = parent[cur];
        if par == u32::MAX {
            break;
        }
        cur = par as usize;
    }
    pts.reverse();
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grid from explicit lines; `blocked_edges` lists vertex pairs that are
    /// not visible ((x_index, y_index) pairs, either direction).
    fn test_grid(xs: &[f64], ys: &[f64]) -> Grid {
        let extra: Vec<Point> = xs
            .iter()
            .flat_map(|&x| ys.iter().map(move |&y| Point { x, y }))
            .collect();
        Grid::build(&[], &extra, 0.0)
    }

    fn never_blocked(_: Point, _: Point) -> bool {
        false
    }

    #[test]
    fn straight_when_clear() {
        let g = test_grid(&[0.0, 10.0, 20.0], &[5.0]);
        let path = astar(&g, (0, 0), Dir4::East, (2, 0), Dir4::East, 100.0, never_blocked)
            .expect("reachable");
        assert_eq!(
            path,
            vec![
                Point { x: 0.0, y: 5.0 },
                Point { x: 10.0, y: 5.0 },
                Point { x: 20.0, y: 5.0 },
            ]
        );
    }

    #[test]
    fn l_path_counts_bends_deterministically() {
        // 2x2 grid; two symmetric equal-cost L paths exist — the tie must
        // resolve identically on every run (fixed expansion order + seq).
        let run = || {
            let g = test_grid(&[0.0, 10.0], &[0.0, 10.0]);
            astar(&g, (0, 0), Dir4::East, (1, 1), Dir4::South, 100.0, never_blocked)
                .expect("reachable")
        };
        let p1 = run();
        let p2 = run();
        assert_eq!(p1, p2);
        assert_eq!(p1.len(), 3);
        assert_eq!(p1.first().copied(), Some(Point { x: 0.0, y: 0.0 }));
        assert_eq!(p1.last().copied(), Some(Point { x: 10.0, y: 10.0 }));
        // start dir East + approach dir South: the (0,0)→(10,0)→(10,10) path
        // has exactly 1 bend; East-first wins the tie by expansion order.
        assert_eq!(p1[1], Point { x: 10.0, y: 0.0 });
    }

    #[test]
    fn detours_around_blocked_steps() {
        // 3x3 grid; the direct row y=1 is blocked at (0,1)→(1,1).
        let g = test_grid(&[0.0, 10.0, 20.0], &[0.0, 10.0, 20.0]);
        let blocked = |a: Point, b: Point| {
            (a == Point { x: 0.0, y: 10.0 } && b == Point { x: 10.0, y: 10.0 })
                || (a == Point { x: 10.0, y: 10.0 } && b == Point { x: 0.0, y: 10.0 })
        };
        let path = astar(&g, (0, 1), Dir4::East, (2, 1), Dir4::East, 100.0, blocked)
            .expect("detour exists");
        assert_eq!(path.first().copied(), Some(Point { x: 0.0, y: 10.0 }));
        assert_eq!(path.last().copied(), Some(Point { x: 20.0, y: 10.0 }));
        // Must leave the middle row to get around the blocked step.
        assert!(path.iter().any(|p| p.y != 10.0));
    }

    #[test]
    fn unreachable_goal_returns_none() {
        // Goal (1,0) is isolated: both its incident steps are blocked.
        let g = test_grid(&[0.0, 10.0, 20.0], &[0.0]);
        let goal = Point { x: 10.0, y: 0.0 };
        let blocked = move |a: Point, b: Point| a == goal || b == goal;
        assert_eq!(
            astar(&g, (0, 0), Dir4::East, (1, 0), Dir4::East, 100.0, blocked),
            None
        );
    }

    #[test]
    fn zero_bend_penalty_prefers_shortest() {
        // With bend_penalty = 0 the L path and straight-ish paths tie on
        // length; result is still deterministic and endpoint-correct.
        let g = test_grid(&[0.0, 10.0], &[0.0, 10.0]);
        let p = astar(&g, (0, 0), Dir4::East, (1, 1), Dir4::South, 0.0, never_blocked)
            .expect("reachable");
        assert_eq!(p.first().copied(), Some(Point { x: 0.0, y: 0.0 }));
        assert_eq!(p.last().copied(), Some(Point { x: 10.0, y: 10.0 }));
    }
}
