//! Layout diagnostics contract (architecture.md §3.4, roadmap phase C).
//!
//! Structured observations carried alongside [`crate::result::LayoutResult`]:
//! warnings (bind-time, non-fatal), relaxations (softened preferences — the
//! channel exists before the first producer), and `params_hash` (attribute
//! layout regressions to params vs code). Hard failures (`Unsupported` /
//! `InfeasibleConstraint`) stay hard errors — they never become diagnostics.

use serde::{Deserialize, Serialize};

/// Observations from one layout run. Empty by default; never affects geometry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutDiagnostics {
    /// Non-fatal observations (e.g. unknown option keys at bind time).
    pub warnings: Vec<LayoutWarning>,
    /// Softened preferences. No producer in this build — the channel is
    /// reserved for future bounded rework (Channel rip-up, Gate fallback);
    /// every relaxation must land here (architecture.md §3.4).
    pub relaxations: Vec<Relaxation>,
    /// Deterministic hash of the bound typed params (16 lowercase hex chars).
    /// Empty string for layouts without typed params (D6: no fake values).
    pub params_hash: String,
}

/// A single non-fatal observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutWarning {
    pub message: String,
}

/// A recorded soft relaxation of a preference (never a hard constraint).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relaxation {
    /// Fixed vocabulary naming the relaxed rule (e.g. `"channel-rip-up"`).
    pub rule: String,
    pub detail: String,
}
