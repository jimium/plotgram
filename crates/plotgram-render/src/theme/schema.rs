//! Theme file schema: the on-disk JSON structure (V2 flattened).

use serde::Deserialize;
use std::collections::BTreeMap;

/// Top-level theme file structure.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeFile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub extends: Option<String>,
    pub tokens: Tokens,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub kind_styles: BTreeMap<String, BTreeMap<String, StyleValue>>,
}

/// Design tokens.
#[derive(Debug, Clone, Deserialize)]
pub struct Tokens {
    #[serde(default)]
    pub colors: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub palette: BTreeMap<String, BTreeMap<String, StyleValue>>,
    #[serde(default)]
    pub typography: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub strokes: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub radius: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub spacing: BTreeMap<String, StyleValue>,
}

/// Default element styles (raw, pre-compilation).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Defaults {
    #[serde(default)]
    pub canvas: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub title: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub node: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub edge: BTreeMap<String, StyleValue>,
    #[serde(default)]
    pub group: BTreeMap<String, StyleValue>,
}

/// A style value in theme JSON (string, number, bool, or array).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum StyleValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<f64>),
}

impl StyleValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    /// Resolve to a display string (for SVG attributes).
    pub fn to_svg_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Number(n) => {
                if *n == (*n as i64) as f64 {
                    (*n as i64).to_string()
                } else {
                    format!("{n:.2}")
                }
            }
            Self::Boolean(b) => b.to_string(),
            Self::Array(arr) => arr
                .iter()
                .map(|n| {
                    if *n == (*n as i64) as f64 {
                        (*n as i64).to_string()
                    } else {
                        format!("{n:.2}")
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}
