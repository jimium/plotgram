//! Build errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error(transparent)]
    Parse(#[from] plotgram_parse::ParseError),

    #[error(transparent)]
    Layout(#[from] plotgram_engine::LayoutError),

    #[error("measure: {0}")]
    Measure(String),

    #[error("build stage `{stage}` not implemented: {detail}")]
    NotImplemented { stage: &'static str, detail: String },
}
