//! Hierarchical layout algorithm (Sugiyama-style): FAS → Network-Simplex
//! ranking → properify → median+transpose ordering → port finalize →
//! preliminary Metric → D1.0 TrackOrder + DemandBoard LayerGap → main/
//! cross-axis coordinates (BK ideal + global VPSC, two passes) → orthogonal Ink.
//!
//! The core (`compose` / `metric` / `ink`) runs entirely in canonical
//! top-to-bottom space; this module is the only place that converts to/from
//! the physical orientation (`orient.rs`, [`plotgram_algo::orientation`]).
//! See `docs/design/layout/hierarchical/architecture.md` for the target
//! contract and `docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md`
//! for this implementation's scope decisions relative to it.

mod channel;
mod compose;
mod debug;
mod demand;
mod ink;
mod metric;
mod model;
mod orient;
mod params;

pub use debug::{build_debug_trace, LayoutDebugTrace};

use plotgram_algo::orientation::{self as algo_orient, Orientation as AlgoOrientation};
use plotgram_engine_api::{
    EdgeGeometryMode, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput, LayoutWarning,
};
use plotgram_model::diagnostics::LayoutDiagnostics;
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::{AlongSpec, PortRef};
use plotgram_model::result::{EdgePath, EdgePlacement, NodePlacement};
use std::collections::BTreeMap;

pub use params::{
    BindResult, GroupAlign, GroupPolicy, GroupSizing, HierarchicalParams, HierarchicalPreset,
    Orientation, RoutingStyle,
};

