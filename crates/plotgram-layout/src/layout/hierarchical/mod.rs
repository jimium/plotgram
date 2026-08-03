//! Hierarchical layout algorithm (Sugiyama-style): FAS → Network-Simplex
//! ranking → properify → median+transpose ordering → port finalize → main/
//! cross-axis coordinates (damped barycenter relaxation + VPSC) → orthogonal Ink.
//!
//! The core (`compose` / `metric` / `ink`) runs entirely in canonical
//! top-to-bottom space; this module is the only place that converts to/from
//! the physical orientation (`orient.rs`, [`plotgram_algo::orientation`]).
//! See `docs/design/layout/hierarchical/architecture.md` for the target
//! contract and `docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md`
//! for this implementation's scope decisions relative to it.

mod compose;
mod debug;
mod ink;
mod metric;
mod model;
mod orient;
mod params;

pub use debug::{build_debug_trace, LayoutDebugTrace};

use plotgram_algo::orientation::{self as algo_orient, Orientation as AlgoOrientation};
use plotgram_engine_api::{
    EdgeGeometryMode, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput,
};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::PortRef;
use plotgram_model::result::{EdgePath, EdgePlacement, NodePlacement};

pub use params::{
    BindResult, GroupAlign, GroupPolicy, GroupSizing, HierarchicalParams, HierarchicalPreset,
    Orientation, RoutingStyle,
};

use model::ElemKey;
use orient::{from_algo_point, to_algo_point, to_algo_size};

#[derive(Debug, Default, Clone, Copy)]
pub struct HierarchicalLayout;

impl LayoutAlgorithm for HierarchicalLayout {
    fn name(&self) -> &'static str {
        "hierarchical"
    }

    fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
        compute(input).map(|(output, _captures)| output)
    }
}

