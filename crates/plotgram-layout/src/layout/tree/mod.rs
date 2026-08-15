//! Tree layout skeleton: spanning forest + layered centered placer.
//!
//! Not Reingold–Tilford / Buchheim (those land with a real subtree placer).
//! Core runs in canonical top-to-bottom space; orientation is a Stage wrap
//! via [`plotgram_algo::orientation`]. Independent EdgeRouter is allowed.

mod compose;
mod ink;
mod metric;
mod params;
mod plan;

pub use params::{Orientation, TreeParams, TreePreset, TreeRoutingStyle};

use plotgram_algo::orientation as algo_orient;
use plotgram_engine_api::{LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput, LayoutWarning};
use plotgram_model::diagnostics::LayoutDiagnostics;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::{AlongSpec, PortRef, Side};
use plotgram_model::result::{EdgePath, EdgePlacement, NodePlacement};

#[derive(Debug, Default, Clone, Copy)]
pub struct TreeLayout;

impl LayoutAlgorithm for TreeLayout {
    fn name(&self) -> &'static str {
        "tree"
    }

    fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
        compute(input)
    }
}

fn compute(input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
    let bound = TreeParams::bind(input.options)?;
    let params = &bound.params;
    let mut diagnostics = LayoutDiagnostics {
        warnings: bound
            .warnings
            .iter()
            .map(|w| LayoutWarning {
                message: w.message.clone(),
            })
            .collect(),
        relaxations: Vec::new(),
        params_hash: params.hash(),
        hierarchical: None,
    };

    let plan = compose::compose(input.graph, params)?;
    for id in &plan.nodes {
        let is_root = plan.roots.iter().any(|r| r == id);
        if is_root {
            if plan.parent.contains_key(id) {
                return Err(LayoutError::message(format!(
                    "tree: invariant: root `{id}` has a parent"
                )));
            }
        } else if !plan.parent.contains_key(id) {
            return Err(LayoutError::message(format!(
                "tree: invariant: non-root `{id}` has no parent"
            )));
        }
    }
    if !plan.extra_edge_ids.is_empty() {
        diagnostics.warnings.push(LayoutWarning {
            message: format!(
                "tree: {} non-tree edge(s) are not used as parent links",
                plan.extra_edge_ids.len()
            ),
        });
    }
    let canonical_frames = metric::assign(&plan, params, input.node_sizes)?;
    let canonical_edges = ink::expand(
        input.graph,
        &plan,
        &canonical_frames,
        params,
        input.edge_geometry,
    )?;

    let orientation = to_algo_orientation(params.orientation);
    let mut nodes: Vec<NodePlacement> = plan
        .nodes
        .iter()
        .filter_map(|id| {
            let frame = canonical_frames.get(id)?;
            Some(NodePlacement {
                id: id.clone(),
                frame: canonical_rect_to_physical(orientation, *frame),
            })
        })
        .collect();

    let mut edges: Vec<EdgePlacement> = canonical_edges
        .into_iter()
        .map(|mut e| {
            e.path = transform_path(orientation, e.path);
            e.from_port = e.from_port.map(|p| port_out(orientation, p));
            e.to_port = e.to_port.map(|p| port_out(orientation, p));
            e
        })
        .collect();

    normalize_to_origin(&mut nodes, &mut edges);

    Ok(LayoutOutput {
        nodes,
        edges,
        groups: Vec::new(),
        owns_group_frames: false,
        diagnostics,
        decorations: Vec::new(),
    })
}

fn to_algo_orientation(o: Orientation) -> algo_orient::Orientation {
    match o {
        Orientation::TopToBottom => algo_orient::Orientation::Tb,
        Orientation::BottomToTop => algo_orient::Orientation::Bt,
        Orientation::LeftToRight => algo_orient::Orientation::Lr,
        Orientation::RightToLeft => algo_orient::Orientation::Rl,
    }
}

fn from_algo_point(p: algo_orient::Point) -> Point {
    Point { x: p.x, y: p.y }
}

fn to_algo_point(p: Point) -> algo_orient::Point {
    algo_orient::Point::new(p.x, p.y)
}

fn canonical_rect_to_physical(o: algo_orient::Orientation, r: Rect) -> Rect {
    let corners = [
        Point { x: r.x, y: r.y },
        Point {
            x: r.right(),
            y: r.y,
        },
        Point {
            x: r.x,
            y: r.bottom(),
        },
        Point {
            x: r.right(),
            y: r.bottom(),
        },
    ];
    let transformed: Vec<Point> = corners
        .iter()
        .map(|&p| from_algo_point(o.from_tb_point(to_algo_point(p))))
        .collect();
    let min_x = transformed
        .iter()
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min);
    let min_y = transformed
        .iter()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min);
    let max_x = transformed
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = transformed
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

fn transform_path(o: algo_orient::Orientation, path: EdgePath) -> EdgePath {
    let map = |p: Point| from_algo_point(o.from_tb_point(to_algo_point(p)));
    match path {
        EdgePath::Polyline { points } => EdgePath::polyline(points.into_iter().map(map).collect()),
        EdgePath::Cubic {
            start,
            end,
            controls,
        } => EdgePath::cubic(map(start), map(end), [map(controls[0]), map(controls[1])]),
    }
}

fn port_out(o: algo_orient::Orientation, p: PortRef) -> PortRef {
    let side = match o.from_tb_side(to_algo_side(p.side)) {
        algo_orient::Side::North => Side::North,
        algo_orient::Side::South => Side::South,
        algo_orient::Side::East => Side::East,
        algo_orient::Side::West => Side::West,
    };
    let along = match p.along {
        AlongSpec::Ordered { .. } => p.along,
        AlongSpec::LocalOffset(pt) => {
            AlongSpec::LocalOffset(from_algo_point(o.from_tb_point(to_algo_point(pt))))
        }
    };
    PortRef { side, along }
}

fn to_algo_side(s: Side) -> algo_orient::Side {
    match s {
        Side::North => algo_orient::Side::North,
        Side::South => algo_orient::Side::South,
        Side::East => algo_orient::Side::East,
        Side::West => algo_orient::Side::West,
    }
}

fn normalize_to_origin(nodes: &mut [NodePlacement], edges: &mut [EdgePlacement]) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for n in nodes.iter() {
        min_x = min_x.min(n.frame.x);
        min_y = min_y.min(n.frame.y);
    }
    for e in edges.iter() {
        for p in e.path.samples() {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() {
        return;
    }
    if min_x.abs() < 1e-9 && min_y.abs() < 1e-9 {
        return;
    }
    for n in nodes.iter_mut() {
        n.frame.x -= min_x;
        n.frame.y -= min_y;
    }
    for e in edges.iter_mut() {
        match &mut e.path {
            EdgePath::Polyline { points } => {
                for p in points {
                    p.x -= min_x;
                    p.y -= min_y;
                }
            }
            EdgePath::Cubic {
                start,
                end,
                controls,
            } => {
                start.x -= min_x;
                start.y -= min_y;
                end.x -= min_x;
                end.y -= min_y;
                controls[0].x -= min_x;
                controls[0].y -= min_y;
                controls[1].x -= min_x;
                controls[1].y -= min_y;
            }
        }
    }
}
