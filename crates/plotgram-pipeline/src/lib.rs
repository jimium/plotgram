//! End-to-end orchestration (ADR-005 / ADR-006).
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

mod error;
mod measure_graph;
mod options;

pub use error::PipelineError;
pub use options::PipelineOptions;

use plotgram_engine::run as run_layout;
use plotgram_model::render::RenderInput;
use plotgram_parse::parse;
use plotgram_render::render_svg;

use crate::measure_graph::measure_node_sizes;

/// Compile `.pgm` source to an SVG string.
pub fn compile_svg(source: &str, options: &PipelineOptions) -> Result<String, PipelineError> {
    let parsed = parse(source)?;
    let node_sizes = measure_node_sizes(&parsed.graph, options)?;

    let meta = parsed.meta.clone();
    let contract = parsed.into_contract(node_sizes);
    let layout = run_layout(&contract)?;

    let input = RenderInput {
        graph: contract.graph,
        layout,
        meta,
    };
    Ok(render_svg(&input))
}

/// Compile source to [`plotgram_model::result::LayoutResult`] only (no SVG).
pub fn compile_layout(
    source: &str,
    options: &PipelineOptions,
) -> Result<plotgram_model::result::LayoutResult, PipelineError> {
    let parsed = parse(source)?;
    let node_sizes = measure_node_sizes(&parsed.graph, options)?;
    let contract = parsed.into_contract(node_sizes);
    Ok(run_layout(&contract)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_svg_surfaces_parse_not_implemented() {
        let err = compile_svg("diagram { profile: flowchart }", &PipelineOptions::default())
            .unwrap_err();
        assert!(matches!(err, PipelineError::Parse(_)));
    }
}
