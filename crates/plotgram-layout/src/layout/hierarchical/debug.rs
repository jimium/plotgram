//! DebugTrace projection for the hierarchical kernel (debug-profile.md).
//!
//! Read-only projection of this run's compose/metric/ink decisions into the
//! `LayoutDebugTrace` envelope (docs/design/layout/debug-inspector.md). The
//! projector re-uses the exact same [`super::compute`] pipeline as the
//! product `layout()` — same decisions, one code path — and never writes
//! geometry back. All exported geometry is **physical** space: converted via
//! `orient.rs` and shifted by the identical normalize pass (debug-profile.md
//! §1, parent §5.4).

use std::collections::BTreeMap;

use plotgram_engine_api::{LayoutError, LayoutInput};
use plotgram_model::diagnostics::{PartitionBandObs, PartitionRowBandObs};
use plotgram_model::geometry::{Point, Rect};
use plotgram_model::graph::Graph;
use serde::Serialize;

use super::compose::ports::EdgePorts;
use super::ink::route::CanonicalEdge;
use super::metric::anchor::port_anchor;
use super::model::{ElemKey, PlanGraph, RealGraph, Segment};
use super::orient::{from_algo_point, from_algo_side, to_algo_orientation, to_algo_point};
use super::params::{HierarchicalParams, Orientation};
use super::{canonical_rect_to_physical, compute};

const SCHEMA_VERSION: u32 = 1;
/// Canonical (implementation) name — `extension.kind` is always this.
const CANONICAL_KIND: &str = "hierarchical";

/// Intermediate decisions captured alongside the product output by
/// [`super::compute`]. Never read by the product path.
pub struct Captures<'a> {
    pub orientation: Orientation,
    pub params: HierarchicalParams,
    pub graph: &'a Graph,
    pub real_graph: RealGraph,
    pub plan: PlanGraph,
    pub ports: BTreeMap<String, EdgePorts>,
    pub canonical_frames: Vec<Rect>,
    pub canonical_edges: Vec<CanonicalEdge>,
    /// D1.2 Channel routes (edge_id → track id sequence).
    pub channel_routes: BTreeMap<String, Vec<u32>>,
    pub channel_track_count: usize,
    /// True when group-cut Gate IR was active for this run.
    pub channel_used_gates: bool,
    /// D1.3.3 RouteOrderWriter commit order.
    pub channel_route_order: Vec<String>,
    /// Normalize-stage translate applied to the product output; the
    /// projection applies the identical shift so trace geometry matches the
    /// product pixel-for-pixel.
    pub shift: (f64, f64),
    /// Consumed partition column bands, physical coordinates (partition-grid
    /// .md PG-2); empty when the grid is not consumed.
    pub partition_bands: Vec<PartitionBandObs>,
    /// Consumed partition row bands, physical coordinates (PG-3).
    pub partition_row_bands: Vec<PartitionRowBandObs>,
}

// ── Envelope (debug-inspector.md §5, debug-profile.md §2) ─────────

