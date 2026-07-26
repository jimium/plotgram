//! Stage 3：Channel IR 挂接（镜像 `group_ir`）。
//!
//! 生产路径走 [`crate::layout::kernel::coordinate::main_axis::solve_main_axis_with_cross_tracks`]。
//! 本模块保留 gap 抬升辅助与诊断用 `attach_channel_ir_cross_tracks`。

use std::collections::{BTreeMap, HashMap};

use crate::layout::atlas::channel::{Occupancy, Substrate, TrackOrient};
use crate::layout::atlas::channel_metric::{channel_band_width, inflate_layer_gaps};
use crate::layout::atlas::plan::Plan;
use crate::layout::kernel::coordinate::model::{
    ConstraintSource, ConstraintSourceKind, CoordinateProblem, HardConstraint, NodeVariable,
    SolveAxis, VarKind,
};
use crate::layout::NodeLayout;

/// 由 Plan + Substrate 回放占用后，按 Cross track 抬高层间缝。
pub fn inflate_gaps_from_channel(
    base_gaps: &[f64],
    plan: &Plan,
    substrate: &Substrate,
    occupancy: &Occupancy,
) -> Vec<f64> {
    let rank_count = plan.substrate.rank_count;
    let mut dem: BTreeMap<usize, f64> = BTreeMap::new();
    for t in substrate.tracks() {
        if t.orient != TrackOrient::Cross {
            continue;
        }
        let lanes = occupancy.lane_demand(t.id);
        if lanes == 0 {
            continue;
        }
        let line = t.line;
        if line == 0 || line >= rank_count {
            continue;
        }
        let gap_idx = line - 1;
        let need = channel_band_width(lanes);
        let e = dem.entry(gap_idx).or_insert(0.0);
        *e = e.max(need);
    }
    inflate_layer_gaps(base_gaps, &dem)
}

/// 诊断 / 扩展：为已有 Main 轴问题追加 Track 变量与分离约束。
///
/// 正确映射：扫描 `VarKind::Axis` 的 `rank` 字段得到层顶 var_id。
pub fn attach_channel_ir_cross_tracks(
    problem: &mut CoordinateProblem,
    substrate: &Substrate,
    occupancy: &Occupancy,
    rank_count: usize,
) {
    if problem.axis != SolveAxis::Main {
        return;
    }
    let mut rank_vars: Vec<Option<usize>> = vec![None; rank_count];
    for v in &problem.vars {
        if v.kind == VarKind::Axis && v.rank < rank_count {
            rank_vars[v.rank] = Some(v.var_id);
        }
    }

    let mut tracks: Vec<_> = substrate
        .tracks()
        .filter(|t| t.orient == TrackOrient::Cross)
        .collect();
    tracks.sort_by_key(|t| t.id);

    for t in tracks {
        let lanes = occupancy.lane_demand(t.id);
        if lanes == 0 {
            continue;
        }
        let line = t.line;
        if line == 0 || line >= rank_count {
            continue;
        }
        let (Some(left), Some(right)) = (rank_vars[line - 1], rank_vars.get(line).copied().flatten())
        else {
            continue;
        };
        let band = channel_band_width(lanes);
        let half = band * 0.5;
        let left_h = problem.vars[left].axis_size;
        let vid = problem.vars.len();
        problem.vars.push(NodeVariable {
            var_id: vid,
            stable_id: format!("track#{}", t.id.0),
            kind: VarKind::Track,
            rank: line - 1,
            order: usize::MAX,
            axis_size: band,
            movable: true,
        });
        let left_top = problem.initial.values.get(left).copied().unwrap_or(0.0);
        let right_top = problem.initial.values.get(right).copied().unwrap_or(left_top + left_h + band);
        problem.initial.values.push((left_top + left_h + right_top) * 0.5);

        problem.hard.push(HardConstraint::MinSeparation {
            left,
            right: vid,
            distance: left_h + half,
            source: ConstraintSource {
                kind: ConstraintSourceKind::RouteDemand,
                nodes: vec![format!("track#{}", t.id.0)],
                note: "channel cross-track below previous rank",
            },
        });
        problem.hard.push(HardConstraint::MinSeparation {
            left: vid,
            right,
            distance: half,
            source: ConstraintSource {
                kind: ConstraintSourceKind::RouteDemand,
                nodes: vec![format!("track#{}", t.id.0)],
                note: "channel cross-track above next rank",
            },
        });
    }
}

