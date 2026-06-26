//! 路由友好性评估器（V1 诊断模式）
//!
//! 在节点布局完成后、边路由前，基于拓扑与布局位置快速预测当前布局对边路由是否友好。
//! 输出 `FriendlinessReport`（复合分数 + 热点定位 + 调整建议），写入 `LayoutHints.friendliness_report`。
//!
//! 五维友好度度量（设计文档 §4.2）：
//! 1. 正交通道占用度（Phase 1.5 替换 RUDY）— `congestion.rs`
//! 2. 长边跨层度（Sugiyama rank）— `long_edge.rs`
//! 3. group 间距充裕度 — `group_gap.rs`
//! 4. 穿障预测度 — `crossing_predict.rs`
//! 5. 端口冲突度 — `port_conflict.rs`
//!
//! Phase 1.5 改进：
//! - `channel_congestion` 由 RUDY 直线密度改为正交通道占用度（RUDY r=0.01 脱钩）
//! - 权重按布局族（层次 / 力导向 / 放射）分组校准
//! - 复合分数目标 Pearson > 0.6（见 `benchmarks/phase1_5_correlation.md`）
//!
//! Phase 2（V2 反馈模式）：
//! - `adjuster` 模块在 V1 评估后对穿障热点做局部节点位移，减少预测穿障

pub mod adjuster;
pub mod congestion;
pub mod crossing_predict;
pub mod group_gap;
pub mod long_edge;
pub mod port_conflict;

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::LayoutResult;
use serde::Serialize;

/// 路由友好性评估报告
#[derive(Debug, Clone, Serialize)]
pub struct FriendlinessReport {
    /// 复合友好度分数（越低越友好，0 = 完美）
    pub score: f64,
    /// 五维子分数
    pub congestion_score: f64,
    pub long_edge_score: usize,
    pub gap_adequacy_score: f64,
    pub predicted_crossings: usize,
    pub port_conflict_score: f64,
    /// 热点区域（分数高的局部区域）
    pub hotspots: Vec<Hotspot>,
}

/// 热点类型
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum HotspotKind {
    ChannelCongestion,
    LongEdgeSpan,
    GroupGapInsufficient,
    PredictedCrossing,
    PortConflict,
}

/// 热点区域
#[derive(Debug, Clone, Serialize)]
pub struct Hotspot {
    /// 热点类型
    pub kind: HotspotKind,
    /// 热点区域包围框 (left, top, right, bottom)；无明确位置时为全图
    pub bbox: (f64, f64, f64, f64),
    /// 局部严重度（0..1）
    pub severity: f64,
    /// 相关边索引
    pub edge_indices: Vec<usize>,
    /// 相关节点 / group ID
    pub element_ids: Vec<String>,
}

/// 布局族（Phase 1.5 分组校准用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutFamily {
    /// 层次类：sugiyama / sugiyama-v2 / flowchart / er / state（有 rank，长边度量有效）
    Hierarchical,
    /// 力导向类：force-directed
    ForceDirected,
    /// 放射/分组类：circular / architecture / mindmap
    Radial,
}

/// 根据布局算法名推断布局族
pub fn family_of(layout_name: &str) -> LayoutFamily {
    match layout_name {
        "sugiyama" | "sugiyama-v2" | "flowchart" | "er" | "state" => LayoutFamily::Hierarchical,
        "force-directed" => LayoutFamily::ForceDirected,
        "circular" | "architecture" | "mindmap" => LayoutFamily::Radial,
        _ => LayoutFamily::Hierarchical, // 默认按层次处理
    }
}

/// 五维度量 z-score 校准参数（Phase 1.5 分族校准）
///
/// 每个度量的 (mean, std)，用于将原始值转换为 z-score。
/// 来源：792 样本分族统计（见 `benchmarks/phase1_5_correlation.md` §5b）。
#[derive(Debug, Clone, Copy)]
pub struct CalibrationParams {
    pub congestion: (f64, f64),
    pub long_edge: (f64, f64),
    pub group_gap: (f64, f64),
    pub predicted_crossings: (f64, f64),
    pub port_conflict: (f64, f64),
}

impl CalibrationParams {
    /// z-score 归一化；σ=0 时返回 0（度量在该族内恒定，无区分力）
    fn zscore(&self, value: f64, params: (f64, f64)) -> f64 {
        let (mean, std) = params;
        if std < f64::EPSILON {
            0.0
        } else {
            (value - mean) / std
        }
    }
}

