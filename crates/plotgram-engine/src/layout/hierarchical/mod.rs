//! Hierarchical layout algorithm (Sugiyama-style subset).
//!
//! Stub: longest-path ranks, packing from preferred sizes, port inference,
//! built-in orthogonal ink via [`crate::route::core`]. Extract to
//! `plotgram-layout-hierarchical` when this tree grows large.

mod ink;
mod place;
mod ports;
mod rank;

use plotgram_engine_api::{
    EdgeGeometryMode, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput,
};
use plotgram_model::result::EdgePlacement;

pub use place::HierarchicalLayout;

impl LayoutAlgorithm for HierarchicalLayout {
    fn name(&self) -> &'static str {
        "hierarchical"
    }

    fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
        let ranks = rank::assign_ranks(input.graph)?;
        let nodes = place::place_nodes(input.graph, input.node_sizes, input.options, &ranks)?;
        let edges = ports::build_edge_stubs(input.graph, &nodes, input.options)?;

        let edges = match input.edge_geometry {
            EdgeGeometryMode::Builtin => ink::route_builtin(&nodes, edges)?,
            EdgeGeometryMode::DeferToRouter => edges
                .into_iter()
                .map(|mut e| {
                    e.path.points.clear();
                    e
                })
                .collect::<Vec<EdgePlacement>>(),
        };

        Ok(LayoutOutput { nodes, edges })
    }
}