use model::ElemKey;
use compose::ports::ResolvedPort;
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

    // Diagnostics exit (roadmap phase C): bind warnings surface here instead
    // of being dropped; hard failures above/below stay hard failures.
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
    };

    if params.group_policy == GroupPolicy::StrongMacro {
        return Err(LayoutError::message(
            "hierarchical: group_policy `strong-macro` is Unsupported in this build \
             (see docs/design/layout/hierarchical/notes/2026-08-02-mvp-scope.md §0.4)",
        ));
    }
    if matches!(params.routing_style, RoutingStyle::Octilinear) {
        return Err(LayoutError::message(
            "hierarchical routing_style `octilinear` is not implemented yet",
        ));
    }

    let orientation = orient::to_algo_orientation(params.orientation);

    // --- Compose ---------------------------------------------------
    let mut real_graph = compose::graph_index::build_real_graph(input.graph);
    compose::cycle::remove_cycles(&mut real_graph);
    let ranks = compose::rank::assign_ranks(&real_graph)?;
    let mut plan = compose::properify::properify(&real_graph, &ranks);
    // Author critical-path marks feed P3 ordering weights (edge-parameters §2.5).
    let critical_edges: std::collections::BTreeSet<String> = real_graph
        .edges
        .iter()
        .filter(|e| e.critical)
        .map(|e| e.edge_id.clone())
        .collect();
    compose::order::order_layers(&mut plan, &critical_edges);

    // Canonical node sizes are needed before port finalize (FIXED_POS
    // boundary validation) — measured sizes, never invented.
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

    let port_assignment = compose::ports::assign_ports(
        &real_graph,
        &plan,
        orientation,
        &canonical_size,
        params.auto_edge_grouping,
    )?;
    let compose::ports::PortAssignment {
        ports,
        bundles: end_bundles,
    } = port_assignment;

    // --- Metric ------------------------------------------------------
    let size_of = |elem_idx: usize| -> algo_orient::Size {
        match &plan.elems[elem_idx].key {
            ElemKey::Real(id) => canonical_size[real_graph.index_of[id]],
            ElemKey::Virtual { .. } => algo_orient::Size::new(0.0, 0.0),
        }
    };

    // D1.2: Channel search + rip-up; BundlePlan (end-bus + optional corridor).
    let route_plan =
        channel::route_edges_channel(&plan, &real_graph, &ports, &end_bundles, params)?;
    diagnostics.relaxations.extend(route_plan.relaxations.iter().cloned());
    compose::verify::verify_plan(&plan, &real_graph, &ports, &route_plan)?;

    // Preliminary main (base layer_gap) so cross-axis pass-2 can expand
    // port anchors; TrackOrder then reads pixel X and Demand expands gaps.
    let prelim_gaps: Vec<f64> = if plan.layers.len() > 1 {
        vec![params.layer_gap; plan.layers.len() - 1]
    } else {
        Vec::new()
    };
    let main_prelim = metric::main_axis::assign_main_axis(&plan, &size_of, &prelim_gaps);
    // Infeasibility here can only come from crossing BK blocks — a bug, not
    // a layout contingency: fail hard, never fall back (architecture.md §3.4).
    let cross = metric::cross_axis::assign_cross_axis(
        &plan,
        &real_graph,
        &ports,
        &size_of,
        &main_prelim,
        params.node_gap,
    )
    .map_err(|e| {
        LayoutError::message(format!("hierarchical: cross-axis VPSC solve failed: {e}"))
    })?;

    let prelim_frames: Vec<Rect> = (0..plan.elems.len())
        .map(|i| {
            let s = size_of(i);
            Rect::new(
                cross[i] - s.width / 2.0,
                main_prelim[i],
                s.width,
                s.height,
            )
        })
        .collect();

    // D1.1 TrackOrder (L3) on substrate tracks from pixel spans.
    let track_order = compose::track_order::assign_track_order(
        &plan,
        &real_graph,
        &ports,
        &prelim_frames,
        &route_plan.bundles,
        &route_plan,
    );
    let layer_gaps = demand::resolved_layer_gaps(
        plan.layers.len(),
        params.layer_gap,
        params.edge_gap,
        &track_order,
    );
    let main = metric::main_axis::assign_main_axis(&plan, &size_of, &layer_gaps);

    let canonical_frames: Vec<Rect> = (0..plan.elems.len())
        .map(|i| {
            let s = size_of(i);
            Rect::new(cross[i] - s.width / 2.0, main[i], s.width, s.height)
        })
        .collect();

    let track_coords = metric::track::assign_track_coords(
        &plan,
        &main,
        &cross,
        &size_of,
        &track_order,
        &route_plan,
        params.edge_gap,
    );

    let bus_levels = metric::bus::assign_bus_levels(
        &route_plan.bundles,
        &ports,
        &real_graph,
        &plan,
        &canonical_frames,
        params.layer_gap,
    );

    // --- Ink -----------------------------------------------------------
    let mut canonical_edges = ink::route::route_edges(
        &real_graph,
        &plan,
        &ports,
        &canonical_frames,
        &bus_levels,
        &route_plan,
        &track_order,
        &track_coords,
        params.routing_style,
        params.layer_gap,
        params.node_gap,
    )?;
    ink::verify::verify_no_illegal_overlap(&canonical_edges, &route_plan.bundles)?;
    let real_frames: Vec<(String, Rect)> = real_graph
        .ids
        .iter()
        .map(|id| {
            let ei = plan.index_of[&ElemKey::Real(id.clone())];
            (id.clone(), canonical_frames[ei])
        })
        .collect();
    ink::verify::verify_no_node_penetration(&canonical_edges, &real_frames)?;
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
            let transform = |p: Point| from_algo_point(orientation.from_tb_point(to_algo_point(p)));
            let path = match &ce.path {
                ink::route::InkPath::Polyline(points) => {
                    EdgePath::polyline(points.iter().copied().map(transform).collect())
                }
                ink::route::InkPath::Cubic {
                    start,
                    end,
                    controls,
                } => EdgePath::cubic(
                    transform(*start),
                    transform(*end),
                    [transform(controls[0]), transform(controls[1])],
                ),
            };
            EdgePlacement {
                id: ce.id.clone(),
                source: ce.source.clone(),
                target: ce.target.clone(),
                path,
                from_port: Some(port_ref_out(orientation, ce.from_port)),
                to_port: Some(port_ref_out(orientation, ce.to_port)),
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

    let channel_track_count = route_plan.substrate.tracks().count();
    let channel_used_gates = route_plan.used_gates;
    let channel_routes: BTreeMap<String, Vec<u32>> = route_plan
        .routes
        .iter()
        .map(|(id, topo)| {
            let channel::RouteTopology::Orthogonal(path) = topo;
            (
                id.clone(),
                path.tracks.iter().map(|t| t.0).collect(),
            )
        })
        .collect();

    let captures = debug::Captures {
        orientation: params.orientation,
        params: *params,
        graph: input.graph,
        real_graph,
        plan,
        ports,
        canonical_frames,
        canonical_edges,
        channel_routes,
        channel_track_count,
        channel_used_gates,
        shift,
    };

    Ok((
        LayoutOutput {
            nodes,
            edges,
            diagnostics,
        },
        captures,
    ))
}

