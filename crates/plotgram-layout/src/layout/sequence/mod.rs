//! Sequence layout: participant axis + message time axis, builtin message ink.
//!
//! M4 (architecture.md §10): combined-fragment frames as `Decoration::FragmentFrame`
//! (not `Graph::groups`). M3: attach depth, lifeline notch, greedy/local order,
//! pin / before. No independent `EdgeRouter` (S6).
//!
//! Pipeline: bind → Compose → DemandBoard → Metric → Ink. Independent
//! `EdgeRouter` is rejected at [`LayoutAlgorithm::layout`] (S6).

mod compose;
mod demand;
mod ink;
mod metric;
mod params;
mod plan;
mod verify;

pub use params::{
    LifelineGapStyle, LifelineOrder, SelfLoopRowPolicy, SequenceParams, SequencePreset,
};

use plotgram_engine_api::{
    EdgeGeometryMode, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput, LayoutWarning,
};
use plotgram_model::diagnostics::LayoutDiagnostics;
use plotgram_model::result::{Decoration, NodePlacement};

/// Classify a sequence diagnostic into a typed [`LayoutError`].
pub(crate) fn seq_err(msg: impl Into<String>) -> LayoutError {
    let m = msg.into();
    if m.contains("invariant:") {
        LayoutError::invariant(m)
    } else if let Some(rest) = m.split_once("unsupported:") {
        LayoutError::unsupported(rest.1.trim().to_string())
    } else {
        LayoutError::invalid_input(m)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SequenceLayout;

impl LayoutAlgorithm for SequenceLayout {
    fn name(&self) -> &'static str {
        "sequence"
    }

    fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
        // S6: this kernel owns message geometry. The facade will otherwise
        // unconditionally run a router over DeferToRouter output.
        if input.edge_geometry == EdgeGeometryMode::DeferToRouter {
            return Err(LayoutError::LayoutCannotDeferEdges {
                layout: "sequence".into(),
            });
        }
        compute(input)
    }
}

fn compute(input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
    let bound = SequenceParams::bind(input.options)?;
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

    let composed = compose::compose(input.graph, params)?;
    for w in composed.warnings {
        diagnostics.warnings.push(LayoutWarning { message: w });
    }
    let plan = composed.plan;
    verify::verify_plan(&plan)?;

    let demand = demand::publish_floors(input.graph, &plan, params, input.node_sizes);
    let metric = metric::assign(&plan, params, input.node_sizes, &demand)?;
    verify::verify_metric(&plan, &metric, &demand, params)?;

    let edges = ink::expand(input.graph, &plan, &metric, params)?;
    verify::verify_ink(&plan, &metric, &edges, params)?;

    let mut nodes: Vec<NodePlacement> = plan
        .lifeline_order
        .iter()
        .map(|id| NodePlacement {
            id: id.clone(),
            frame: metric.participant_frames[id],
        })
        .collect();
    let mut edges = edges;
    let mut decorations = build_decorations(&plan, &metric, params);
    normalize_to_origin(&mut nodes, &mut edges, &mut decorations);

    Ok(LayoutOutput {
        nodes,
        edges,
        groups: Vec::new(),
        owns_group_frames: true,
        diagnostics,
        decorations,
    })
}

fn build_decorations(
    plan: &plan::SeqPlan,
    metric: &plan::SeqMetric,
    params: &SequenceParams,
) -> Vec<Decoration> {
    let mut out = Vec::new();
    for id in &plan.lifeline_order {
        let x = metric.lifeline_x[id];
        let gaps = if params.lifeline_gap_style == LifelineGapStyle::Notch {
            metric
                .lifeline_crossings
                .get(id)
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        out.push(Decoration::Lifeline {
            id: format!("lifeline:{id}"),
            participant: id.clone(),
            x,
            y0: metric.lifeline_y0,
            y1: metric.lifeline_y1,
            gaps,
        });
    }
    for span in &plan.activation_spans {
        let Some(frame) = metric.activation_frames.get(&span.id).copied() else {
            continue;
        };
        out.push(Decoration::Activation {
            id: span.id.clone(),
            lifeline_id: format!("lifeline:{}", span.lifeline),
            frame,
            depth: span.depth,
        });
    }
    for frag in &plan.fragments {
        let Some(frame) = metric.fragment_frames.get(&frag.id).copied() else {
            continue;
        };
        let operands = metric
            .fragment_operand_ys
            .get(&frag.id)
            .cloned()
            .unwrap_or_default();
        out.push(Decoration::FragmentFrame {
            id: frag.decoration_id.clone(),
            operator: frag.operator.clone(),
            label: frag.label.clone(),
            frame,
            operands,
        });
    }
    out
}

fn normalize_to_origin(
    nodes: &mut [NodePlacement],
    edges: &mut [plotgram_model::result::EdgePlacement],
    decorations: &mut [Decoration],
) {
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
    for d in decorations.iter() {
        let b = d.bbox();
        min_x = min_x.min(b.x);
        min_y = min_y.min(b.y);
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
            plotgram_model::result::EdgePath::Polyline { points } => {
                for p in points {
                    p.x -= min_x;
                    p.y -= min_y;
                }
            }
            plotgram_model::result::EdgePath::Cubic {
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
    for d in decorations.iter_mut() {
        d.translate(-min_x, -min_y);
    }
}