/// 五维度量权重 + 校准参数（Phase 1.5 分族校准结果）
///
/// 来源：`benchmarks/phase1_5_correlation.md` 按布局族分组的 Pearson 相关性归一化。
/// 各族权重不同：层次类长边度量有效；力导向类端口冲突更显著；放射类 group 间距更重要。
///
/// Phase 1.5 归一化策略：z-score（替代软饱和 x/(x+threshold)）。
/// z-score 保留完整动态范围，Pearson vs enc 从 0.41 提升至 0.57。
#[derive(Debug, Clone, Copy)]
pub struct FriendlinessWeights {
    /// 通道拥堵度权重
    pub w_congestion: f64,
    /// 长边跨层度权重
    pub w_long_edge: f64,
    /// group 间距权重
    pub w_group_gap: f64,
    /// 穿障预测权重
    pub w_predicted_crossings: f64,
    /// 端口冲突权重
    pub w_port_conflict: f64,
    /// z-score 校准参数
    pub calibration: CalibrationParams,
}

impl FriendlinessWeights {
    /// 全局默认权重（Phase 1.5 校准后；作为无族信息时的回退）
    ///
    /// 来源：792 样本全局 Pearson vs edge_node_crossings 归一化。
    /// 校准参数使用层次类（占 69% 样本，作为默认族）。
    pub fn global_default() -> Self {
        Self {
            w_congestion: 0.19,
            w_long_edge: 0.13,
            w_group_gap: 0.01,
            w_predicted_crossings: 0.47,
            w_port_conflict: 0.20,
            calibration: CalibrationParams {
                congestion: (10.91, 10.03),
                long_edge: (1.62, 5.56),
                group_gap: (0.17, 2.93),
                predicted_crossings: (5.93, 9.91),
                port_conflict: (148.27, 292.06),
            },
        }
    }

    /// 层次类布局权重（sugiyama / flowchart / er / state）
    ///
    /// 来源：549 样本分族 Pearson 归一化。predicted_crossings 主导（r=0.62），
    /// long_edge 次之（r=0.24），congestion/port 弱（r<0.06），group_gap 负相关故置 0。
    pub fn for_hierarchical() -> Self {
        Self {
            w_congestion: 0.06,
            w_long_edge: 0.26,
            w_group_gap: 0.0,
            w_predicted_crossings: 0.66,
            w_port_conflict: 0.02,
            calibration: CalibrationParams {
                congestion: (10.91, 10.03),
                long_edge: (1.62, 5.56),
                group_gap: (0.17, 2.93),
                predicted_crossings: (5.93, 9.91),
                port_conflict: (148.27, 292.06),
            },
        }
    }

    /// 力导向类布局权重（force-directed）
    ///
    /// 来源：169 样本分族 Pearson 归一化。port_conflict 最强（r=0.69），
    /// predicted_crossings（r=0.61）与 channel_congestion（r=0.60）紧随，
    /// group_gap 弱（r=0.01），long_edge 恒为 0。
    pub fn for_force_directed() -> Self {
        Self {
            w_congestion: 0.31,
            w_long_edge: 0.0,
            w_group_gap: 0.01,
            w_predicted_crossings: 0.32,
            w_port_conflict: 0.36,
            calibration: CalibrationParams {
                congestion: (11.47, 8.39),
                long_edge: (0.0, 0.0), // 力导向无 rank，恒为 0
                group_gap: (1.95, 10.41),
                predicted_crossings: (3.95, 8.30),
                port_conflict: (147.82, 257.14),
            },
        }
    }

    /// 放射/分组类布局权重（circular / architecture / mindmap）
    ///
    /// 来源：74 样本分族 Pearson 归一化。predicted_crossings（r=0.71）与
    /// channel_congestion（r=0.60）最强，port_conflict（r=0.46）与
    /// group_gap（r=0.05）弱，long_edge 恒为 0。
    pub fn for_radial() -> Self {
        Self {
            w_congestion: 0.33,
            w_long_edge: 0.0,
            w_group_gap: 0.03,
            w_predicted_crossings: 0.39,
            w_port_conflict: 0.25,
            calibration: CalibrationParams {
                congestion: (10.70, 7.70),
                long_edge: (0.0, 0.0), // 放射类无 rank
                group_gap: (1.51, 6.66),
                predicted_crossings: (4.08, 5.92),
                port_conflict: (100.78, 132.21),
            },
        }
    }

