use crate::ast::Diagram;
use crate::error::FixAction;
use crate::layout::{GroupLayoutWarningKind, LayoutResult};
use crate::types::DiagramType;
use serde_json::json;

use super::{AdviceConfidence, LayoutKnob, LintAdvice, LayoutViolation, LintRuleId};

const DEFAULT_MARGIN: f64 = 8.0;

pub fn generate_lint_advices(
    diagram: &Diagram,
    result: &LayoutResult,
    violations: &[LayoutViolation],
) -> Vec<LintAdvice> {
    let mut advices = Vec::new();
    for (index, violation) in violations.iter().enumerate() {
        advices.extend(generate_for_violation(index, diagram, result, violation));
    }
    advices.sort_by(|a, b| {
        a.violation_index
            .cmp(&b.violation_index)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| a.text.cmp(&b.text))
    });
    advices
}

fn generate_for_violation(
    violation_index: usize,
    diagram: &Diagram,
    result: &LayoutResult,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    match violation.rule {
        LintRuleId::GroupOverlap => group_overlap_advices(violation_index, diagram, result, violation),
        LintRuleId::NodeOutsideGroup => node_outside_group_advices(violation_index, violation),
        LintRuleId::ChildGroupOutsideParent => {
            child_group_outside_parent_advices(violation_index, violation)
        }
        LintRuleId::SiblingWidthRatio => sibling_width_ratio_advices(violation_index, diagram, result, violation),
        LintRuleId::EdgeOnGroupBorder => edge_on_group_border_advices(violation_index, violation),
        LintRuleId::EdgeCrossesGroupInterior => {
            edge_crosses_group_interior_advices(violation_index, violation)
        }
        LintRuleId::NodeOverlap => node_overlap_advices(violation_index, violation),
        LintRuleId::EdgeThroughNode => edge_through_node_advices(violation_index, violation),
        LintRuleId::EdgeCrossing => edge_crossing_advices(violation_index, violation),
        LintRuleId::LabelNodeOverlap | LintRuleId::LabelLabelOverlap => {
            label_overlap_advices(violation_index, violation)
        }
        LintRuleId::UnrelatedEdgeTrunkMerge => unrelated_trunk_merge_advices(violation_index, violation),
    }
}

fn group_overlap_advices(
    violation_index: usize,
    diagram: &Diagram,
    result: &LayoutResult,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    let delta = suggested_gap_delta(violation.metric);
    let group_a = violation.group_ids.first().map(String::as_str).unwrap_or("group_a");
    let group_b = violation.group_ids.get(1).map(String::as_str).unwrap_or("group_b");
    let has_group_warning = result.hints.group_layout_warnings.iter().any(|warning| {
        warning.kind == GroupLayoutWarningKind::GroupOverlap
            && ((warning.group_id == group_a && warning.other_id == group_b)
                || (warning.group_id == group_b && warning.other_id == group_a))
    });
    let overlap_note = if has_group_warning {
        "当前 hints 也检测到了同一对 group 的重叠。"
    } else {
        "这是几何重叠型硬错误。"
    };

    let mut advices = vec![LintAdvice {
        violation_index,
        text: format!(
            "优先增大 diagram 级 `group_frame.gap`，建议至少增加 {delta:.0}px，让 '{group_a}' 与 '{group_b}' 拉开。{overlap_note}"
        ),
        priority: 1,
        confidence: AdviceConfidence::High,
        knobs: vec![LayoutKnob::GroupFrame {
            field: "gap".to_string(),
            suggested: format!("{delta:.0}"),
            rationale: "group overlap 最常见的首选修复是扩大 group 走廊。".to_string(),
        }],
        fix: Some(FixAction {
            action: "set_group_frame_field".to_string(),
            payload: json!({
                "field": "gap",
                "suggested": delta.ceil() as i64,
                "mode": "increase"
            }),
        }),
    }];

    if diagram.diagram_type == DiagramType::Architecture {
        let already_equalized = result
            .hints
            .group_frame_report
            .as_ref()
            .map(|report| report.equalized)
            .unwrap_or(false);
        if !already_equalized {
            advices.push(LintAdvice {
                violation_index,
                text: "若当前未显式统一条带，architecture 图可优先尝试 `group_frame: strips` 或 `group_frame { track: equal }`。".to_string(),
                priority: 2,
                confidence: AdviceConfidence::Medium,
                knobs: vec![LayoutKnob::GroupFrame {
                    field: "preset".to_string(),
                    suggested: "strips".to_string(),
                    rationale: "等宽条带通常能改善架构图 group 的互相挤压。".to_string(),
                }],
                fix: None,
            });
        }
    }

    advices.push(LintAdvice {
        violation_index,
        text: "若增大 gap 后仍重叠，说明问题更像拓扑/分组边界设计，需要考虑拆组或调整跨组关系。".to_string(),
        priority: 3,
        confidence: AdviceConfidence::Low,
        knobs: vec![LayoutKnob::Topology {
            action: "split_group".to_string(),
            targets: violation.group_ids.clone(),
            rationale: "当 group 自身内容过密时，单纯调 gap 往往不够。".to_string(),
        }],
        fix: None,
    });
    advices
}

