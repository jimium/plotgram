//! Edge port model: side + slot (dsl-spec §7.4).
//!
//! An edge connects not just "which node" but "which side of the node, at
//! which discrete slot on that side". Two layers:
//!
//! - [`PortConstraint`]: the *author's* optional pin, parsed from edge attrs
//!   (`from_side` / `to_side` / `from_slot` / `to_slot`, dsl-spec §7.4.2).
//!   Side without slot is legal (slot stays algorithm-assignable).
//! - [`PortRef`]: the *resolved* port. Written by the layout composition
//!   phase (port decision) — never invented by measure or ink (§7.4.1
//!   write-discipline). Carried on `result::EdgePlacement`.
//!
//! Sides are node-frame relative and do not rename under canvas transposition
//! (LTR/TTB only affects the default inference strategy).

use std::fmt;

use crate::attr::{AttrMap, AttrValue};

/// Which side of a node's frame an edge anchors to (closed four-way set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    North,
    South,
    East,
    West,
}

impl Side {
    /// Parse a side atom. Closed set — unknown atoms are an error, no fallback
    /// (dsl-spec §7.4.2 rule 5).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "north" => Some(Self::North),
            "south" => Some(Self::South),
            "east" => Some(Self::East),
            "west" => Some(Self::West),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A resolved edge port: side + discrete slot (dsl-spec §7.4.1).
///
/// Written by the layout composition phase; layout must honor a fully pinned
/// author constraint or degrade *explicitly* (no silent side changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PortRef {
    /// Which side of the node frame.
    pub side: Side,
    /// Discrete slot on that side (`0, 1, 2, …`) for de-overlapping parallel edges.
    pub slot: u32,
}

/// An author port constraint parsed from edge attrs (dsl-spec §7.4.2).
///
/// `slot: None` = side is pinned but the algorithm may pick any slot on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PortConstraint {
    pub side: Side,
    pub slot: Option<u32>,
}

/// Port constraint validation error (dsl-spec §7.4.2 rules 3 & 5).
#[derive(Debug, Clone, PartialEq)]
pub enum PortConstraintError {
    /// `*_slot` present without its `*_side` (rule 3).
    SlotWithoutSide { slot_key: &'static str },
    /// Side atom outside the closed set (rule 5), or non-atom value.
    InvalidSide { side_key: &'static str, value: String },
    /// Slot is not a non-negative integer number.
    InvalidSlot { slot_key: &'static str, value: String },
}

impl fmt::Display for PortConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlotWithoutSide { slot_key } => {
                write!(f, "`{slot_key}` requires its matching side key (dsl-spec §7.4.2)")
            }
            Self::InvalidSide { side_key, value } => {
                write!(f, "`{side_key}`: `{value}` is not one of north/south/east/west")
            }
            Self::InvalidSlot { slot_key, value } => {
                write!(f, "`{slot_key}`: `{value}` is not a non-negative integer")
            }
        }
    }
}

impl std::error::Error for PortConstraintError {}

/// Extract and validate one end's port constraint from an attr map.
///
/// Returns `Ok(None)` when neither key is present (port fully algorithm-decided).
pub fn port_constraint(
    attrs: &AttrMap,
    side_key: &'static str,
    slot_key: &'static str,
) -> Result<Option<PortConstraint>, PortConstraintError> {
    let side = match attrs.get(side_key) {
        None => None,
        Some(v) => {
            let atom = v.as_str().unwrap_or_default();
            Some(Side::parse(atom).ok_or_else(|| PortConstraintError::InvalidSide {
                side_key,
                value: v.to_string(),
            })?)
        }
    };
    let slot = match attrs.get(slot_key) {
        None => None,
        Some(v) => Some(parse_slot(v).ok_or_else(|| PortConstraintError::InvalidSlot {
            slot_key,
            value: v.to_string(),
        })?),
    };
    match (side, slot) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err(PortConstraintError::SlotWithoutSide { slot_key }),
        (Some(side), slot) => Ok(Some(PortConstraint { side, slot })),
    }
}

/// A slot must be a non-negative integer number (dsl-spec §7.4.2).
fn parse_slot(v: &AttrValue) -> Option<u32> {
    let n = v.as_f64()?;
    if n >= 0.0 && n.fract() == 0.0 && n <= u32::MAX as f64 {
        Some(n as u32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, AttrValue)]) -> AttrMap {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    #[test]
    fn side_closed_set_round_trips() {
        for s in ["north", "south", "east", "west"] {
            assert_eq!(Side::parse(s).unwrap().as_str(), s);
        }
        assert_eq!(Side::parse("up"), None, "closed set: no fallback");
        assert_eq!(Side::parse("North"), None, "atoms are case-sensitive");
    }

    #[test]
    fn constraint_extraction_rules() {
        // Rule 1: both absent → None
        assert_eq!(port_constraint(&attrs(&[]), "from_side", "from_slot"), Ok(None));

        // Rule 2: side only → slot stays algorithm-assignable
        let a = attrs(&[("from_side", AttrValue::Atom("south".into()))]);
        assert_eq!(
            port_constraint(&a, "from_side", "from_slot"),
            Ok(Some(PortConstraint { side: Side::South, slot: None }))
        );

        // Rule 4: side + slot → fully pinned
        let a = attrs(&[
            ("from_side", AttrValue::Atom("south".into())),
            ("from_slot", AttrValue::Num(1.0)),
        ]);
        assert_eq!(
            port_constraint(&a, "from_side", "from_slot"),
            Ok(Some(PortConstraint { side: Side::South, slot: Some(1) }))
        );

        // Rule 3: slot without side → error
        let a = attrs(&[("from_slot", AttrValue::Num(0.0))]);
        assert_eq!(
            port_constraint(&a, "from_side", "from_slot"),
            Err(PortConstraintError::SlotWithoutSide { slot_key: "from_slot" })
        );

        // Rule 5: unknown side atom → error, no fallback
        let a = attrs(&[("from_side", AttrValue::Atom("center".into()))]);
        assert!(matches!(
            port_constraint(&a, "from_side", "from_slot"),
            Err(PortConstraintError::InvalidSide { .. })
        ));
    }

    #[test]
    fn slot_must_be_non_negative_integer() {
        for bad in [AttrValue::Num(-1.0), AttrValue::Num(1.5), AttrValue::Str("2".into())] {
            let a = attrs(&[
                ("to_side", AttrValue::Atom("north".into())),
                ("to_slot", bad),
            ]);
            assert!(matches!(
                port_constraint(&a, "to_side", "to_slot"),
                Err(PortConstraintError::InvalidSlot { .. })
            ));
        }
    }
}
