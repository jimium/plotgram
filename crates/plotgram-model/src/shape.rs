//! Node geometry vocabulary (dsl-spec §14.6).
//!
//! Closed product set shared by parse, archetype, layout (future port
//! policy), and render. Paint paths stay in `plotgram-render`; this module
//! only names the shapes.

use std::fmt;

/// Product node shape (dsl-spec §14.6 — 12 atoms).
///
/// Unknown atoms are rejected at parse/lift time (no silent fallback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeShape {
    Rect,
    RoundedRect,
    Circle,
    Diamond,
    Cylinder,
    Hexagon,
    Stadium,
    Person,
    Parallelogram,
    Document,
    Cloud,
    Subprocess,
}

impl NodeShape {
    /// All atoms in declaration order (dsl-spec §14.6).
    pub const ALL: &'static [Self] = &[
        Self::Rect,
        Self::RoundedRect,
        Self::Circle,
        Self::Diamond,
        Self::Cylinder,
        Self::Hexagon,
        Self::Stadium,
        Self::Person,
        Self::Parallelogram,
        Self::Document,
        Self::Cloud,
        Self::Subprocess,
    ];

    /// Resolve chain fallback when no theme / DSL / archetype shape is set.
    pub const DEFAULT: Self = Self::RoundedRect;

    /// Parse a shape atom. Closed set — unknown returns `None`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rect" => Some(Self::Rect),
            "rounded_rect" => Some(Self::RoundedRect),
            "circle" => Some(Self::Circle),
            "diamond" => Some(Self::Diamond),
            "cylinder" => Some(Self::Cylinder),
            "hexagon" => Some(Self::Hexagon),
            "stadium" => Some(Self::Stadium),
            "person" => Some(Self::Person),
            "parallelogram" => Some(Self::Parallelogram),
            "document" => Some(Self::Document),
            "cloud" => Some(Self::Cloud),
            "subprocess" => Some(Self::Subprocess),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::RoundedRect => "rounded_rect",
            Self::Circle => "circle",
            Self::Diamond => "diamond",
            Self::Cylinder => "cylinder",
            Self::Hexagon => "hexagon",
            Self::Stadium => "stadium",
            Self::Person => "person",
            Self::Parallelogram => "parallelogram",
            Self::Document => "document",
            Self::Cloud => "cloud",
            Self::Subprocess => "subprocess",
        }
    }
}

impl fmt::Display for NodeShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_set_round_trips() {
        assert_eq!(NodeShape::ALL.len(), 12);
        for &s in NodeShape::ALL {
            assert_eq!(NodeShape::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn unknown_atom_rejected() {
        assert_eq!(NodeShape::parse("triangle"), None);
        assert_eq!(NodeShape::parse("rounded-rect"), None);
        assert_eq!(NodeShape::parse(""), None);
    }

    #[test]
    fn display_matches_atom() {
        assert_eq!(NodeShape::RoundedRect.to_string(), "rounded_rect");
        assert_eq!(NodeShape::DEFAULT.as_str(), "rounded_rect");
    }
}
