//! EdgeFeatures 采集与 EdgeDifficulty 评分（含量纲归一化）。

use crate::ast::Diagram;
use crate::layout::edge::edge_routing_orthogonal::estimate_layer_band_demands;
use crate::layout::edge::segment_pair::parallel_gap_for_diagram;
use crate::layout::edge_band_demand::EdgeBandDemandProfile;
use crate::layout::{LayoutResult, NodeLayout};
use std::collections::HashMap;

use super::corridor::CorridorModel;
use super::grid::{compute_grid_demand, edge_grid_overflows};
use super::pierce::obstacle_hits_for_edge;
use super::port::aggregate_port_pressure;
use super::types::{BandDemand, CorridorRisk, EdgeFeatures};

/// 归一化参考（P4.0a）。
const REF_PX: f64 = 48.0;
const REF_EDGES: f64 = 4.0;
const REF_PORT: f64 = 4.0;
const REF_HITS: f64 = 3.0;
const REF_GRID: f64 = 3.0;
const REF_SPAN: f64 = 4.0;

fn norm01(x: f64, ref_v: f64) -> f64 {
    if ref_v <= 0.0 {
        return 0.0;
    }
    (x / ref_v).clamp(0.0, 1.0)
}

/// EdgeDifficulty 权重（默认 α=1, β=2, γ=1, δ=1, ε=1, ζ=1）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DifficultyProfile {
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub delta: f64,
    pub epsilon: f64,
    pub zeta: f64,
}

impl Default for DifficultyProfile {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            beta: 2.0,
            gamma: 1.0,
            delta: 1.0,
            epsilon: 1.0,
            zeta: 1.0,
        }
    }
}

pub fn score_edge(f: &EdgeFeatures, w: &DifficultyProfile) -> f64 {
    let band_n = norm01(f.band_deficit, REF_PX);
    let corr_n = norm01(f.corridor_overflow, REF_EDGES);
    let channel = band_n.max(corr_n);
    let port_n = norm01(f.port_pressure as f64, REF_PORT);
    let hits_n = norm01(f.obstacle_hits as f64, REF_HITS);
    let grid_n = norm01(f.grid_overflow as f64, REF_GRID);
    let span_n = norm01(f.span_ranks as f64, REF_SPAN);
    w.alpha * span_n
        + w.beta * f.corridor_risk.as_weight()
        + w.gamma * channel
        + w.delta * port_n
        + w.epsilon * hits_n
        + w.zeta * grid_n
}

pub fn score_edges(features: &[EdgeFeatures], w: &DifficultyProfile) -> Vec<(usize, f64)> {
    let mut out: Vec<(usize, f64)> = features
        .iter()
        .map(|f| (f.edge_index, score_edge(f, w)))
        .collect();
    out.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    out
}

/// 优先 hints.sugiyama_ranks；缺失则 y 聚类（与 estimate_layer_band_demands 同阈值）。
pub fn resolve_ranks(
    nodes: &HashMap<String, NodeLayout>,
    sugiyama_ranks: Option<&HashMap<String, usize>>,
) -> HashMap<String, usize> {
    if let Some(ranks) = sugiyama_ranks {
        if !ranks.is_empty() {
            return ranks.clone();
        }
    }
    cluster_ranks_by_y(nodes)
}

/// 与 `estimate_layer_band_demands` 相同的 y 聚类层号。
pub fn cluster_ranks_by_y(nodes: &HashMap<String, NodeLayout>) -> HashMap<String, usize> {
    let mut centers: Vec<(String, f64)> = nodes
        .iter()
        .map(|(id, nl)| (id.clone(), nl.y + nl.height * 0.5))
        .collect();
    centers.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut layers: Vec<Vec<String>> = Vec::new();
    for (id, cy) in centers {
        if layers
            .last()
            .and_then(|l| l.first())
            .and_then(|id0| nodes.get(id0))
            .is_some_and(|nl0| (cy - (nl0.y + nl0.height * 0.5)).abs() <= 40.0)
        {
            layers.last_mut().unwrap().push(id);
        } else {
            layers.push(vec![id]);
        }
    }
    layers
        .into_iter()
        .enumerate()
        .flat_map(|(li, ents)| ents.into_iter().map(move |id| (id, li)))
        .collect()
}

fn bands_to_demand(nodes: &HashMap<String, NodeLayout>, diagram: &Diagram) -> Vec<BandDemand> {
    let parallel_gap = parallel_gap_for_diagram(diagram.diagram_type.clone());
    let profile = EdgeBandDemandProfile::for_layer_band_diagnosis(24.0);
    estimate_layer_band_demands(nodes, &diagram.relations, parallel_gap, profile)
        .into_iter()
        .map(|b| BandDemand {
            upper_layer_y: b.upper_layer_y,
            lower_layer_y: b.lower_layer_y,
            effective_gap: b.effective_gap,
            crossing_edges: b.crossing_edges,
            demand: b.demand,
            deficit: b.deficit,
        })
        .collect()
}