    /// 按布局族获取权重
    pub fn for_family(family: LayoutFamily) -> Self {
        match family {
            LayoutFamily::Hierarchical => Self::for_hierarchical(),
            LayoutFamily::ForceDirected => Self::for_force_directed(),
            LayoutFamily::Radial => Self::for_radial(),
        }
    }
}

impl Default for FriendlinessWeights {
    fn default() -> Self {
        Self::global_default()
    }
}

/// 路由友好性评估器
#[derive(Debug, Clone)]
pub struct RoutingFriendlinessEvaluator {
    pub weights: FriendlinessWeights,
}

impl Default for RoutingFriendlinessEvaluator {
    fn default() -> Self {
        Self {
            weights: FriendlinessWeights::global_default(),
        }
    }
}

impl RoutingFriendlinessEvaluator {
    /// 按布局算法名构造评估器（Phase 1.5 分族权重）
    pub fn for_layout(layout_name: &str) -> Self {
        Self {
            weights: FriendlinessWeights::for_family(family_of(layout_name)),
        }
    }

    /// 评估布局的路由友好性
    pub fn evaluate(&self, diagram: &Diagram, result: &LayoutResult) -> FriendlinessReport {
        let congestion = congestion::evaluate(diagram, result);
        let long_edge = long_edge::evaluate(diagram, result);
        let group_gap = group_gap::evaluate(diagram, result);
        let crossing = crossing_predict::evaluate(diagram, result);
        let port = port_conflict::evaluate(diagram, result);

        let mut hotspots = Vec::new();

        // 拥堵热点（正交通道占用度：峰值边数 > 8 视为热点）
        if congestion.score > 8.0 {
            if let Some(bbox) = congestion.hotspot_bbox {
                hotspots.push(Hotspot {
                    kind: HotspotKind::ChannelCongestion,
                    bbox,
                    severity: (congestion.score / 20.0).min(1.0),
                    edge_indices: vec![],
                    element_ids: vec![],
                });
            }
        }

        // 长边热点
        if long_edge.count > 0 {
            hotspots.push(Hotspot {
                kind: HotspotKind::LongEdgeSpan,
                bbox: layout_bbox(result),
                severity: (long_edge.count as f64 / 10.0).min(1.0),
                edge_indices: long_edge.edge_indices.clone(),
                element_ids: vec![],
            });
        }

        // group 间距热点
        for hg in &group_gap.insufficient_pairs {
            hotspots.push(Hotspot {
                kind: HotspotKind::GroupGapInsufficient,
                bbox: layout_bbox(result),
                severity: (hg.deficit / 50.0).min(1.0),
                edge_indices: vec![],
                element_ids: vec![hg.group1.clone(), hg.group2.clone()],
            });
        }

        // 穿障预测热点
        if crossing.count > 0 {
            hotspots.push(Hotspot {
                kind: HotspotKind::PredictedCrossing,
                bbox: layout_bbox(result),
                severity: (crossing.count as f64 / 20.0).min(1.0),
                edge_indices: crossing.edge_indices.clone(),
                element_ids: vec![],
            });
        }

        // 端口冲突热点
        for pc in &port.conflict_nodes {
            if let Some(nl) = result.nodes.get(&pc.node_id) {
                hotspots.push(Hotspot {
                    kind: HotspotKind::PortConflict,
                    bbox: (nl.x, nl.y, nl.x + nl.width, nl.y + nl.height),
                    severity: (pc.deficit / 100.0).min(1.0),
                    edge_indices: vec![],
                    element_ids: vec![pc.node_id.clone()],
                });
            }
        }

        // 复合分数：各子分数 z-score 归一化后加权求和
        // Phase 1.5：z-score 替代软饱和 x/(x+threshold)，保留完整动态范围。
        // z-score 可为负（比均值更友好），0 = 该族平均水平。
        let cal = &self.weights.calibration;
        let congestion_z = cal.zscore(congestion.score, cal.congestion);
        let long_edge_z = cal.zscore(long_edge.count as f64, cal.long_edge);
        let group_gap_z = cal.zscore(group_gap.deficit, cal.group_gap);
        let crossing_z = cal.zscore(crossing.count as f64, cal.predicted_crossings);
        let port_z = cal.zscore(port.score, cal.port_conflict);

        let w = &self.weights;
        let score = w.w_congestion * congestion_z
            + w.w_long_edge * long_edge_z
            + w.w_group_gap * group_gap_z
            + w.w_predicted_crossings * crossing_z
            + w.w_port_conflict * port_z;

        FriendlinessReport {
            score,
            congestion_score: congestion.score,
            long_edge_score: long_edge.count,
            gap_adequacy_score: group_gap.deficit,
            predicted_crossings: crossing.count,
            port_conflict_score: port.score,
            hotspots,
        }
    }
}

