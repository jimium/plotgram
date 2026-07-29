//! Attribute value and free-form attribute map.
//!
//! Aligned with dsl-spec §2.6: attribute values are string | atom | number | boolean
//! (diagram-level `algorithm_config` is modeled as [`crate::contract::AlgorithmRef`],
//! not nested inside [`AttrValue`]).
//!
//! **Known limit:** [`AttrValue`] has no nested map / nested algorithm_config.
//! Attribute blocks stay flat `key → scalar`. If a future DSL needs nested maps
//! in attrs, extend this enum explicitly — do not overload `Atom`.

use std::collections::BTreeMap;
use std::fmt;

/// A single attribute value (dsl-spec scalar `<attribute_value>` forms).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AttrValue {
    /// Quoted string literal.
    Str(String),
    /// Unquoted atom (algorithm names, variant, icon, status, …).
    Atom(String),
    /// Numeric literal.
    Num(f64),
    /// Boolean literal.
    Bool(bool),
}

impl AttrValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) | Self::Atom(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

impl fmt::Display for AttrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str(s) => write!(f, "\"{s}\""),
            Self::Atom(s) => write!(f, "{s}"),
            Self::Num(n) => write!(f, "{n}"),
            Self::Bool(b) => write!(f, "{b}"),
        }
    }
}

/// Free-form ordered attribute map.
///
/// Keys may carry namespace prefixes (`style.fill`, `meta.author`).
/// Uses `BTreeMap` for deterministic iteration (AGENTS.md §2).
pub type AttrMap = BTreeMap<String, AttrValue>;
