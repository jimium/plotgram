//! Typed tree layout parameters + AttrMap bind.
//!
//! Skeleton: layered centered placer (not Reingold–Tilford / Buchheim).
//! Radial / AR placers fail honestly until those milestones land.

use plotgram_engine_api::LayoutError;
use plotgram_model::attr::AttrMap;

use crate::params::{BindError, BindWarning, OptionsBinder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    TopToBottom,
    BottomToTop,
    LeftToRight,
    RightToLeft,
}

impl Orientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopToBottom => "top-to-bottom",
            Self::BottomToTop => "bottom-to-top",
            Self::LeftToRight => "left-to-right",
            Self::RightToLeft => "right-to-left",
        }
    }
}

/// Built-in tree edge style when the layout owns ink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeRoutingStyle {
    #[default]
    Orthogonal,
    Straight,
}

impl TreeRoutingStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Orthogonal => "orthogonal",
            Self::Straight => "straight",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreePreset {
    #[default]
    Default,
    Compact,
    Spacious,
}

impl TreePreset {
    pub fn from_atom(s: &str) -> Option<Self> {
        match s {
            "default" | "standard" => Some(Self::Default),
            "compact" => Some(Self::Compact),
            "spacious" => Some(Self::Spacious),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Compact => "compact",
            Self::Spacious => "spacious",
        }
    }

    pub fn apply(self, params: &mut TreeParams) {
        match self {
            Self::Default => {}
            Self::Compact => {
                params.node_gap = 16.0;
                params.layer_gap = 24.0;
            }
            Self::Spacious => {
                params.node_gap = 40.0;
                params.layer_gap = 56.0;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeParams {
    pub orientation: Orientation,
    pub node_gap: f64,
    pub layer_gap: f64,
    pub routing_style: TreeRoutingStyle,
    /// Optional explicit root id. `None` = indegree-0 nodes in declaration order.
    pub root: Option<String>,
}

impl Default for TreeParams {
    fn default() -> Self {
        Self {
            orientation: Orientation::TopToBottom,
            node_gap: 24.0,
            layer_gap: 40.0,
            routing_style: TreeRoutingStyle::Orthogonal,
            root: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BindResult {
    pub params: TreeParams,
    pub preset: TreePreset,
    pub warnings: Vec<BindWarning>,
}

impl TreeParams {
    pub fn bind(options: &AttrMap) -> Result<BindResult, LayoutError> {
        let mut binder = OptionsBinder::new(options);

        if let Some(raw) = binder.get_atom("placer").map_err(bind_err)? {
            match raw {
                "layered" | "default" | "centered" => {}
                "radial" | "balloon" | "bus" => {
                    return Err(LayoutError::message(format!(
                        "tree: unsupported: placer `{raw}`"
                    )));
                }
                other => {
                    return Err(LayoutError::message(format!(
                        "option `placer`: unknown value `{other}` \
                         (expected one of: layered, radial, balloon, bus)"
                    )));
                }
            }
        }

        let preset = match binder.get_atom("preset").map_err(bind_err)? {
            Some(raw) => TreePreset::from_atom(raw).ok_or_else(|| {
                LayoutError::message(format!(
                    "option `preset`: unknown value `{raw}` (expected one of: default, compact, spacious)"
                ))
            })?,
            None => TreePreset::Default,
        };

        let mut params = Self::default();
        preset.apply(&mut params);

        if let Some(o) = binder
            .get_enum_any(
                &["orientation", "direction"],
                &[
                    ("top-to-bottom", Orientation::TopToBottom),
                    ("ttb", Orientation::TopToBottom),
                    ("bottom-to-top", Orientation::BottomToTop),
                    ("btt", Orientation::BottomToTop),
                    ("left-to-right", Orientation::LeftToRight),
                    ("ltr", Orientation::LeftToRight),
                    ("right-to-left", Orientation::RightToLeft),
                    ("rtl", Orientation::RightToLeft),
                ],
            )
            .map_err(bind_err)?
        {
            params.orientation = o;
        }

        if let Some(v) = binder
            .get_f64_any(&["node_gap", "node_distance"])
            .map_err(bind_err)?
        {
            params.node_gap = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["layer_gap", "layer_distance"])
            .map_err(bind_err)?
        {
            params.layer_gap = v.max(0.0);
        }
        if let Some(rs) = binder
            .get_enum(
                "routing_style",
                &[
                    ("orthogonal", TreeRoutingStyle::Orthogonal),
                    ("straight", TreeRoutingStyle::Straight),
                ],
            )
            .map_err(bind_err)?
        {
            params.routing_style = rs;
        }
        if let Some(raw) = binder.get_atom("root").map_err(bind_err)? {
            params.root = Some(raw.to_string());
        }

        Ok(BindResult {
            params,
            preset,
            warnings: binder.unknown_warnings(),
        })
    }

    pub fn hash(&self) -> String {
        let canonical = format!(
            "orientation={}|node_gap={:e}|layer_gap={:e}|routing_style={}|root={}",
            self.orientation.as_str(),
            self.node_gap,
            self.layer_gap,
            self.routing_style.as_str(),
            self.root.as_deref().unwrap_or(""),
        );
        format!("{:016x}", fnv1a_64(canonical.as_bytes()))
    }
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn bind_err(err: BindError) -> LayoutError {
    LayoutError::message(err.message)
}
