//! Edge / anchor port model (dsl-spec §7.4).
//!
//! Two layers:
//!
//! - [`PortConstraint`]: author pin. Edges: FREE (`None`) or [`FixedSide`]
//!   via `from_side` / `to_side`. Group anchors may also use [`FixedOrder`]
//!   (`side` + optional `slot`). No ratio / pos / candidate-side DSL on edges.
//! - [`PortRef`]: resolved port (`side` + [`AlongSpec`]). Written by Compose
//!   onto [`crate::result::EdgePlacement`]; ink must not invent ports.
//!
//! [`FixedSide`]: PortConstraint::FixedSide
//! [`FixedOrder`]: PortConstraint::FixedOrder

use std::fmt;

use crate::attr::AttrMap;
use crate::geometry::Point;

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
    /// Parse a side atom. Closed set — unknown atoms are an error, no fallback.
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

/// How a resolved port sits along its side.
///
/// `Ordered.order` is relative order only — Metric expands dense centered
/// pixels. `LocalOffset` is algorithm-owned (e.g. self-loop), not DSL.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlongSpec {
    /// Stable relative order within the (node, side) group.
    Ordered { order: u32, count: u32 },
    /// Node-local point on the frame boundary (self-loop etc.).
    LocalOffset(Point),
}

/// A resolved edge port: side + along-spec.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PortRef {
    pub side: Side,
    pub along: AlongSpec,
}

/// Author port constraint.
///
/// Edges only lift [`FixedSide`] (or FREE). [`FixedOrder`] is for
/// `group_anchor` nodes (`side` + optional `slot`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PortConstraint {
    /// Pin the side; Compose assigns along-side order.
    FixedSide { side: Side },
    /// Pin side + relative order key (group anchors only).
    FixedOrder { side: Side, order: u32 },
}

impl PortConstraint {
    pub fn pinned_side(&self) -> Option<Side> {
        match self {
            Self::FixedSide { side } | Self::FixedOrder { side, .. } => Some(*side),
        }
    }

    pub fn order_key(&self) -> Option<u32> {
        match self {
            Self::FixedOrder { order, .. } => Some(*order),
            Self::FixedSide { .. } => None,
        }
    }
}

