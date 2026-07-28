//! LayoutContract: the engine entry point.
//!
//! Per ADR-001 and dsl-spec §8: the engine receives an algorithm name + parameters + graph model.
//! It does NOT receive `diagram_type`. All profile defaults are already expanded before this point.
//!
//! Theme / title / render_style never appear here — see [`crate::render::RenderMeta`].

use crate::attr::AttrMap;
use crate::graph::Graph;

/// An algorithm reference: name + free-form options.
///
/// Corresponds to dsl-spec §2.7 `<algorithm_config>`.
/// Examples: `hierarchical { direction: top-to-bottom }`, `orthogonal`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AlgorithmRef {
    /// Algorithm name (open atom; validated by engine registry, not by DSL).
    pub name: String,
    /// Algorithm-specific options (free map; unknown keys → warning at engine level).
    pub options: AttrMap,
}

impl AlgorithmRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            options: AttrMap::new(),
        }
    }

    pub fn with_options(name: impl Into<String>, options: AttrMap) -> Self {
        Self {
            name: name.into(),
            options,
        }
    }
}

/// The layout contract handed to the engine.
///
/// Pipeline: `.pgm → parse → profile expand → LayoutContract → engine`
///
/// Contains everything the engine needs; contains nothing it shouldn't (no diagram_type).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayoutContract {
    /// Layout algorithm (e.g. "hierarchical", "tree", "circular", "sequence").
    pub layout: AlgorithmRef,
    /// Independent edge router after layout.
    ///
    /// - `None` — layout provides built-in edge geometry (e.g. hierarchical orthogonal ink,
    ///   sequence messages). Prefer this for Hier-first profiles.
    /// - `Some` — freeze nodes, then run a separate router (`orthogonal`, `organic`, …).
    pub edge_routing: Option<AlgorithmRef>,
    /// The graph model (nodes, edges, groups).
    pub graph: Graph,
}
