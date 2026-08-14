//! Hierarchical layout algorithm (Sugiyama-style): FAS → Network-Simplex
//! ranking → properify → median+transpose ordering → port finalize →
//! preliminary Metric → D1.0 TrackOrder + D1.3.4 DemandBoard LayerGap → main/
//! cross-axis coordinates (BK ideal + global VPSC, two passes) → orthogonal Ink.
//!
//! The core (`compose` / `metric` / `ink`) runs entirely in canonical
//! top-to-bottom space; this module is the only place that converts to/from
//! the physical orientation (`orient.rs`, [`plotgram_algo::orientation`]).
//! See `docs/design/layout/hierarchical/architecture.md` for the contract.

mod channel;
mod compose;
mod debug;
mod demand;
pub mod group_frame;
mod ink;
mod metric;
mod model;
mod orient;
mod params;
mod strong_macro;

pub use debug::{build_debug_trace, LayoutDebugTrace};
pub use group_frame::{GROUP_FRAME_GAP, GROUP_LABEL_TOP_PAD, GROUP_PAD};
pub use ink::verify::{
    group_penetration_violations, verify_no_group_penetration, GroupPenetrationViolation,
};
pub use metric::partition_bands::PARTITION_EMPTY_BAND_MIN;

use plotgram_algo::orientation::{self as algo_orient, Orientation as AlgoOrientation};
use plotgram_engine_api::{
    EdgeGeometryMode, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput, LayoutWarning,
};
use plotgram_model::diagnostics::{
    HierarchicalObs, LayoutDiagnostics, PartitionBandObs, PartitionRowBandObs,
};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::port::{AlongSpec, PortRef};
use plotgram_model::result::{EdgePath, EdgePlacement, GroupPlacement, NodePlacement};
use std::collections::{BTreeMap, BTreeSet};

pub use params::{
    BindResult, GroupPolicy, HierarchicalParams, HierarchicalPreset, Orientation,
    PartitionUnassigned, RoutingStyle, SymmetryPlace,
};

use compose::ports::{EdgePorts, ResolvedPort};
use model::ElemKey;
use orient::{from_algo_point, to_algo_point, to_algo_size};
use std::cell::Cell;

thread_local! {
    /// Set when ink penetration is observed under group-gate routing; the
    /// next `compute` pass forces root-scope Channel (and skips order pads).
    pub(crate) static CHANNEL_FORCE_ROOT: Cell<bool> = const { Cell::new(false) };
}

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
    let diagnostics = LayoutDiagnostics {
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

    if matches!(params.routing_style, RoutingStyle::Octilinear) {
        return Err(LayoutError::message(
            "hierarchical routing_style `octilinear` is not implemented yet",
        ));
    }

    // PG-0: partition facts must be consistent before any layout decision
    // (partition-grid.md) — hard failure, never silent.
    input
        .graph
        .validate_partition()
        .map_err(|e| LayoutError::message(format!("hierarchical: invalid partition grid: {e}")))?;

    if params.partition_unassigned == PartitionUnassigned::Reject {
        if let Some(grid) = &input.graph.partition {
            if !grid.is_empty() {
                for n in input.graph.all_nodes() {
                    if n.partition_cell.as_ref().map_or(true, |c| c.is_empty()) {
                        return Err(LayoutError::message(format!(
                            "hierarchical: node `{}` has no partition cell \
                             (partition_unassigned=reject)",
                            n.id
                        )));
                    }
                }
            }
        }
    }

    let orientation = orient::to_algo_orientation(params.orientation);

    // FAS runs once per policy; the StrongMacro front branches right after
    // (its ranking happens per block / super-graph, never globally).
    let mut real_graph = compose::graph_index::build_real_graph(input.graph);
    compose::cycle::remove_cycles(&mut real_graph);
    let canonical_size = canonical_sizes(&real_graph, input.node_sizes, orientation)?;

    if params.group_policy == GroupPolicy::StrongMacro {
        let (real_graph, plan, ports, end_bundles, frames, groups, labeled) =
            strong_macro::layout(input, params, orientation, real_graph, &canonical_size)?;
        // D1.2: Channel search + rip-up; BundlePlan (end-bus + optional corridor).
        let route_plan =
            channel::route_edges_channel(&plan, &real_graph, &ports, &end_bundles, params)?;
        return compute_channel_ink_tail(
            input,
            params,
            orientation,
            diagnostics,
            real_graph,
            plan,
            ports,
            &labeled,
            canonical_size,
            route_plan,
            TailFrames::Fixed { frames, groups },
        );
    }

    compute_weak(
        input,
        params,
        orientation,
        diagnostics,
        real_graph,
        canonical_size,
    )
}

