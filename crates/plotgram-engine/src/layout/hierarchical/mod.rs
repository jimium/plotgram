//! Hierarchical layout algorithm (Sugiyama-style subset).
//!
//! Stub: longest-path ranks, packing from preferred sizes, port inference,
//! built-in orthogonal ink via [`crate::route::core`]. Extract to
//! `plotgram-layout-hierarchical` when this tree grows large.

mod ink;
mod params;
mod place;
mod ports;
mod rank;

use plotgram_engine_api::{
    EdgeGeometryMode, LayoutAlgorithm, LayoutError, LayoutInput, LayoutOutput,
};
use plotgram_model::result::EdgePlacement;

pub use params::{
    BindResult, GroupAlign, GroupPolicy, GroupSizing, HierarchicalParams, HierarchicalPreset,
    Orientation, RoutingStyle,
};
pub use place::HierarchicalLayout;

impl LayoutAlgorithm for HierarchicalLayout {
    fn name(&self) -> &'static str {
        "hierarchical"
    }

    fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
        // Bind once at entry; phases must not read the free-form options map.
        let bound = HierarchicalParams::bind(input.options)?;
        let params = &bound.params;

        let ranks = rank::assign_ranks(input.graph)?;
        let nodes = place::place_nodes(input.graph, input.node_sizes, params, &ranks)?;
        let edges = ports::build_edge_stubs(input.graph, &nodes, params)?;

        let edges = match input.edge_geometry {
            EdgeGeometryMode::Builtin => {
                ink::route_builtin(&nodes, edges, params.routing_style)?
            }
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