#[derive(Debug, Clone, Serialize)]
pub struct LayoutDebugTrace {
    pub schema_version: u32,
    /// Registry name as authored (alias recorded verbatim; parent §5.1).
    pub layout: String,
    pub orientation: String,
    pub space: &'static str,
    pub common: CommonDebug,
    pub extension: HierarchicalExtension,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommonDebug {
    pub nodes: Vec<NodeCommonDebug>,
    pub edges: Vec<EdgeCommonDebug>,
    pub groups: Vec<GroupCommonDebug>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeCommonDebug {
    pub id: String,
    pub frame: Rect,
    pub center: Point,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeCommonDebug {
    pub edge_id: String,
    pub source: String,
    pub target: String,
    pub path: Vec<Point>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupCommonDebug {
    pub group_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Weak: Metric group-frame writer. Strong: MacroBlockWriter.
    /// finalize only forwards (`owns_group_frames`).
    pub frame: Option<Rect>,
    pub frame_source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct HierarchicalExtension {
    pub kind: &'static str,
    pub elems: Vec<ElemDebug>,
    pub layers: Vec<LayerDebug>,
    pub edge_plans: Vec<HierEdgeDebug>,
    pub ports: Vec<PortDebug>,
    /// Channel routing is not implemented in this build → always `null`
    /// (D6: explicit absence, no fake tracks).
    pub channels: Option<ChannelDebug>,
    pub metrics: MetricDebug,
    /// Consumed partition column bands, physical cross-axis intervals
    /// (partition-grid.md PG-2); absent when the grid is not consumed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub partition_bands: Vec<PartitionBandDebug>,
    /// Consumed partition row bands, physical main-axis intervals (PG-3).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub partition_row_bands: Vec<PartitionRowBandDebug>,
}

/// One consumed partition column band (partition-grid.md PG-2).
#[derive(Debug, Clone, Serialize)]
pub struct PartitionBandDebug {
    pub column: String,
    pub band: Band,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub empty: bool,
}

/// One consumed partition row band (partition-grid.md PG-3).
#[derive(Debug, Clone, Serialize)]
pub struct PartitionRowBandDebug {
    pub row: String,
    pub band: Band,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub empty: bool,
}

/// D1.3 Channel debug sketch (RouteOrder + Gate/rip-up).
#[derive(Debug, Clone, Serialize)]
pub struct ChannelDebug {
    pub status: String,
    pub substrate_tracks: usize,
    pub routed_edges: usize,
    /// D1.3.3 deterministic commit order.
    pub route_order: Vec<String>,
    pub routes: Vec<ChannelRouteDebug>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelRouteDebug {
    pub edge_id: String,
    pub track_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricDebug {
    pub node_gap: f64,
    pub layer_gap: f64,
}

/// Stable element identity — no dense indices (debug-profile.md §2.1).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ElemKeyDebug {
    #[serde(rename = "real")]
    Real { id: String },
    #[serde(rename = "virtual")]
    Virtual {
        owner_edge: String,
        kind: &'static str,
        ordinal: u32,
    },
    #[serde(rename = "group-boundary")]
    GroupBoundary {
        group: String,
        side: &'static str,
        rank: u32,
    },
    #[serde(rename = "partition-boundary")]
    PartitionBoundary {
        axis: String,
        side: &'static str,
        rank: u32,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ElemDebug {
    pub key: ElemKeyDebug,
    pub rank: u32,
    pub order: u32,
    pub group_path: Vec<String>,
    pub frame: Rect,
    pub center: Point,
    pub kind_tags: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayerDebug {
    pub rank: u32,
    /// Physical main-axis extent covered by this layer's element frames.
    pub main_band: Band,
    pub elem_keys: Vec<ElemKeyDebug>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Band {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HierEdgeDebug {
    pub edge_id: String,
    pub original: Endpoints,
    pub working: Endpoints,
    pub reversed: bool,
    pub segments: Vec<SegmentDebug>,
    pub dummy_chain: Vec<ElemKeyDebug>,
    /// Self-loops bypass rank/order/plan: `"self-loop"` (debug-profile §2.3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Endpoints {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentDebug {
    pub ordinal: u32,
    pub from: ElemKeyDebug,
    pub to: ElemKeyDebug,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortDebug {
    pub edge_id: String,
    pub end: &'static str,
    pub node: String,
    pub side: String,
    /// Relative order within the (node, side) group; `None` for
    /// LocalOffset pins (they carry no order).
    pub slot: Option<u32>,
    pub point: Point,
    /// Fixed vocabulary aligned with edge/anchor `PortConstraint`.
    pub constraint: &'static str,
}

// ── Entry point ────────────────────────────────────────────────────

/// Project this run's decisions into a [`LayoutDebugTrace`].
///
/// `layout_name` is recorded verbatim in the trace envelope. Re-runs the shared
/// pipeline for collection — the product `layout()` path is never touched.
pub fn build_debug_trace<'a>(
    input: LayoutInput<'a>,
    layout_name: &str,
) -> Result<LayoutDebugTrace, LayoutError> {
    let (_, captures) = compute(input)?;
    Ok(project(layout_name, captures))
}

// ── Projection ─────────────────────────────────────────────────────

fn project(layout_name: &str, cap: Captures<'_>) -> LayoutDebugTrace {
    let algo_orient = to_algo_orientation(cap.orientation);
    let shift = cap.shift;

    let phys_rect = |r: Rect| -> Rect {
        let mut r = canonical_rect_to_physical(algo_orient, r);
        r.x += shift.0;
        r.y += shift.1;
        r
    };
    let phys_point = |p: Point| -> Point {
        let mut p = from_algo_point(algo_orient.from_tb_point(to_algo_point(p)));
        p.x += shift.0;
        p.y += shift.1;
        p
    };

    // Per-elem order within its layer (post ordering phase).
    let mut order_of = vec![0u32; cap.plan.elems.len()];
    for layer in &cap.plan.layers {
        for (i, &elem) in layer.iter().enumerate() {
            order_of[elem] = i as u32;
        }
    }

    let elems: Vec<ElemDebug> = cap
        .plan
        .elems
        .iter()
        .enumerate()
        .map(|(i, elem)| {
            let frame = phys_rect(cap.canonical_frames[i]);
            let (key, kind_tags) = match &elem.key {
                ElemKey::Real(id) => (ElemKeyDebug::Real { id: id.clone() }, vec!["real"]),
                ElemKey::Virtual { edge_id, ordinal } => (
                    ElemKeyDebug::Virtual {
                        owner_edge: edge_id.clone(),
                        kind: "long-edge",
                        ordinal: *ordinal,
                    },
                    vec!["virtual", "long-edge-dummy"],
                ),
                ElemKey::GroupBoundary { group, rank, side } => (
                    ElemKeyDebug::GroupBoundary {
                        group: group.clone(),
                        side: match side {
                            crate::layout::hierarchical::model::BoundarySide::Left => "left",
                            crate::layout::hierarchical::model::BoundarySide::Right => "right",
                        },
                        rank: *rank,
                    },
                    vec!["group-boundary"],
                ),
                ElemKey::PartitionBoundary { axis, rank, side } => (
                    ElemKeyDebug::PartitionBoundary {
                        axis: axis.clone(),
                        side: match side {
                            crate::layout::hierarchical::model::BoundarySide::Left => "left",
                            crate::layout::hierarchical::model::BoundarySide::Right => "right",
                        },
                        rank: *rank,
                    },
                    vec!["partition-boundary"],
                ),
                ElemKey::OrderPad { rank, ordinal } => (
                    ElemKeyDebug::Virtual {
                        owner_edge: format!("pad:{rank}"),
                        kind: "order-pad",
                        ordinal: *ordinal,
                    },
                    vec!["order-pad"],
                ),
            };
            ElemDebug {
                key,
                rank: elem.rank,
                order: order_of[i],
                group_path: elem.group_path.clone(),
                center: frame.center(),
                frame,
                kind_tags,
            }
        })
        .collect();

    let layers: Vec<LayerDebug> = cap
        .plan
        .layers
        .iter()
        .enumerate()
        .map(|(rank, layer)| {
            // Layer band: canonical bbox of the layer's frames → physical,
            // then read along the physical main axis.
            let mut bbox: Option<Rect> = None;
            for &elem in layer {
                let r = cap.canonical_frames[elem];
                bbox = Some(match bbox {
                    None => r,
                    Some(b) => Rect::new(
                        b.x.min(r.x),
                        b.y.min(r.y),
                        b.right().max(r.right()) - b.x.min(r.x),
                        b.bottom().max(r.bottom()) - b.y.min(r.y),
                    ),
                });
            }
            let main_band = match bbox {
                Some(b) => {
                    let p = phys_rect(b);
                    if cap.orientation.is_vertical() {
                        Band {
                            start: p.y,
                            end: p.bottom(),
                        }
                    } else {
                        Band {
                            start: p.x,
                            end: p.right(),
                        }
                    }
                }
                None => Band {
                    start: 0.0,
                    end: 0.0,
                },
            };
            LayerDebug {
                rank: rank as u32,
                main_band,
                elem_keys: layer
                    .iter()
                    .map(|&elem| elem_key_debug(&cap.plan.elems[elem].key))
                    .collect(),
            }
        })
        .collect();

    let edge_plans = project_edge_plans(&cap);
    let ports = project_ports(&cap, algo_orient, &phys_point);

    let nodes: Vec<NodeCommonDebug> = cap
        .real_graph
        .ids
        .iter()
        .map(|id| {
            let elem = cap.plan.index_of[&ElemKey::Real(id.clone())];
            let frame = phys_rect(cap.canonical_frames[elem]);
            NodeCommonDebug {
                id: id.clone(),
                center: frame.center(),
                frame,
            }
        })
        .collect();

    let edges: Vec<EdgeCommonDebug> = cap
        .canonical_edges
        .iter()
        .map(|ce| EdgeCommonDebug {
            edge_id: ce.id.clone(),
            source: ce.source.clone(),
            target: ce.target.clone(),
            path: ce.path.samples().iter().copied().map(phys_point).collect(),
        })
        .collect();

    let mut groups = Vec::new();
    collect_groups(&cap.graph.groups, None, &mut groups);

    LayoutDebugTrace {
        schema_version: SCHEMA_VERSION,
        layout: layout_name.to_string(),
        orientation: cap.orientation.as_str().to_string(),
        space: "physical",
        common: CommonDebug {
            nodes,
            edges,
            groups,
        },
        extension: HierarchicalExtension {
            kind: CANONICAL_KIND,
            elems,
            layers,
            edge_plans,
            ports,
            channels: Some(ChannelDebug {
                status: if cap.channel_used_gates {
                    "d1.3-gate".into()
                } else {
                    "d1.3-root-scope".into()
                },
                substrate_tracks: cap.channel_track_count,
                routed_edges: cap.channel_routes.len(),
                route_order: cap.channel_route_order.clone(),
                routes: cap
                    .channel_routes
                    .iter()
                    .map(|(id, tracks)| ChannelRouteDebug {
                        edge_id: id.clone(),
                        track_ids: tracks.clone(),
                    })
                    .collect(),
            }),
            metrics: MetricDebug {
                node_gap: cap.params.node_gap,
                layer_gap: cap.params.layer_gap,
            },
            partition_bands: cap
                .partition_bands
                .iter()
                .map(|b| PartitionBandDebug {
                    column: b.column.clone(),
                    band: Band {
                        start: b.start,
                        end: b.end,
                    },
                    empty: b.empty,
                })
                .collect(),
            partition_row_bands: cap
                .partition_row_bands
                .iter()
                .map(|b| PartitionRowBandDebug {
                    row: b.row.clone(),
                    band: Band {
                        start: b.start,
                        end: b.end,
                    },
                    empty: b.empty,
                })
                .collect(),
        },
        notes: vec![
            "hierarchical: D1.3 Channel (RouteOrder + Gate/ScopeMask + bounded rip-up)".to_string(),
        ],
    }
}

fn elem_key_debug(key: &ElemKey) -> ElemKeyDebug {
    match key {
        ElemKey::Real(id) => ElemKeyDebug::Real { id: id.clone() },
        ElemKey::Virtual { edge_id, ordinal } => ElemKeyDebug::Virtual {
            owner_edge: edge_id.clone(),
            kind: "long-edge",
            ordinal: *ordinal,
        },
        ElemKey::GroupBoundary { group, rank, side } => ElemKeyDebug::GroupBoundary {
            group: group.clone(),
            side: match side {
                crate::layout::hierarchical::model::BoundarySide::Left => "left",
                crate::layout::hierarchical::model::BoundarySide::Right => "right",
            },
            rank: *rank,
        },
        ElemKey::PartitionBoundary { axis, rank, side } => ElemKeyDebug::PartitionBoundary {
            axis: axis.clone(),
            side: match side {
                crate::layout::hierarchical::model::BoundarySide::Left => "left",
                crate::layout::hierarchical::model::BoundarySide::Right => "right",
            },
            rank: *rank,
        },
        ElemKey::OrderPad { rank, ordinal } => ElemKeyDebug::Virtual {
            owner_edge: format!("pad:{rank}"),
            kind: "order-pad",
            ordinal: *ordinal,
        },
    }
}

fn collect_groups(
    groups: &[plotgram_model::graph::Group],
    parent: Option<String>,
    out: &mut Vec<GroupCommonDebug>,
) {
    for g in groups {
        out.push(GroupCommonDebug {
            group_id: g.id.clone(),
            parent: parent.clone(),
            frame: None,
            frame_source: "none",
        });
        collect_groups(&g.groups, Some(g.id.clone()), out);
    }
}

fn project_edge_plans(cap: &Captures<'_>) -> Vec<HierEdgeDebug> {
    let mut plans =
        Vec::with_capacity(cap.real_graph.edges.len() + cap.real_graph.self_loops.len());

    for e in &cap.real_graph.edges {
        let mut segs: Vec<&Segment> = cap
            .plan
            .segments
            .iter()
            .filter(|s| s.edge_id == e.edge_id)
            .collect();
        segs.sort_by_key(|s| s.ordinal);

        // Dummy chain in original source→target order (ink's FAS direction
        // invariant — see ink/route.rs `chain_in_original_order`).
        let mut chain: Vec<usize> = Vec::with_capacity(segs.len() + 1);
        if let Some(first) = segs.first() {
            chain.push(first.from);
            chain.extend(segs.iter().map(|s| s.to));
        }
        if e.reversed {
            chain.reverse();
        }
        let dummy_chain: Vec<ElemKeyDebug> = chain
            .iter()
            .filter(|&&elem| cap.plan.elems[elem].key.is_virtual())
            .map(|&elem| elem_key_debug(&cap.plan.elems[elem].key))
            .collect();

        plans.push(HierEdgeDebug {
            edge_id: e.edge_id.clone(),
            original: Endpoints {
                source: cap.real_graph.ids[e.original_source].clone(),
                target: cap.real_graph.ids[e.original_target].clone(),
            },
            working: Endpoints {
                source: cap.real_graph.ids[e.working_source].clone(),
                target: cap.real_graph.ids[e.working_target].clone(),
            },
            reversed: e.reversed,
            segments: segs
                .iter()
                .map(|s| SegmentDebug {
                    ordinal: s.ordinal,
                    from: elem_key_debug(&cap.plan.elems[s.from].key),
                    to: elem_key_debug(&cap.plan.elems[s.to].key),
                })
                .collect(),
            dummy_chain,
            note: None,
        });
    }

    for (edge_id, node_idx) in &cap.real_graph.self_loops {
        let node = cap.real_graph.ids[*node_idx].clone();
        plans.push(HierEdgeDebug {
            edge_id: edge_id.clone(),
            original: Endpoints {
                source: node.clone(),
                target: node.clone(),
            },
            working: Endpoints {
                source: node.clone(),
                target: node.clone(),
            },
            reversed: false,
            segments: Vec::new(),
            dummy_chain: Vec::new(),
            note: Some("self-loop"),
        });
    }

    plans
}

fn project_ports(
    cap: &Captures<'_>,
    algo_orient: plotgram_algo::orientation::Orientation,
    phys_point: &impl Fn(Point) -> Point,
) -> Vec<PortDebug> {
    let mut out = Vec::with_capacity(cap.real_graph.edges.len() * 2);
    for e in &cap.real_graph.edges {
        let edge_ports = match cap.ports.get(&e.edge_id) {
            Some(p) => p,
            None => continue,
        };
        for (end, node_idx, resolved, constraint) in [
            (
                "source",
                e.original_source,
                edge_ports.source,
                e.from_port.clone(),
            ),
            (
                "target",
                e.original_target,
                edge_ports.target,
                e.to_port.clone(),
            ),
        ] {
            let elem = cap.plan.index_of[&ElemKey::Real(cap.real_graph.ids[node_idx].clone())];
            let canonical_anchor = port_anchor(cap.canonical_frames[elem], resolved);
            let side = from_algo_side(algo_orient.from_tb_side(resolved.side));
            out.push(PortDebug {
                edge_id: e.edge_id.clone(),
                end,
                node: cap.real_graph.ids[node_idx].clone(),
                side: side_str(side),
                slot: match resolved.along {
                    plotgram_model::port::AlongSpec::Ordered { order, .. } => Some(order),
                    _ => None,
                },
                point: phys_point(canonical_anchor),
                constraint: constraint_str(constraint.as_ref()),
            });
        }
    }
    out
}

fn constraint_str(constraint: Option<&plotgram_model::port::PortConstraint>) -> &'static str {
    use plotgram_model::port::PortConstraint::*;
    match constraint {
        None => "free",
        Some(FixedSide { .. }) => "fixed-side",
        Some(FixedOrder { .. }) => "fixed-order",
    }
}

fn side_str(side: plotgram_model::port::Side) -> String {
    use plotgram_model::port::Side::*;
    match side {
        North => "N",
        South => "S",
        East => "E",
        West => "W",
    }
    .to_string()
}
