//! Edge port model: five-tier author constraint + resolved port (dsl-spec §7.4).
//!
//! Two layers:
//!
//! - [`PortConstraint`]: the *author's* optional pin — five tiers aligned with
//!   ELK/yFiles (architecture.md §7.1): FREE (`None`), [`FixedSide`],
//!   [`FixedOrder`], [`FixedRatio`], [`FixedPos`], plus [`Candidates`] side
//!   sets. Carried as first-class fields on [`crate::graph::Edge`]
//!   (`from_port` / `to_port`). DSL keys `from_side` / … are lifted by
//!   [`crate::graph::Edge::lift_structural_attrs`].
//! - [`PortRef`]: the *resolved* port (side + [`AlongSpec`]). Written by the
//!   layout composition phase onto [`crate::result::EdgePlacement`] — never
//!   invented by measure or ink.
//!
//! Sides are node-frame relative and do not rename under canvas transposition
//! (LTR/TTB only affects the default inference strategy).
//!
//! [`FixedSide`]: PortConstraint::FixedSide
//! [`FixedOrder`]: PortConstraint::FixedOrder
//! [`FixedRatio`]: PortConstraint::FixedRatio
//! [`FixedPos`]: PortConstraint::FixedPos
//! [`Candidates`]: PortConstraint::Candidates

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

/// How a resolved port sits along its side (architecture.md §7.1 `along_spec`).
///
/// `Ordered.order` expresses **relative order only** — it is not a pixel
/// truth. Metric expands the dense, centered pixel anchor from `(order,
/// count)` against the final node frame.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlongSpec {
    /// Stable relative order within the (node, side) group; `count` is the
    /// number of ordered ports sharing the side.
    Ordered { order: u32, count: u32 },
    /// Author-pinned ratio along the side (`∈ [0,1]`, FIXED_RATIO).
    Ratio(f64),
    /// Author-pinned node-local point on the frame boundary (FIXED_POS),
    /// relative to the frame's origin in the resolved orientation space.
    LocalOffset(Point),
}

/// A resolved edge port: side + along-spec (dsl-spec §7.4.1).
///
/// Written by the layout composition phase; layout must honor a fully pinned
/// author constraint or degrade *explicitly* (no silent side changes).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PortRef {
    /// Which side of the node frame.
    pub side: Side,
    /// How the port sits along that side.
    pub along: AlongSpec,
}

/// An author port constraint parsed from edge attrs (dsl-spec §7.4.2).
///
/// Five tiers (architecture.md §7.1); `None` on the edge field = FREE.
/// Sides / local points are in the node's *physical* frame — layout
/// canonicalizes them (architecture.md §9.3).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PortConstraint {
    /// FIXED_SIDE: author pins the side; the algorithm assigns the order.
    FixedSide { side: Side },
    /// FIXED_ORDER: author pins the side and a relative order key on it
    /// (smaller keys come first). Pixel position is Metric's expansion.
    FixedOrder { side: Side, order: u32 },
    /// FIXED_RATIO: author pins the along-side ratio (`∈ [0,1]`).
    FixedRatio { side: Side, ratio: f64 },
    /// FIXED_POS: author pins a node-local point; it must lie on the node
    /// boundary (validated by layout against the measured size — hard fail,
    /// never a silent no-op).
    FixedPos { local: Point },
    /// Author offers a candidate side set; the algorithm scores and picks one
    /// (ports-and-channel.md §2). Empty sets are rejected at parse time.
    Candidates { sides: Vec<Side> },
}

impl PortConstraint {
    /// The pinned side for tiers that have exactly one (FixedSide /
    /// FixedOrder / FixedRatio); `None` for FixedPos / Candidates.
    pub fn pinned_side(&self) -> Option<Side> {
        match self {
            Self::FixedSide { side } | Self::FixedOrder { side, .. } | Self::FixedRatio { side, .. } => {
                Some(*side)
            }
            Self::FixedPos { .. } | Self::Candidates { .. } => None,
        }
    }

    /// The author order key (FIXED_ORDER only).
    pub fn order_key(&self) -> Option<u32> {
        match self {
            Self::FixedOrder { order, .. } => Some(*order),
            _ => None,
        }
    }
}

