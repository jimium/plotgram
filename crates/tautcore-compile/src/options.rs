//! Options for a build run (theme id, measure overrides, …).

/// Build knobs. Theme resolution belongs here — not in engine.
#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// Theme id override (`None` = from diagram `theme:` or default).
    pub theme: Option<String>,
    /// When true, skip content-block measure (label-only sizes).
    pub labels_only: bool,
}
