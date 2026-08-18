//! Typed tree layout parameters + AttrMap bind.
//!
//! M1–M5 consume named placers below. Other named placers fail honestly
//! (`Unsupported`).

use tautcore_engine_api::LayoutError;
use tautcore_model::attr::AttrMap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeRoutingStyle {
    #[default]
    Orthogonal,
    Straight,
    Polyline,
    OrthogonalAtRoot,
}

impl TreeRoutingStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Orthogonal => "orthogonal",
            Self::Straight => "straight",
            Self::Polyline => "polyline",
            Self::OrthogonalAtRoot => "orthogonal-at-root",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootAlignment {
    #[default]
    Center,
    Median,
    Leading,
    Trailing,
    CenterOfPorts,
    LeadingOnBus,
    TrailingOnBus,
}

impl RootAlignment {
    pub fn from_atom(s: &str) -> Option<Self> {
        match s {
            "center" => Some(Self::Center),
            "median" => Some(Self::Median),
            "leading" | "left" => Some(Self::Leading),
            "trailing" | "right" => Some(Self::Trailing),
            "center-of-ports" | "center_of_ports" => Some(Self::CenterOfPorts),
            "leading-on-bus" | "leading_on_bus" => Some(Self::LeadingOnBus),
            "trailing-on-bus" | "trailing_on_bus" => Some(Self::TrailingOnBus),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::Median => "median",
            Self::Leading => "leading",
            Self::Trailing => "trailing",
            Self::CenterOfPorts => "center-of-ports",
            Self::LeadingOnBus => "leading-on-bus",
            Self::TrailingOnBus => "trailing-on-bus",
        }
    }
}

/// Implemented subtree placers. Unknown names error; named-but-unshipped → Unsupported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlacerId {
    #[default]
    SingleLayer,
    LevelAligned,
    SingleSplitLayered,
    LeftRight,
    Bus,
    DoubleLayer,
    Dendrogram,
    Assistant,
    Compact,
    AspectRatio,
    Radial,
    Balloon,
}

