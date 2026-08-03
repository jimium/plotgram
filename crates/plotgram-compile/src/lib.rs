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

mod error;
mod measure_graph;
mod options;

pub use error::BuildError;
pub use options::BuildOptions;

use plotgram_engine::run as run_layout;
use plotgram_model::render::RenderInput;
use plotgram_parse::parse;
use plotgram_render::render_svg;

use crate::measure_graph::measure_node_sizes;

/// Build `.pgm` source into an SVG string.
pub fn build_svg(source: &str, options: &BuildOptions) -> Result<String, BuildError> {
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
        let err = build_svg("diagram { node a { label: 42 } }", &BuildOptions::default())
            .unwrap_err();
        assert!(matches!(err, BuildError::Parse(_)));
    }
}