/// 分治路径：按全局 rank 缝 Demand 把下方节点整体下推。
///
/// 测量当前相邻 rank 的最小缝，不足则整体平移 `rank > gap_idx` 的节点。
pub fn expand_nodes_by_cross_gap_demands(
    nodes: &mut HashMap<String, NodeLayout>,
    ranks: &HashMap<String, usize>,
    gap_demands: &BTreeMap<usize, f64>,
    horizontal: bool,
) {
    if gap_demands.is_empty() || nodes.is_empty() {
        return;
    }
    let max_rank = ranks.values().copied().max().unwrap_or(0);
    // 每 rank 的主轴 span：[min, max] of bottom/top
    let mut rank_lo: Vec<f64> = vec![f64::INFINITY; max_rank + 1];
    let mut rank_hi: Vec<f64> = vec![f64::NEG_INFINITY; max_rank + 1];
    for (id, nl) in nodes.iter() {
        let Some(&r) = ranks.get(id) else {
            continue;
        };
        let (lo, hi) = if horizontal {
            (nl.x, nl.x + nl.width)
        } else {
            (nl.y, nl.y + nl.height)
        };
        rank_lo[r] = rank_lo[r].min(lo);
        rank_hi[r] = rank_hi[r].max(hi);
    }

    let mut cumulative: Vec<f64> = vec![0.0; max_rank + 1];
    for gap_idx in 0..max_rank {
        let need = gap_demands.get(&gap_idx).copied().unwrap_or(0.0);
        let actual = if rank_lo[gap_idx + 1].is_finite() && rank_hi[gap_idx].is_finite() {
            rank_lo[gap_idx + 1] - rank_hi[gap_idx]
        } else {
            need
        };
        let extra = (need - actual).max(0.0);
        cumulative[gap_idx + 1] = cumulative[gap_idx] + extra;
    }
    // 传播：rank r 的位移 = 所有 gap < r 的累计
    for r in 1..=max_rank {
        // already set as prefix sum above
        let _ = r;
    }

    let shift = cumulative[max_rank];
    if shift <= 1e-9 {
        return;
    }
    for (id, nl) in nodes.iter_mut() {
        let Some(&r) = ranks.get(id) else {
            continue;
        };
        let dy = cumulative[r];
        if dy <= 1e-9 {
            continue;
        }
        if horizontal {
            nl.x += dy;
        } else {
            nl.y += dy;
        }
    }
}

/// Cross 轴：按 Main 走廊 Demand 撑开同层相邻节点间距。
pub fn expand_layer_order_gaps(
    nodes: &mut HashMap<String, NodeLayout>,
    layers: &[Vec<String>],
    gap_demands: &BTreeMap<usize, f64>,
    horizontal: bool,
) {
    if gap_demands.is_empty() {
        return;
    }
    for layer in layers {
        if layer.len() < 2 {
            continue;
        }
        let mut ordered: Vec<String> = layer.clone();
        ordered.sort_by(|a, b| {
            let (ca, cb) = match (nodes.get(a), nodes.get(b)) {
                (Some(na), Some(nb)) => {
                    if horizontal {
                        (na.y, nb.y)
                    } else {
                        (na.x, nb.x)
                    }
                }
                _ => (0.0, 0.0),
            };
            ca.partial_cmp(&cb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });

        for i in 0..ordered.len().saturating_sub(1) {
            let need = gap_demands.get(&i).copied().unwrap_or(0.0);
            if need <= 0.0 {
                continue;
            }
            let left_id = ordered[i].clone();
            let right_id = ordered[i + 1].clone();
            let (Some(nl), Some(nr)) = (nodes.get(&left_id).cloned(), nodes.get(&right_id).cloned())
            else {
                continue;
            };
            let (left_hi, right_lo) = if horizontal {
                (nl.y + nl.height, nr.y)
            } else {
                (nl.x + nl.width, nr.x)
            };
            let extra = (need - (right_lo - left_hi)).max(0.0);
            if extra <= 1e-9 {
                continue;
            }
            for id in ordered.iter().skip(i + 1) {
                if let Some(n) = nodes.get_mut(id) {
                    if horizontal {
                        n.y += extra;
                    } else {
                        n.x += extra;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::atlas::channel::{GateId, TrackId};
    use crate::layout::atlas::plan::{Plan, SubstrateSketch};

    #[test]
    fn inflate_noop_when_no_occupancy() {
        let plan = Plan {
            substrate: SubstrateSketch {
                rank_count: 3,
                order_count: 2,
            },
            ..Plan::default()
        };
        let substrate = Substrate::default();
        let occ = Occupancy::new();
        let base = vec![40.0, 40.0];
        let out = inflate_gaps_from_channel(&base, &plan, &substrate, &occ);
        assert_eq!(out, base);
        let _ = (TrackId(0), GateId(0));
    }

    #[test]
    fn expand_nodes_pushes_lower_ranks() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
                ..Default::default()
            },
        );
        nodes.insert(
            "b".into(),
            NodeLayout {
                x: 0.0,
                y: 50.0,
                width: 40.0,
                height: 40.0,
                ..Default::default()
            },
        );
        let mut ranks = HashMap::new();
        ranks.insert("a".into(), 0);
        ranks.insert("b".into(), 1);
        let mut dem = BTreeMap::new();
        dem.insert(0, 40.0); // need 40, actual gap = 10 → push 30
        expand_nodes_by_cross_gap_demands(&mut nodes, &ranks, &dem, false);
        assert!((nodes["b"].y - 80.0).abs() < 1e-6);
        assert!((nodes["a"].y - 0.0).abs() < 1e-6);
    }
}
