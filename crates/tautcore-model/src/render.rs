//! Render-side inputs: diagram chrome + pairing graph with layout geometry.
//!
//! Layout engine does not consume these types. Theme / title / render_style are
//! dsl-spec diagram attributes that never enter [`crate::contract::LayoutContract`].

use crate::attr::AttrMap;
use crate::graph::Graph;
use crate::result::LayoutResult;

/// Diagram chrome for the renderer (dsl-spec §4.2 minus layout/edge_routing).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RenderMeta {
    pub title: Option<String>,
    /// Theme atom (e.g. `common.clean-light`).
    pub theme: Option<String>,
    /// Pen/skin atom (e.g. `standard`).
    pub render_style: Option<String>,
    /// Leftover diagram attrs incl. `meta.*` namespace (dsl-spec §14.1).
    #[serde(default)]
    pub extra: AttrMap,
}

/// Everything the renderer needs to produce SVG (or other backends).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenderInput {
    /// Structural IR: shape, variant, arrow, style.*, labels text sources.
    pub graph: Graph,
    /// Geometry from the engine.
    pub layout: LayoutResult,
    /// Title / theme / render_style.
    pub meta: RenderMeta,
}
