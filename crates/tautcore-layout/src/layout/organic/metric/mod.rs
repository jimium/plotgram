//! Metric: PivotMDS init → SGD stress → overlap removal → component packing.

mod overlap;
mod packing;
mod pivot_mds;
mod stress;

use std::collections::BTreeMap;

use tautcore_engine_api::LayoutError;
use tautcore_model::geometry::{Rect, Size};
use tautcore_model::sizes::NodeSizes;

use super::demand::OrgDemandBoard;
use super::geom::{aabb, frame_at, Prng};
use super::org_err;
use super::params::OrganicParams;
use super::plan::{OrgComponent, OrganicMetric, OrganicPlan};
use overlap::remove_overlaps;
use packing::pack_shelves;
use pivot_mds::{bfs_hops, pivot_mds_2d, select_pivots};
use stress::{solve as stress_solve, StressPair};

/// Components up to this size use exact all-pairs stress; larger ones use
/// sparse pairs (edges + pivots) — ebook 04 §2.4.
const ALL_PAIRS_MAX: usize = 600;

pub fn assign(
    plan: &OrganicPlan,
    params: &OrganicParams,
    sizes: &NodeSizes,
    demand: &OrgDemandBoard,
) -> Result<OrganicMetric, LayoutError> {
    let mut size_of: BTreeMap<String, Size> = BTreeMap::new();
    for id in &plan.nodes {
        let s = sizes
            .get(id)
            .ok_or_else(|| tautcore_model::MissingNodeSize {
                node_id: id.clone(),
            })?;
        size_of.insert(id.clone(), demand.node_size(id, s));
    }

    let gap = demand.node_gap(params.minimum_node_distance);
    let mut prng = Prng::new(params.seed);
    let mut frames: BTreeMap<String, Rect> = BTreeMap::new();

    for comp in &plan.components {
        let centers = layout_component(comp, &size_of, params, gap, &mut prng)?;
        for (i, id) in comp.nodes.iter().enumerate() {
            let sz = size_of.get(id).copied().unwrap_or(Size::new(0.0, 0.0));
            frames.insert(id.clone(), frame_at(sz, centers[i].0, centers[i].1));
        }
    }

    pack(plan, params, demand, &mut frames);
    Ok(OrganicMetric { frames })
}

fn layout_component(
    comp: &OrgComponent,
    size_of: &BTreeMap<String, Size>,
    params: &OrganicParams,
    gap: f64,
    prng: &mut Prng,
) -> Result<Vec<(f64, f64)>, LayoutError> {
    let n = comp.len();
    let k = params.preferred_edge_length;
    if n == 0 {
        return Ok(Vec::new());
    }
    if n == 1 {
        return Ok(vec![(0.0, 0.0)]);
    }

    // PivotMDS init; pivots double as the sparse-pair set for large graphs.
    let pivots = select_pivots(&comp.adjacency);

    // Distance model: BFS hops × preferred edge length.
    let pairs = if n <= ALL_PAIRS_MAX {
        all_pairs(comp, k)
    } else {
        sparse_pairs(comp, &pivots, k)
    };
    let pivot_dists: Vec<Vec<f64>> = pivots
        .iter()
        .map(|&p| {
            bfs_hops(&comp.adjacency, p)
                .into_iter()
                .map(|d| d.unwrap_or(0) as f64)
                .collect()
        })
        .collect();
    let mut centers = pivot_mds_2d(n, &pivot_dists);

    // Scale init so mean pair distance roughly matches the stress targets,
    // then jitter deterministically to avoid exact coincidences.
    rescale(&mut centers, &pairs);
    for p in centers.iter_mut() {
        let (jx, jy) = prng.jitter();
        p.0 += jx * 1e-3 * k;
        p.1 += jy * 1e-3 * k;
    }

    stress_solve(&mut centers, &pairs, params.iterations, prng);

    if !params.allow_node_overlaps {
        let local_sizes: Vec<Size> = comp
            .nodes
            .iter()
            .map(|id| size_of.get(id).copied().unwrap_or(Size::new(0.0, 0.0)))
            .collect();
        remove_overlaps(&mut centers, &local_sizes, gap).map_err(|e| {
            org_err(format!("organic: invariant: overlap removal failed: {e}"))
        })?;
    }

    for p in centers.iter() {
        if !p.0.is_finite() || !p.1.is_finite() {
            return Err(org_err(format!(
                "organic: invariant: non-finite coordinate in component {}",
                comp.id
            )));
        }
    }
    Ok(centers)
}

fn all_pairs(comp: &OrgComponent, k: f64) -> Vec<StressPair> {
    let n = comp.len();
    let mut pairs = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        let dist = bfs_hops(&comp.adjacency, i);
        for j in (i + 1)..n {
            let d = dist[j].map(|h| h as f64 * k).unwrap_or(k);
            pairs.push(StressPair::new(i, j, d));
        }
    }
    pairs
}

fn sparse_pairs(comp: &OrgComponent, pivots: &[usize], k: f64) -> Vec<StressPair> {
    let n = comp.len();
    let mut seen = std::collections::BTreeSet::new();
    let mut pairs = Vec::new();
    for (i, nbrs) in comp.adjacency.iter().enumerate() {
        for &j in nbrs {
            if i < j {
                seen.insert((i, j));
                pairs.push(StressPair::new(i, j, k));
            }
        }
    }
    for &p in pivots {
        let dist = bfs_hops(&comp.adjacency, p);
        for j in 0..n {
            if j == p {
                continue;
            }
            let key = if p < j { (p, j) } else { (j, p) };
            if seen.insert(key) {
                let d = dist[j].map(|h| h as f64 * k).unwrap_or(k * 3.0);
                pairs.push(StressPair::new(key.0, key.1, d));
            }
        }
    }
    pairs
}

/// Uniform scale so the mean current pair distance matches the mean target.
fn rescale(centers: &mut [(f64, f64)], pairs: &[StressPair]) {
    if centers.len() < 2 || pairs.is_empty() {
        return;
    }
    let mean_target: f64 = pairs.iter().map(|p| p.d).sum::<f64>() / pairs.len() as f64;
    let mean_current: f64 = pairs
        .iter()
        .map(|p| {
            let dx = centers[p.j].0 - centers[p.i].0;
            let dy = centers[p.j].1 - centers[p.i].1;
            (dx * dx + dy * dy).sqrt()
        })
        .sum::<f64>()
        / pairs.len() as f64;
    if mean_current < 1e-9 {
        return;
    }
    let s = mean_target / mean_current;
    for p in centers.iter_mut() {
        p.0 *= s;
        p.1 *= s;
    }
}

fn pack(
    plan: &OrganicPlan,
    params: &OrganicParams,
    demand: &OrgDemandBoard,
    frames: &mut BTreeMap<String, Rect>,
) {
    if plan.components.len() <= 1 {
        return;
    }
    let gap = demand.component_gap(params.component_gap);
    let boxes: Vec<(u32, Rect)> = plan
        .components
        .iter()
        .filter_map(|comp| {
            let iter = comp.nodes.iter().filter_map(|id| frames.get(id).copied());
            aabb(iter).map(|r| (comp.id, r))
        })
        .collect();
    let offsets = pack_shelves(&boxes, gap, params.aspect_ratio);
    for comp in &plan.components {
        let Some((dx, dy)) = offsets.get(&comp.id) else {
            continue;
        };
        for id in &comp.nodes {
            if let Some(f) = frames.get_mut(id) {
                f.x += dx;
                f.y += dy;
            }
        }
    }
}