/// Port constraint validation error.
#[derive(Debug, Clone, PartialEq)]
pub enum PortConstraintError {
    /// Side atom outside the closed set, or non-atom value.
    InvalidSide { side_key: &'static str, value: String },
    /// Slot/order is not a non-negative integer (anchor `slot` only).
    InvalidSlot { slot_key: &'static str, value: String },
    /// `slot` without `side` on a group anchor.
    SlotWithoutSide { slot_key: &'static str },
    /// Removed edge port key still present (`from_slot`, `from_ratio`, …).
    UnsupportedEdgePortKey { key: &'static str },
    /// `critical` present but not a boolean literal (sugar for `weight: 2.0`).
    InvalidCritical { value: String },
    /// `undirected` present but not a boolean literal.
    InvalidUndirected { value: String },
    /// `weight` present but not a positive finite number.
    InvalidWeight { value: String },
    /// Both `critical: true` and `weight` specified — ambiguous.
    CriticalWeightConflict,
    /// Manual `edge_group` was removed; use layout `auto_edge_grouping`.
    UnsupportedEdgeGroup,
}

impl fmt::Display for PortConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCritical { value } => {
                write!(f, "`critical` expects a boolean, got {value}")
            }
            Self::InvalidUndirected { value } => {
                write!(f, "`undirected` expects a boolean, got {value}")
            }
            Self::InvalidWeight { value } => {
                write!(
                    f,
                    "`weight` expects a positive finite number, got {value}"
                )
            }
            Self::CriticalWeightConflict => {
                write!(
                    f,
                    "cannot specify both `critical: true` and `weight` on the same edge \
                     — `critical: true` is sugar for `weight: 2.0`"
                )
            }
            Self::UnsupportedEdgeGroup => {
                write!(
                    f,
                    "`edge_group` is unsupported; enable layout `auto_edge_grouping` \
                     for automatic fan-in/fan-out merging (dsl-spec §7.4.3)"
                )
            }
            Self::InvalidSide { side_key, value } => {
                write!(f, "`{side_key}`: `{value}` is not one of north/south/east/west")
            }
            Self::InvalidSlot { slot_key, value } => {
                write!(f, "`{slot_key}`: `{value}` is not a non-negative integer")
            }
            Self::SlotWithoutSide { slot_key } => {
                write!(f, "`{slot_key}` requires `side` on a group_anchor (dsl-spec §5.7)")
            }
            Self::UnsupportedEdgePortKey { key } => {
                write!(
                    f,
                    "`{key}` is unsupported; edge ports only accept `from_side` / `to_side` \
                     (dsl-spec §7.4)"
                )
            }
        }
    }
}

impl std::error::Error for PortConstraintError {}

/// Edge DSL keys that used to encode finer port tiers — hard-rejected now.
pub const REMOVED_EDGE_PORT_KEYS: &[&str] = &[
    "from_slot",
    "to_slot",
    "from_ratio",
    "to_ratio",
    "from_x",
    "from_y",
    "to_x",
    "to_y",
    "from_sides",
    "to_sides",
];

pub const FROM_SIDE_KEY: &str = "from_side";
pub const TO_SIDE_KEY: &str = "to_side";

/// Lift an edge-end constraint: FREE or FixedSide only.
pub fn edge_port_constraint(
    attrs: &AttrMap,
    side_key: &'static str,
) -> Result<Option<PortConstraint>, PortConstraintError> {
    for &key in REMOVED_EDGE_PORT_KEYS {
        if attrs.contains_key(key) {
            return Err(PortConstraintError::UnsupportedEdgePortKey { key });
        }
    }
    match attrs.get(side_key) {
        None => Ok(None),
        Some(v) => {
            let atom = v.as_str().unwrap_or_default();
            let side = Side::parse(atom).ok_or_else(|| PortConstraintError::InvalidSide {
                side_key,
                value: v.to_string(),
            })?;
            Ok(Some(PortConstraint::FixedSide { side }))
        }
    }
}

/// Group-anchor keys: `side` (required with role) + optional `slot`.
pub const ANCHOR_SIDE_KEY: &str = "side";
pub const ANCHOR_SLOT_KEY: &str = "slot";

/// Lift a group_anchor port constraint from node attrs.
pub fn anchor_port_constraint(
    attrs: &AttrMap,
) -> Result<Option<PortConstraint>, PortConstraintError> {
    let side = parse_side_opt(attrs, ANCHOR_SIDE_KEY)?;
    let slot = parse_slot_opt(attrs, ANCHOR_SLOT_KEY)?;
    match (side, slot) {
        (None, None) => Ok(None),
        (None, Some(_)) => Err(PortConstraintError::SlotWithoutSide {
            slot_key: ANCHOR_SLOT_KEY,
        }),
        (Some(side), None) => Ok(Some(PortConstraint::FixedSide { side })),
        (Some(side), Some(order)) => Ok(Some(PortConstraint::FixedOrder { side, order })),
    }
}

fn parse_side_opt(attrs: &AttrMap, key: &'static str) -> Result<Option<Side>, PortConstraintError> {
    match attrs.get(key) {
        None => Ok(None),
        Some(v) => {
            let atom = v.as_str().unwrap_or_default();
            Ok(Some(Side::parse(atom).ok_or_else(|| {
                PortConstraintError::InvalidSide {
                    side_key: key,
                    value: v.to_string(),
                }
            })?))
        }
    }
}

fn parse_slot_opt(attrs: &AttrMap, key: &'static str) -> Result<Option<u32>, PortConstraintError> {
    match attrs.get(key) {
        None => Ok(None),
        Some(v) => {
            let n = v.as_f64();
            if !n.is_some_and(|n| n >= 0.0 && n.fract() == 0.0 && n <= u32::MAX as f64) {
                return Err(PortConstraintError::InvalidSlot {
                    slot_key: key,
                    value: v.to_string(),
                });
            }
            Ok(Some(n.unwrap() as u32))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attr::AttrValue;

    fn attrs(pairs: &[(&str, AttrValue)]) -> AttrMap {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    #[test]
    fn side_closed_set_round_trips() {
        for s in ["north", "south", "east", "west"] {
            assert_eq!(Side::parse(s).unwrap().as_str(), s);
        }
        assert_eq!(Side::parse("up"), None);
        assert_eq!(Side::parse("North"), None);
    }

    #[test]
    fn edge_side_only() {
        assert_eq!(edge_port_constraint(&attrs(&[]), FROM_SIDE_KEY), Ok(None));
        let a = attrs(&[("from_side", AttrValue::Atom("south".into()))]);
        assert_eq!(
            edge_port_constraint(&a, FROM_SIDE_KEY),
            Ok(Some(PortConstraint::FixedSide { side: Side::South }))
        );
        let a = attrs(&[("from_side", AttrValue::Atom("center".into()))]);
        assert!(matches!(
            edge_port_constraint(&a, FROM_SIDE_KEY),
            Err(PortConstraintError::InvalidSide { .. })
        ));
    }

    #[test]
    fn removed_edge_keys_rejected() {
        for key in ["from_slot", "from_ratio", "from_x", "from_sides", "to_slot"] {
            let a = attrs(&[(key, AttrValue::Num(0.0))]);
            assert_eq!(
                edge_port_constraint(&a, FROM_SIDE_KEY),
                Err(PortConstraintError::UnsupportedEdgePortKey { key }),
                "{key}"
            );
        }
    }

    #[test]
    fn anchor_side_and_optional_slot() {
        let a = attrs(&[("side", AttrValue::Atom("west".into()))]);
        assert_eq!(
            anchor_port_constraint(&a),
            Ok(Some(PortConstraint::FixedSide { side: Side::West }))
        );
        let a = attrs(&[
            ("side", AttrValue::Atom("west".into())),
            ("slot", AttrValue::Num(2.0)),
        ]);
        assert_eq!(
            anchor_port_constraint(&a),
            Ok(Some(PortConstraint::FixedOrder {
                side: Side::West,
                order: 2
            }))
        );
        let a = attrs(&[("slot", AttrValue::Num(0.0))]);
        assert_eq!(
            anchor_port_constraint(&a),
            Err(PortConstraintError::SlotWithoutSide {
                slot_key: "slot"
            })
        );
    }
}
