//! Sequence IR: [`SeqPlan`] (Compose) and [`SeqMetric`] (Metric).
//!
//! Types follow `docs/design/layout/sequence/architecture.md` §3.
//! `LifelineSide` is private to this kernel — it does not map onto
//! [`plotgram_model::port::Side`] (no `Center` there).

use plotgram_model::geometry::{Point, Rect};
use std::collections::BTreeMap;

/// Attachment side relative to a lifeline centre axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifelineSide {
    East,
    West,
    /// Stub / Lost-Found (post-M0). Kept in the IR so Ink never invents a side.
    #[allow(dead_code)]
    Center,
}

impl LifelineSide {
    pub fn sign(self) -> f64 {
        match self {
            Self::East => 1.0,
            Self::West => -1.0,
            Self::Center => 0.0,
        }
    }
}

/// Horizontal offset from the lifeline axis to a message terminal.
///
/// `activation_depth` is 0 = on-axis (+ inset only); `d > 0` attaches to the
/// outer face of the bar whose 0-based span depth is `d - 1`.
pub fn terminal_dx(
    side: LifelineSide,
    activation_depth: u32,
    bar_width: f64,
    bar_inset: f64,
    endpoint_inset: f64,
) -> f64 {
    let half = if activation_depth == 0 {
        0.0
    } else {
        bar_width / 2.0 + f64::from(activation_depth - 1) * bar_inset
    };
    side.sign() * (half + endpoint_inset)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Call,
    Reply,
    SelfCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageTiming {
    SyncSameRow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRouteTopo {
    Horizontal,
    SelfLoop { side: LifelineSide },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachSpec {
    pub side: LifelineSide,
    pub activation_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageAttach {
    pub from: AttachSpec,
    pub to: AttachSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagePlan {
    pub edge_id: String,
    pub from: String,
    pub to: String,
    pub kind: MessageKind,
    pub timing: MessageTiming,
    /// Send row (sync: the only row; self DoubleRow: the upper row).
    pub row: u32,
    /// Exclusive of sync's single row; SelfCall DoubleRow uses `row + 1`.
    pub end_row: u32,
    pub attach: MessageAttach,
    pub route: MessageRouteTopo,
}

/// Closed activation interval on one lifeline (axes.md §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationSpan {
    /// Stable id: `activation:{edge_id}:{ordinal}`.
    pub id: String,
    pub lifeline: String,
    pub start_row: u32,
    pub end_row: u32,
    /// Nesting depth (0 = outermost). Attach uses `depth + 1`.
    pub depth: u32,
}

/// Combined fragment (alt / loop / …) covering a row × lifeline interval.
///
/// Writer: FragmentWriter (Compose). Metric expands `frame`; Ink/render paint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentPlan {
    /// Author id from the `fragment` edge attr (stable).
    pub id: String,
    /// Decoration id: `fragment:{id}`.
    pub decoration_id: String,
    /// UML operator atom (`alt`, `loop`, `region`, …).
    pub operator: String,
    pub label: Option<String>,
    /// Member messages in declaration order.
    pub edge_ids: Vec<String>,
    pub start_row: u32,
    pub end_row: u32,
    /// Inclusive indices into `lifeline_order`.
    pub lifeline_lo: u32,
    pub lifeline_hi: u32,
    /// Nest depth from the root (0 = outermost).
    pub depth: u32,
    /// Immediate containing fragment, if nested.
    pub parent: Option<String>,
    /// Last row of each operand except the last (divider sits after that row).
    pub operand_splits: Vec<u32>,
}

/// Padding / title band for fragment frames (architecture.md §4 / 19 §4).
pub const FRAGMENT_PAD: f64 = 8.0;
pub const FRAGMENT_PAD_STEP: f64 = 6.0;
pub const FRAGMENT_TITLE_H: f64 = 16.0;

/// Discrete decisions written by Compose. Downstream only reads.
#[derive(Debug, Clone)]
pub struct SeqPlan {
    /// Secondary axis, declaration-stable.
    pub lifeline_order: Vec<String>,
    /// Messages in time order (= edge declaration order).
    pub messages: Vec<MessagePlan>,
    /// Dense row count after SelfCall row policy.
    pub row_count: u32,
    /// Activation intervals (declaration-stable; unpaired → warning + close).
    pub activation_spans: Vec<ActivationSpan>,
    /// Explicit pin: vertex id → required slot (0-based). Verifier reads this.
    pub lifeline_pins: BTreeMap<String, u32>,
    /// Combined fragments in declaration-stable id order.
    pub fragments: Vec<FragmentPlan>,
}

impl SeqPlan {
    pub fn lifeline_index(&self, id: &str) -> Option<usize> {
        self.lifeline_order.iter().position(|x| x == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MessageTerminals {
    pub from: Point,
    pub to: Point,
}

/// Coordinates written by Metric. Ink only expands.
#[derive(Debug, Clone)]
pub struct SeqMetric {
    pub participant_frames: BTreeMap<String, Rect>,
    pub lifeline_x: BTreeMap<String, f64>,
    pub row_y: Vec<f64>,
    pub row_height: Vec<f64>,
    pub message_terminals: BTreeMap<String, MessageTerminals>,
    /// Header band bottom (max participant height).
    pub header_bottom: f64,
    /// Lifeline decoration start y (header bottom).
    pub lifeline_y0: f64,
    /// Lifeline decoration end y (past last row).
    pub lifeline_y1: f64,
    /// Activation bar frames keyed by [`ActivationSpan::id`].
    pub activation_frames: BTreeMap<String, Rect>,
    /// Crossing y values per lifeline (declaration-stable keys). Empty vec if none.
    pub lifeline_crossings: BTreeMap<String, Vec<f64>>,
    /// Fragment frames keyed by [`FragmentPlan::id`].
    pub fragment_frames: BTreeMap<String, Rect>,
    /// Operand divider y values keyed by [`FragmentPlan::id`].
    pub fragment_operand_ys: BTreeMap<String, Vec<f64>>,
}
