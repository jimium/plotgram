//! Hierarchical quality metrics for hier_eval / observation (P1).
//!
//! Geometry-neutral: computed from [`LayoutResult`] + diagnostics only.
//! Does not affect layout.

use std::collections::{BTreeMap, BTreeSet};

use plotgram_model::diagnostics::HierarchicalObs;
use plotgram_model::geometry::Point;
use plotgram_model::result::LayoutResult;
use plotgram_router::core::overlap_len;

const EPS: f64 = 1e-6;
/// Default `node_gap` when the authored options omit it (HierarchicalParams).
pub const DEFAULT_NODE_GAP: f64 = 24.0;

/// Cross-axis selector for physical coordinates after orientation-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAxis {
    /// Top-to-bottom / bottom-to-top — fan mirrors along X.
    X,
    /// Left-to-right / right-to-left — fan mirrors along Y.
    Y,
}

impl CrossAxis {
    pub fn from_direction_token(token: &str) -> Self {
        match token {
            "left-to-right" | "right-to-left" | "lr" | "rl" => Self::Y,
            _ => Self::X,
        }
    }

    /// Best-effort parse from `.pgm` source (`direction: …` inside layout block).
    pub fn from_source(source: &str) -> Self {
        for line in source.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("direction:") {
                let tok = rest
                    .trim()
                    .trim_matches(',')
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                return Self::from_direction_token(tok);
            }
            if let Some(idx) = t.find("direction:") {
                let rest = &t[idx + "direction:".len()..];
                let tok = rest
                    .split([',', '}', ' '])
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                if !tok.is_empty() {
                    return Self::from_direction_token(tok);
                }
            }
        }
        Self::X
    }

    fn center(self, frame: &plotgram_model::geometry::Rect) -> f64 {
        match self {
            Self::X => frame.x + frame.width / 2.0,
            Self::Y => frame.y + frame.height / 2.0,
        }
    }

    fn extent_lo_hi(self, frame: &plotgram_model::geometry::Rect) -> (f64, f64) {
        match self {
            Self::X => (frame.x, frame.right()),
            Self::Y => (frame.y, frame.bottom()),
        }
    }
}

/// P1 hierarchical observation metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct HierQualityMetrics {
    pub symmetry_deviation_max: f64,
    pub symmetry_deviation_sum: f64,
    pub gap_uniformity_max: f64,
    /// Fraction of 1:1 chain edges whose endpoints are not cross-collinear.
    pub straightness: f64,
    pub overlap_len: f64,
    pub channel_used_gates: bool,
    pub relaxations: usize,
    pub ripup_rounds: u32,
    /// `channel-group-fallback` relaxation count (per-edge widenings plus
    /// any whole-diagram gate-route fallback; group-frame-d2.md §8.12).
    pub gate_fallback_events: usize,
}

/// Compute P1 metrics from a finished layout.
pub fn compute(result: &LayoutResult, cross: CrossAxis, node_gap: f64) -> HierQualityMetrics {
    let gap = if node_gap > EPS {
        node_gap
    } else {
        DEFAULT_NODE_GAP
    };
    let obs = result
        .diagnostics
        .hierarchical
        .as_ref()
        .cloned()
        .unwrap_or_default();

    let by_id: BTreeMap<&str, &plotgram_model::result::NodePlacement> =
        result.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Undirected adjacency + directed out/in for fan detection.
    let mut out: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut inn: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for e in &result.edges {
        out.entry(e.source.as_str())
            .or_default()
            .push(e.target.as_str());
        inn.entry(e.target.as_str())
            .or_default()
            .push(e.source.as_str());
    }
    for v in out.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    for v in inn.values_mut() {
        v.sort_unstable();
        v.dedup();
    }

    let (sym_max, sym_sum, gap_uni) = fan_metrics(&by_id, &out, &inn, cross, gap);
    let straightness = straightness_ratio(&result.edges, &out, &inn, &by_id, cross);
    let overlap = compute_overlap_len(&result.edges, &obs);

    HierQualityMetrics {
        symmetry_deviation_max: sym_max,
        symmetry_deviation_sum: sym_sum,
        gap_uniformity_max: gap_uni,
        straightness,
        overlap_len: overlap,
        channel_used_gates: obs.channel_used_gates,
        relaxations: result.diagnostics.relaxations.len(),
        ripup_rounds: obs.ripup_rounds,
        gate_fallback_events: obs.gate_fallback_events,
    }
}