impl PlacerId {
    pub fn from_atom(s: &str) -> Result<Self, PlacerAtom> {
        match s {
            "single-layer" | "layered" | "default" | "centered" => Ok(Self::SingleLayer),
            "level-aligned" => Ok(Self::LevelAligned),
            "single-split-layered" | "split-layered" => Ok(Self::SingleSplitLayered),
            "left-right" => Ok(Self::LeftRight),
            "bus" => Ok(Self::Bus),
            "double-layer" => Ok(Self::DoubleLayer),
            "dendrogram" => Ok(Self::Dendrogram),
            "assistant" => Ok(Self::Assistant),
            "compact" => Ok(Self::Compact),
            "aspect-ratio" | "aspect_ratio" => Ok(Self::AspectRatio),
            "radial" => Ok(Self::Radial),
            "balloon" => Ok(Self::Balloon),
            "single-split" | "multi-layer" | "fixed" => Err(PlacerAtom::Unsupported(s.to_string())),
            other => Err(PlacerAtom::Unknown(other.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SingleLayer => "single-layer",
            Self::LevelAligned => "level-aligned",
            Self::SingleSplitLayered => "single-split-layered",
            Self::LeftRight => "left-right",
            Self::Bus => "bus",
            Self::DoubleLayer => "double-layer",
            Self::Dendrogram => "dendrogram",
            Self::Assistant => "assistant",
            Self::Compact => "compact",
            Self::AspectRatio => "aspect-ratio",
            Self::Radial => "radial",
            Self::Balloon => "balloon",
        }
    }

    pub fn is_layered(self) -> bool {
        matches!(self, Self::SingleLayer | Self::LevelAligned)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacerAtom {
    Unsupported(String),
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubtreeTransform {
    #[default]
    None,
    RotateLeft,
    RotateRight,
}

impl SubtreeTransform {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::RotateLeft => "rotate-left",
            Self::RotateRight => "rotate-right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitPolicy {
    #[default]
    Half,
    Alternate,
}

impl SplitPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Half => "half",
            Self::Alternate => "alternate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitSide {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusSlot {
    Left,
    Right,
    Bottom,
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
                params.min_first_segment = 24.0;
            }
            Self::Spacious => {
                params.node_gap = 40.0;
                params.layer_gap = 56.0;
                params.min_first_segment = 56.0;
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
    pub root_alignment: RootAlignment,
    pub min_first_segment: f64,
    pub root: Option<String>,
    pub placer: PlacerId,
    pub split_policy: SplitPolicy,
    pub preferred_aspect_ratio: f64,
}

impl Default for TreeParams {
    fn default() -> Self {
        Self {
            orientation: Orientation::TopToBottom,
            node_gap: 24.0,
            layer_gap: 40.0,
            routing_style: TreeRoutingStyle::Orthogonal,
            root_alignment: RootAlignment::Center,
            min_first_segment: 40.0,
            root: None,
            placer: PlacerId::SingleLayer,
            split_policy: SplitPolicy::Half,
            preferred_aspect_ratio: 1.0,
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

        let placer = match binder.get_atom("placer").map_err(bind_err)? {
            Some(raw) => match PlacerId::from_atom(raw) {
                Ok(p) => p,
                Err(PlacerAtom::Unsupported(name)) => {
                    return Err(LayoutError::unsupported(format!("tree: placer `{name}`")));
                }
                Err(PlacerAtom::Unknown(name)) => {
                    return Err(LayoutError::message(format!(
                        "option `placer`: unknown value `{name}` \
                         (expected one of: single-layer, single-split-layered, \
                         level-aligned, left-right, bus, double-layer, \
                         dendrogram, assistant, compact, aspect-ratio, radial, balloon)"
                    )));
                }
            },
            None => PlacerId::SingleLayer,
        };

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
        params.placer = placer;

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
                    ("polyline", TreeRoutingStyle::Polyline),
                    ("orthogonal-at-root", TreeRoutingStyle::OrthogonalAtRoot),
                ],
            )
            .map_err(bind_err)?
        {
            params.routing_style = rs;
        }
        if let Some(raw) = binder.get_atom("root_alignment").map_err(bind_err)? {
            params.root_alignment = RootAlignment::from_atom(raw).ok_or_else(|| {
                LayoutError::message(format!(
                    "option `root_alignment`: unknown value `{raw}` \
                     (expected one of: center, median, leading, trailing, \
                     center-of-ports, leading-on-bus, trailing-on-bus)"
                ))
            })?;
        }
        if let Some(v) = binder
            .get_f64_any(&["min_first_segment", "minimum_first_segment_length"])
            .map_err(bind_err)?
        {
            params.min_first_segment = v.max(0.0);
        }
        if let Some(raw) = binder.get_atom("root").map_err(bind_err)? {
            params.root = Some(raw.to_string());
        }
        if let Some(v) = binder
            .get_f64_any(&["preferred_aspect_ratio", "aspect_ratio"])
            .map_err(bind_err)?
        {
            params.preferred_aspect_ratio = v.max(0.0);
        }
        if let Some(raw) = binder.get_atom("split_policy").map_err(bind_err)? {
            params.split_policy = match raw {
                "half" => SplitPolicy::Half,
                "alternate" => SplitPolicy::Alternate,
                other => {
                    return Err(LayoutError::message(format!(
                        "option `split_policy`: unknown value `{other}` \
                         (expected one of: half, alternate)"
                    )));
                }
            };
        }

        Ok(BindResult {
            params,
            preset,
            warnings: binder.unknown_warnings(),
        })
    }

    pub fn hash(&self) -> String {
        let canonical = format!(
            "orientation={}|node_gap={:e}|layer_gap={:e}|routing_style={}|root_alignment={}|min_first_segment={:e}|root={}|placer={}|split_policy={}|preferred_aspect_ratio={:e}",
            self.orientation.as_str(),
            self.node_gap,
            self.layer_gap,
            self.routing_style.as_str(),
            self.root_alignment.as_str(),
            self.min_first_segment,
            self.root.as_deref().unwrap_or(""),
            self.placer.as_str(),
            self.split_policy.as_str(),
            self.preferred_aspect_ratio,
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