/// Weak policy: global Sugiyama compose → Metric (`J(x)` + VPSC) → shared
/// Channel/Ink tail.
fn compute_weak<'g>(
    input: LayoutInput<'g>,
    params: &HierarchicalParams,
    orientation: algo_orient::Orientation,
    diagnostics: LayoutDiagnostics,
    mut real_graph: model::RealGraph,
    canonical_size: Vec<algo_orient::Size>,
) -> Result<(LayoutOutput, debug::Captures<'g>), LayoutError> {
    // --- Compose ---------------------------------------------------
    let mut ranks = compose::rank::assign_ranks(&real_graph)?;
    let axes = real_graph
        .partition
        .as_ref()
        .map(|g| compose::partition_axes::ConsumedAxes::from_grid(g, orientation));
    let main_rank_plan = if let Some(axes) = &axes {
        compose::partition_rank::clamp_partition_main_ranks(&real_graph, &mut ranks, axes)?
    } else {
        None
    };
    // Undirected edges: zero-span ones bypass ordering/properify/channel into
    // `intra_layer` (Ink side-links); the rest flow as normal downward edges.
    compose::properify::split_intra_layer(&mut real_graph, &ranks);
    let mut plan = compose::properify::properify(&real_graph, &ranks);
    if let Some(axes) = &axes {
        plan.partition_cross_kind = axes.cross;
        plan.partition_main_kind = axes.main;
    }
    if let (Some(axes), Some(rp)) = (axes.as_ref(), main_rank_plan.as_ref()) {
        compose::partition_rank::apply_main_rank_plan(&mut plan, &real_graph, axes, rp);
    }
    // Author edge weights feed P3 ordering (edge-parameters §2.5).
    let edge_weights: std::collections::BTreeMap<String, f64> = real_graph
        .edges
        .iter()
        .map(|e| (e.edge_id.clone(), e.weight))
        .collect();
    compose::boundary::insert_group_boundaries(&mut plan);
    // Cross-axis clamps (TB/BT → columns, LR/RL → rows). Empty cross_ids
    // early-exits inside (idempotent on the CHANNEL_FORCE_ROOT retry pass).
    if let Some(axes) = &axes {
        compose::partition_boundary::insert_partition_cross_boundaries(
            &mut plan,
            &real_graph,
            axes,
        )?;
    }
    let reversed_edges: std::collections::BTreeSet<String> = real_graph
        .edges
        .iter()
        .filter(|e| e.reversed)
        .map(|e| e.edge_id.clone())
        .collect();
    compose::order::order_layers(
        &mut plan,
        &edge_weights,
        params.group_boundary_weight,
        &reversed_edges,
    );

    // Group ids carrying a label (frame top pad reserves the label band).
    let labeled = collect_labeled_groups(&input.graph.groups);

    let port_assignment =
        compose::ports::assign_ports(&real_graph, &plan, orientation, params.auto_edge_grouping)?;
    let compose::ports::PortAssignment {
        ports,
        bundles: end_bundles,
    } = port_assignment;

    // D1.2: Channel search + rip-up; BundlePlan (end-bus + optional corridor).
    // Runs before the preliminary Metric solve — the original weak ordering,
    // preserved bit-for-bit (weak outputs must not move).
    let route_plan =
        channel::route_edges_channel(&plan, &real_graph, &ports, &end_bundles, params)?;

    // --- Metric (preliminary pass) ----------------------------------
    // Preliminary main (base layer_gap) so cross-axis pass-2 can expand
    // port anchors; TrackOrder then reads pixel X and Demand expands gaps.
    // The DemandBoard solve + final frames happen inside the shared tail
    // (TrackOrder must exist before demand publishes).
    let prelim_gaps: Vec<f64> = if plan.layers.len() > 1 {
        vec![params.layer_gap; plan.layers.len() - 1]
    } else {
        Vec::new()
    };
    let size_of = |elem_idx: usize| -> algo_orient::Size {
        elem_size(&plan, &real_graph, &canonical_size, elem_idx)
    };
    let main_prelim =
        metric::main_axis::assign_main_axis(&plan, &size_of, &prelim_gaps, params.layer_alignment);
    // Infeasibility here can only come from crossing BK blocks — a bug, not
    // a layout contingency: fail hard, never fall back (architecture.md §3.4).
    let cross = metric::cross_axis::assign_cross_axis(
        &plan,
        &real_graph,
        &ports,
        &size_of,
        &main_prelim,
        &params,
    )
    .map_err(|e| {
        LayoutError::message(format!("hierarchical: cross-axis VPSC solve failed: {e}"))
    })?;

    let prelim_frames: Vec<Rect> = (0..plan.elems.len())
        .map(|i| {
            let s = elem_size(&plan, &real_graph, &canonical_size, i);
            Rect::new(cross[i] - s.width / 2.0, main_prelim[i], s.width, s.height)
        })
        .collect();

    compute_channel_ink_tail(
        input,
        params,
        orientation,
        diagnostics,
        real_graph,
        plan,
        ports,
        &labeled,
        canonical_size,
        route_plan,
        TailFrames::WeakSolve {
            prelim_frames,
            prelim_cross: cross,
        },
    )
}

/// How the shared tail obtains final canonical frames / group frames.
enum TailFrames {
    /// Weak: prelim frames feed PortLanes/TrackOrder; the DemandBoard then
    /// resolves layer gaps and the final main-axis solve produces frames.
    /// `prelim_cross` is the raw VPSC cross vector, passed through untouched
    /// (re-deriving it from `x + w/2` is not float-identical and flips
    /// downstream tie-breaks — weak output must stay bit-stable).
    /// Group frames are solved afterwards via [`metric::group_frames`].
    WeakSolve {
        prelim_frames: Vec<Rect>,
        prelim_cross: Vec<f64>,
    },
    /// StrongMacro: the macro-block writer already placed every node frame
    /// and every group frame (strong-macro.md §5.2 / group-frame-d2.md §6.3
    /// — the tail never re-solves coordinates or group envelopes).
    Fixed {
        frames: Vec<Rect>,
        groups: Vec<GroupPlacement>,
    },
}