/// Full compose → metric → ink pipeline shared by the product `layout()`
/// and the debug trace projector ([`debug::build_debug_trace`]): same
/// decisions, one code path — the trace is a read-only projection, never a
/// second source of truth (docs/design/layout/debug-inspector.md §4.1).
fn compute(input: LayoutInput<'_>) -> Result<(LayoutOutput, debug::Captures<'_>), LayoutError> {
    let bound = HierarchicalParams::bind(input.options)?;
    let params = &bound.params;

    if params.group_policy == GroupPolicy::StrongMacro {
        return Err(LayoutError::message(
            "hierarchical: group_policy `strong-macro` is Unsupported in this build \
             (see docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md §0.4)",
        ));
    }
    if !matches!(params.routing_style, RoutingStyle::Orthogonal) {
        return Err(LayoutError::message(format!(
            "hierarchical routing_style `{}` is not implemented yet",
            params.routing_style.as_str()
        )));
    }

    let orientation = orient::to_algo_orientation(params.orientation);

    // --- Compose ---------------------------------------------------
    let mut real_graph = compose::graph_index::build_real_graph(input.graph);
    compose::cycle::remove_cycles(&mut real_graph);
    let ranks = compose::rank::assign_ranks(&real_graph)?;
    let mut plan = compose::properify::properify(&real_graph, &ranks);
    compose::order::order_layers(&mut plan);
    let ports = compose::ports::assign_ports(&real_graph, &plan, orientation);

    // --- Metric ------------------------------------------------------
    let mut canonical_size = vec![algo_orient::Size::new(0.0, 0.0); real_graph.ids.len()];
    for (i, id) in real_graph.ids.iter().enumerate() {
        let size = input
            .node_sizes
            .get(id)
            .ok_or_else(|| plotgram_model::MissingNodeSize {
                node_id: id.clone(),
            })?;
        canonical_size[i] = orientation.to_tb_size(to_algo_size(size));
    }
    let size_of = |elem_idx: usize| -> algo_orient::Size {
        match &plan.elems[elem_idx].key {
            ElemKey::Real(id) => canonical_size[real_graph.index_of[id]],
            ElemKey::Virtual { .. } => algo_orient::Size::new(0.0, 0.0),
        }
    };

    let main = metric::main_axis::assign_main_axis(&plan, &size_of, params.layer_gap);
    let cross = metric::cross_axis::assign_cross_axis(&plan, &size_of, params.node_gap);

    let canonical_frames: Vec<Rect> = (0..plan.elems.len())
        .map(|i| {
            let s = size_of(i);
            Rect::new(cross[i] - s.width / 2.0, main[i], s.width, s.height)
        })
        .collect();

    // --- Ink -----------------------------------------------------------
    let mut canonical_edges =
        ink::route::route_edges(&real_graph, &plan, &ports, &canonical_frames);
    canonical_edges.extend(ink::selfloop::self_loop_edges(
        &real_graph.self_loops,
        &real_graph.ids,
        &canonical_frames,
        params.node_gap,
    ));

    // --- Orientation-out + assemble the public contract ----------------
    let mut nodes = Vec::with_capacity(real_graph.ids.len());
    for id in &real_graph.ids {
        let elem_idx = plan.index_of[&ElemKey::Real(id.clone())];
        nodes.push(NodePlacement {
            id: id.clone(),
            frame: canonical_rect_to_physical(orientation, canonical_frames[elem_idx]),
        });
    }

    let mut edges: Vec<EdgePlacement> = canonical_edges
        .iter()
        .map(|ce| {
            let points: Vec<Point> = ce
                .path
                .iter()
                .map(|&p| from_algo_point(orientation.from_tb_point(to_algo_point(p))))
                .collect();
            EdgePlacement {
                id: ce.id.clone(),
                source: ce.source.clone(),
                target: ce.target.clone(),
                path: EdgePath::polyline(points),
                from_port: Some(PortRef {
                    side: orient::from_algo_side(orientation.from_tb_side(ce.from_port.side)),
                    slot: ce.from_port.slot,
                }),
                to_port: Some(PortRef {
                    side: orient::from_algo_side(orientation.from_tb_side(ce.to_port.side)),
                    slot: ce.to_port.slot,
                }),
            }
        })
        .collect();

    let shift = normalize_to_origin(&mut nodes, &mut edges);

    let edges = match input.edge_geometry {
        EdgeGeometryMode::Builtin => edges,
        EdgeGeometryMode::DeferToRouter => edges
            .into_iter()
            .map(|mut e| {
                e.path = EdgePath::polyline(Vec::new());
                e
            })
            .collect(),
    };

    let captures = debug::Captures {
        orientation: params.orientation,
        params: *params,
        graph: input.graph,
        real_graph,
        plan,
        ports,
        canonical_frames,
        canonical_edges,
        shift,
    };

    Ok((LayoutOutput { nodes, edges }, captures))
}

/// Orientation transforms are bit-exact but not sign-preserving (Bt/Rl can
/// produce negative coordinates) — take a canonical rect's 4 corners through
/// the transform and rebuild the physical bbox rather than assuming the
/// "top-left" corner stays the minimum corner.
pub(crate) fn canonical_rect_to_physical(o: AlgoOrientation, r: Rect) -> Rect {
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

/// `NormalizeStage`: uniform whole-graph translate so the drawing starts at
/// the origin (Bt/Rl orientation can leave everything at negative
/// coordinates — see [`canonical_rect_to_physical`]). Returns the applied
/// shift so the debug projector can apply the identical transform.
fn normalize_to_origin(nodes: &mut [NodePlacement], edges: &mut [EdgePlacement]) -> (f64, f64) {
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
        return (0.0, 0.0); // empty graph
    }
    if min_x.abs() < 1e-9 && min_y.abs() < 1e-9 {
        return (0.0, 0.0);
    }
    for n in nodes.iter_mut() {
        n.frame.x -= min_x;
        n.frame.y -= min_y;
    }
    for e in edges.iter_mut() {
        translate_edge_path(&mut e.path, -min_x, -min_y);
    }
    (-min_x, -min_y)
}

fn translate_edge_path(path: &mut EdgePath, dx: f64, dy: f64) {
    match path {
        EdgePath::Polyline { points } => {
            for p in points {
                p.x += dx;
                p.y += dy;
            }
        }
        EdgePath::Cubic {
            start,
            end,
            controls,
        } => {
            start.x += dx;
            start.y += dy;
            end.x += dx;
            end.y += dy;
            controls[0].x += dx;
            controls[0].y += dy;
            controls[1].x += dx;
            controls[1].y += dy;
        }
    }
}
