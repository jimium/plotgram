//! 粗网格 congestion：L 骨架栅格化流量。

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::{LayoutResult, NodeLayout};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

use super::pierce::{preferred_l_skeleton, LSkeleton};

/// 与 `ORTHO_SLOT_PITCH` 数值对齐，语义独立为 grid pitch。
pub const GRID_DEMAND_PITCH: f64 = 40.0;

/// 边级 soft_cap：cell load 超过此值计入 overflow。
pub const GRID_SOFT_CAP: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct GridCellKey {
    pub ix: i64,
    pub iy: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GridDemand {
    pub pitch: f64,
    /// 确定性：BTreeMap
    pub loads: BTreeMap<GridCellKey, usize>,
}

impl GridDemand {
    pub fn max_load(&self) -> usize {
        self.loads.values().copied().max().unwrap_or(0)
    }
}

fn cell_key(p: Point, pitch: f64) -> GridCellKey {
    GridCellKey {
        ix: (p.x / pitch).floor() as i64,
        iy: (p.y / pitch).floor() as i64,
    }
}

/// 沿正交段按 pitch 步长采样累加（含端点）。
fn accumulate_segment(loads: &mut BTreeMap<GridCellKey, usize>, a: Point, b: Point, pitch: f64) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = dx.abs().max(dy.abs());
    if len < 1e-6 {
        *loads.entry(cell_key(a, pitch)).or_insert(0) += 1;
        return;
    }
    let steps = ((len / pitch).ceil() as usize).max(1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let p = Point::new(a.x + dx * t, a.y + dy * t);
        *loads.entry(cell_key(p, pitch)).or_insert(0) += 1;
    }
}

fn accumulate_skeleton(
    loads: &mut BTreeMap<GridCellKey, usize>,
    from: Point,
    to: Point,
    sk: &LSkeleton,
    pitch: f64,
) {
    for (a, b) in sk.segments(from, to) {
        accumulate_segment(loads, a, b, pitch);
    }
}

fn skeleton_max_cell_load(
    loads: &BTreeMap<GridCellKey, usize>,
    from: Point,
    to: Point,
    sk: &LSkeleton,
    pitch: f64,
) -> usize {
    let mut tmp = BTreeMap::new();
    accumulate_skeleton(&mut tmp, from, to, sk, pitch);
    tmp.keys()
        .map(|k| loads.get(k).copied().unwrap_or(0))
        .max()
        .unwrap_or(0)
}

/// 全图 GridDemand：每边贡献一次（取 hits 更少的 L）。
pub fn compute_grid_demand(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
) -> GridDemand {
    let pitch = GRID_DEMAND_PITCH;
    let mut loads: BTreeMap<GridCellKey, usize> = BTreeMap::new();
    let mut edges: Vec<(usize, &str, &str)> = diagram
        .relations
        .iter()
        .enumerate()
        .map(|(i, r)| (i, r.from.as_str(), r.to.as_str()))
        .collect();
    edges.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, from, to) in edges {
        if from == to {
            continue;
        }
        if let Some((fp, tp, sk)) = preferred_l_skeleton(from, to, nodes) {
            accumulate_skeleton(&mut loads, fp, tp, &sk, pitch);
        }
    }
    GridDemand { pitch, loads }
}

/// 边级 grid_overflow = max(0, max_cell_on_edge - soft_cap)。
pub fn edge_grid_overflows(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    grid: &GridDemand,
) -> HashMap<usize, usize> {
    let mut out = HashMap::new();
    for (i, rel) in diagram.relations.iter().enumerate() {
        let from = rel.from.as_str();
        let to = rel.to.as_str();
        let Some((fp, tp, sk)) = preferred_l_skeleton(from, to, nodes) else {
            out.insert(i, 0);
            continue;
        };
        let mx = skeleton_max_cell_load(&grid.loads, fp, tp, &sk, grid.pitch);
        out.insert(i, mx.saturating_sub(GRID_SOFT_CAP));
    }
    out
}

pub fn compute_grid_demand_from_result(diagram: &Diagram, result: &LayoutResult) -> GridDemand {
    compute_grid_demand(diagram, &result.nodes)
}

pub fn dump_grid_demand_if_enabled(diagram: &Diagram, result: &LayoutResult) {
    if std::env::var_os("PLOTGRAM_DUMP_GRID_DEMAND").is_none() {
        return;
    }
    let grid = compute_grid_demand_from_result(diagram, result);
    crate::perf_log!(
        "[grid-demand] pitch={:.0} cells={} max_load={}",
        grid.pitch,
        grid.loads.len(),
        grid.max_load()
    );
    for (k, load) in grid.loads.iter().filter(|(_, l)| **l > 0).take(16) {
        crate::perf_log!("  cell[{},{}] load={}", k.ix, k.iy, load);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
    use std::collections::HashMap;

    fn rel(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn grid_deterministic_and_nonzero() {
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".into(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
        );
        nodes.insert(
            "b".into(),
            NodeLayout {
                x: 200.0,
                y: 200.0,
                width: 20.0,
                height: 20.0,
            },
        );
        let diagram = crate::ast::Diagram {
            relations: vec![rel("a", "b")],
            ..Default::default()
        };
        let g1 = compute_grid_demand(&diagram, &nodes);
        let g2 = compute_grid_demand(&diagram, &nodes);
        assert_eq!(g1.loads, g2.loads);
        assert!(g1.max_load() >= 1);
    }
}
