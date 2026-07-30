//! Content AST — structure only, zero geometry (`content-md-spec.md` §5).

use serde::{Deserialize, Serialize};

/// Parsed content block document. May be empty (empty input -> zero blocks).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentDoc {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Block {
    /// Consecutive text lines; each physical line is one hard-broken `Line`.
    Paragraph { lines: Vec<Line> },
    /// Single-level list; consecutive same-kind items only.
    List { ordered: bool, items: Vec<ListItem> },
    /// `---` thematic break.
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    /// Ordered lists only: the author-written number, never renumbered.
    pub number: Option<u32>,
    pub line: Line,
}

/// One rendered line: a non-empty sequence of styled runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub runs: Vec<Run>,
}

/// A maximal span of uniformly-styled text. Adjacent same-style runs are merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    pub style: RunStyle,
}

/// Exactly one style per run — no nesting, no combinations (spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStyle {
    Plain,
    Strong,
    Emph,
    Code,
}

impl Run {
    pub fn new(text: impl Into<String>, style: RunStyle) -> Self {
        Self { text: text.into(), style }
    }
}
