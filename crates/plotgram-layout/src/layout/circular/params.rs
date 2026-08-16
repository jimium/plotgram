//! Typed circular layout parameters + AttrMap bind.
//!
//! Product defaults match yFiles (`bcc-compact`, `spectral`, `interior`,
//! `cycle`). M3 adds node `circle` / `partition` overlays (`custom`).
//! Named-but-unshipped strategies fail honestly.

use plotgram_engine_api::LayoutError;
use plotgram_model::attr::AttrMap;

use crate::params::{BindError, BindWarning, OptionsBinder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Partitioning {
    #[default]
    BccCompact,
    BccIsolated,
    SingleCycle,
    Custom,
}

impl Partitioning {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BccCompact => "bcc-compact",
            Self::BccIsolated => "bcc-isolated",
            Self::SingleCycle => "single-cycle",
            Self::Custom => "custom",
        }
    }

    fn from_atom(s: &str) -> Result<Self, AtomError> {
        match s {
            "bcc-compact" | "bcc_compact" => Ok(Self::BccCompact),
            "bcc-isolated" | "bcc_isolated" => Ok(Self::BccIsolated),
            "single-cycle" | "single_cycle" => Ok(Self::SingleCycle),
            "custom" => Ok(Self::Custom),
            other => Err(AtomError::Unknown(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PartitionStyle {
    #[default]
    Cycle,
    Disk,
    Organic,
    CompactDisk,
}

impl PartitionStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cycle => "cycle",
            Self::Disk => "disk",
            Self::Organic => "organic",
            Self::CompactDisk => "compact-disk",
        }
    }

    fn from_atom(s: &str) -> Result<Self, AtomError> {
        match s {
            "cycle" => Ok(Self::Cycle),
            "disk" => Err(AtomError::Unsupported(s.to_string())),
            "organic" => Err(AtomError::Unsupported(s.to_string())),
            "compact-disk" | "compact_disk" => Err(AtomError::Unsupported(s.to_string())),
            other => Err(AtomError::Unknown(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CircleOrder {
    #[default]
    Spectral,
    Bfs,
    Declaration,
}

impl CircleOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spectral => "spectral",
            Self::Bfs => "bfs",
            Self::Declaration => "declaration",
        }
    }

    fn from_atom(s: &str) -> Result<Self, AtomError> {
        match s {
            "spectral" => Ok(Self::Spectral),
            "bfs" => Ok(Self::Bfs),
            "declaration" => Ok(Self::Declaration),
            "from-sketch" | "from_sketch" => Err(AtomError::Unsupported(s.to_string())),
            other => Err(AtomError::Unknown(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingPolicy {
    #[default]
    Interior,
    Exterior,
    Automatic,
}

impl RoutingPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Interior => "interior",
            Self::Exterior => "exterior",
            Self::Automatic => "automatic",
        }
    }

    fn from_atom(s: &str) -> Result<Self, AtomError> {
        match s {
            "interior" => Ok(Self::Interior),
            "exterior" => Ok(Self::Exterior),
            "automatic" => Err(AtomError::Unsupported(s.to_string())),
            other => Err(AtomError::Unknown(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CircularPreset {
    #[default]
    Default,
    Compact,
    Spacious,
}

impl CircularPreset {
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

    pub fn apply(self, params: &mut CircularParams) {
        match self {
            Self::Default => {}
            Self::Compact => {
                params.node_gap = 16.0;
                params.component_gap = 32.0;
            }
            Self::Spacious => {
                params.node_gap = 40.0;
                params.component_gap = 72.0;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CircularParams {
    pub partitioning: Partitioning,
    pub partition_style: PartitionStyle,
    pub order: CircleOrder,
    pub routing_policy: RoutingPolicy,
    pub node_gap: f64,
    pub component_gap: f64,
    pub min_radius: f64,
    pub rotation: f64,
    pub place_children_on_common_radius: bool,
    pub from_sketch: bool,
}

impl Default for CircularParams {
    fn default() -> Self {
        Self {
            partitioning: Partitioning::BccCompact,
            partition_style: PartitionStyle::Cycle,
            order: CircleOrder::Spectral,
            routing_policy: RoutingPolicy::Interior,
            node_gap: 24.0,
            component_gap: 48.0,
            min_radius: 0.0,
            rotation: 0.0,
            place_children_on_common_radius: true,
            from_sketch: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BindResult {
    pub params: CircularParams,
    pub preset: CircularPreset,
    pub warnings: Vec<BindWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AtomError {
    Unsupported(String),
    Unknown(String),
}

impl CircularParams {
    pub fn bind(options: &AttrMap) -> Result<BindResult, LayoutError> {
        let mut binder = OptionsBinder::new(options);

        let preset = match binder.get_atom("preset").map_err(bind_err)? {
            Some(raw) => CircularPreset::from_atom(raw).ok_or_else(|| {
                LayoutError::message(format!(
                    "option `preset`: unknown value `{raw}` \
                     (expected one of: default, compact, spacious)"
                ))
            })?,
            None => CircularPreset::Default,
        };

        let mut params = Self::default();
        preset.apply(&mut params);

        if let Some(raw) = binder
            .get_atom_any(&["partitioning", "partitioning_policy"])
            .map_err(bind_err)?
        {
            params.partitioning = match Partitioning::from_atom(raw) {
                Ok(v) => v,
                Err(AtomError::Unsupported(name)) => {
                    return Err(LayoutError::unsupported(format!(
                        "circular: partitioning `{name}`"
                    )));
                }
                Err(AtomError::Unknown(name)) => {
                    return Err(LayoutError::message(format!(
                        "option `partitioning`: unknown value `{name}` \
                         (expected one of: bcc-compact, bcc-isolated, single-cycle, custom)"
                    )));
                }
            };
        }

        if let Some(raw) = binder
            .get_atom_any(&["partition_style", "style"])
            .map_err(bind_err)?
        {
            params.partition_style = match PartitionStyle::from_atom(raw) {
                Ok(v) => v,
                Err(AtomError::Unsupported(name)) => {
                    return Err(LayoutError::unsupported(format!(
                        "circular: partition_style `{name}`"
                    )));
                }
                Err(AtomError::Unknown(name)) => {
                    return Err(LayoutError::message(format!(
                        "option `partition_style`: unknown value `{name}` \
                         (expected one of: cycle, disk, organic, compact-disk)"
                    )));
                }
            };
        }

        if let Some(raw) = binder.get_atom("order").map_err(bind_err)? {
            params.order = match CircleOrder::from_atom(raw) {
                Ok(v) => v,
                Err(AtomError::Unsupported(name)) => {
                    return Err(LayoutError::unsupported(format!(
                        "circular: order `{name}`"
                    )));
                }
                Err(AtomError::Unknown(name)) => {
                    return Err(LayoutError::message(format!(
                        "option `order`: unknown value `{name}` \
                         (expected one of: spectral, bfs, declaration)"
                    )));
                }
            };
        }

        if let Some(raw) = binder
            .get_atom_any(&["routing_policy", "edge_routing_policy"])
            .map_err(bind_err)?
        {
            params.routing_policy = match RoutingPolicy::from_atom(raw) {
                Ok(v) => v,
                Err(AtomError::Unsupported(name)) => {
                    return Err(LayoutError::unsupported(format!(
                        "circular: routing_policy `{name}`"
                    )));
                }
                Err(AtomError::Unknown(name)) => {
                    return Err(LayoutError::message(format!(
                        "option `routing_policy`: unknown value `{name}` \
                         (expected one of: interior, exterior, automatic)"
                    )));
                }
            };
        }

        if let Some(v) = binder
            .get_f64_any(&["node_gap", "node_distance", "minimum_node_distance"])
            .map_err(bind_err)?
        {
            params.node_gap = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["component_gap", "component_distance"])
            .map_err(bind_err)?
        {
            params.component_gap = v.max(0.0);
        }
        if let Some(v) = binder
            .get_f64_any(&["min_radius", "minimum_radius"])
            .map_err(bind_err)?
        {
            params.min_radius = v.max(0.0);
        }
        if let Some(v) = binder.get_f64_any(&["rotation"]).map_err(bind_err)? {
            params.rotation = v;
        }
        if let Some(v) = binder
            .get_bool("place_children_on_common_radius")
            .map_err(bind_err)?
        {
            params.place_children_on_common_radius = v;
        }
        if let Some(v) = binder.get_bool("from_sketch").map_err(bind_err)? {
            params.from_sketch = v;
        }

        gate_m3(&params)?;

        Ok(BindResult {
            params,
            preset,
            warnings: binder.unknown_warnings(),
        })
    }

    pub fn hash(&self) -> String {
        let canonical = format!(
            "partitioning={}|partition_style={}|order={}|routing_policy={}|\
             node_gap={:e}|component_gap={:e}|min_radius={:e}|rotation={:e}|\
             place_children_on_common_radius={}|from_sketch={}",
            self.partitioning.as_str(),
            self.partition_style.as_str(),
            self.order.as_str(),
            self.routing_policy.as_str(),
            self.node_gap,
            self.component_gap,
            self.min_radius,
            self.rotation,
            if self.place_children_on_common_radius {
                1
            } else {
                0
            },
            if self.from_sketch { 1 } else { 0 },
        );
        format!("{:016x}", fnv1a_64(canonical.as_bytes()))
    }
}

/// M3 ships `bcc-compact` / `bcc-isolated` / `single-cycle` / `custom`
/// with spectral/bfs/declaration and interior/exterior CYCLE.
/// `automatic`, disk styles, and `from_sketch` stay `Unsupported`.
fn gate_m3(params: &CircularParams) -> Result<(), LayoutError> {
    match params.partitioning {
        Partitioning::SingleCycle
        | Partitioning::BccCompact
        | Partitioning::BccIsolated
        | Partitioning::Custom => {}
    }
    match params.order {
        CircleOrder::Bfs | CircleOrder::Declaration | CircleOrder::Spectral => {}
    }
    if params.partition_style != PartitionStyle::Cycle {
        return Err(LayoutError::unsupported(format!(
            "circular: partition_style `{}`",
            params.partition_style.as_str()
        )));
    }
    match params.routing_policy {
        RoutingPolicy::Interior | RoutingPolicy::Exterior => {}
        RoutingPolicy::Automatic => {
            return Err(LayoutError::unsupported(format!(
                "circular: routing_policy `{}`",
                params.routing_policy.as_str()
            )));
        }
    }
    if params.from_sketch {
        return Err(LayoutError::unsupported(
            "circular: from_sketch".to_string(),
        ));
    }
    if !params.place_children_on_common_radius {
        return Err(LayoutError::unsupported(
            "circular: place_children_on_common_radius `false`".to_string(),
        ));
    }
    Ok(())
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
