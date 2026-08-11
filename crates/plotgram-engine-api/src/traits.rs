//! LayoutAlgorithm / EdgeRouter traits (yFiles-style roles).

use plotgram_model::attr::AttrMap;
use plotgram_model::diagnostics::LayoutDiagnostics;
use plotgram_model::graph::Graph;
use plotgram_model::result::{EdgePlacement, GroupPlacement, NodePlacement};
use plotgram_model::sizes::NodeSizes;

use crate::scene::RouteScene;
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
/// Canvas / labels are finalized by the engine facade after layout (and
/// optional routing). Group frames belong to the layout: a layout that owns
/// them writes them into [`LayoutOutput::groups`] and sets
/// [`LayoutOutput::owns_group_frames`]; the facade then passes them through
/// unchanged — even when the vector is empty — and never re-derives over them
/// (write-authority: group-frame-d2.md §6.2 / §6.3).
#[derive(Debug, Clone)]
pub struct LayoutOutput {
    pub nodes: Vec<NodePlacement>,
    /// When [`EdgeGeometryMode::DeferToRouter`], paths may be empty; ports should
    /// still be resolved when the layout owns port decisions.
    pub edges: Vec<EdgePlacement>,
    /// Group frames written by the layout (hierarchical Weak: Metric frame
    /// variables; StrongMacro: MacroBlockWriter). Empty when the graph has no
    /// framed groups, or when the layout does not own group geometry.
    pub groups: Vec<GroupPlacement>,
    /// When `true`, the engine finalize must not fall back to union+pad even if
    /// `groups` is empty (hierarchical always sets this). When `false`, empty
    /// `groups` means "layout did not write frames" and the facade may derive.
    pub owns_group_frames: bool,
    /// Structured observations (warnings / relaxations / params_hash).
    /// Never affects geometry; empty default for layouts without diagnostics.
    pub diagnostics: LayoutDiagnostics,
}

/// Places nodes (and optionally routes edges with built-in ink).
pub trait LayoutAlgorithm: Send + Sync {
    /// Registry key (e.g. `"hierarchical"`).
    fn name(&self) -> &'static str;

    fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError>;
}

/// Independent edge router: nodes frozen, writes final edge paths.
///
/// Consumes a [`RouteScene`] (obstacles + terminals + params) and produces
/// one [`EdgePlacement`] per edge in `scene.edge_order`. Must not modify
/// nodes, ports, or invent terminals — only writes `path`.
pub trait EdgeRouter: Send + Sync {
    /// Registry key (e.g. `"orthogonal"`).
    fn name(&self) -> &'static str;

    /// Route all edges described in `scene`.
    ///
    /// Output order matches `scene.edge_order`.
    fn route(&self, scene: &RouteScene) -> Result<Vec<EdgePlacement>, LayoutError>;
}