/// Port constraint validation error (dsl-spec §7.4.2).
#[derive(Debug, Clone, PartialEq)]
pub enum PortConstraintError {
    /// `*_slot` present without its `*_side` (rule 3).
    SlotWithoutSide { slot_key: &'static str },
    /// `*_ratio` present without its `*_side`.
    RatioWithoutSide { ratio_key: &'static str },
    /// Side atom outside the closed set (rule 5), or non-atom value.
    InvalidSide { side_key: &'static str, value: String },
    /// Slot/order is not a non-negative integer number.
    InvalidSlot { slot_key: &'static str, value: String },
    /// Ratio is not a finite number in `[0,1]`.
    InvalidRatio { ratio_key: &'static str, value: String },
    /// `*_x` / `*_y` given without its partner (FIXED_POS needs both).
    PosPartial { present_key: &'static str, missing_key: &'static str },
    /// Coordinate for FIXED_POS is not a finite number.
    InvalidPosCoord { key: &'static str, value: String },
    /// Keys from more than one tier on the same end (tiers are exclusive).
    ConflictingTier { keys: Vec<&'static str> },
    /// Candidate side list is empty.
    EmptyCandidates { sides_key: &'static str },
    /// `critical` present but not a boolean literal.
    InvalidCritical { value: String },
    /// Manual `edge_group` was removed; use layout `auto_edge_grouping`.
    UnsupportedEdgeGroup,
}

impl fmt::Display for PortConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCritical { value } => {
                write!(f, "`critical` expects a boolean, got {value}")
            }
            Self::UnsupportedEdgeGroup => {
                write!(
                    f,
                    "`edge_group` is unsupported; enable layout `auto_edge_grouping` \
                     for automatic fan-in/fan-out merging (dsl-spec §7.4.3)"
                )
            }
            Self::SlotWithoutSide { slot_key } => {
                write!(f, "`{slot_key}` requires its matching side key (dsl-spec §7.4.2)")
            }
            Self::RatioWithoutSide { ratio_key } => {
                write!(f, "`{ratio_key}` requires its matching side key (dsl-spec §7.4.2)")
            }
            Self::InvalidSide { side_key, value } => {
                write!(f, "`{side_key}`: `{value}` is not one of north/south/east/west")
            }
            Self::InvalidSlot { slot_key, value } => {
                write!(f, "`{slot_key}`: `{value}` is not a non-negative integer")
            }
            Self::InvalidRatio { ratio_key, value } => {
                write!(f, "`{ratio_key}`: `{value}` is not a finite number in [0,1]")
            }
            Self::PosPartial {
                present_key,
                missing_key,
            } => {
                write!(f, "`{present_key}` requires its partner `{missing_key}` (FIXED_POS needs both)")
            }
            Self::InvalidPosCoord { key, value } => {
                write!(f, "`{key}`: `{value}` is not a finite number")
            }
            Self::ConflictingTier { keys } => {
                write!(
                    f,
                    "port keys {} span more than one constraint tier; tiers are exclusive (dsl-spec §7.4.2)",
                    keys.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")
                )
            }
            Self::EmptyCandidates { sides_key } => {
                write!(f, "`{sides_key}`: candidate side list must not be empty")
            }
        }
    }
}

impl std::error::Error for PortConstraintError {}

/// The attr keys one port end is lifted from (dsl-spec §7.4.2).
#[derive(Debug, Clone, Copy)]
pub struct PortKeys {
    pub side: &'static str,
    pub slot: &'static str,
    pub ratio: &'static str,
    pub x: &'static str,
    pub y: &'static str,
    pub sides: &'static str,
}

/// Edge-end key sets (dsl-spec §7.4.2).
pub const FROM_PORT_KEYS: PortKeys = PortKeys {
    side: "from_side",
    slot: "from_slot",
    ratio: "from_ratio",
    x: "from_x",
    y: "from_y",
    sides: "from_sides",
};

/// Edge-end key sets (dsl-spec §7.4.2).
pub const TO_PORT_KEYS: PortKeys = PortKeys {
    side: "to_side",
    slot: "to_slot",
    ratio: "to_ratio",
    x: "to_x",
    y: "to_y",
    sides: "to_sides",
};

/// Extract and validate one end's port constraint from an attr map.
///
/// Returns `Ok(None)` when no key is present (port fully algorithm-decided,
/// FREE). Tiers are exclusive: mixing keys from different tiers is an error,
/// never a silent pick.
pub fn port_constraint(
    attrs: &AttrMap,
    keys: PortKeys,
) -> Result<Option<PortConstraint>, PortConstraintError> {
    let present: Vec<&'static str> = [keys.side, keys.slot, keys.ratio, keys.x, keys.y, keys.sides]
        .into_iter()
        .filter(|k| attrs.contains_key(*k))
        .collect();
    if present.is_empty() {
        return Ok(None);
    }

    let side = parse_side_opt(attrs, keys.side)?;
    let slot = parse_slot_opt(attrs, keys.slot)?;
    let ratio = parse_ratio_opt(attrs, keys.ratio)?;
    let x = parse_coord_opt(attrs, keys.x)?;
    let y = parse_coord_opt(attrs, keys.y)?;
    let sides = parse_sides_opt(attrs, keys.sides)?;

    // FIXED_POS tier: x + y, nothing else.
    if x.is_some() || y.is_some() {
        let conflicts: Vec<&'static str> = [keys.side, keys.slot, keys.ratio, keys.sides]
            .into_iter()
            .filter(|k| attrs.contains_key(*k))
            .collect();
        if !conflicts.is_empty() {
            return Err(PortConstraintError::ConflictingTier {
                keys: merge_present([keys.x, keys.y], &conflicts),
            });
        }
        let x = x.ok_or(PortConstraintError::PosPartial {
            present_key: keys.y,
            missing_key: keys.x,
        })?;
        let y = y.ok_or(PortConstraintError::PosPartial {
            present_key: keys.x,
            missing_key: keys.y,
        })?;
        return Ok(Some(PortConstraint::FixedPos {
            local: Point { x, y },
        }));
    }

    // CANDIDATES tier: sides list, nothing else.
    if let Some(sides) = sides {
        let conflicts: Vec<&'static str> = [keys.side, keys.slot, keys.ratio]
            .into_iter()
            .filter(|k| attrs.contains_key(*k))
            .collect();
        if !conflicts.is_empty() {
            return Err(PortConstraintError::ConflictingTier {
                keys: merge_present([keys.sides], &conflicts),
            });
        }
        if sides.is_empty() {
            return Err(PortConstraintError::EmptyCandidates {
                sides_key: keys.sides,
            });
        }
        return Ok(Some(PortConstraint::Candidates { sides }));
    }

    // Side-based tiers.
    match (side, slot, ratio) {
        (None, Some(_), _) => Err(PortConstraintError::SlotWithoutSide { slot_key: keys.slot }),
        (None, _, Some(_)) => Err(PortConstraintError::RatioWithoutSide {
            ratio_key: keys.ratio,
        }),
        (Some(side), Some(order), None) => Ok(Some(PortConstraint::FixedOrder { side, order })),
        (Some(side), None, Some(ratio)) => Ok(Some(PortConstraint::FixedRatio { side, ratio })),
        (Some(_), Some(_), Some(_)) => Err(PortConstraintError::ConflictingTier {
            keys: merge_present([keys.side], &[keys.slot, keys.ratio]),
        }),
        (Some(side), None, None) => Ok(Some(PortConstraint::FixedSide { side })),
        (None, None, None) => Ok(None),
    }
}

fn merge_present(
    head: impl IntoIterator<Item = &'static str>,
    tail: &[&'static str],
) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = head.into_iter().collect();
    out.extend_from_slice(tail);
    out
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

fn parse_ratio_opt(attrs: &AttrMap, key: &'static str) -> Result<Option<f64>, PortConstraintError> {
    match attrs.get(key) {
        None => Ok(None),
        Some(v) => {
            let n = v.as_f64();
            if !n.is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n)) {
                return Err(PortConstraintError::InvalidRatio {
                    ratio_key: key,
                    value: v.to_string(),
                });
            }
            Ok(Some(n.unwrap()))
        }
    }
}

fn parse_coord_opt(attrs: &AttrMap, key: &'static str) -> Result<Option<f64>, PortConstraintError> {
    match attrs.get(key) {
        None => Ok(None),
        Some(v) => {
            let n = v.as_f64();
            if !n.is_some_and(f64::is_finite) {
                return Err(PortConstraintError::InvalidPosCoord {
                    key,
                    value: v.to_string(),
                });
            }
            Ok(Some(n.unwrap()))
        }
    }
}

/// Candidate side list: one atom or a comma-separated atom list (AttrValue
/// has no list type). Closed set; duplicates are dropped keeping first
/// occurrence order.
fn parse_sides_opt(
    attrs: &AttrMap,
    key: &'static str,
) -> Result<Option<Vec<Side>>, PortConstraintError> {
    let Some(v) = attrs.get(key) else { return Ok(None) };
    let raw = v.as_str().unwrap_or_default();
    let mut sides = Vec::new();
    for atom in raw.split(',') {
        let atom = atom.trim();
        if atom.is_empty() {
            continue;
        }
        let side = Side::parse(atom).ok_or_else(|| PortConstraintError::InvalidSide {
            side_key: key,
            value: atom.to_string(),
        })?;
        if !sides.contains(&side) {
            sides.push(side);
        }
    }
    // Empty here (e.g. `""` or only commas) is validated by the caller via
    // `EmptyCandidates` once tier selection knows `sides` was authored.
    Ok(Some(sides))
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
        assert_eq!(Side::parse("up"), None, "closed set: no fallback");
        assert_eq!(Side::parse("North"), None, "atoms are case-sensitive");
    }

    #[test]
    fn constraint_extraction_rules() {
        let k = FROM_PORT_KEYS;

        // Rule 1: all absent → None (FREE)
        assert_eq!(port_constraint(&attrs(&[]), k), Ok(None));

        // Rule 2: side only → FIXED_SIDE
        let a = attrs(&[("from_side", AttrValue::Atom("south".into()))]);
        assert_eq!(
            port_constraint(&a, k),
            Ok(Some(PortConstraint::FixedSide { side: Side::South }))
        );

        // Rule 4: side + slot → FIXED_ORDER (slot = relative order key)
        let a = attrs(&[
            ("from_side", AttrValue::Atom("south".into())),
            ("from_slot", AttrValue::Num(1.0)),
        ]);
        assert_eq!(
            port_constraint(&a, k),
            Ok(Some(PortConstraint::FixedOrder {
                side: Side::South,
                order: 1
            }))
        );

        // Rule 3: slot without side → error
        let a = attrs(&[("from_slot", AttrValue::Num(0.0))]);
        assert_eq!(
            port_constraint(&a, k),
            Err(PortConstraintError::SlotWithoutSide { slot_key: "from_slot" })
        );

        // Rule 5: unknown side atom → error, no fallback
        let a = attrs(&[("from_side", AttrValue::Atom("center".into()))]);
        assert!(matches!(
            port_constraint(&a, k),
            Err(PortConstraintError::InvalidSide { .. })
        ));
    }

    #[test]
    fn new_tier_extraction() {
        let k = TO_PORT_KEYS;

        // FIXED_RATIO
        let a = attrs(&[
            ("to_side", AttrValue::Atom("north".into())),
            ("to_ratio", AttrValue::Num(0.25)),
        ]);
        assert_eq!(
            port_constraint(&a, k),
            Ok(Some(PortConstraint::FixedRatio {
                side: Side::North,
                ratio: 0.25
            }))
        );

        // FIXED_POS
        let a = attrs(&[("to_x", AttrValue::Num(12.0)), ("to_y", AttrValue::Num(0.0))]);
        assert_eq!(
            port_constraint(&a, k),
            Ok(Some(PortConstraint::FixedPos {
                local: Point { x: 12.0, y: 0.0 }
            }))
        );

        // CANDIDATES (comma list, duplicate dropped)
        let a = attrs(&[("to_sides", AttrValue::Atom("south, east, south".into()))]);
        assert_eq!(
            port_constraint(&a, k),
            Ok(Some(PortConstraint::Candidates {
                sides: vec![Side::South, Side::East]
            }))
        );
    }

    #[test]
    fn slot_must_be_non_negative_integer() {
        let k = TO_PORT_KEYS;
        for bad in [AttrValue::Num(-1.0), AttrValue::Num(1.5), AttrValue::Str("2".into())] {
            let a = attrs(&[
                ("to_side", AttrValue::Atom("north".into())),
                ("to_slot", bad),
            ]);
            assert!(matches!(
                port_constraint(&a, k),
                Err(PortConstraintError::InvalidSlot { .. })
            ));
        }
    }

    #[test]
    fn validation_errors_table() {
        let k = FROM_PORT_KEYS;
        let cases: Vec<(AttrMap, PortConstraintError)> = vec![
            (
                attrs(&[("from_ratio", AttrValue::Num(0.5))]),
                PortConstraintError::RatioWithoutSide { ratio_key: "from_ratio" },
            ),
            (
                attrs(&[
                    ("from_side", AttrValue::Atom("east".into())),
                    ("from_ratio", AttrValue::Num(1.5)),
                ]),
                PortConstraintError::InvalidRatio {
                    ratio_key: "from_ratio",
                    value: "1.5".into(),
                },
            ),
            (
                attrs(&[("from_x", AttrValue::Num(3.0))]),
                PortConstraintError::PosPartial {
                    present_key: "from_x",
                    missing_key: "from_y",
                },
            ),
            (
                attrs(&[
                    ("from_side", AttrValue::Atom("east".into())),
                    ("from_x", AttrValue::Num(3.0)),
                    ("from_y", AttrValue::Num(0.0)),
                ]),
                PortConstraintError::ConflictingTier {
                    keys: vec!["from_x", "from_y", "from_side"],
                },
            ),
            (
                attrs(&[
                    ("from_sides", AttrValue::Atom("north".into())),
                    ("from_slot", AttrValue::Num(0.0)),
                    ("from_side", AttrValue::Atom("north".into())),
                ]),
                PortConstraintError::ConflictingTier {
                    keys: vec!["from_sides", "from_side", "from_slot"],
                },
            ),
            (
                attrs(&[("from_sides", AttrValue::Atom(" , ".into()))]),
                PortConstraintError::EmptyCandidates { sides_key: "from_sides" },
            ),
        ];
        for (i, (a, expected)) in cases.into_iter().enumerate() {
            assert_eq!(port_constraint(&a, k), Err(expected), "case {i}");
        }
    }
}