/// 计算布局包围框
fn layout_bbox(result: &LayoutResult) -> (f64, f64, f64, f64) {
    (0.0, 0.0, result.total_width, result.total_height)
}

/// AABB 预过滤器：快速判断节点是否完全位于线段包围盒外
///
/// 在调用 `segment_intersects_aabb` 之前，用此函数做快速剔除。
/// 仅涉及比较运算（无除法），远快于 slab 方法。
/// 返回 `true` 表示节点 AABB 完全在线段包围盒之外，无需进一步检测。
#[inline]
pub(crate) fn node_outside_segment_bbox(
    p1: Point,
    p2: Point,
    nl: &crate::layout::NodeLayout,
) -> bool {
    let edge_min_x = p1.x.min(p2.x);
    let edge_max_x = p1.x.max(p2.x);
    let edge_min_y = p1.y.min(p2.y);
    let edge_max_y = p1.y.max(p2.y);
    nl.x + nl.width < edge_min_x
        || nl.x > edge_max_x
        || nl.y + nl.height < edge_min_y
        || nl.y > edge_max_y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, Span};
    use crate::layout::{LayoutHints, NodeLayout};
    use crate::types::DiagramType;
    use std::collections::HashMap;

    fn dummy_span() -> Span {
        Span::dummy()
    }

    fn make_diagram_2nodes() -> Diagram {
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: dummy_span(),
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: dummy_span(),
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: dummy_span(),
            }],
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 1,
            },
        }
    }

    fn make_result_2nodes() -> LayoutResult {
        LayoutResult {
            nodes: HashMap::from([
                (
                    "a".to_string(),
                    NodeLayout {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 50.0,
                        ..Default::default()
                    },
                ),
                (
                    "b".to_string(),
                    NodeLayout {
                        x: 200.0,
                        y: 0.0,
                        width: 100.0,
                        height: 50.0,
                        ..Default::default()
                    },
                ),
            ]),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 300.0,
            total_height: 50.0,
            hints: LayoutHints::default(),
        }
    }

    #[test]
    fn test_evaluator_simple_no_crossings() {
        let evaluator = RoutingFriendlinessEvaluator::default();
        let diagram = make_diagram_2nodes();
        let result = make_result_2nodes();
        let report = evaluator.evaluate(&diagram, &result);

        // 两节点水平排列，无边穿障
        assert_eq!(report.predicted_crossings, 0);
        assert_eq!(report.long_edge_score, 0); // 无 rank
        assert_eq!(report.gap_adequacy_score, 0.0); // 无 group
        // 分数应该较低（友好）
        assert!(report.score < 0.5, "score should be low for friendly layout, got {}", report.score);
    }

    #[test]
    fn test_evaluator_detects_crossing() {
        let evaluator = RoutingFriendlinessEvaluator::default();
        let diagram = make_diagram_2nodes();

        // 基线：a、b 水平排列，无穿障
        let result_baseline = make_result_2nodes();
        let report_baseline = evaluator.evaluate(&diagram, &result_baseline);

        // 在 a 和 b 之间插入节点 c，使 a→b 直线穿过 c
        let mut result = make_result_2nodes();
        result.nodes.insert(
            "c".to_string(),
            NodeLayout {
                x: 100.0,
                y: 10.0,
                width: 80.0,
                height: 30.0,
                ..Default::default()
            },
        );

        let report = evaluator.evaluate(&diagram, &result);
        assert!(
            report.predicted_crossings > 0,
            "should detect a→b line crossing through c"
        );
        // z-score 语义下分数可能为负（比族均值更友好），
        // 改为相对比较：有穿障的分数应高于无穿障的基线。
        assert!(
            report.score > report_baseline.score,
            "crossing layout should score higher (less friendly) than baseline; \
             baseline={}, crossing={}",
            report_baseline.score,
            report.score
        );
    }

    #[test]
    fn test_congestion_empty() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![],
            relations: vec![],
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 0,
            },
        };
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups: HashMap::new(),
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints::default(),
        };
        let r = congestion::evaluate(&diagram, &result);
        assert_eq!(r.score, 0.0);
    }
}