/// 采集边级特征（无折线）。`corridor` 可选。
pub fn collect_edge_features(
    diagram: &Diagram,
    result: &LayoutResult,
    corridor: Option<&CorridorModel>,
) -> Vec<EdgeFeatures> {
    let ranks = resolve_ranks(&result.nodes, result.hints.sugiyama_ranks.as_ref());
    let bands = bands_to_demand(&result.nodes, diagram);
    let port = aggregate_port_pressure(&result.nodes, &diagram.relations);
    let mut port_max: HashMap<String, usize> = HashMap::new();
    for p in &port {
        let e = port_max.entry(p.node_id.clone()).or_insert(0);
        *e = (*e).max(p.count);
    }

    let mut band_by_rank_pair: HashMap<(usize, usize), f64> = HashMap::new();
    let mut rank_ys: HashMap<usize, (f64, usize)> = HashMap::new();
    for (id, &r) in &ranks {
        if let Some(nl) = result.nodes.get(id) {
            let cy = nl.y + nl.height * 0.5;
            let e = rank_ys.entry(r).or_insert((0.0, 0));
            e.0 += cy;
            e.1 += 1;
        }
    }
    let mut ordered_ranks: Vec<(usize, f64)> = rank_ys
        .into_iter()
        .map(|(r, (sum, n))| (r, sum / n.max(1) as f64))
        .collect();
    ordered_ranks.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    for (i, band) in bands.iter().enumerate() {
        if i + 1 > ordered_ranks.len().saturating_sub(1) {
            break;
        }
        let ra = ordered_ranks[i].0;
        let rb = ordered_ranks[i + 1].0;
        band_by_rank_pair.insert((ra.min(rb), ra.max(rb)), band.deficit);
    }
    let max_band_def = bands.iter().map(|b| b.deficit).fold(0.0_f64, f64::max);

    let over_set = corridor
        .map(|m| m.overloaded_indices())
        .unwrap_or_default();
    let demand_by_idx: HashMap<usize, &super::types::CorridorDemand> = corridor
        .map(|m| m.demands.iter().map(|d| (d.corridor_index, d)).collect())
        .unwrap_or_default();

    let grid = compute_grid_demand(diagram, &result.nodes);
    let grid_ov = edge_grid_overflows(diagram, &result.nodes, &grid);

    let mut features = Vec::with_capacity(diagram.relations.len());
    for (edge_index, rel) in diagram.relations.iter().enumerate() {
        let from = rel.from.as_str();
        let to = rel.to.as_str();
        let rf = ranks.get(from).copied();
        let rt = ranks.get(to).copied();
        let span_ranks = match (rf, rt) {
            (Some(a), Some(b)) => a.abs_diff(b),
            _ => 0,
        };

        let mut corridor_risk = CorridorRisk::SameOrUnknown;
        let mut corridor_overflow = 0.0_f64;
        if let Some(model) = corridor {
            if model.no_chain_edges.binary_search(&edge_index).is_ok() {
                corridor_risk = CorridorRisk::NoChain;
            } else if let Some(chain) = model.edge_chains.get(&edge_index) {
                let mut overflow = 0usize;
                let mut any_over = false;
                for &c_idx in chain {
                    if let Some(d) = demand_by_idx.get(&c_idx) {
                        overflow = overflow.max(d.overflow());
                        if over_set.contains(&c_idx) {
                            any_over = true;
                        }
                    }
                }
                corridor_overflow = overflow as f64;
                corridor_risk = if any_over {
                    CorridorRisk::Overloaded
                } else {
                    CorridorRisk::ChainOk
                };
            }
        }

        let band_deficit = match (rf, rt) {
            (Some(a), Some(b)) if a != b => {
                let key = (a.min(b), a.max(b));
                band_by_rank_pair.get(&key).copied().unwrap_or_else(|| {
                    if a.abs_diff(b) == 1 {
                        max_band_def
                    } else {
                        0.0
                    }
                })
            }
            _ => 0.0,
        };

        let port_pressure = port_max
            .get(from)
            .copied()
            .unwrap_or(0)
            .max(port_max.get(to).copied().unwrap_or(0));

        let obstacle_hits = obstacle_hits_for_edge(from, to, &result.nodes);
        let grid_overflow = grid_ov.get(&edge_index).copied().unwrap_or(0);

        features.push(EdgeFeatures {
            edge_index,
            from: from.to_string(),
            to: to.to_string(),
            span_ranks,
            corridor_risk,
            band_deficit,
            corridor_overflow,
            port_pressure,
            obstacle_hits,
            grid_overflow,
        });
    }
    features.sort_by_key(|f| f.edge_index);
    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::types::LayoutHints;

    #[test]
    fn score_weights_default_normalized() {
        let f = EdgeFeatures {
            edge_index: 0,
            from: "a".into(),
            to: "b".into(),
            span_ranks: 2,
            corridor_risk: CorridorRisk::NoChain,
            band_deficit: 48.0, // norm 1
            corridor_overflow: 2.0, // norm 0.5
            port_pressure: 4,      // norm 1
            obstacle_hits: 0,
            grid_overflow: 0,
        };
        let w = DifficultyProfile::default();
        // α·(2/4) + β·1 + γ·max(1,0.5) + δ·1 + ε·0 + ζ·0
        // = 0.5 + 2 + 1 + 1 = 4.5
        assert!((score_edge(&f, &w) - 4.5).abs() < 1e-9);
    }

    #[test]
    fn cluster_ranks_two_layers() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            },
        );
        nodes.insert(
            "b".into(),
            NodeLayout {
                x: 0.0,
                y: 100.0,
                width: 40.0,
                height: 20.0,
            },
        );
        let ranks = cluster_ranks_by_y(&nodes);
        assert_eq!(ranks["a"], 0);
        assert_eq!(ranks["b"], 1);
        let _ = LayoutHints::default();
    }
}
