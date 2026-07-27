//! 空间占用抽象：`Occupant` / `Demand`（Stage 0 交付 0.2，23 号文 §2）。
//!
//! 度量相（相 II）的统一词汇：节点、组框、通道、label 都是「占空间的东西」，
//! 对每根轴报出 [`Demand`]，由求解器统一分配坐标。
//!
//! **Substrate 不在此重复定义**：rank × order 骨架已落地在
//! [`super::channel::substrate::Substrate`]，度量相直接消费同一结构，此处仅 re-export。

use super::channel::{TrackId, TrackOrient};
use super::provenance::Provenance;

pub use super::channel::substrate::Substrate;

/// 占用者标识（跨类型统一编号，由构建方分配并保证稳定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OccupantId(pub u32);

/// 占用者类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OccupantKind {
    Node,
    GroupFrame,
    Channel,
    LabelBox,
}

/// 求解轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Axis {
    X,
    Y,
}

/// 单轴空间需求。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Demand {
    /// 硬下界：低于此值即不可行（走 RelaxationLadder，不静默压缩）。
    pub min: f64,
    /// 期望值：无争用时的目标尺寸。
    pub preferred: f64,
    /// 弹性权重；0 = 刚性（不参与富余空间分配）。
    pub grow: f64,
}

impl Demand {
    /// 刚性需求：min = preferred，不吃富余空间。
    pub fn rigid(size: f64) -> Self {
        Self {
            min: size,
            preferred: size,
            grow: 0.0,
        }
    }

    pub fn is_rigid(&self) -> bool {
        self.grow == 0.0
    }
}

/// 空间占用者：向度量相报出每根轴的需求。
pub trait Occupant {
    fn id(&self) -> OccupantId;
    fn kind(&self) -> OccupantKind;
    fn demand(&self, axis: Axis) -> Demand;
    fn provenance(&self) -> &Provenance;
}

/// Stage 3：通道 Occupant——一条 track 上的 lane 带。
#[derive(Debug, Clone)]
pub struct ChannelOccupant {
    pub track_id: TrackId,
    pub orient: TrackOrient,
    /// 法向带宽（`lanes * pitch + 2 * clearance`）。
    pub band: Demand,
    pub provenance: Provenance,
}

impl ChannelOccupant {
    pub fn from_lanes(track_id: TrackId, orient: TrackOrient, lanes: u32) -> Self {
        Self::from_lanes_and_label_band(track_id, orient, lanes, 0.0)
    }

    /// `label_band`：Cross 上 label 法向合计（Main 传 0）；与 `cross_gap_demands` 同口径。
    pub fn from_lanes_and_label_band(
        track_id: TrackId,
        orient: TrackOrient,
        lanes: u32,
        label_band: f64,
    ) -> Self {
        let width = super::channel_metric::cross_track_band_need(lanes, label_band);
        Self {
            track_id,
            orient,
            band: Demand::rigid(width),
            provenance: Provenance::with_detail(
                "metric/channel:lane_demand",
                format!(
                    "track={} lanes={} label_band={label_band:.1}",
                    track_id.0, lanes
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rigid_demand_has_zero_grow_and_equal_bounds() {
        let d = Demand::rigid(120.0);
        assert_eq!(d.min, 120.0);
        assert_eq!(d.preferred, 120.0);
        assert!(d.is_rigid());

        let flexible = Demand {
            min: 80.0,
            preferred: 120.0,
            grow: 1.0,
        };
        assert!(!flexible.is_rigid());
    }
}
