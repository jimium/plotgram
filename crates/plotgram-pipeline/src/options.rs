//! Options for a compile run (theme id, measure overrides, …).

/// Orchestrator knobs. Theme resolution belongs here — not in engine.
#[derive(Debug, Clone, Default)]
pub struct PipelineOptions {
    /// Theme id override (`None` = from diagram `theme:` or default).
    pub theme: Option<String>,
    /// When true, skip content-block measure (label-only sizes).
    pub labels_only: bool,
}
