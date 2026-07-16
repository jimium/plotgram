//! 层通道预算（S2）：邻层边带 / 同排水平走廊需求 → 布局阶段 gap 下界。
//!
//! 写权：仅布局（Sugiyama / architecture coordinate / two_phase macro rank）。
//! 路由后禁止用本模块补缝。
//!
//! 设计：按 demand 预测，而非「无组固定加宽」——简单图 demand 低时仍落在 base_gap。

use crate::ast::{ArrowType, Relation};
use crate::types::DiagramType;
use std::collections::{HashMap, HashSet};

/// 图种系数：共用公式，不同 scale。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeBandDemandProfile {
    /// 跨缝边数 × parallel_gap × 本系数
    pub parallel_scale: f64,
    /// FanIn/FanOut 最大梳齿 × parallel_gap × 本系数
    pub fanin_scale: f64,
    /// 固定标签带（px）
    pub label_band: f64,
    /// 有标签的跨缝边额外带宽（每条）
    pub label_per_edge: f64,
    /// 相对 base_gap 的额外上限（防止画布爆炸）
    pub max_extra: f64,
    /// S4：侧通道（监控枢纽）水平 gutter 系数 × parallel_gap × lanes
    pub side_channel_scale: f64,
    /// S4：侧通道固定基线（px）
    pub side_channel_base: f64,
    /// S4：侧通道 gutter 上限（px）
    pub side_channel_max: f64,
    /// 同排相邻节点：跨层边 × parallel_gap × 本系数 → 水平走廊
    pub horizontal_parallel_scale: f64,
    /// 同排相邻：跨层有标签边每条的水平带宽
    pub horizontal_label_per: f64,
    /// 同排水平走廊相对 NODE_GAP 的额外上限
    pub horizontal_max_extra: f64,
}

impl EdgeBandDemandProfile {
    /// 仅知图种时保守按「有组」处理（不抬无组可读余量）。
    pub fn for_diagram_type(dt: DiagramType) -> Self {
        Self::for_diagram(dt, true)
    }

    /// `has_groups=false` 的 architecture：抬竖直可读系数 + 启水平走廊 demand。
    /// 系数仍乘在 demand 上——边少/无标签时 gap 不会超过 base。
    pub fn for_diagram(dt: DiagramType, has_groups: bool) -> Self {
        match dt {
            DiagramType::Architecture => {
                let mut p = Self {
                    parallel_scale: 0.55,
                    fanin_scale: 0.35,
                    label_band: 32.0,
                    label_per_edge: 4.0,
                    max_extra: 48.0,
                    side_channel_scale: 0.4,
                    side_channel_base: 12.0,
                    side_channel_max: 20.0,
                    horizontal_parallel_scale: 0.0,
                    horizontal_label_per: 0.0,
                    horizontal_max_extra: 0.0,
                };
                if !has_groups {
                    // 竖直：给挤廊图更多可读余量（demand 高才吃到 cap）
                    p.parallel_scale = 0.65;
                    p.fanin_scale = 0.42;
                    p.label_band = 40.0;
                    p.label_per_edge = 8.0;
                    p.max_extra = 88.0;
                    // 水平：同排节点间按跨层边/标签预测走廊
                    p.horizontal_parallel_scale = 0.45;
                    p.horizontal_label_per = 10.0;
                    p.horizontal_max_extra = 56.0;
                }
                p
            }
            _ => Self {
                parallel_scale: 0.4,
                fanin_scale: 0.2,
                label_band: 20.0,
                label_per_edge: 1.0,
                max_extra: 24.0,
                side_channel_scale: 0.0,
                side_channel_base: 0.0,
                side_channel_max: 0.0,
                horizontal_parallel_scale: 0.0,
                horizontal_label_per: 0.0,
                horizontal_max_extra: 0.0,
            },
        }
    }
}

/// 单条邻层缝的需求分解（可观测 / 单测）。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeBandDemandBreakdown {
    pub crossing_edges: usize,
    pub labeled_crossing: usize,
    pub max_fanin: usize,
    pub max_fanout: usize,
    pub demand: f64,
}

