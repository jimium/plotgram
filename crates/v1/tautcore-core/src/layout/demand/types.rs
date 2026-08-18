//! 压力模型共享类型。

use crate::layout::group::CorridorAxis;
use crate::layout::Port;
use serde::Serialize;

/// 邻层边带 demand vs 有效间隙。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BandDemand {
    pub upper_layer_y: f64,
    pub lower_layer_y: f64,
    pub effective_gap: f64,
    pub crossing_edges: usize,
    pub demand: f64,
    pub deficit: f64,
}

/// 走廊负载（与 B2 `CorridorOccupancy` 字段同构）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorridorDemand {
    pub corridor_index: usize,
    pub axis: CorridorAxis,
    pub group_a: String,
    pub group_b: String,
    pub load: usize,
    pub capacity: usize,
    pub span: f64,
    pub gap: f64,
}

impl CorridorDemand {
    pub fn overflow(&self) -> usize {
        self.load.saturating_sub(self.capacity)
    }

    pub fn is_over(&self) -> bool {
        self.capacity > 0 && self.load > self.capacity
    }
}

/// 节点端口侧压力。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortPressure {
    pub node_id: String,
    pub side: Port,
    pub count: usize,
}

/// 跨组走廊风险档（β 项；非边级硬门禁）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum CorridorRisk {
    /// 同 leaf 或无法判定 leaf
    SameOrUnknown = 0,
    /// 有廊链且未超容
    ChainOk = 1,
    /// 跨 leaf 但无廊链
    NoChain = 2,
    /// 廊链上存在超容（load > capacity）
    Overloaded = 3,
}

impl CorridorRisk {
    pub fn as_weight(self) -> f64 {
        match self {
            CorridorRisk::SameOrUnknown => 0.0,
            CorridorRisk::ChainOk => 0.25,
            CorridorRisk::NoChain => 1.0,
            CorridorRisk::Overloaded => 1.5,
        }
    }
}

/// 边级难度特征（无折线即可算）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EdgeFeatures {
    pub edge_index: usize,
    pub from: String,
    pub to: String,
    pub span_ranks: usize,
    pub corridor_risk: CorridorRisk,
    pub band_deficit: f64,
    pub corridor_overflow: f64,
    pub port_pressure: usize,
    /// P1：L 骨架障碍命中数（max 两候选）
    pub obstacle_hits: usize,
    /// P3：网格 cell overflow（相对 soft_cap）
    pub grid_overflow: usize,
}