/// Canonical resolved port → physical [`PortRef`] (orientation-out pass).
/// Side round-trips through `from_tb_side`; `LocalOffset` is a node-local
/// vector, so the same point transform's inverse takes it back to physical
/// local coordinates.
fn port_ref_out(orientation: AlgoOrientation, rp: ResolvedPort) -> PortRef {
    let along = match rp.along {
        AlongSpec::Ordered { .. } => rp.along,
        AlongSpec::LocalOffset(p) => AlongSpec::LocalOffset(from_algo_point(
            orientation.from_tb_point(to_algo_point(p)),
        )),
    };
    PortRef {
        side: orient::from_algo_side(orientation.from_tb_side(rp.side)),
        along,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::attr::{AttrMap, AttrValue};
    use plotgram_model::geometry::Size;
    use plotgram_model::graph::{Arrow, Edge, Graph, Node, NodeRole};
    use plotgram_model::sizes::NodeSizes;

    fn layout_with_options(options: AttrMap) -> LayoutOutput {
        let node = |id: &str| Node {
            id: id.to_string(),
            label: None,
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs: AttrMap::new(),
        };
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![Edge {
                id: "e0".to_string(),
                source: "a".to_string(),
                target: "b".to_string(),
                arrow: Arrow::Forward,
                label: None,
                head_label: None,
                tail_label: None,
                from_port: None,
                to_port: None,
                critical: false,
                attrs: AttrMap::new(),
            }],
            groups: vec![],
            partition: None,
        };
        let mut sizes = NodeSizes::new();
        sizes.insert("a", Size::new(60.0, 30.0));
        sizes.insert("b", Size::new(60.0, 30.0));
        HierarchicalLayout
            .layout(LayoutInput {
                graph: &graph,
                node_sizes: &sizes,
                options: &options,
                edge_geometry: EdgeGeometryMode::Builtin,
            })
            .expect("minimal two-node layout must succeed")
    }

    #[test]
    fn diagnostics_carry_bind_warnings_and_params_hash() {
        // Table: (extra option keys, expected warning count).
        let cases: &[(&[&str], usize)] = &[(&[], 0), (&["bogus"], 1), (&["aaa", "zzz"], 2)];
        for (keys, expected) in cases {
            let options: AttrMap = keys
                .iter()
                .map(|k| ((*k).to_string(), AttrValue::Num(1.0)))
                .collect();
            let out = layout_with_options(options);
            assert_eq!(
                out.diagnostics.warnings.len(),
                *expected,
                "keys={keys:?}"
            );
            for (w, key) in out.diagnostics.warnings.iter().zip(*keys) {
                assert!(w.message.contains(key), "{}", w.message);
            }
            // No soft relaxation exists in this build — the channel stays empty.
            assert!(out.diagnostics.relaxations.is_empty());
            assert_eq!(out.diagnostics.params_hash.len(), 16);
        }
    }

    #[test]
    fn params_hash_attributes_layout_changes_to_params() {
        let base = layout_with_options(AttrMap::new());
        let same = layout_with_options(AttrMap::new());
        assert_eq!(base.diagnostics.params_hash, same.diagnostics.params_hash);

        let mut changed = AttrMap::new();
        changed.insert("node_gap".to_string(), AttrValue::Num(99.0));
        let other = layout_with_options(changed);
        assert_ne!(base.diagnostics.params_hash, other.diagnostics.params_hash);
    }
}
