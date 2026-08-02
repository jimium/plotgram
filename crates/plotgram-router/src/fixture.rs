//! Scene fixture file format for text-driven testing.
//!
//! Fixtures live as `tests/scenes/*.json`. Each file is a [`SceneFixture`]:
//! metadata (name, description, capability requirement) + a [`RouteScene`].
//!
//! ```text
//! {
//!   "name": "blocker_center",
//!   "description": "A blocker obstacle between source and target.",
//!   "requires": "search",
//!   "scene": { ... }
//! }
//! ```

use plotgram_engine_api::RouteScene;

/// Minimum algorithm capability required to pass this fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Requires {
    /// No search needed — direct elbow path is valid (no obstacles in the way).
    None,
    /// Obstacle-avoiding search (OVG + A*) required.
    Search,
    /// Group boundary crossing support required.
    Group,
}

impl std::fmt::Display for Requires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Search => write!(f, "search"),
            Self::Group => write!(f, "group"),
        }
    }
}

/// A self-contained scene fixture with metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SceneFixture {
    /// Stable fixture name (matches file stem).
    pub name: String,
    /// Human-readable description of the routing challenge.
    pub description: String,
    /// Minimum capability an algorithm must have to pass this fixture.
    pub requires: Requires,
    /// The routing scene (algorithm input).
    pub scene: RouteScene,
}
