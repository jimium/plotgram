//! Profile: DSL `profile:` atom → default algorithm configuration.
//!
//! Per dsl-spec §4 / §8 and ADR-001: expansion happens BEFORE the engine sees the contract.
//! **Engine code must not `use plotgram_model::profile`** (no profile-name branching).
//!
//! During rebuild, [`Profile::for_type`] in this module is the working source of defaults.
//! A future engine algorithm registry may override names/options; it still must not take
//! [`DiagramType`] (profile id) as an input.

use crate::attr::{AttrMap, AttrValue};
use crate::contract::AlgorithmRef;

/// Closed set of DSL `profile:` atoms (dsl-spec §1.2).
///
/// Lives in the DSL/profile layer only. Must NOT leak into layout/routing engine code.
/// (Rust name remains `DiagramType` for now; DSL surface key is `profile`.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DiagramType {
    Flowchart,
    Sequence,
    Architecture,
    State,
    Er,
    Mindmap,
}

impl DiagramType {
    /// Parse from DSL atom string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "flowchart" => Some(Self::Flowchart),
            "sequence" => Some(Self::Sequence),
            "architecture" => Some(Self::Architecture),
            "state" => Some(Self::State),
            "er" => Some(Self::Er),
            "mindmap" => Some(Self::Mindmap),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Sequence => "sequence",
            Self::Architecture => "architecture",
            Self::State => "state",
            Self::Er => "er",
            Self::Mindmap => "mindmap",
        }
    }

    /// Whether self-loops are allowed by default for this diagram type.
    ///
    /// Sequence is `true` because `A -> A` is a SelfCall message on the time
    /// axis, not a generic isolated loop (layout/sequence/scope.md).
    pub fn allows_self_loop(&self) -> bool {
        matches!(self, Self::Flowchart | Self::State | Self::Sequence)
    }
}

/// Expanded profile defaults for a diagram type.
///
/// Produced by [`Profile::for_type`]; consumed during profile expansion
/// to fill missing `layout` / `edge_routing` in the DSL.
#[derive(Debug, Clone)]
pub struct Profile {
    pub diagram_type: DiagramType,
    /// Default layout algorithm.
    pub default_layout: AlgorithmRef,
    /// Default edge routing (`None` = layout provides built-in edge geometry).
    pub default_edge_routing: Option<AlgorithmRef>,
    /// Whether self-loops are permitted.
    pub allow_self_loop: bool,
}

impl Profile {
    /// Defaults for a diagram type (rebuild working table; dsl-spec §8).
    ///
    /// Hierarchical profiles use `edge_routing: None` — orthogonal geometry is
    /// produced by the layout’s built-in path (not a separate EdgeRouter stage).
    /// Authors may still set `edge_routing: orthogonal` explicitly for an
    /// independent router after layout.
    pub fn for_type(dt: DiagramType) -> Self {
        match dt {
            DiagramType::Flowchart => Self {
                diagram_type: dt,
                default_layout: AlgorithmRef::new("hierarchical"),
                default_edge_routing: None,
                allow_self_loop: true,
            },
            DiagramType::Architecture => Self {
                diagram_type: dt,
                default_layout: AlgorithmRef::new("hierarchical"),
                default_edge_routing: None,
                allow_self_loop: false,
            },
            DiagramType::State => Self {
                diagram_type: dt,
                default_layout: AlgorithmRef::new("hierarchical"),
                default_edge_routing: None,
                allow_self_loop: true,
            },
            DiagramType::Sequence => Self {
                diagram_type: dt,
                default_layout: AlgorithmRef::new("sequence"),
                default_edge_routing: None,
                // Sequence `A -> A` is a SelfCall message, not a generic loop.
                allow_self_loop: true,
            },
            DiagramType::Mindmap => {
                let mut options = AttrMap::new();
                options.insert(
                    "placer".into(),
                    AttrValue::Atom("single-split-layered".into()),
                );
                Self {
                    diagram_type: dt,
                    default_layout: AlgorithmRef::with_options("tree", options),
                    default_edge_routing: None,
                    allow_self_loop: false,
                }
            }
            DiagramType::Er => Self {
                diagram_type: dt,
                default_layout: AlgorithmRef::new("circular"),
                default_edge_routing: None,
                allow_self_loop: false,
            },
        }
    }
}