/// Shared Channel → TrackOrder → Bus → Ink → assemble tail, policy-agnostic
/// (strong-macro.md §5.1: the existing tail runs with no policy branch; only
/// the frame source differs).
#[allow(clippy::too_many_arguments)]
fn compute_channel_ink_tail<'g>(
    input: LayoutInput<'g>,
    params: &HierarchicalParams,
    orientation: algo_orient::Orientation,
    mut diagnostics: LayoutDiagnostics,
    real_graph: model::RealGraph,
    plan: model::PlanGraph,
    mut ports: BTreeMap<String, EdgePorts>,
    labeled: &BTreeSet<String>,
    canonical_size: Vec<algo_orient::Size>,
    route_plan: channel::ChannelRoutePlan,
    frames: TailFrames,
) -> Result<(LayoutOutput, debug::Captures<'g>), LayoutError> {
    let size_of = |elem_idx: usize| -> algo_orient::Size {
        elem_size(&plan, &real_graph, &canonical_size, elem_idx)
    };
    diagnostics
        .relaxations
        .extend(route_plan.relaxations.iter().cloned());
    let mut bus_edge_ids: Vec<String> = compose::bundle::end_bus_edge_ids(&route_plan.bundles)
        .into_iter()
        .collect();
    bus_edge_ids.sort();
    let gate_fallback_events = route_plan
        .relaxations
        .iter()
        .filter(|r| r.rule == "channel-group-fallback")
        .count();
    diagnostics.hierarchical = Some(HierarchicalObs {
        channel_used_gates: route_plan.used_gates,
        ripup_rounds: route_plan.ripup_rounds,
        bus_edge_ids,
        gate_fallback_events,
        layer_gap_demands: BTreeMap::new(),
        layer_gaps: Vec::new(),
        gate_capacity_seams: 0,
        partition_bands: Vec::new(),
        partition_row_bands: Vec::new(),
    });
    compose::verify::verify_plan(&plan, &real_graph, &ports, &route_plan)?;

    let (main, cross, canonical_frames, track_order, canonical_group_placements) = match frames {
        TailFrames::WeakSolve {
            prelim_frames,
            prelim_cross,
        } => {
            // Twin N/S corridors: absolute port lanes (expectations §6.2 /
            // port-lanes.md). Must run after cross frames exist; before
            // TrackOrder / Ink read anchors.
            metric::port_lane::apply_port_lanes(
                &plan,
                &real_graph,
                &mut ports,
                &prelim_frames,
                params.edge_gap,
            );

            // D1.1 TrackOrder (L3) on substrate tracks from pixel spans.
            let track_order = compose::track_order::assign_track_order(
                &plan,
                &real_graph,
                &ports,
                &prelim_frames,
                &route_plan.bundles,
                &route_plan,
            );
            // D1.3.4 MetricBudget DemandBoard: Channel lane counts + group
            // shell bands → LayerGap, then freeze (the band demand is
            // track-count aware, so it publishes after TrackOrder).
            let mut demand_board = demand::DemandBoard::new();
            demand::publish_channel_layer_gap_demand(
                &mut demand_board,
                &track_order,
                params.layer_gap,
                params.edge_gap,
            );
            demand::publish_group_layer_gap_demand(
                &mut demand_board,
                &plan,
                labeled,
                &track_order,
                params.edge_gap,
            );
            // D₂.2b §8.11: per-gate crossing counts raise the adjacent seams
            // (Macro-aligned cap) — a pre-route floor next to the channel /
            // shell producers. Only when gates are actually used: a
            // root-scope fallback diagram has no gate lanes to budget.
            let gate_capacity_seams = if route_plan.used_gates {
                demand::publish_gate_capacity_demand(
                    &mut demand_board,
                    &plan,
                    &real_graph,
                    params.layer_gap,
                    params.edge_gap,
                )
            } else {
                0
            };
            // PG-1: empty partition bands publish their minimum width for
            // observability / PG-2 — the cross solve reads the band plan
            // directly, so DemandBoard keeps its LayerGap-only remit.
            if let Some(bands) =
                metric::partition_bands::PartitionBandPlan::build(&plan, params.node_gap)
            {
                demand::publish_partition_band_demand(&mut demand_board, &bands);
            }
            demand::publish_partition_row_gap_demand(&mut demand_board, &plan);
            demand_board.freeze();
            let layer_gaps =
                demand::resolved_layer_gaps(plan.layers.len(), params.layer_gap, &demand_board);
            // MetricVerifier demand floor (coordinate-and-demand.md §9):
            // expose published demands + resolved gaps for eval gates.
            if let Some(obs) = diagnostics.hierarchical.as_mut() {
                obs.layer_gap_demands = demand_board.layer_gap_lower_bounds();
                obs.layer_gaps = layer_gaps.clone();
                obs.gate_capacity_seams = gate_capacity_seams;
            }
            let main = metric::main_axis::assign_main_axis(
                &plan,
                &size_of,
                &layer_gaps,
                params.layer_alignment,
            );
            let canonical_frames: Vec<Rect> = (0..plan.elems.len())
                .map(|i| {
                    let s = size_of(i);
                    Rect::new(prelim_frames[i].x, main[i], s.width, s.height)
                })
                .collect();
            // D₂.0: Weak group-frame true source = Metric Fit VPSC
            // (group-frame-d2.md §6.2). Strong never enters this arm.
            let groups = metric::group_frames::solve_group_frames(
                &input.graph.groups,
                &plan,
                &canonical_frames,
                labeled,
            )
            .map_err(|e| {
                LayoutError::message(format!("hierarchical: group frame solve failed: {e}"))
            })?;
            (main, prelim_cross, canonical_frames, track_order, groups)
        }
        TailFrames::Fixed { frames, groups } => {
            metric::port_lane::apply_port_lanes(
                &plan,
                &real_graph,
                &mut ports,
                &frames,
                params.edge_gap,
            );
            // StrongMacro: TrackOrder reads the final (macro-placed) frames.
            let track_order = compose::track_order::assign_track_order(
                &plan,
                &real_graph,
                &ports,
                &frames,
                &route_plan.bundles,
                &route_plan,
            );
            let main: Vec<f64> = frames.iter().map(|f| f.y).collect();
            let cross: Vec<f64> = (0..plan.elems.len())
                .map(|i| frames[i].x + size_of(i).width / 2.0)
                .collect();
            (main, cross, frames, track_order, groups)
        }
    };
    let shell_bands = group_frame::group_shell_bands(&plan, labeled);
    // SM-4 + D₂.0 §8.4: outer Main rails clear the layout-owned group
    // envelopes (Weak: Metric Fit; Strong: MacroBlockWriter — never a second
    // VPSC frame solve on Strong).
    let group_obstacles: Vec<(String, (f64, f64, f64, f64))> = canonical_group_placements
        .iter()
        .map(|g| {
            (
                g.id.clone(),
                (g.frame.x, g.frame.y, g.frame.right(), g.frame.bottom()),
            )
        })
        .collect();
    let lane_facts = main_lane_facts(
        &plan,
        &real_graph,
        &ports,
        &canonical_frames,
        params.edge_gap,
    );
    let (track_coords, track_relaxations) = metric::track::assign_track_coords(
        &plan,
        &main,
        &cross,
        &size_of,
        &track_order,
        &route_plan,
        &shell_bands,
        params.edge_gap,
        &group_obstacles,
        &lane_facts,
    );
    diagnostics.relaxations.extend(track_relaxations);

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
        params.port_stub,
    )?;
    // Self-loops must pass both InkVerifiers (review §2.8) — extend before
    // either check so stubs cannot bypass overlap / obstacle assertions.
    canonical_edges.extend(ink::selfloop::self_loop_edges(
        &real_graph.self_loops,
        &real_graph.ids,
        &canonical_frames,
        params.node_gap,
    ));
    // Zero-span undirected edges (side-links) — same rule: extend before the
    // verifiers so they cannot bypass overlap / endpoint assertions.
    canonical_edges.extend(ink::intralayer::intra_layer_edges(
        &real_graph.intra_layer,
        &real_graph.ids,
        &plan,
        &canonical_frames,
        params.edge_gap,
    ));
    ink::verify::verify_no_illegal_overlap(&canonical_edges, &route_plan.bundles, params.edge_gap)?;
    diagnostics
        .relaxations
        .extend(ink::verify::segment_overlap_relaxations(
            &canonical_edges,
            &route_plan.bundles,
            params.edge_gap,
        ));
    let real_frames: Vec<(String, Rect)> = real_graph
        .ids
        .iter()
        .map(|id| {
            let ei = plan.index_of[&ElemKey::Real(id.clone())];
            (id.clone(), canonical_frames[ei])
        })
        .collect();
    ink::verify::verify_endpoints_exact(&canonical_edges, &real_frames)?;
    ink::verify::verify_port_stubs_normal(
        &canonical_edges,
        matches!(params.routing_style, RoutingStyle::Orthogonal),
    )?;
    for ce in &canonical_edges {
        if let Some(bends) = ink::verify::polyline_bend_count(&ce.path) {
            if bends > params.max_bends_budget as usize {
                diagnostics
                    .relaxations
                    .push(plotgram_model::diagnostics::Relaxation {
                        rule: "ink-max-bends-budget".into(),
                        detail: format!(
                            "edge `{}` has {bends} bends > max_bends_budget={}",
                            ce.id, params.max_bends_budget
                        ),
                    });
            }
        }
    }
    match ink::verify::verify_no_node_penetration(
        &canonical_edges,
        &real_frames,
        matches!(params.routing_style, RoutingStyle::Orthogonal),
    ) {
        Ok(()) => {}
        Err(err) if route_plan.used_gates && !CHANNEL_FORCE_ROOT.get() => {
            // Gate corridors + order pads can still produce a legal Channel
            // path that pens a real node; retry once with root-scope Channel.
            let _ = err;
            CHANNEL_FORCE_ROOT.set(true);
            let out = compute(input);
            CHANNEL_FORCE_ROOT.set(false);
            return out;
        }
        Err(err) => return Err(err),
    }

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

    // Orientation-out for group frames + the same whole-graph shift applied
    // to nodes/edges (finalize passes these through unchanged).
    let groups: Vec<GroupPlacement> = canonical_group_placements
        .iter()
        .map(|g| {
            let mut frame = canonical_rect_to_physical(orientation, g.frame);
            frame.x += shift.0;
            frame.y += shift.1;
            GroupPlacement {
                id: g.id.clone(),
                frame,
            }
        })
        .collect();

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

    // PG-2/PG-3: project solved partition band intervals into physical
    // space. Column obs always physical x (left→right); row obs always
    // physical y (top→bottom). Cross/main mapping follows orientation.
    let layer_gaps_for_rows: &[f64] = diagnostics
        .hierarchical
        .as_ref()
        .map(|h| h.layer_gaps.as_slice())
        .unwrap_or(&[]);
    let has_clamps = plan.elems.iter().any(|e| e.key.is_partition_boundary());
    let (cross_bands, main_bands) = if has_clamps {
        (
            metric::partition_bands::band_coords(&plan, &cross),
            metric::partition_bands::row_band_coords(
                &plan,
                &main,
                &size_of,
                layer_gaps_for_rows,
            ),
        )
    } else if !plan.partition_columns.is_empty() || !plan.partition_rows.is_empty() {
        metric::partition_bands::band_coords_from_member_frames(
            &plan,
            &canonical_frames,
            params.node_gap,
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let (partition_bands, partition_row_bands) = project_partition_obs(
        plan.partition_cross_kind,
        &cross_bands,
        &main_bands,
        orientation,
        shift,
    );
    if let Some(obs) = diagnostics.hierarchical.as_mut() {
        obs.partition_bands = partition_bands.clone();
        obs.partition_row_bands = partition_row_bands.clone();
    }

    let channel_track_count = route_plan.substrate.tracks().count();
    let channel_used_gates = route_plan.used_gates;
    let channel_route_order = route_plan.route_order.clone();
    let channel_routes: BTreeMap<String, Vec<u32>> = route_plan
        .routes
        .iter()
        .map(|(id, topo)| {
            let channel::RouteTopology::Orthogonal(path) = topo;
            (id.clone(), path.tracks.iter().map(|t| t.0).collect())
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
        channel_route_order,
        shift,
        partition_bands,
        partition_row_bands,
    };

    Ok((
        LayoutOutput {
            nodes,
            edges,
            groups,
            owns_group_frames: true,
            diagnostics,
        },
        captures,
    ))
}

/// Canonical (TB) node sizes, dense over `real_graph.ids` — measured sizes,
/// never invented.
fn canonical_sizes(
    real_graph: &model::RealGraph,
    node_sizes: &plotgram_model::sizes::NodeSizes,
    orientation: algo_orient::Orientation,
) -> Result<Vec<algo_orient::Size>, LayoutError> {
    let mut out = vec![algo_orient::Size::new(0.0, 0.0); real_graph.ids.len()];
    for (i, id) in real_graph.ids.iter().enumerate() {
        let size = node_sizes
            .get(id)
            .ok_or_else(|| plotgram_model::MissingNodeSize {
                node_id: id.clone(),
            })?;
        out[i] = orientation.to_tb_size(to_algo_size(size));
    }
    Ok(out)
}

/// Element size: real nodes carry measured size; every zero-width elem
/// (virtual / boundary / pad) contributes none.
fn elem_size(
    plan: &model::PlanGraph,
    real_graph: &model::RealGraph,
    canonical_size: &[algo_orient::Size],
    elem_idx: usize,
) -> algo_orient::Size {
    match &plan.elems[elem_idx].key {
        ElemKey::Real(id) => canonical_size[real_graph.index_of[id]],
        ElemKey::Virtual { .. }
        | ElemKey::GroupBoundary { .. }
        | ElemKey::PartitionBoundary { .. }
        | ElemKey::OrderPad { .. } => algo_orient::Size::new(0.0, 0.0),
    }
}

/// Endpoint facts every Main corridor needs to place its X: the finalized
/// port anchors, the endpoint rank interval, and the endpoint elems.
fn main_lane_facts(
    plan: &model::PlanGraph,
    real_graph: &model::RealGraph,
    ports: &BTreeMap<String, EdgePorts>,
    frames: &[Rect],
    edge_gap: f64,
) -> metric::track::MainLaneFacts {
    let mut facts = metric::track::MainLaneFacts::default();
    for e in &real_graph.edges {
        let Some(rp) = ports.get(&e.edge_id) else {
            continue;
        };
        let src = plan
            .index_of
            .get(&ElemKey::Real(real_graph.ids[e.original_source].clone()));
        let tgt = plan
            .index_of
            .get(&ElemKey::Real(real_graph.ids[e.original_target].clone()));
        let (Some(&src), Some(&tgt)) = (src, tgt) else {
            continue;
        };
        facts.endpoint_x.insert(
            e.edge_id.clone(),
            (
                metric::anchor::port_anchor(frames[src], rp.source).x,
                metric::anchor::port_anchor(frames[tgt], rp.target).x,
            ),
        );
        let (sr, tr) = (plan.elems[src].rank as usize, plan.elems[tgt].rank as usize);
        let (lo, hi) = (sr.min(tr), sr.max(tr));
        facts.rank_span.insert(e.edge_id.clone(), (lo, hi));
        facts.endpoint_elems.insert(e.edge_id.clone(), (src, tgt));

        // A rail exactly on an E/W face makes the port stub run *along* the
        // face; keep it one clearance margin off, matching `clear_main_x`.
        let margin = edge_gap.max(1.0) * 0.5;
        let mut bounds = (f64::NEG_INFINITY, f64::INFINITY);
        for (elem, port) in [(src, rp.source), (tgt, rp.target)] {
            match port.side {
                algo_orient::Side::East => bounds.0 = bounds.0.max(frames[elem].right() + margin),
                algo_orient::Side::West => bounds.1 = bounds.1.min(frames[elem].x - margin),
                _ => {}
            }
        }
        facts.x_bounds.insert(e.edge_id.clone(), bounds);
        // Only groups that contain *both* ends: a rail may live inside a
        // group it is entirely internal to, never inside one it merely
        // enters (that crossing belongs to a gate).
        facts.own_groups.insert(
            e.edge_id.clone(),
            plan.elems[src]
                .group_path
                .iter()
                .filter(|g| plan.elems[tgt].group_path.contains(g))
                .cloned()
                .collect(),
        );

        let ns = |side| matches!(side, algo_orient::Side::North | algo_orient::Side::South);
        let (lo_side, hi_side) = if sr <= tr {
            (rp.source.side, rp.target.side)
        } else {
            (rp.target.side, rp.source.side)
        };
        let cross_lo = if ns(lo_side) { lo + 1 } else { lo };
        let cross_hi = if ns(hi_side) {
            hi.saturating_sub(1)
        } else {
            hi
        };
        facts.corridor_ranks.insert(
            e.edge_id.clone(),
            (cross_lo <= cross_hi).then_some((cross_lo, cross_hi)),
        );
    }
    facts
}

/// Group ids carrying a label (recursive — nested groups included); their
/// frame top pad reserves the label band.
fn collect_labeled_groups(
    groups: &[plotgram_model::graph::Group],
) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    fn walk(groups: &[plotgram_model::graph::Group], out: &mut std::collections::BTreeSet<String>) {
        for g in groups {
            if g.label.is_some() {
                out.insert(g.id.clone());
            }
            walk(&g.groups, out);
        }
    }
    walk(groups, &mut out);
    out
}

/// Canonical resolved port → physical [`PortRef`] (orientation-out pass).
/// Side round-trips through `from_tb_side`; `LocalOffset` is a node-local
/// vector, so the same point transform's inverse takes it back to physical
/// local coordinates.
fn port_ref_out(orientation: AlgoOrientation, rp: ResolvedPort) -> PortRef {
    let along = match rp.along {
        AlongSpec::Ordered { .. } => rp.along,
        AlongSpec::LocalOffset(p) => {
            AlongSpec::LocalOffset(from_algo_point(orientation.from_tb_point(to_algo_point(p))))
        }
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

fn sorted_pair(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn canonical_x_to_physical_x(
    o: AlgoOrientation,
    start: f64,
    end: f64,
    shift: (f64, f64),
) -> (f64, f64) {
    let p = canonical_rect_to_physical(o, Rect::new(start, 0.0, (end - start).max(0.0), 1.0));
    sorted_pair(p.x + shift.0, p.right() + shift.0)
}

fn canonical_y_to_physical_y(
    o: AlgoOrientation,
    start: f64,
    end: f64,
    shift: (f64, f64),
) -> (f64, f64) {
    let p = canonical_rect_to_physical(o, Rect::new(0.0, start, 1.0, (end - start).max(0.0)));
    sorted_pair(p.y + shift.1, p.bottom() + shift.1)
}

fn canonical_y_to_physical_x(
    o: AlgoOrientation,
    start: f64,
    end: f64,
    shift: (f64, f64),
) -> (f64, f64) {
    let p = canonical_rect_to_physical(o, Rect::new(0.0, start, 1.0, (end - start).max(0.0)));
    sorted_pair(p.x + shift.0, p.right() + shift.0)
}

fn canonical_x_to_physical_y(
    o: AlgoOrientation,
    start: f64,
    end: f64,
    shift: (f64, f64),
) -> (f64, f64) {
    let p = canonical_rect_to_physical(o, Rect::new(start, 0.0, (end - start).max(0.0), 1.0));
    sorted_pair(p.y + shift.1, p.bottom() + shift.1)
}

fn project_partition_obs(
    cross_kind: model::PartitionAxisKind,
    cross_bands: &[metric::partition_bands::PartitionBandCoords],
    main_bands: &[metric::partition_bands::PartitionRowBandCoords],
    orientation: AlgoOrientation,
    shift: (f64, f64),
) -> (Vec<PartitionBandObs>, Vec<PartitionRowBandObs>) {
    match cross_kind {
        model::PartitionAxisKind::Columns => {
            let cols = cross_bands
                .iter()
                .map(|b| {
                    let (start, end) =
                        canonical_x_to_physical_x(orientation, b.start, b.end, shift);
                    PartitionBandObs {
                        column: b.column.clone(),
                        start,
                        end,
                        empty: b.empty,
                    }
                })
                .collect();
            let rows = main_bands
                .iter()
                .map(|b| {
                    let (start, end) =
                        canonical_y_to_physical_y(orientation, b.start, b.end, shift);
                    PartitionRowBandObs {
                        row: b.row.clone(),
                        start,
                        end,
                        empty: b.empty,
                    }
                })
                .collect();
            (cols, rows)
        }
        model::PartitionAxisKind::Rows => {
            let cols = main_bands
                .iter()
                .map(|b| {
                    let (start, end) =
                        canonical_y_to_physical_x(orientation, b.start, b.end, shift);
                    PartitionBandObs {
                        column: b.row.clone(),
                        start,
                        end,
                        empty: b.empty,
                    }
                })
                .collect();
            let rows = cross_bands
                .iter()
                .map(|b| {
                    let (start, end) =
                        canonical_x_to_physical_y(orientation, b.start, b.end, shift);
                    PartitionRowBandObs {
                        row: b.column.clone(),
                        start,
                        end,
                        empty: b.empty,
                    }
                })
                .collect();
            (cols, rows)
        }
    }
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
                weight: None,
                undirected: false,
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
            assert_eq!(out.diagnostics.warnings.len(), *expected, "keys={keys:?}");
            for (w, key) in out.diagnostics.warnings.iter().zip(*keys) {
                assert!(w.message.contains(key), "{}", w.message);
            }
            // Soft relaxations only appear for Channel rip-up / group fallback;
            // this two-node (ungrouped) fixture stays empty.
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

    /// PG-0: every `PartitionError` class fails hard at the layout entry —
    /// never silent.
    #[test]
    fn invalid_partition_fails_layout_entry() {
        use plotgram_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

        fn node_with_cell(id: &str, cell: Option<PartitionCell>) -> Node {
            Node {
                id: id.to_string(),
                label: None,
                shape: None,
                role: NodeRole::Entity,
                host_group: None,
                anchor: None,
                partition_cell: cell,
                attrs: AttrMap::new(),
            }
        }

        // (grid, node cell, expected message fragment)
        let cases: &[(Option<PartitionGrid>, Option<PartitionCell>, &str)] = &[
            (None, Some(PartitionCell::col("x")), "no `partition` grid"),
            (
                Some(PartitionGrid {
                    columns: vec![PartitionAxis::new("sales")],
                    rows: vec![],
                }),
                Some(PartitionCell::col("missing")),
                "unknown partition column `missing`",
            ),
            (
                Some(PartitionGrid {
                    columns: vec![PartitionAxis::new("sales")],
                    rows: vec![PartitionAxis::new("intake")],
                }),
                Some(PartitionCell::row("missing")),
                "unknown partition row `missing`",
            ),
            (
                Some(PartitionGrid {
                    columns: vec![PartitionAxis::new("a"), PartitionAxis::new("a")],
                    rows: vec![],
                }),
                None,
                "declared more than once",
            ),
            (
                Some(PartitionGrid {
                    columns: vec![PartitionAxis::new("a")],
                    rows: vec![],
                }),
                None,
                "conflicts with a node or group id",
            ),
        ];
        for (i, (grid, cell, fragment)) in cases.iter().enumerate() {
            // Last case relies on node id `a` colliding with column id `a`.
            let node_id = if i == 4 { "a" } else { "n" };
            let graph = Graph {
                nodes: vec![node_with_cell(node_id, cell.clone())],
                edges: vec![],
                groups: vec![],
                partition: grid.clone(),
            };
            let mut sizes = NodeSizes::new();
            sizes.insert(node_id, Size::new(60.0, 30.0));
            let err = HierarchicalLayout
                .layout(LayoutInput {
                    graph: &graph,
                    node_sizes: &sizes,
                    options: &AttrMap::new(),
                    edge_geometry: EdgeGeometryMode::Builtin,
                })
                .expect_err("invalid partition must fail hard");
            let msg = err.to_string();
            assert!(
                msg.contains(fragment),
                "case {i}: `{fragment}` not in `{msg}`"
            );
        }
    }

    /// PG-4 gate: StrongMacro + grid lays out (bands after expand);
    /// LR consumes rows as cross / columns as main; rows are consumed (PG-3).
    #[test]
    fn partition_consumption_gate_boundaries() {
        use plotgram_model::partition::{PartitionAxis, PartitionGrid};

        fn graph_with_grid(with_rows: bool) -> Graph {
            Graph {
                nodes: vec![Node {
                    id: "n".to_string(),
                    label: None,
                    shape: None,
                    role: NodeRole::Entity,
                    host_group: None,
                    anchor: None,
                    partition_cell: None,
                    attrs: AttrMap::new(),
                }],
                edges: vec![],
                groups: vec![],
                partition: Some(PartitionGrid {
                    columns: vec![PartitionAxis::new("col")],
                    rows: if with_rows {
                        vec![PartitionAxis::new("row")]
                    } else {
                        vec![]
                    },
                }),
            }
        }

        fn run(graph: &Graph, options: AttrMap) -> Result<LayoutOutput, LayoutError> {
            let mut sizes = NodeSizes::new();
            sizes.insert("n", Size::new(60.0, 30.0));
            HierarchicalLayout.layout(LayoutInput {
                graph,
                node_sizes: &sizes,
                options: &options,
                edge_geometry: EdgeGeometryMode::Builtin,
            })
        }

        let opts = |pairs: &[(&str, &str)]| -> AttrMap {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), AttrValue::Str(v.to_string())))
                .collect()
        };

        // StrongMacro + grid → lays out (PG-4); unassigned node is free zone.
        let out = run(
            &graph_with_grid(false),
            opts(&[("group_policy", "strong-macro")]),
        )
        .expect("strong-macro + partition must lay out");
        assert!(
            out.diagnostics
                .warnings
                .iter()
                .all(|w| !w.message.contains("not consumed")),
            "strong-macro should consume: {:?}",
            out.diagnostics.warnings
        );

        // LR + columns-only → columns consumed as main-axis; no warning.
        let out = run(
            &graph_with_grid(false),
            opts(&[("orientation", "left-to-right")]),
        )
        .expect("LR + partition must still lay out");
        assert!(
            out.diagnostics
                .warnings
                .iter()
                .all(|w| !w.message.contains("not consumed")),
            "LR should consume: {:?}",
            out.diagnostics.warnings
        );

        // Rows declared → consumed (PG-3), no warning.
        let out = run(&graph_with_grid(true), opts(&[])).expect("rows + TB weak must lay out");
        assert!(
            out.diagnostics
                .warnings
                .iter()
                .all(|w| !w.message.contains("not consumed")),
            "rows should be consumed: {:?}",
            out.diagnostics.warnings
        );
        let obs = out.diagnostics.hierarchical.as_ref().unwrap();
        assert_eq!(obs.partition_row_bands.len(), 1);
        assert!(obs.partition_row_bands[0].empty);

        // partition_unassigned=reject with a free node fails hard.
        let err = run(
            &graph_with_grid(false),
            opts(&[("partition_unassigned", "reject")]),
        )
        .expect_err("reject + unassigned node must fail");
        assert!(
            err.to_string().contains("partition_unassigned=reject"),
            "unexpected: {err}"
        );
    }

    fn edge(id: &str, s: &str, t: &str) -> Edge {
        Edge {
            id: id.to_string(),
            source: s.to_string(),
            target: t.to_string(),
            arrow: Arrow::Forward,
            label: None,
            head_label: None,
            tail_label: None,
            from_port: None,
            to_port: None,
            weight: None,
            undirected: false,
            attrs: AttrMap::new(),
        }
    }

    /// PG-1 end-to-end: a three-column swimlane lays out with globally
    /// separated bands in declaration order (snapshot of node frames).
    #[test]
    fn three_column_swimlane_layout_matches_snapshot() {
        use plotgram_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

        let node = |id: &str, col: Option<&str>| Node {
            id: id.to_string(),
            label: None,
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: col.map(PartitionCell::col),
            attrs: AttrMap::new(),
        };
        let graph = Graph {
            nodes: vec![
                node("place_order", Some("customer")),
                node("confirm", Some("sales")),
                node("pick", Some("warehouse")),
                node("ship", Some("warehouse")),
            ],
            edges: vec![
                edge("e0", "place_order", "confirm"),
                edge("e1", "confirm", "pick"),
                edge("e2", "pick", "ship"),
            ],
            groups: vec![],
            partition: Some(PartitionGrid {
                columns: vec![
                    PartitionAxis::new("customer"),
                    PartitionAxis::new("sales"),
                    PartitionAxis::new("warehouse"),
                ],
                rows: vec![],
            }),
        };
        let mut sizes = NodeSizes::new();
        for id in ["place_order", "confirm", "pick", "ship"] {
            sizes.insert(id, Size::new(80.0, 32.0));
        }
        let out = HierarchicalLayout
            .layout(LayoutInput {
                graph: &graph,
                node_sizes: &sizes,
                options: &AttrMap::new(),
                edge_geometry: EdgeGeometryMode::Builtin,
            })
            .expect("swimlane layout");

        let mut frames: Vec<(&str, Rect)> =
            out.nodes.iter().map(|n| (n.id.as_str(), n.frame)).collect();
        frames.sort_by(|a, b| a.0.cmp(b.0));
        insta::assert_json_snapshot!(serde_json::json!(frames
            .iter()
            .map(|(id, f)| serde_json::json!({
                "id": id,
                "x": f.x,
                "y": f.y,
                "w": f.width,
                "h": f.height,
            }))
            .collect::<Vec<_>>()));

        // Bands separate in declaration order: customer < sales < warehouse.
        let span = |id: &str| {
            let f = out.nodes.iter().find(|n| n.id == id).unwrap().frame;
            (f.x, f.right())
        };
        let (_, c_r) = span("place_order");
        let (s_l, s_r) = span("confirm");
        let (w_l, _) = span("pick");
        assert!(c_r < s_l, "customer band must sit left of sales");
        assert!(s_r < w_l, "sales band must sit left of warehouse");
    }

    /// PG-2: observed band intervals are physical coordinates mirroring the
    /// solved geometry — every member frame sits inside its band and bands
    /// separate in declaration order.
    #[test]
    fn swimlane_obs_bands_mirror_node_frames() {
        use plotgram_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

        let node = |id: &str, col: Option<&str>| Node {
            id: id.to_string(),
            label: None,
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: col.map(PartitionCell::col),
            attrs: AttrMap::new(),
        };
        let graph = Graph {
            nodes: vec![
                node("place_order", Some("customer")),
                node("confirm", Some("sales")),
                node("pick", Some("warehouse")),
                node("ship", Some("warehouse")),
            ],
            edges: vec![
                edge("e0", "place_order", "confirm"),
                edge("e1", "confirm", "pick"),
                edge("e2", "pick", "ship"),
            ],
            groups: vec![],
            partition: Some(PartitionGrid {
                columns: vec![
                    PartitionAxis::new("customer"),
                    PartitionAxis::new("sales"),
                    PartitionAxis::new("warehouse"),
                ],
                rows: vec![],
            }),
        };
        let mut sizes = NodeSizes::new();
        for id in ["place_order", "confirm", "pick", "ship"] {
            sizes.insert(id, Size::new(80.0, 32.0));
        }
        let out = HierarchicalLayout
            .layout(LayoutInput {
                graph: &graph,
                node_sizes: &sizes,
                options: &AttrMap::new(),
                edge_geometry: EdgeGeometryMode::Builtin,
            })
            .expect("swimlane layout");

        let obs = out
            .diagnostics
            .hierarchical
            .as_ref()
            .expect("hierarchical obs");
        let bands = &obs.partition_bands;
        let ids: Vec<&str> = bands.iter().map(|b| b.column.as_str()).collect();
        assert_eq!(ids, vec!["customer", "sales", "warehouse"]);
        assert!(bands.iter().all(|b| !b.empty), "no empty column here");
        for (id, ci) in [
            ("place_order", 0usize),
            ("confirm", 1),
            ("pick", 2),
            ("ship", 2),
        ] {
            let f = out.nodes.iter().find(|n| n.id == id).unwrap().frame;
            let b = &bands[ci];
            assert!(
                f.x >= b.start && f.right() <= b.end,
                "{id} frame [{}, {}] must sit inside band `{}` [{}, {}]",
                f.x,
                f.right(),
                b.column,
                b.start,
                b.end
            );
        }
        for w in bands.windows(2) {
            assert!(
                w[0].end <= w[1].start,
                "bands must separate in declaration order: {:?}",
                bands
            );
        }
    }

    /// §10 discipline: a grid with no columns is NOT consumed — the layout
    /// stays bit-identical to the same graph without any partition block.
    #[test]
    fn empty_partition_grid_is_bit_identical_to_no_partition() {
        use plotgram_model::partition::PartitionGrid;

        let graph_with = |grid: Option<PartitionGrid>| Graph {
            nodes: vec![
                Node {
                    id: "a".to_string(),
                    label: None,
                    shape: None,
                    role: NodeRole::Entity,
                    host_group: None,
                    anchor: None,
                    partition_cell: None,
                    attrs: AttrMap::new(),
                },
                Node {
                    id: "b".to_string(),
                    label: None,
                    shape: None,
                    role: NodeRole::Entity,
                    host_group: None,
                    anchor: None,
                    partition_cell: None,
                    attrs: AttrMap::new(),
                },
            ],
            edges: vec![edge("e0", "a", "b")],
            groups: vec![],
            partition: grid,
        };
        let run = |grid: Option<PartitionGrid>| {
            let graph = graph_with(grid);
            let mut sizes = NodeSizes::new();
            sizes.insert("a", Size::new(60.0, 30.0));
            sizes.insert("b", Size::new(60.0, 30.0));
            HierarchicalLayout
                .layout(LayoutInput {
                    graph: &graph,
                    node_sizes: &sizes,
                    options: &AttrMap::new(),
                    edge_geometry: EdgeGeometryMode::Builtin,
                })
                .expect("layout")
        };

        let none = run(None);
        let empty_grid = run(Some(PartitionGrid::default()));
        let frames = |o: &LayoutOutput| -> Vec<(String, Rect)> {
            o.nodes.iter().map(|n| (n.id.clone(), n.frame)).collect()
        };
        let paths = |o: &LayoutOutput| -> Vec<(String, plotgram_model::result::EdgePath)> {
            o.edges
                .iter()
                .map(|e| (e.id.clone(), e.path.clone()))
                .collect()
        };
        assert_eq!(
            frames(&none),
            frames(&empty_grid),
            "node frames must be bit-identical"
        );
        assert_eq!(
            paths(&none),
            paths(&empty_grid),
            "edge geometry must be bit-identical"
        );
        // PG-2: unconsumed grids publish no bands (the obs key stays absent)
        // and diagnostics are bit-identical as well.
        for o in [&none, &empty_grid] {
            let bands = o
                .diagnostics
                .hierarchical
                .as_ref()
                .map(|h| h.partition_bands.is_empty())
                .unwrap_or(true);
            assert!(bands, "unconsumed grid must publish no partition bands");
        }
        assert_eq!(
            none.diagnostics, empty_grid.diagnostics,
            "diagnostics must be bit-identical"
        );
    }

    /// PG-3: a 2×2 matrix packs rows in declaration order and keeps columns
    /// as cross-axis bands.
    #[test]
    fn matrix_row_bands_follow_declaration_order() {
        use plotgram_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

        let node = |id: &str, col: &str, row: &str| Node {
            id: id.to_string(),
            label: None,
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: Some(PartitionCell::at(col, row)),
            attrs: AttrMap::new(),
        };
        let graph = Graph {
            nodes: vec![
                node("a0", "left", "top"),
                node("b0", "right", "top"),
                node("a1", "left", "bot"),
                node("b1", "right", "bot"),
            ],
            edges: vec![
                edge("e0", "a0", "a1"),
                edge("e1", "b0", "b1"),
            ],
            groups: vec![],
            partition: Some(PartitionGrid {
                columns: vec![PartitionAxis::new("left"), PartitionAxis::new("right")],
                rows: vec![PartitionAxis::new("top"), PartitionAxis::new("bot")],
            }),
        };
        let mut sizes = NodeSizes::new();
        for id in ["a0", "b0", "a1", "b1"] {
            sizes.insert(id, Size::new(60.0, 30.0));
        }
        let out = HierarchicalLayout
            .layout(LayoutInput {
                graph: &graph,
                node_sizes: &sizes,
                options: &AttrMap::new(),
                edge_geometry: EdgeGeometryMode::Builtin,
            })
            .expect("matrix layout");
        let obs = out.diagnostics.hierarchical.as_ref().unwrap();
        assert_eq!(
            obs.partition_bands
                .iter()
                .map(|b| b.column.as_str())
                .collect::<Vec<_>>(),
            vec!["left", "right"]
        );
        assert_eq!(
            obs.partition_row_bands
                .iter()
                .map(|b| b.row.as_str())
                .collect::<Vec<_>>(),
            vec!["top", "bot"]
        );
        let frame = |id: &str| out.nodes.iter().find(|n| n.id == id).unwrap().frame;
        assert!(frame("a0").right() <= frame("b0").x + 1e-6);
        assert!(frame("a0").bottom() <= frame("a1").y + 1e-6);
        let top = &obs.partition_row_bands[0];
        let bot = &obs.partition_row_bands[1];
        assert!(top.end <= bot.start + 1e-6, "row bands must not invert");
        for id in ["a0", "b0"] {
            let f = frame(id);
            assert!(f.y + 1e-6 >= top.start && f.bottom() <= top.end + 1e-6);
        }
        for id in ["a1", "b1"] {
            let f = frame(id);
            assert!(f.y + 1e-6 >= bot.start && f.bottom() <= bot.end + 1e-6);
        }
    }

    /// PG-4: the same 2-column grid round-trips across four orientations —
    /// members stay inside the physical column band (x) after OrientationOut.
    #[test]
    fn partition_four_orientation_round_trip() {
        use plotgram_model::partition::{PartitionAxis, PartitionCell, PartitionGrid};

        let node = |id: &str, col: &str| Node {
            id: id.to_string(),
            label: None,
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: Some(PartitionCell::col(col)),
            attrs: AttrMap::new(),
        };
        let graph = Graph {
            nodes: vec![
                node("l", "left"),
                node("r", "right"),
            ],
            edges: vec![edge("e0", "l", "r")],
            groups: vec![],
            partition: Some(PartitionGrid {
                columns: vec![PartitionAxis::new("left"), PartitionAxis::new("right")],
                rows: vec![],
            }),
        };
        let mut sizes = NodeSizes::new();
        sizes.insert("l", Size::new(60.0, 30.0));
        sizes.insert("r", Size::new(60.0, 30.0));
        for ori in [
            "top-to-bottom",
            "bottom-to-top",
            "left-to-right",
            "right-to-left",
        ] {
            let mut options = AttrMap::new();
            options.insert("orientation".into(), AttrValue::Str(ori.into()));
            let out = HierarchicalLayout
                .layout(LayoutInput {
                    graph: &graph,
                    node_sizes: &sizes,
                    options: &options,
                    edge_geometry: EdgeGeometryMode::Builtin,
                })
                .unwrap_or_else(|e| panic!("{ori}: {e}"));
            let obs = out.diagnostics.hierarchical.as_ref().unwrap();
            assert_eq!(
                obs.partition_bands.len(),
                2,
                "{ori}: expected 2 column bands, got {:?}",
                obs.partition_bands
            );
            assert_eq!(obs.partition_bands[0].column, "left");
            assert_eq!(obs.partition_bands[1].column, "right");
            let frame = |id: &str| out.nodes.iter().find(|n| n.id == id).unwrap().frame;
            for (id, ci) in [("l", 0usize), ("r", 1)] {
                let f = frame(id);
                let b = &obs.partition_bands[ci];
                assert!(
                    f.x + 1e-4 >= b.start && f.right() <= b.end + 1e-4,
                    "{ori}: node `{id}` x=[{:.3},{:.3}] escapes band {} [{:.3},{:.3}]",
                    f.x,
                    f.right(),
                    b.column,
                    b.start,
                    b.end
                );
            }
        }
    }
}
