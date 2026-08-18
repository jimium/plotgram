//! Built-in shape → port side policy (no DSL).
//!
//! Compose PortWriter (`assign_ports`) is the sole consumer: FREE honors
//! [`ShapePortPolicy`]; author FixedSide wins.
//! See ports-and-channel.md «Shape port policy».

use crate::port::Side;
use crate::shape::NodeShape;

/// Default port-side policy for a [`NodeShape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapePortPolicy {
    /// Sides that may receive FREE ports (non-empty).
    pub allowed: &'static [Side],
    /// Overflow / fallback attempt order; must cover `allowed`.
    pub preference: &'static [Side],
    /// Soft per-side FREE budget before same-face overflow counting;
    /// does **not** force a face change when primary is allowed (`None` = unbounded).
    pub capacity_per_side: Option<u32>,
}

impl ShapePortPolicy {
    pub fn allows(self, side: Side) -> bool {
        self.allowed.contains(&side)
    }
}

const NSEW: &[Side] = &[Side::North, Side::South, Side::East, Side::West];
/// Rect-family preference placeholder; Compose merges with topology `side_preference`.
const PREF_NSEW: &[Side] = NSEW;
/// Cylinder: NS preferred over EW when overflowing.
const PREF_NS_EW: &[Side] = &[Side::North, Side::South, Side::East, Side::West];
/// Person: no North (into the head); South then E/W.
const PERSON_ALLOWED: &[Side] = &[Side::South, Side::East, Side::West];
const PREF_PERSON: &[Side] = &[Side::South, Side::East, Side::West];

const OPEN: ShapePortPolicy = ShapePortPolicy {
    allowed: NSEW,
    preference: PREF_NSEW,
    capacity_per_side: None,
};

const DIAMOND: ShapePortPolicy = ShapePortPolicy {
    allowed: NSEW,
    preference: PREF_NSEW,
    capacity_per_side: Some(1),
};

const CYLINDER: ShapePortPolicy = ShapePortPolicy {
    allowed: NSEW,
    preference: PREF_NS_EW,
    capacity_per_side: None,
};

const PERSON: ShapePortPolicy = ShapePortPolicy {
    allowed: PERSON_ALLOWED,
    preference: PREF_PERSON,
    capacity_per_side: None,
};

/// Look up the built-in port policy for `shape`.
pub fn policy_for(shape: NodeShape) -> ShapePortPolicy {
    match shape {
        NodeShape::Rect
        | NodeShape::RoundedRect
        | NodeShape::Stadium
        | NodeShape::Parallelogram
        | NodeShape::Document
        | NodeShape::Cloud
        | NodeShape::Subprocess
        | NodeShape::Circle
        | NodeShape::Hexagon => OPEN,
        NodeShape::Diamond => DIAMOND,
        NodeShape::Cylinder => CYLINDER,
        NodeShape::Person => PERSON,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_has_nonempty_allowed_covered_by_preference() {
        for &shape in NodeShape::ALL {
            let p = policy_for(shape);
            assert!(!p.allowed.is_empty(), "{shape}: empty allowed");
            for &s in p.allowed {
                assert!(
                    p.preference.contains(&s),
                    "{shape}: preference must cover allowed side {s}"
                );
            }
            for &s in p.preference {
                assert!(
                    p.allowed.contains(&s),
                    "{shape}: preference side {s} not in allowed"
                );
            }
        }
    }

    #[test]
    fn person_forbids_north() {
        let p = policy_for(NodeShape::Person);
        assert!(!p.allows(Side::North));
        assert!(p.allows(Side::South));
    }

    #[test]
    fn diamond_capacity_one() {
        assert_eq!(policy_for(NodeShape::Diamond).capacity_per_side, Some(1));
    }

    #[test]
    fn default_shape_is_open() {
        let p = policy_for(NodeShape::DEFAULT);
        assert_eq!(p.capacity_per_side, None);
        assert_eq!(p.allowed.len(), 4);
    }
}