/// 统计 upper→lower（及反向）直接邻层边，计算边带需求。
pub fn edge_band_demand(
    upper_ids: &[String],
    lower_ids: &[String],
    relations: &[Relation],
    parallel_gap: f64,
    profile: EdgeBandDemandProfile,
) -> EdgeBandDemandBreakdown {
    let upper: HashSet<&str> = upper_ids.iter().map(|s| s.as_str()).collect();
    let lower: HashSet<&str> = lower_ids.iter().map(|s| s.as_str()).collect();

    let mut crossing = 0usize;
    let mut labeled = 0usize;
    let mut fanin: HashMap<String, usize> = HashMap::new();
    let mut fanout: HashMap<String, usize> = HashMap::new();

    for rel in relations {
        let from = rel.from.as_str();
        let to = rel.to.as_str();
        let forward = upper.contains(from) && lower.contains(to);
        let backward = lower.contains(from) && upper.contains(to);
        if !forward && !backward {
            continue;
        }
        crossing += 1;
        if rel.label.is_some() || rel.head_label.is_some() || rel.tail_label.is_some() {
            labeled += 1;
        }
        if forward {
            *fanout.entry(from.to_string()).or_insert(0) += 1;
            *fanin.entry(to.to_string()).or_insert(0) += 1;
        } else {
            *fanout.entry(from.to_string()).or_insert(0) += 1;
            *fanin.entry(to.to_string()).or_insert(0) += 1;
        }
    }

    let max_fanin = fanin.values().copied().max().unwrap_or(0);
    let max_fanout = fanout.values().copied().max().unwrap_or(0);
    let comb = max_fanin.max(max_fanout);

    let demand = (crossing as f64) * parallel_gap * profile.parallel_scale
        + (comb as f64) * parallel_gap * profile.fanin_scale
        + (labeled as f64) * profile.label_per_edge
        + profile.label_band;

    EdgeBandDemandBreakdown {
        crossing_edges: crossing,
        labeled_crossing: labeled,
        max_fanin,
        max_fanout,
        demand,
    }
}

/// 对每条邻层缝：`gap = max(base_gap, min(demand, base_gap + max_extra))`。
pub fn layer_gaps_from_demand(
    layers: &[Vec<String>],
    relations: &[Relation],
    base_gap: f64,
    parallel_gap: f64,
    profile: EdgeBandDemandProfile,
) -> Vec<f64> {
    if layers.len() < 2 {
        return Vec::new();
    }
    let mut gaps = Vec::with_capacity(layers.len() - 1);
    for i in 0..layers.len() - 1 {
        let bd = edge_band_demand(&layers[i], &layers[i + 1], relations, parallel_gap, profile);
        let capped = bd.demand.min(base_gap + profile.max_extra);
        gaps.push(base_gap.max(capped));
    }
    gaps
}

/// 将 demand 折成「相对 base 的额外量」（供已有 adaptive_extra 取 max）。
pub fn demand_extra_over_base(
    upper_ids: &[String],
    lower_ids: &[String],
    relations: &[Relation],
    base_gap: f64,
    parallel_gap: f64,
    profile: EdgeBandDemandProfile,
) -> f64 {
    let bd = edge_band_demand(upper_ids, lower_ids, relations, parallel_gap, profile);
    let target = bd.demand.min(base_gap + profile.max_extra);
    (target - base_gap).max(0.0)
}

/// 同排相邻节点水平缝：按两端跨层边/标签 demand 加宽（需求低则 ≈ base）。
pub fn adjacent_rank_gap(
    left: &str,
    right: &str,
    layer_ids: &HashSet<&str>,
    relations: &[Relation],
    base_gap: f64,
    parallel_gap: f64,
    profile: EdgeBandDemandProfile,
) -> f64 {
    if profile.horizontal_max_extra <= 0.0 {
        return base_gap;
    }
    let (c_l, l_l) = node_cross_layer_stats(left, layer_ids, relations);
    let (c_r, l_r) = node_cross_layer_stats(right, layer_ids, relations);
    let demand = ((c_l + c_r) as f64) * parallel_gap * profile.horizontal_parallel_scale
        + ((l_l + l_r) as f64) * profile.horizontal_label_per;
    base_gap + demand.min(profile.horizontal_max_extra)
}

