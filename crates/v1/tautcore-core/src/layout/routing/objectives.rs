//! 路由软目标 / 罚项注册表（Phase 6 / doc 23 A1）。
//!
//! **硬约束不进本表**：穿节点 / 穿组由构造与 `path_is_clean` /
//! `path_avoids_group_interiors` 排除，禁止用「大数罚项」伪装禁止语义。
//!
//! 本模块只登记**同一可行域内的排序权重**。CI
//! （`benchmarks/scripts/check-penalty-ratio.sh`）断言
//! [`SOFT_RANKING`] 中 max/min ≤ [`MAX_SOFT_RATIO`]。
//!
//! 新增罚项须：① 写入本表；② `#[doc]` 说明「为何不是硬约束」；③ PR 说明。

/// 软排序常量允许的最大/最小比（A1）。
pub const MAX_SOFT_RATIO: f64 = 100.0;

/// 工程收口：软罚项项数上限（棘轮 ≤8）。
pub const MAX_SOFT_ITEMS: usize = 8;

// ─── Soft ranking（计入 CI 比）──────────────────────────────────────────────

/// 每折点惩罚。为何不是硬约束：弯折合法，仅偏好更直路径。
pub const BEND_PENALTY: f64 = 28.0;

/// 端点几何偏好（净空不足 + 端口出射偏离）。为何不是硬约束：几何仍合法，仅排序偏好。
pub const ENDPOINT_GEOMETRY_PENALTY: f64 = 42.0;

/// 标签距己边净空不足（`ENDPOINT_GEOMETRY` 别名，消费点语义保留）。
pub const PATH_CLEARANCE_PENALTY: f64 = ENDPOINT_GEOMETRY_PENALTY;

/// 端口出射方向偏离（`ENDPOINT_GEOMETRY` 别名）。
pub const PORT_DIRECTION_PENALTY: f64 = ENDPOINT_GEOMETRY_PENALTY;

/// 标签与 group 壳重叠。为何不是硬约束：端点附近允许软化；硬壳穿组另判。
pub const GROUP_OVERLAP_PENALTY: f64 = 50.0;

/// 正交交叉惩罚（Legacy scorer）。为何不是硬约束：交叉可接受时排序；硬禁穿障另有过滤。
pub const CROSSING_PENALTY: f64 = 70.0;

/// 标签压无关边。为何不是硬约束：可读性排序，非拓扑禁止。
pub const FOREIGN_EDGE_PENALTY: f64 = 100.0;

/// 可见图路径擦 group。为何不是硬约束：硬穿组已过滤；此项惩罚擦边/近距。
pub const GROUP_CROSSING_PENALTY: f64 = 280.0;

/// 空间 AABB/段重叠（标签互叠 + 边段共线重叠）。为何不是硬约束：无完美位时仍选重叠最小者；
/// 有意合流由 NeedsSeparation 另轨。
pub const SPATIAL_OVERLAP_PENALTY: f64 = 280.0;

/// 标签互相重叠（`SPATIAL_OVERLAP` 别名）。
pub const LABEL_OVERLAP_PENALTY: f64 = SPATIAL_OVERLAP_PENALTY;

/// 边段共线重叠（`SPATIAL_OVERLAP` 别名）。
pub const EDGE_OVERLAP_PENALTY: f64 = SPATIAL_OVERLAP_PENALTY;

/// 标签压节点（软余量）。为何不是硬约束：候选全撞节点时仍要选出重叠最小者；
/// 禁止语义由候选过滤 / 后置避让承担。量级压到 ≤100×`BEND_PENALTY`（A1）。
pub const NODE_OVERLAP_PENALTY: f64 = 2800.0;

/// 供 CI / 审计枚举的软排序表（名称稳定、按量级升序）。
///
/// 走廊/通道超载已迁出：由 `ResourceCapacity` 硬约束承担，不再占软排序位。
pub const SOFT_RANKING: &[(&str, f64)] = &[
    ("BEND_PENALTY", BEND_PENALTY),
    ("ENDPOINT_GEOMETRY_PENALTY", ENDPOINT_GEOMETRY_PENALTY),
    ("GROUP_OVERLAP_PENALTY", GROUP_OVERLAP_PENALTY),
    ("CROSSING_PENALTY", CROSSING_PENALTY),
    ("FOREIGN_EDGE_PENALTY", FOREIGN_EDGE_PENALTY),
    ("GROUP_CROSSING_PENALTY", GROUP_CROSSING_PENALTY),
    ("SPATIAL_OVERLAP_PENALTY", SPATIAL_OVERLAP_PENALTY),
    ("NODE_OVERLAP_PENALTY", NODE_OVERLAP_PENALTY),
];

/// 软排序 max/min；供单测与脚本。
pub fn soft_ranking_ratio() -> f64 {
    let mut min_v = f64::INFINITY;
    let mut max_v = 0.0_f64;
    for &(_, v) in SOFT_RANKING {
        if v > 0.0 && v.is_finite() {
            min_v = min_v.min(v);
            max_v = max_v.max(v);
        }
    }
    if min_v > 0.0 && min_v.is_finite() {
        max_v / min_v
    } else {
        f64::INFINITY
    }
}

// ─── Rates（不计入 CI 比：单位不同 / 未接线诊断）────────────────────────────

/// 路径与已占段重叠的每像素费率。为何不是硬约束：连续量费率，非禁止语义。
pub const OVERLAP_PENALTY_PER_PX: f64 = 3.0;

/// 标签-节点重叠面积加权。为何不是硬约束：在 `NODE_OVERLAP_PENALTY` 之上细排。
pub const NODE_OVERLAP_AREA_WEIGHT: f64 = 100.0;

/// 走廊超载诊断费率（未接入生产 scorer；保留供 channel_load 单测）。
pub const CORRIDOR_OVER_PENALTY: f64 = 80.0;

/// 通道负载诊断费率（未接入生产 scorer；保留供 channel_load 单测）。
pub const CHANNEL_LOAD_PENALTY: f64 = 200.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_ranking_ratio_within_a1() {
        let r = soft_ranking_ratio();
        assert!(
            r <= MAX_SOFT_RATIO + 1e-9,
            "soft ranking ratio {r} exceeds MAX_SOFT_RATIO {}",
            MAX_SOFT_RATIO
        );
    }

    #[test]
    fn soft_ranking_item_count_within_cap() {
        assert!(
            SOFT_RANKING.len() <= MAX_SOFT_ITEMS,
            "SOFT_RANKING has {} items > MAX_SOFT_ITEMS {}",
            SOFT_RANKING.len(),
            MAX_SOFT_ITEMS
        );
    }
}
