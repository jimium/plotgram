//! Organic layout (force-directed family): PivotMDS → SGD stress → VPSC
//! overlap removal → shelf packing.
//!
//! Reference: yFiles `OrganicLayout` / `SmartOrganicLayout`; ebook
//! `docs/reference/yfiles/ebook/04-力导向与stress布局.md` §8. Fully
//! deterministic: fixed iteration counts + explicit seed PRNG; no
//! wall-clock sampling (WASM-safe).

mod compose;
mod demand;
mod geom;
mod ink;
mod metric;
mod params;
mod plan;
mod verify;

pub use params::{OrganicParams, OrganicPreset};

use plotgram_engine_api::{
    EdgeGeometryMode, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput, LayoutWarning,
};
use plotgram_model::diagnostics::LayoutDiagnostics;
use plotgram_model::result::{EdgePath, EdgePlacement, NodePlacement};

pub(crate) fn org_err(msg: impl Into<String>) -> LayoutError {
    let m = msg.into();
    if m.contains("invariant:") {
        LayoutError::invariant(m)
    } else {
        LayoutError::invalid_input(m)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OrganicLayout;

impl LayoutAlgorithm for OrganicLayout {
    fn name(&self) -> &'static str {
        "organic"
    }

    fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
        compute(input)
    }
}

fn compute(input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
    let bound = OrganicParams::bind(input.options)?;
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

    let (plan, compose_warnings) = compose::compose(input.graph)?;
    for message in compose_warnings {
        diagnostics.warnings.push(LayoutWarning { message });
    }
    verify::verify_plan(&plan)?;
    verify::verify_edge_roles(
        &plan,
        input
            .graph
            .edges_in_declaration_order()
            .into_iter()
            .map(|e| e.id.clone()),
    )?;

    let demand = demand::publish_floors(&plan, params, input.node_sizes);
    let metric = metric::assign(&plan, params, input.node_sizes, &demand)?;
    verify::verify_metric(&plan, &metric, &demand, params)?;

    let mut edges = ink::expand(input.graph, &metric, input.edge_geometry);
    verify::verify_ink(
        &edges,
        &metric,
        input.edge_geometry == EdgeGeometryMode::DeferToRouter,
    )?;

    let mut nodes: Vec<NodePlacement> = plan
        .nodes
        .iter()
        .filter_map(|id| {
            let frame = metric.frames.get(id)?;
            Some(NodePlacement {
                id: id.clone(),
                frame: *frame,
            })
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
        if let EdgePath::Polyline { points } = &mut e.path {
            for p in points {
                p.x -= min_x;
                p.y -= min_y;
            }
        }
    }
}
