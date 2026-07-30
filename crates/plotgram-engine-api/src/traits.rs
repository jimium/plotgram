//! LayoutAlgorithm / EdgeRouter traits (yFiles-style roles).

use plotgram_model::attr::AttrMap;
use plotgram_model::graph::Graph;
use plotgram_model::result::{EdgePlacement, NodePlacement};
use plotgram_model::sizes::NodeSizes;

use crate::LayoutError;

/// Whether the layout must write final edge geometry or defer to an [`EdgeRouter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeGeometryMode {
    /// Layout is the edge-geometry writer (built-in ink).
    Builtin,
    /// Layout freezes nodes (+ port decisions); edge paths are placeholders for the router.
    DeferToRouter,
}

/// Input to [`LayoutAlgorithm::layout`].
#[derive(Debug, Clone, Copy)]
pub struct LayoutInput<'a> {
    pub graph: &'a Graph,
    pub node_sizes: &'a NodeSizes,
    pub options: &'a AttrMap,
    pub edge_geometry: EdgeGeometryMode,
}

/// Partial geometry from a layout algorithm.
///
/// Group frames / canvas / labels are typically finalized by the engine facade
/// after layout (and optional routing).
#[derive(Debug, Clone)]
pub struct LayoutOutput {
    pub nodes: Vec<NodePlacement>,
    /// When [`EdgeGeometryMode::DeferToRouter`], paths may be empty; ports should
    /// still be resolved when the layout owns port decisions.
    pub edges: Vec<EdgePlacement>,
}

/// Places nodes (and optionally routes edges with built-in ink).
pub trait LayoutAlgorithm: Send + Sync {
    /// Registry key (e.g. `"hierarchical"`).
    fn name(&self) -> &'static str;

    fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError>;
}

/// Input to [`EdgeRouter::route`] after nodes are frozen.
#[derive(Debug, Clone, Copy)]
pub struct RouteInput<'a> {
    pub graph: &'a Graph,
    pub nodes: &'a [NodePlacement],
    /// Edge stubs from layout (ids / endpoints / optional ports); paths replaced.
    pub edges: &'a [EdgePlacement],
    pub options: &'a AttrMap,
}

/// Independent edge router: nodes frozen, writes final edge paths.
pub trait EdgeRouter: Send + Sync {
    fn name(&self) -> &'static str;

    fn route(&self, input: RouteInput<'_>) -> Result<Vec<EdgePlacement>, LayoutError>;
}