fn fan_metrics(
    by_id: &BTreeMap<&str, &plotgram_model::result::NodePlacement>,
    out: &BTreeMap<&str, Vec<&str>>,
    inn: &BTreeMap<&str, Vec<&str>>,
    cross: CrossAxis,
    node_gap: f64,
) -> (f64, f64, f64) {
    let mut hubs: BTreeSet<&str> = BTreeSet::new();
    for (id, kids) in out {
        if kids.len() >= 2 {
            hubs.insert(id);
        }
    }
    for (id, parents) in inn {
        if parents.len() >= 2 {
            hubs.insert(id);
        }
    }

    let mut sum = 0.0;
    let mut max_dev = 0.0;
    let mut max_uni = 0.0;

    for hub in hubs {
        let Some(hn) = by_id.get(hub) else {
            continue;
        };
        // Prefer down-fan (children); else up-fan (parents).
        let kids: &[&str] = out
            .get(hub)
            .filter(|k| k.len() >= 2)
            .map(|v| v.as_slice())
            .or_else(|| inn.get(hub).filter(|k| k.len() >= 2).map(|v| v.as_slice()))
            .unwrap_or(&[]);
        if kids.len() < 2 {
            continue;
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let mut centers = Vec::with_capacity(kids.len());
        for &cid in kids {
            let Some(cn) = by_id.get(cid) else {
                continue;
            };
            let (a, b) = cross.extent_lo_hi(&cn.frame);
            lo = lo.min(a);
            hi = hi.max(b);
            centers.push(cross.center(&cn.frame));
        }
        if !lo.is_finite() || centers.len() < 2 {
            continue;
        }
        let mid = (lo + hi) / 2.0;
        let hub_c = cross.center(&hn.frame);
        let dev = (hub_c - mid).abs() / node_gap;
        sum += dev;
        max_dev = f64::max(max_dev, dev);

        centers.sort_by(|a, b| a.total_cmp(b));
        let gaps: Vec<f64> = centers.windows(2).map(|w| w[1] - w[0]).collect();
        if gaps.len() >= 2 {
            let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
            if mean > EPS {
                let var = gaps.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / gaps.len() as f64;
                max_uni = f64::max(max_uni, var.sqrt() / mean);
            }
        }
    }
    (max_dev, sum, max_uni)
}

/// 1:1 chain edges = both endpoints have total degree allowing a unique
/// neighbor on each end of this edge (out∪in size == 1 at each end for the
/// undirected sense of "chain link"). Count fraction whose endpoint cross
/// coords differ.
fn straightness_ratio(
    edges: &[plotgram_model::result::EdgePlacement],
    out: &BTreeMap<&str, Vec<&str>>,
    inn: &BTreeMap<&str, Vec<&str>>,
    by_id: &BTreeMap<&str, &plotgram_model::result::NodePlacement>,
    cross: CrossAxis,
) -> f64 {
    let undirected_deg = |id: &str| -> usize {
        let mut n = BTreeSet::new();
        if let Some(v) = out.get(id) {
            n.extend(v.iter().copied());
        }
        if let Some(v) = inn.get(id) {
            n.extend(v.iter().copied());
        }
        n.len()
    };

    let mut eligible = 0usize;
    let mut bent = 0usize;
    for e in edges {
        let ds = undirected_deg(&e.source);
        let dt = undirected_deg(&e.target);
        // 1:1 chain link: each end has exactly one undirected neighbor.
        if ds != 1 || dt != 1 {
            continue;
        }
        let Some(sn) = by_id.get(e.source.as_str()) else {
            continue;
        };
        let Some(tn) = by_id.get(e.target.as_str()) else {
            continue;
        };
        eligible += 1;
        if (cross.center(&sn.frame) - cross.center(&tn.frame)).abs() > EPS {
            bent += 1;
        }
    }
    if eligible == 0 {
        0.0
    } else {
        bent as f64 / eligible as f64
    }
}

fn compute_overlap_len(
    edges: &[plotgram_model::result::EdgePlacement],
    obs: &HierarchicalObs,
) -> f64 {
    let bus: BTreeSet<&str> = obs.bus_edge_ids.iter().map(|s| s.as_str()).collect();
    let mut segs: Vec<(usize, Point, Point)> = Vec::new();
    for (ei, e) in edges.iter().enumerate() {
        if bus.contains(e.id.as_str()) {
            continue;
        }
        let Some(pts) = e.path.polyline_points() else {
            continue;
        };
        for w in pts.windows(2) {
            segs.push((ei, w[0], w[1]));
        }
    }
    let mut total = 0.0;
    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            if segs[i].0 == segs[j].0 {
                continue; // same edge
            }
            total += overlap_len(segs[i].1, segs[i].2, segs[j].1, segs[j].2);
        }
    }
    total
}

/// Parse `node_gap` from layout options in source; fallback [`DEFAULT_NODE_GAP`].
pub fn node_gap_from_source(source: &str) -> f64 {
    for line in source.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("node_gap:") {
            let tok = rest.trim().trim_matches(',').trim();
            if let Ok(v) = tok.parse::<f64>() {
                return v;
            }
        }
        if let Some(idx) = t.find("node_gap:") {
            let rest = &t[idx + "node_gap:".len()..];
            let tok = rest
                .split([',', '}', ' '])
                .find(|s| !s.is_empty())
                .unwrap_or("");
            if let Ok(v) = tok.parse::<f64>() {
                return v;
            }
        }
    }
    DEFAULT_NODE_GAP
}