fn node_outside_group_advices(
    violation_index: usize,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    let suggested = suggested_padding_value(violation.metric);
    let entity_id = violation
        .entity_ids
        .first()
        .map(String::as_str)
        .unwrap_or("entity");
    let group_id = violation
        .group_ids
        .first()
        .map(String::as_str)
        .unwrap_or("group");
    vec![
        LintAdvice {
            violation_index,
            text: format!(
                "节点 '{entity_id}' 超出分组 '{group_id}'，优先增大 `layout: ... {{ group_padding: {suggested} }}`。"
            ),
            priority: 1,
            confidence: AdviceConfidence::High,
            knobs: vec![LayoutKnob::LayoutOption {
                key: "group_padding".to_string(),
                suggested: suggested.to_string(),
                rationale: "containment 违规通常先用 padding 吃掉外溢距离。".to_string(),
            }],
            fix: Some(FixAction {
                action: "set_layout_option".to_string(),
                payload: json!({
                    "key": "group_padding",
                    "suggested": suggested
                }),
            }),
        },
        LintAdvice {
            violation_index,
            text: format!(
                "如果仅加 padding 仍越界，再把 group '{group_id}' 的 `layout` 改为更宽松的 `vertical` / `grid` / `fan-out`。"
            ),
            priority: 2,
            confidence: AdviceConfidence::Medium,
            knobs: vec![LayoutKnob::GroupLayout {
                group_id: group_id.to_string(),
                suggested: "grid".to_string(),
                rationale: "组内排列过紧时，padding 只能缓解边界，不解决内容堆叠。".to_string(),
            }],
            fix: None,
        },
    ]
}

fn child_group_outside_parent_advices(
    violation_index: usize,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    let suggested = suggested_padding_value(violation.metric);
    let child_group = violation
        .entity_ids
        .first()
        .map(String::as_str)
        .unwrap_or("child_group");
    let parent_group = violation
        .group_ids
        .first()
        .map(String::as_str)
        .unwrap_or("parent_group");
    vec![
        LintAdvice {
            violation_index,
            text: format!(
                "子分组 '{child_group}' 超出父分组 '{parent_group}'，先检查父层 `group_padding` 是否足够，建议至少提到 {suggested}px。"
            ),
            priority: 1,
            confidence: AdviceConfidence::Medium,
            knobs: vec![LayoutKnob::LayoutOption {
                key: "group_padding".to_string(),
                suggested: suggested.to_string(),
                rationale: "父组 padding 不足会直接导致子组框被裁切。".to_string(),
            }],
            fix: None,
        },
        LintAdvice {
            violation_index,
            text: format!(
                "若父层 padding 正常，再把子分组 '{child_group}' 的 `layout` 改成更矮或更规整的模式，如 `grid` / `vertical`。"
            ),
            priority: 2,
            confidence: AdviceConfidence::Medium,
            knobs: vec![LayoutKnob::GroupLayout {
                group_id: child_group.to_string(),
                suggested: "grid".to_string(),
                rationale: "子组内部内容过高时，父组扩边距不是唯一解。".to_string(),
            }],
            fix: None,
        },
    ]
}

