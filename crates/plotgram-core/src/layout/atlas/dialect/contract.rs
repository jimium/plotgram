//! HierarchicalContract：Dialect 编译产物（22 §3.3 / 23 §7）。
//!
//! 度量相只消费本契约 + Diagram 几何输入；不再按 DiagramType 分支。

use super::profile::HierarchicalProfile;
use super::scheme::SchemeId;
use crate::layout::atlas::space::{Demand, OccupantId};
use crate::layout::kernel::coordinate::model::{HardConstraint, ObjectiveTerm};

/// 占位体条目（Stage 5 首版：Dialect 声明，度量相逐步填充）。
#[derive(Debug, Clone)]
pub struct ContractOccupant {
    pub id: OccupantId,
    pub demand_x: Demand,
    pub demand_y: Demand,
}

/// Hierarchical 方言 → 度量相的唯一入口契约。
#[derive(Debug, Clone)]
pub struct HierarchicalContract {
    pub scheme_id: SchemeId,
    pub profile: HierarchicalProfile,
    pub occupants: Vec<ContractOccupant>,
    pub demands: Vec<(OccupantId, Demand)>,
    pub hard: Vec<HardConstraint>,
    pub lex: Vec<ObjectiveTerm>,
}

impl HierarchicalContract {
    pub fn from_scheme(scheme: &super::scheme::Scheme) -> Self {
        Self {
            scheme_id: scheme.id,
            profile: scheme.profile,
            occupants: Vec::new(),
            demands: Vec::new(),
            hard: Vec::new(),
            lex: Vec::new(),
        }
    }
}
