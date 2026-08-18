//! Typed organic layout parameters + AttrMap bind.
//!
//! Reference: yFiles `OrganicLayout` / `SmartOrganicLayout` (docs/reference/
//! yfiles/ebook/04-力导向与stress布局.md §6/§8). Pipeline is
//! `PivotMDS init → SGD stress → VPSC overlap removal → shelf packing`,
//! fully deterministic: fixed iteration counts + explicit `seed` PRNG,
//! no data-dependent stopping, no wall-clock sampling (WASM-safe).

use tautcore_engine_api::LayoutError;
use tautcore_model::attr::AttrMap;

use crate::params::{BindError, BindWarning, OptionsBinder};

/// Default ideal edge length (yFiles `preferredEdgeLength`, scaled for our
/// node sizes ≈ 60×28).
pub const DEFAULT_EDGE_LENGTH: f64 = 60.0;
/// Default overlap-removal gap (yFiles `minimumNodeDistance`).
pub const DEFAULT_NODE_DISTANCE: f64 = 24.0;
/// Default SGD stress epochs (ebook §6: SGD 15–30).
pub const DEFAULT_ITERATIONS: u32 = 30;
/// Default packing aspect ratio (yFiles `aspectRatio`).
pub const DEFAULT_ASPECT_RATIO: f64 = 1.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrganicPreset {
    #[default]
    Default,
    Compact,
    Spacious,
}

impl OrganicPreset {
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

    pub fn apply(self, params: &mut OrganicParams) {
        match self {
            Self::Default => {}
            Self::Compact => {
                params.preferred_edge_length = 40.0;
                params.minimum_node_distance = 12.0;
                params.component_gap = 32.0;
            }
            Self::Spacious => {
                params.preferred_edge_length = 96.0;
                params.minimum_node_distance = 40.0;
                params.component_gap = 72.0;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrganicParams {
    /// Ideal edge length `k` (yFiles `preferredEdgeLength`). Sets the global
    /// scale: stress target distances = BFS hops × k.
    pub preferred_edge_length: f64,
    /// Minimum node-to-node clearance enforced by overlap removal
    /// (yFiles `minimumNodeDistance`).
    pub minimum_node_distance: f64,
    /// SGD stress epochs (ebook §6: 15–30).
    pub iterations: u32,
    /// Explicit PRNG seed for pair shuffling / degeneracy jitter. Same input
    /// + same seed ⇒ bit-identical output (workspace determinism rule).
    pub seed: u64,
    /// Packing target aspect ratio for multi-component graphs.
    pub aspect_ratio: f64,
    /// Clearance between packed components.
    pub component_gap: f64,
    /// When `true`, skip overlap removal (yFiles `nodeOverlapAllowed`).
    pub allow_node_overlaps: bool,
}

impl Default for OrganicParams {
    fn default() -> Self {
        Self {
            preferred_edge_length: DEFAULT_EDGE_LENGTH,
            minimum_node_distance: DEFAULT_NODE_DISTANCE,
            iterations: DEFAULT_ITERATIONS,
            seed: 0,
            aspect_ratio: DEFAULT_ASPECT_RATIO,
            component_gap: 48.0,
            allow_node_overlaps: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BindResult {
    pub params: OrganicParams,
    pub preset: OrganicPreset,
    pub warnings: Vec<BindWarning>,
}

impl OrganicParams {
    pub fn bind(options: &AttrMap) -> Result<BindResult, LayoutError> {
        let mut binder = OptionsBinder::new(options);

        let preset = match binder.get_atom("preset").map_err(bind_err)? {
            Some(raw) => OrganicPreset::from_atom(raw).ok_or_else(|| {
                LayoutError::message(format!(
                    "option `preset`: unknown value `{raw}` \
                     (expected one of: default, compact, spacious)"
                ))
            })?,
            None => OrganicPreset::Default,
        };

        let mut params = Self::default();
        preset.apply(&mut params);

        if let Some(v) = binder
            .get_f64_any(&["preferred_edge_length", "edge_length", "ideal_edge_length"])
            .map_err(bind_err)?
        {
            params.preferred_edge_length = v;
        }
        if let Some(v) = binder
            .get_f64_any(&[
                "minimum_node_distance",
                "node_gap",
                "min_node_gap",
            ])
            .map_err(bind_err)?
        {
            params.minimum_node_distance = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["iterations", "stress_iterations"])
            .map_err(bind_err)?
        {
            if !(0.0..=1000.0).contains(&v) {
                return Err(LayoutError::message(format!(
                    "option `iterations`: {v} out of range 0..=1000"
                )));
            }
            params.iterations = v.round().max(0.0) as u32;
        }
        if let Some(v) = binder.get_f64_any(&["seed"]).map_err(bind_err)? {
            if !v.is_finite() || v < 0.0 {
                return Err(LayoutError::message(format!(
                    "option `seed`: {v} must be a finite non-negative number"
                )));
            }
            params.seed = v as u64;
        }
        if let Some(v) = binder.get_f64_any(&["aspect_ratio"]).map_err(bind_err)? {
            if !(0.25..=4.0).contains(&v) {
                return Err(LayoutError::message(format!(
                    "option `aspect_ratio`: {v} out of range 0.25..=4.0"
                )));
            }
            params.aspect_ratio = v;
        }
        if let Some(v) = binder
            .get_f64_any(&["component_gap", "component_distance"])
            .map_err(bind_err)?
        {
            params.component_gap = v.max(0.0);
        }
        if let Some(v) = binder.get_bool("allow_node_overlaps").map_err(bind_err)? {
            params.allow_node_overlaps = v;
        }
        if let Some(v) = binder.get_bool("node_overlap_allowed").map_err(bind_err)? {
            params.allow_node_overlaps = v;
        }

        if params.preferred_edge_length <= 0.0 {
            return Err(LayoutError::message(format!(
                "option `preferred_edge_length`: {} must be > 0",
                params.preferred_edge_length
            )));
        }

        Ok(BindResult {
            params,
            preset,
            warnings: binder.unknown_warnings(),
        })
    }

    pub fn hash(&self) -> String {
        let canonical = format!(
            "preferred_edge_length={:e}|minimum_node_distance={:e}|iterations={}|\
             seed={}|aspect_ratio={:e}|component_gap={:e}|allow_node_overlaps={}",
            self.preferred_edge_length,
            self.minimum_node_distance,
            self.iterations,
            self.seed,
            self.aspect_ratio,
            self.component_gap,
            if self.allow_node_overlaps { 1 } else { 0 },
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