fn sibling_width_ratio_advices(
    violation_index: usize,
    diagram: &Diagram,
    result: &LayoutResult,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    let ratio = violation.metric.unwrap_or_default();
    let wider = violation.group_ids.first().map(String::as_str).unwrap_or("wide_group");
    let narrower = violation
        .group_ids
        .get(1)
        .map(String::as_str)
        .unwrap_or("narrow_group");

    if diagram.diagram_type == DiagramType::Architecture {
        let already_equalized = result
            .hints
            .group_frame_report
            .as_ref()
            .map(|report| report.equalized)
            .unwrap_or(false);
        if !already_equalized {
            return vec![LintAdvice {
                violation_index,
                text: format!(
                    "同级条带宽比 {ratio:.3} 偏大，architecture 图优先改 diagram 级 `group_frame: strips`，或显式设 `group_frame {{ track: equal, gap: 40 }}`。"
                ),
                priority: 1,
                confidence: AdviceConfidence::High,
                knobs: vec![LayoutKnob::GroupFrame {
                    field: "preset".to_string(),
                    suggested: "strips".to_string(),
                    rationale: "strips / track: equal 是架构图宽比失衡的首选调节入口。".to_string(),
                }],
                fix: Some(FixAction {
                    action: "set_group_frame_preset".to_string(),
                    payload: json!({
                        "preset": "strips"
                    }),
                }),
            }];
        }
    }

    vec![
        LintAdvice {
            violation_index,
            text: format!(
                "当前 group frame 已做等宽整形，但 '{wider}' 与 '{narrower}' 的视觉宽比仍偏大；若想接受内容驱动宽度，可改 `group_frame.track: fit`。"
            ),
            priority: 1,
            confidence: AdviceConfidence::Medium,
            knobs: vec![LayoutKnob::GroupFrame {
                field: "track".to_string(),
                suggested: "fit".to_string(),
                rationale: "fit 允许 sibling 保留内容驱动宽度，避免硬追求等宽。".to_string(),
            }],
            fix: None,
        },
        LintAdvice {
            violation_index,
            text: format!(
                "若仍想保持对称感，就调整更宽 group 的组内 `layout`，减少单组内容横向膨胀。"
            ),
            priority: 2,
            confidence: AdviceConfidence::Medium,
            knobs: vec![LayoutKnob::GroupLayout {
                group_id: wider.to_string(),
                suggested: "grid".to_string(),
                rationale: "宽度失衡往往来自组内布局把节点一字排开。".to_string(),
            }],
            fix: None,
        },
    ]
}

fn edge_on_group_border_advices(
    violation_index: usize,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    let group_id = violation.group_ids.first().map(String::as_str).unwrap_or("group");
    vec![LintAdvice {
        violation_index,
        text: format!(
            "边贴着分组 '{group_id}' 边框通常是正交走廊的预期现象，建议优先忽略，不要先去调大 gap。"
        ),
        priority: 1,
        confidence: AdviceConfidence::High,
        knobs: vec![LayoutKnob::IgnoreRule {
            rule: violation.rule.as_str().to_string(),
            rationale: "这是常见的布局噪音，而非必须自动修复的问题。".to_string(),
        }],
        fix: None,
    }]
}

fn edge_crosses_group_interior_advices(
    violation_index: usize,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    let group_id = violation.group_ids.first().map(String::as_str).unwrap_or("group");
    vec![
        LintAdvice {
            violation_index,
            text: format!(
                "边穿过无关分组 '{group_id}' 内部时，先尝试增大相关 group 间距，给正交路由留出走廊。"
            ),
            priority: 1,
            confidence: AdviceConfidence::Medium,
            knobs: vec![LayoutKnob::GroupFrame {
                field: "gap".to_string(),
                suggested: "40".to_string(),
                rationale: "更多 group 走廊通常能减少边钻入无关容器。".to_string(),
            }],
            fix: None,
        },
        LintAdvice {
            violation_index,
            text: "如果仍穿组，问题更可能在拓扑或分层，检查关系声明是否迫使边跨越无关容器。".to_string(),
            priority: 2,
            confidence: AdviceConfidence::Low,
            knobs: vec![LayoutKnob::Topology {
                action: "check_relation_topology".to_string(),
                targets: violation.group_ids.clone(),
                rationale: "路由只是结果，跨组声明方式常是根因。".to_string(),
            }],
            fix: None,
        },
    ]
}

