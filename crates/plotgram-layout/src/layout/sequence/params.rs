//! Typed sequence layout parameters + AttrMap bind.
//!
//! Defaults follow `docs/design/layout/sequence/architecture.md` §7.
//! Unsupported-but-named options fail hard (never silent).

use plotgram_engine_api::LayoutError;
use plotgram_model::attr::AttrMap;

use crate::params::{BindError, BindWarning, OptionsBinder};

/// How a self-call occupies the time axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelfLoopRowPolicy {
    /// Occupies two consecutive logical rows (`y0` / `y1` = those row centres).
    #[default]
    DoubleRow,
    /// Occupies one row; Metric raises that row's height and splits `y0`/`y1`
    /// about the row centre.
    SingleTall,
}

impl SelfLoopRowPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DoubleRow => "double-row",
            Self::SingleTall => "single-tall",
        }
    }
}

/// Lifeline secondary-axis order strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifelineOrder {
    /// Node declaration order (product default).
    #[default]
    Declaration,
    /// Greedy first-appearance insertion (MinLA).
    Greedy,
    /// Greedy then pairwise swaps of unpinned vertices.
    Local,
}

impl LifelineOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declaration => "declaration",
            Self::Greedy => "greedy",
            Self::Local => "local",
        }
    }
}

/// Crossing decoration on a crossed lifeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifelineGapStyle {
    /// Life line drawn continuous; message paints above (M0 default).
    None,
    /// Leave a gap on the lifeline at each crossing y (M2 default).
    #[default]
    Notch,
}

impl LifelineGapStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Notch => "notch",
        }
    }
}

/// Named packs that only replace a subset of [`SequenceParams`] defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SequencePreset {
    #[default]
    Default,
    Compact,
    Spacious,
}

impl SequencePreset {
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

    pub fn apply(self, params: &mut SequenceParams) {
        match self {
            Self::Default => {}
            Self::Compact => {
                params.participant_gap = 32.0;
                params.message_gap = 20.0;
            }
            Self::Spacious => {
                params.participant_gap = 72.0;
                params.message_gap = 40.0;
            }
        }
    }
}

/// Fully determined sequence parameters (no `Option` — bind fills every field).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SequenceParams {
    pub participant_gap: f64,
    pub message_gap: f64,
    pub self_loop_width: f64,
    pub self_loop_row_policy: SelfLoopRowPolicy,
    pub lifeline_order: LifelineOrder,
    pub lifeline_gap_style: LifelineGapStyle,
    /// First message centre offset below the tallest participant header.
    pub first_message_offset: f64,
    /// Horizontal inset from the lifeline axis to a message terminal (depth 0).
    pub message_endpoint_inset: f64,
    /// When true, message label width publishes spacing demand (M1).
    pub label_to_gap: bool,
    /// Activation bar width (px).
    pub activation_width: f64,
    /// Nested activation bars step east by this amount (visual only in M1).
    pub activation_inset: f64,
    /// Unpaired / unclosed activations hard-fail when true; else warning.
    pub activation_strict: bool,
}

impl Default for SequenceParams {
    fn default() -> Self {
        Self {
            participant_gap: 48.0,
            message_gap: 28.0,
            self_loop_width: 40.0,
            self_loop_row_policy: SelfLoopRowPolicy::DoubleRow,
            lifeline_order: LifelineOrder::Declaration,
            lifeline_gap_style: LifelineGapStyle::Notch,
            first_message_offset: 30.0,
            message_endpoint_inset: 4.0,
            label_to_gap: true,
            activation_width: 10.0,
            activation_inset: 4.0,
            activation_strict: false,
        }
    }
}

/// Result of [`SequenceParams::bind`].
#[derive(Debug, Clone)]
pub struct BindResult {
    pub params: SequenceParams,
    pub preset: SequencePreset,
    pub warnings: Vec<BindWarning>,
}