fn node_cross_layer_stats(
    node: &str,
    layer_ids: &HashSet<&str>,
    relations: &[Relation],
) -> (usize, usize) {
    let mut crossing = 0usize;
    let mut labeled = 0usize;
    for rel in relations {
        let from = rel.from.as_str();
        let to = rel.to.as_str();
        let other = if from == node {
            to
        } else if to == node {
            from
        } else {
            continue;
        };
        if layer_ids.contains(other) {
            continue;
        }
        crossing += 1;
        if rel.label.is_some() || rel.head_label.is_some() || rel.tail_label.is_some() {
            labeled += 1;
        }
    }
    (crossing, labeled)
}

/// S4：监控枢纽侧通道水平 gutter（TB 布局左右外环占位）。
///
/// 谓词与 `feedback_side::monitor_hub_edge_indices` 一致：同目标被动入边 ≥ 3。
/// 均衡后约一半车道落在一侧，按该车道数计费。
pub fn side_channel_gutter(
    relations: &[Relation],
    parallel_gap: f64,
    profile: EdgeBandDemandProfile,
) -> f64 {
    if profile.side_channel_max <= 0.0 {
        return 0.0;
    }
    let mut inbound: HashMap<&str, usize> = HashMap::new();
    for rel in relations {
        if rel.arrow != ArrowType::Passive {
            continue;
        }
        if rel.from.as_str() == rel.to.as_str() {
            continue;
        }
        *inbound.entry(rel.to.as_str()).or_insert(0) += 1;
    }
    let max_hub = inbound.values().copied().max().unwrap_or(0);
    if max_hub < 3 {
        return 0.0;
    }
    // 均衡后单侧车道 ≈ ceil(n/2)，再与 MAX_SAME_SIDE_FEEDBACK(=3) 对齐
    let lanes_per_side = ((max_hub + 1) / 2).min(3);
    let demand = (lanes_per_side as f64) * parallel_gap * profile.side_channel_scale
        + profile.side_channel_base;
    demand.min(profile.side_channel_max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};

    fn rel(from: &str, to: &str, label: Option<&str>) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: label.map(|s| s.to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn fanin_comb_raises_demand_deterministically() {
        let upper = vec!["a".into(), "b".into(), "c".into()];
        let lower = vec!["pg".into(), "redis".into()];
        let relations = vec![
            rel("a", "pg", Some("r")),
            rel("b", "pg", Some("r")),
            rel("c", "pg", Some("r")),
            rel("a", "redis", None),
            rel("b", "redis", None),
        ];
        // 有组语义：与历史系数一致
        let profile = EdgeBandDemandProfile::for_diagram(DiagramType::Architecture, true);
        let d1 = edge_band_demand(&upper, &lower, &relations, 12.0, profile);
        let d2 = edge_band_demand(&upper, &lower, &relations, 12.0, profile);
        assert_eq!(d1, d2);
        assert_eq!(d1.crossing_edges, 5);
        assert_eq!(d1.max_fanin, 3);
        assert_eq!(d1.max_fanout, 2);
        // 5*12*0.55 + 3*12*0.35 + 3*4 + 32 = 33 + 12.6 + 12 + 32 = 89.6
        assert!((d1.demand - 89.6).abs() < 1e-9);
    }

    #[test]
    fn ungrouped_raises_demand_but_sparse_stays_at_base() {
        let layers = vec![vec!["a".into()], vec!["b".into()]];
        let relations = vec![rel("a", "b", None)];
        let profile = EdgeBandDemandProfile::for_diagram(DiagramType::Architecture, false);
        let gaps = layer_gaps_from_demand(&layers, &relations, 72.0, 12.0, profile);
        // demand = 1*12*0.65 + 40 = 47.8 < 72 → 仍 base
        assert!((gaps[0] - 72.0).abs() < 1e-9);
    }

    #[test]
    fn adjacent_rank_gap_scales_with_cross_layer_labels() {
        let layer: HashSet<&str> = ["order", "user"].into_iter().collect();
        let relations = vec![
            rel("order", "pg", Some("rw")),
            rel("order", "kafka", Some("ev")),
            rel("user", "pg", Some("rw")),
            rel("user", "kafka", Some("ev")),
        ];
        let profile = EdgeBandDemandProfile::for_diagram(DiagramType::Architecture, false);
        let g = adjacent_rank_gap(
            "order",
            "user",
            &layer,
            &relations,
            32.0,
            12.0,
            profile,
        );
        // c=2+2, l=2+2 → 4*12*0.45 + 4*10 = 21.6+40 = 61.6 → cap 56 → 32+56
        assert!((g - 88.0).abs() < 1e-9);
        let grouped = EdgeBandDemandProfile::for_diagram(DiagramType::Architecture, true);
        assert_eq!(
            adjacent_rank_gap("order", "user", &layer, &relations, 32.0, 12.0, grouped),
            32.0
        );
    }

    #[test]
    fn layer_gaps_respect_base_and_cap() {
        let layers = vec![
            vec!["a".into()],
            vec!["b".into(), "c".into()],
        ];
        let relations = vec![rel("a", "b", None), rel("a", "c", None)];
        let profile = EdgeBandDemandProfile {
            parallel_scale: 10.0,
            fanin_scale: 0.0,
            label_band: 0.0,
            label_per_edge: 0.0,
            max_extra: 20.0,
            side_channel_scale: 0.0,
            side_channel_base: 0.0,
            side_channel_max: 0.0,
            horizontal_parallel_scale: 0.0,
            horizontal_label_per: 0.0,
            horizontal_max_extra: 0.0,
        };
        let gaps = layer_gaps_from_demand(&layers, &relations, 72.0, 12.0, profile);
        assert_eq!(gaps.len(), 1);
        // raw demand = 2*12*10 = 240 → capped to 72+20 = 92
        assert!((gaps[0] - 92.0).abs() < 1e-9);
    }

    #[test]
    fn side_channel_gutter_for_passive_hub() {
        let relations = vec![
            Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("hub"),
                arrow: ArrowType::Passive,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: Span::dummy(),
            },
            Relation {
                from: Identifier::new_unchecked("b"),
                to: Identifier::new_unchecked("hub"),
                arrow: ArrowType::Passive,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: Span::dummy(),
            },
            Relation {
                from: Identifier::new_unchecked("c"),
                to: Identifier::new_unchecked("hub"),
                arrow: ArrowType::Passive,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: Span::dummy(),
            },
        ];
        let profile = EdgeBandDemandProfile::for_diagram_type(DiagramType::Architecture);
        // 有组默认：侧通道仍开小系数；3 被动入边 → lanes=2
        // 2*12*0.4 + 12 = 21.6，cap 20 → 20
        let g = side_channel_gutter(&relations, 12.0, profile);
        assert!((g - 20.0).abs() < 1e-9);
        let mut with_gutter = profile;
        with_gutter.side_channel_scale = 0.75;
        with_gutter.side_channel_base = 20.0;
        with_gutter.side_channel_max = 40.0;
        let g2 = side_channel_gutter(&relations, 12.0, with_gutter);
        // lanes=2 → 2*12*0.75 + 20 = 38, capped at 40
        assert!((g2 - 38.0).abs() < 1e-9);
        let flowchart = EdgeBandDemandProfile::for_diagram_type(DiagramType::Flowchart);
        assert_eq!(side_channel_gutter(&relations, 12.0, flowchart), 0.0);
    }
}
