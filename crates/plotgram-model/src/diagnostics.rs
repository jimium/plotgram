//! Layout diagnostics contract (architecture.md §3.4, roadmap phase C).
//!
//! Structured observations carried alongside [`crate::result::LayoutResult`]:
//! warnings (bind-time, non-fatal), relaxations (softened preferences — the
//! channel exists before the first producer), and `params_hash` (attribute
//! layout regressions to params vs code). Hard failures (`Unsupported` /
//! `InfeasibleConstraint`) stay hard errors — they never become diagnostics.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Observations from one layout run. Empty by default; never affects geometry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutDiagnostics {
    /// Non-fatal observations (e.g. unknown option keys at bind time).
    pub warnings: Vec<LayoutWarning>,
    /// Softened preferences (Channel rip-up, group-substrate fallback, …).
    /// Every soft relaxation must land here (architecture.md §3.4).
    pub relaxations: Vec<Relaxation>,
    /// Deterministic hash of the bound typed params (16 lowercase hex chars).
    /// Empty string for layouts without typed params (D6: no fake values).
    pub params_hash: String,
    /// Hierarchical-only observation block. `None` for non-hier layouts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchical: Option<HierarchicalObs>,
}

/// Hierarchical layout observations (P1). Geometry-neutral; for eval / measure.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HierarchicalObs {
    /// True when group-cut Gate IR was used (`d1.3-gate`); false = root-scope.
    pub channel_used_gates: bool,
    /// Number of bounded rip-up rounds entered (0 when peak occupancy ≤ 1).
    pub ripup_rounds: u32,
    /// End-bus member edge ids (intentional shared corridors for overlap_len).
    pub bus_edge_ids: Vec<String>,
    /// Count of `channel-group-fallback` relaxations (whole-diagram gate-route
    /// or derive-level impure-group fallbacks; group-frame-d2.md §8.12).
    /// Per-edge ScopeMask widenings were trialled and withdrawn — do not
    /// interpret this counter as a per-edge count.
    #[serde(default)]
    pub gate_fallback_events: usize,
    /// Published `LayerGap` demand lower bounds keyed by seam index
    /// (MetricVerifier demand floor, coordinate-and-demand.md §9).
    #[serde(default)]
    pub layer_gap_demands: BTreeMap<u32, f64>,
    /// Number of seams the gate capacity producer published on
    /// (group-frame-d2.md §8.11/§8.13 observability; 0 off the Weak path).
    #[serde(default)]
    pub gate_capacity_seams: usize,
    /// Resolved main-axis gaps, index = seam (length = n_layers - 1).
    #[serde(default)]
    pub layer_gaps: Vec<f64>,
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