impl SequenceParams {
    pub fn bind(options: &AttrMap) -> Result<BindResult, LayoutError> {
        let mut binder = OptionsBinder::new(options);

        let preset = match binder.get_atom("preset").map_err(bind_err)? {
            Some(raw) => SequencePreset::from_atom(raw).ok_or_else(|| {
                super::seq_err(format!(
                    "option `preset`: unknown value `{raw}` (expected one of: default, compact, spacious)"
                ))
            })?,
            None => SequencePreset::Default,
        };

        let mut params = Self::default();
        preset.apply(&mut params);

        if let Some(v) = binder
            .get_f64_any(&["participant_gap", "node_spacing", "node_gap"])
            .map_err(bind_err)?
        {
            params.participant_gap = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["message_gap", "message_spacing"])
            .map_err(bind_err)?
        {
            params.message_gap = v.max(0.0);
        }
        if let Some(v) = binder.get_f64_any(&["self_loop_width"]).map_err(bind_err)? {
            params.self_loop_width = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["first_message_offset"])
            .map_err(bind_err)?
        {
            params.first_message_offset = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["message_endpoint_inset"])
            .map_err(bind_err)?
        {
            params.message_endpoint_inset = v.max(0.0);
        }
        if let Some(v) = binder.get_bool("label_to_gap").map_err(bind_err)? {
            params.label_to_gap = v;
        }
        if let Some(v) = binder
            .get_f64_any(&["activation_width"])
            .map_err(bind_err)?
        {
            params.activation_width = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["activation_inset"])
            .map_err(bind_err)?
        {
            params.activation_inset = v.max(0.0);
        }
        if let Some(v) = binder.get_bool("activation_strict").map_err(bind_err)? {
            params.activation_strict = v;
        }

        if let Some(p) = binder
            .get_enum(
                "self_loop_row_policy",
                &[
                    ("double-row", SelfLoopRowPolicy::DoubleRow),
                    ("double_row", SelfLoopRowPolicy::DoubleRow),
                    ("single-tall", SelfLoopRowPolicy::SingleTall),
                    ("single_tall", SelfLoopRowPolicy::SingleTall),
                ],
            )
            .map_err(bind_err)?
        {
            params.self_loop_row_policy = p;
        }

        if let Some(raw) = binder.get_atom("lifeline_order").map_err(bind_err)? {
            match raw {
                "declaration" => params.lifeline_order = LifelineOrder::Declaration,
                "greedy" => params.lifeline_order = LifelineOrder::Greedy,
                "local" => params.lifeline_order = LifelineOrder::Local,
                other => {
                    return Err(super::seq_err(format!(
                        "option `lifeline_order`: unknown value `{other}` \
                         (expected one of: declaration, greedy, local)"
                    )));
                }
            }
        }

        if let Some(raw) = binder.get_atom("lifeline_gap_style").map_err(bind_err)? {
            match raw {
                "none" => params.lifeline_gap_style = LifelineGapStyle::None,
                "notch" => params.lifeline_gap_style = LifelineGapStyle::Notch,
                "hop" => {
                    return Err(super::seq_err(format!(
                        "sequence: unsupported: lifeline_gap_style `{raw}` (postponed)"
                    )));
                }
                other => {
                    return Err(super::seq_err(format!(
                        "option `lifeline_gap_style`: unknown value `{other}` \
                         (expected one of: none, notch, hop)"
                    )));
                }
            }
        }

        Ok(BindResult {
            params,
            preset,
            warnings: binder.unknown_warnings(),
        })
    }

    pub fn hash(&self) -> String {
        let canonical = format!(
            "participant_gap={:e}|message_gap={:e}|self_loop_width={:e}|\
             self_loop_row_policy={}|lifeline_order={}|lifeline_gap_style={}|\
             first_message_offset={:e}|message_endpoint_inset={:e}|\
             label_to_gap={}|activation_width={:e}|activation_inset={:e}|\
             activation_strict={}",
            self.participant_gap,
            self.message_gap,
            self.self_loop_width,
            self.self_loop_row_policy.as_str(),
            self.lifeline_order.as_str(),
            self.lifeline_gap_style.as_str(),
            self.first_message_offset,
            self.message_endpoint_inset,
            if self.label_to_gap { 1 } else { 0 },
            self.activation_width,
            self.activation_inset,
            if self.activation_strict { 1 } else { 0 },
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
    super::seq_err(err.message)
}
