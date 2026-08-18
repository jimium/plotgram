//! BundlePlan: intentional shared-trunk topology from `auto_edge_grouping`.
//!
//! Compose writes end-bus bundles (`SourcePrefix` / `TargetSuffix`). Ink joins
//! declared geometry only; Verifier treats BundlePlan as the sole
//! complete-overlap exemption (ink-and-verification.md §5).
//!
//! Note: yFiles Layout Styles demo's "Automatic Bus Routing" is *not* a Hier
//! API — it heuristically fills `gridComponents` / BusDescriptor. That is a
//! separate future feature; do not revive a `bus_routing` boolean.

use std::collections::BTreeSet;

/// How a bundle shares geometry at an edge end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleKind {
    /// Fan-out at original source: SharedPort → Trunk → Bus → Stub.
    SourcePrefix,
    /// Fan-in at original target.
    TargetSuffix,
}

/// Plan-level confluence fact (≥2 edges).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundlePlan {
    pub id: String,
    pub kind: BundleKind,
    /// Member edge ids (declaration / stub order).
    pub member_edges: Vec<String>,
}

impl BundlePlan {
    pub fn is_end_bus(&self) -> bool {
        matches!(
            self.kind,
            BundleKind::SourcePrefix | BundleKind::TargetSuffix
        )
    }

    pub fn at_source(&self) -> bool {
        matches!(self.kind, BundleKind::SourcePrefix)
    }
}

/// Edge ids that participate in an end-bus (skipped by Channel / TrackOrder).
pub fn end_bus_edge_ids(bundles: &[BundlePlan]) -> BTreeSet<String> {
    bundles
        .iter()
        .filter(|b| b.is_end_bus())
        .flat_map(|b| b.member_edges.iter().cloned())
        .collect()
}

/// True iff `a` and `b` co-belong to at least one bundle.
pub fn edges_share_bundle(bundles: &[BundlePlan], a: &str, b: &str) -> bool {
    bundles.iter().any(|bundle| {
        let has_a = bundle.member_edges.iter().any(|e| e == a);
        let has_b = bundle.member_edges.iter().any(|e| e == b);
        has_a && has_b
    })
}