fn node_overlap_advices(violation_index: usize, violation: &LayoutViolation) -> Vec<LintAdvice> {
    vec![
        LintAdvice {
            violation_index,
            text: "节点重叠时，优先增加布局的整体留白（如 `padding` / `group_padding`），再看是否需要拆分密集分组。".to_string(),
            priority: 1,
            confidence: AdviceConfidence::Medium,
            knobs: vec![LayoutKnob::LayoutOption {
                key: "padding".to_string(),
                suggested: "48".to_string(),
                rationale: "全局留白不足会放大节点互相挤压。".to_string(),
            }],
            fix: None,
        },
        LintAdvice {
            violation_index,
            text: "若重叠只发生在单个 group 内，再调整该组的 `layout`，不要一开始就手改坐标。".to_string(),
            priority: 2,
            confidence: AdviceConfidence::Low,
            knobs: vec![LayoutKnob::Topology {
                action: "review_dense_group".to_string(),
                targets: violation.group_ids.clone(),
                rationale: "Group 内部过密更适合改布局模式而非坐标。".to_string(),
            }],
            fix: None,
        },
    ]
}

fn edge_through_node_advices(
    violation_index: usize,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    vec![LintAdvice {
        violation_index,
        text: "边穿节点通常更像路由/拓扑问题，不建议先改 group_frame；先检查是否能调整分层、端点所属 group 或边声明路径。".to_string(),
        priority: 1,
        confidence: AdviceConfidence::Low,
        knobs: vec![LayoutKnob::Topology {
            action: "review_edge_topology".to_string(),
            targets: violation.entity_ids.clone(),
            rationale: "此类问题很少由 group gap 单独解决。".to_string(),
        }],
        fix: None,
    }]
}

fn edge_crossing_advices(
    violation_index: usize,
    _violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    vec![LintAdvice {
        violation_index,
        text: "边交叉默认是 warning，可接受；若要继续优化，再减少跨层关系或增加分层约束。".to_string(),
        priority: 1,
        confidence: AdviceConfidence::Low,
        knobs: vec![LayoutKnob::Topology {
            action: "reduce_cross_layer_edges".to_string(),
            targets: Vec::new(),
            rationale: "crossing 更接近拓扑质量而非硬几何错误。".to_string(),
        }],
        fix: None,
    }]
}

fn label_overlap_advices(
    violation_index: usize,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    let target = violation
        .entity_ids
        .first()
        .cloned()
        .unwrap_or_else(|| "label".to_string());
    vec![LintAdvice {
        violation_index,
        text: "标签重叠时，优先缩短标签文案；如果文案不能改，再间接调整布局或路由。".to_string(),
        priority: 1,
        confidence: AdviceConfidence::Medium,
        knobs: vec![LayoutKnob::Label {
            edge_or_entity: target,
            action: "shorten".to_string(),
            rationale: "label 碰撞通常最直接的修复是缩短文本。".to_string(),
        }],
        fix: None,
    }]
}

fn unrelated_trunk_merge_advices(
    violation_index: usize,
    violation: &LayoutViolation,
) -> Vec<LintAdvice> {
    vec![LintAdvice {
        violation_index,
        text: "非语义 trunk merge 更像路由语义问题，不要优先靠 `group_frame` 或 `gap` 盲调；先检查是否应拆分关系语义。".to_string(),
        priority: 1,
        confidence: AdviceConfidence::High,
        knobs: vec![LayoutKnob::Topology {
            action: "avoid_group_frame_tuning".to_string(),
            targets: violation.related_edge_indices.iter().map(|idx| idx.to_string()).collect(),
            rationale: "这类问题的根因通常不是 group 几何，而是边合并语义。".to_string(),
        }],
        fix: None,
    }]
}

fn suggested_padding_value(metric: Option<f64>) -> i64 {
    let suggested = metric.unwrap_or(24.0) + DEFAULT_MARGIN;
    suggested.ceil().max(24.0) as i64
}

fn suggested_gap_delta(metric: Option<f64>) -> f64 {
    let overlap = metric.unwrap_or(0.0);
    overlap.sqrt().mul_add(0.5, 24.0).clamp(24.0, 96.0)
}
