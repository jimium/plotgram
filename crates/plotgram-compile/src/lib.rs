//! End-to-end build (ADR-005 / ADR-006).
//!
//! ```text
//! .pgm source
//!   → plotgram-parse
//!   → measure NodeSizes (+ ContentLayout) via plotgram-content / label heuristics
//!   → plotgram_engine::run
//!   → plotgram-render (SVG)
//! ```
//!
//! CLI / WASM / server should call this crate — not wire stages themselves.
//!
//! Status: skeleton; full measure + theme load still TODO.

#![forbid(unsafe_code)]

mod audit;
mod error;
mod measure_graph;
mod options;

pub use audit::{compute as compute_metrics, GeometryMetrics};
pub use error::BuildError;
pub use options::BuildOptions;
pub use plotgram_layout::LayoutDebugTrace;

use plotgram_engine::run as run_layout;
use plotgram_engine_api::{EdgeGeometryMode, LayoutInput};
use plotgram_model::render::RenderInput;
use plotgram_model::result::LayoutResult;
use plotgram_parse::parse;
use plotgram_render::render_svg;

use crate::measure_graph::measure_node_sizes;

/// Build `.pgm` source into an SVG string.
pub fn build_svg(source: &str, options: &BuildOptions) -> Result<String, BuildError> {
    Ok(build_svg_with_layout(source, options)?.1)
}

/// Build source into (layout, SVG) in a single pipeline run. Exposed for the
/// `measure` CLI so it can audit geometry + check determinism without tripling
/// the work (audit needs the layout; det needs two SVG renders).
pub fn build_svg_with_layout(
    source: &str,
    options: &BuildOptions,
) -> Result<(LayoutResult, String), BuildError> {
    let parsed = parse(source)?;
    let node_sizes = measure_node_sizes(&parsed.graph, options)?;

    let meta = parsed.meta.clone();
    let contract = parsed.into_contract(node_sizes);
    let layout = run_layout(&contract)?;

    let input = RenderInput {
        graph: contract.graph,
        layout: layout.clone(),
        meta,
    };
    Ok((layout, render_svg(&input)))
}

/// Build source into [`plotgram_model::result::LayoutResult`] only (no SVG).
pub fn build_layout(
    source: &str,
    options: &BuildOptions,
) -> Result<plotgram_model::result::LayoutResult, BuildError> {
    let parsed = parse(source)?;
    let node_sizes = measure_node_sizes(&parsed.graph, options)?;
    let contract = parsed.into_contract(node_sizes);
    Ok(run_layout(&contract)?)
}

/// Build source into the hierarchical [`plotgram_layout::LayoutDebugTrace`]
/// (debug-inspector.md T1). Dispatch is on the contract's layout name at the
/// orchestration layer — never inside the algorithm (ADR-001); layouts
/// without a trace provider fail hard instead of emitting an empty shell.
pub fn build_debug_trace(
    source: &str,
    options: &BuildOptions,
) -> Result<LayoutDebugTrace, BuildError> {
    let parsed = parse(source)?;
    let node_sizes = measure_node_sizes(&parsed.graph, options)?;
    let contract = parsed.into_contract(node_sizes);

    let edge_geometry = if contract.edge_routing.is_some() {
        EdgeGeometryMode::DeferToRouter
    } else {
        EdgeGeometryMode::Builtin
    };

    let name = contract.layout.name.clone();
    match name.as_str() {
        "hierarchical" => {
            let input = LayoutInput {
                graph: &contract.graph,
                node_sizes: &contract.node_sizes,
                options: &contract.layout.options,
                edge_geometry,
            };
            Ok(plotgram_layout::build_debug_trace(input, &name)?)
        }
        other => Err(BuildError::NotImplemented {
            stage: "debug-layout",
            detail: format!("trace unsupported for layout `{other}`"),
        }),
    }
}

/// Validate only: parse + measure + contract construction. No layout, no render.
///
/// Use for the `validate` CLI subcommand and gallery status marking.
pub fn validate(source: &str, options: &BuildOptions) -> Result<(), BuildError> {
    let parsed = parse(source)?;
    let node_sizes = measure_node_sizes(&parsed.graph, options)?;
    let _contract = parsed.into_contract(node_sizes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_svg_end_to_end() {
        let svg = build_svg(
            "diagram {\n  profile: flowchart\n  node a \"A\"\n  node b \"B\"\n  a -> b\n}",
            &BuildOptions::default(),
        )
        .expect("build should produce a minimal diagram");
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn build_svg_surfaces_parse_error() {
        let err =
            build_svg("diagram { node a { label: 42 } }", &BuildOptions::default()).unwrap_err();
        assert!(matches!(err, BuildError::Parse(_)));
    }
}
