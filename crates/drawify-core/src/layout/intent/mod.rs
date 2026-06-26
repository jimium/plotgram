//! Layout Intent 叠加层。
//!
//! 本模块定义"布局意图"的数据结构与报告类型。意图是布局算法之上的
//! 可选修正层，允许调用方在不修改 DSL 的前提下对自动布局做局部调整。
//!
//! 意图分为两类：
//! - [`TopologyIntent`]：影响分层/排序，由布局算法在 `compute_with_overlay`
//!   内部原生消费（如 `below` / `above` 注入 rank 约束边）。
//! - [`GeometricIntent`]：布局后修正坐标，由 `apply_geometric_refinement`
//!   在 grid snap 之前消费（如 `pin` 锁定轴坐标、`align_*` 对齐）。
//!
//! 详见 `docs/architecture/layout-intent-optimized.md`（v2.1）。

use serde::{Deserialize, Serialize};

pub mod topology;
pub mod geometric;

/// 节点固定集合：记录被 `Pin` / `Align*` 意图保护的节点，供 grid snap 跳过。
///
/// - `full`：`PinAxis::Both` 锁定的节点（x、y 都跳过 snap）
/// - `x_only`：`PinAxis::X` 锁定的节点（仅 x 跳过 snap）
/// - `y_only`：`PinAxis::Y` 锁定的节点（仅 y 跳过 snap）
/// - `aligned_vertical`：被 `AlignVertical` 对齐的节点（x 中心对齐，两轴都跳过 snap）
/// - `aligned_horizontal`：被 `AlignHorizontal` 对齐的节点（y 中心对齐，两轴都跳过 snap）
#[derive(Debug, Clone, Default)]
pub struct PinSet {
    pub full: std::collections::HashSet<String>,
    pub x_only: std::collections::HashSet<String>,
    pub y_only: std::collections::HashSet<String>,
    /// `AlignVertical` 对齐的节点（x 中心对齐）。
    pub aligned_vertical: std::collections::HashSet<String>,
    /// `AlignHorizontal` 对齐的节点（y 中心对齐）。
    pub aligned_horizontal: std::collections::HashSet<String>,
}

impl PinSet {
    /// 节点的 x 轴是否被锁定（跳过 rank 轴为 x 的 snap，或 layer 轴为 x 的 snap）。
    pub fn is_x_pinned(&self, id: &str) -> bool {
        self.full.contains(id)
            || self.x_only.contains(id)
            || self.aligned_vertical.contains(id)
            || self.aligned_horizontal.contains(id)
    }

    /// 节点的 y 轴是否被锁定。
    pub fn is_y_pinned(&self, id: &str) -> bool {
        self.full.contains(id)
            || self.y_only.contains(id)
            || self.aligned_vertical.contains(id)
            || self.aligned_horizontal.contains(id)
    }

    /// 在 `horizontal` 布局下，rank 轴（x）是否被锁定。
    pub fn is_rank_pinned(&self, id: &str, horizontal: bool) -> bool {
        if horizontal {
            self.is_x_pinned(id)
        } else {
            self.is_y_pinned(id)
        }
    }

    /// 在 `horizontal` 布局下，layer 轴（y）是否被锁定。
    pub fn is_layer_pinned(&self, id: &str, horizontal: bool) -> bool {
        if horizontal {
            self.is_y_pinned(id)
        } else {
            self.is_x_pinned(id)
        }
    }

    /// 是否有任何节点被锁定。
    pub fn is_empty(&self) -> bool {
        self.full.is_empty()
            && self.x_only.is_empty()
            && self.y_only.is_empty()
            && self.aligned_vertical.is_empty()
            && self.aligned_horizontal.is_empty()
    }
}

/// 拓扑意图：影响分层/排序，在 `strategy.compute_with_overlay` 内部消费。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TopologyIntent {
    /// `from` 应在 `to` 的下游（rank 更大），注入约束边 `from → to`。
    Below { from: String, to: String },
    /// `from` 应在 `to` 的上游（rank 更小），等价于 `Below { from: to, to: from }`。
    Above { from: String, to: String },
}

/// 几何意图：布局后修正坐标，在 `apply_geometric_refinement` 中消费。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeometricIntent {
    /// 固定节点当前坐标（跳过 grid snap 与后续几何调整）。
    ///
    /// 仅轴约束，不含绝对坐标（符合需求稿 §7 非目标）。
    Pin { node: String, axis: PinAxis },
    /// 多节点垂直对齐（x 中心一致）。
    AlignVertical { nodes: Vec<String> },
    /// 多节点水平对齐（y 中心一致）。
    AlignHorizontal { nodes: Vec<String> },
}

/// `Pin` 约束的轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinAxis {
    X,
    Y,
    Both,
}

/// 意图叠加层。
///
/// 作为独立参数透传至 `compute_layout_with_plan_and_overlay`，
/// 不变异 `Diagram`，保持 `relations[i] ↔ edges[i]` 索引契约。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutIntentOverlay {
    #[serde(default)]
    pub topology: Vec<TopologyIntent>,
    #[serde(default)]
    pub geometric: Vec<GeometricIntent>,
}

/// 单条意图的满足状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    /// 完全满足。
    Satisfied,
    /// 部分满足（如跨组意图仅组内生效、对齐后仍有重叠）。
    Partial,
    /// 与其他意图或图结构冲突，已跳过。
    Conflicted,
    /// 意图引用的节点不存在。
    NotFound,
}

/// 单条意图的执行结果。
#[derive(Debug, Clone, Serialize)]
pub struct IntentResult {
    /// 在 `LayoutIntentOverlay.topology` / `geometric` 中的索引（按声明顺序）。
    pub index: usize,
    /// 意图类型标签（`"below"` / `"above"` / `"pin"` / `"align_vertical"` / `"align_horizontal"`）。
    pub kind: &'static str,
    /// 满足状态。
    pub status: IntentStatus,
    /// 可选的说明信息（如冲突原因、部分满足的降级说明）。
    pub message: Option<String>,
}

/// 意图修正报告。
///
/// 由 `compute_layout_with_plan_and_overlay` 返回，汇总每条意图的执行状态。
/// WASM 直接写入 `RenderResult.refinement_report`；Server 序列化为
/// `X-Drawify-Refinement-Report` 响应头。
#[derive(Debug, Clone, Default, Serialize)]
pub struct RefinementReport {
    pub results: Vec<IntentResult>,
    pub satisfied: usize,
    pub partial: usize,
    pub conflicted: usize,
    pub not_found: usize,
}

impl RefinementReport {
    /// 追加一条结果并自动累加计数。
    pub fn push(&mut self, index: usize, kind: &'static str, status: IntentStatus, message: Option<String>) {
        match status {
            IntentStatus::Satisfied => self.satisfied += 1,
            IntentStatus::Partial => self.partial += 1,
            IntentStatus::Conflicted => self.conflicted += 1,
            IntentStatus::NotFound => self.not_found += 1,
        }
        self.results.push(IntentResult { index, kind, status, message });
    }

    /// 合并另一份报告（计数与结果列表都并入 `self`）。
    pub fn merge(&mut self, other: RefinementReport) {
        self.satisfied += other.satisfied;
        self.partial += other.partial;
        self.conflicted += other.conflicted;
        self.not_found += other.not_found;
        self.results.extend(other.results);
    }

    /// 是否为空报告（无任何意图结果）。
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    /// 将指定 index 的结果从 `Satisfied` 降级为 `Partial`，并附带说明信息。
    ///
    /// 用于穿障修正（`refine::run_refine`）破坏对齐后的观测性降级：
    /// 设计 §5.3.1 "首期仅观测，不回滚"。
    /// 若该 index 的结果不是 `Satisfied`，则不做任何操作（不覆盖更严重的状态）。
    pub fn downgrade_to_partial(&mut self, index: usize, message: impl Into<String>) {
        let Some(result) = self.results.iter_mut().find(|r| r.index == index) else {
            return;
        };
        if result.status != IntentStatus::Satisfied {
            return;
        }
        result.status = IntentStatus::Partial;
        result.message = Some(message.into());
        self.satisfied -= 1;
        self.partial += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_default_is_empty() {
        let ov = LayoutIntentOverlay::default();
        assert!(ov.topology.is_empty());
        assert!(ov.geometric.is_empty());
    }

    #[test]
    fn topology_intent_serde_roundtrip() {
        let intent = TopologyIntent::Below {
            from: "a".to_string(),
            to: "b".to_string(),
        };
        let json = serde_json::to_string(&intent).unwrap();
        assert!(json.contains("\"kind\":\"below\""), "json = {json}");
        let back: TopologyIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(intent, back);
    }

    #[test]
    fn above_intent_serde_roundtrip() {
        let intent = TopologyIntent::Above {
            from: "a".to_string(),
            to: "b".to_string(),
        };
        let json = serde_json::to_string(&intent).unwrap();
        assert!(json.contains("\"kind\":\"above\""), "json = {json}");
        let back: TopologyIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(intent, back);
    }

    #[test]
    fn geometric_intent_serde_roundtrip() {
        let pin = GeometricIntent::Pin {
            node: "a".to_string(),
            axis: PinAxis::X,
        };
        let json = serde_json::to_string(&pin).unwrap();
        assert!(json.contains("\"kind\":\"pin\""), "json = {json}");
        let back: GeometricIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(pin, back);

        let align = GeometricIntent::AlignVertical {
            nodes: vec!["a".to_string(), "b".to_string()],
        };
        let json = serde_json::to_string(&align).unwrap();
        assert!(json.contains("\"kind\":\"align_vertical\""), "json = {json}");
        let back: GeometricIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(align, back);
    }

    #[test]
    fn pin_axis_serde_snake_case() {
        let json = serde_json::to_string(&PinAxis::Both).unwrap();
        assert_eq!(json, "\"both\"");
        let back: PinAxis = serde_json::from_str("\"both\"").unwrap();
        assert_eq!(back, PinAxis::Both);
    }

    #[test]
    fn overlay_serde_roundtrip() {
        let ov = LayoutIntentOverlay {
            topology: vec![
                TopologyIntent::Below { from: "a".into(), to: "b".into() },
                TopologyIntent::Above { from: "c".into(), to: "d".into() },
            ],
            geometric: vec![GeometricIntent::Pin { node: "a".into(), axis: PinAxis::Both }],
        };
        let json = serde_json::to_string(&ov).unwrap();
        let back: LayoutIntentOverlay = serde_json::from_str(&json).unwrap();
        assert_eq!(ov, back);
    }

    #[test]
    fn overlay_serde_default_fields() {
        // 缺省字段应回退到空 vec
        let json = r#"{"topology":[]}"#;
        let ov: LayoutIntentOverlay = serde_json::from_str(json).unwrap();
        assert!(ov.topology.is_empty());
        assert!(ov.geometric.is_empty());
    }

    #[test]
    fn report_push_accumulates_counts() {
        let mut report = RefinementReport::default();
        report.push(0, "below", IntentStatus::Satisfied, None);
        report.push(1, "above", IntentStatus::Conflicted, Some("cycle".into()));
        report.push(2, "pin", IntentStatus::NotFound, None);
        report.push(3, "align_vertical", IntentStatus::Partial, Some("cross-group".into()));

        assert_eq!(report.satisfied, 1);
        assert_eq!(report.conflicted, 1);
        assert_eq!(report.not_found, 1);
        assert_eq!(report.partial, 1);
        assert_eq!(report.results.len(), 4);
        assert!(!report.is_empty());
    }

    #[test]
    fn report_merge_combines_counts() {
        let mut a = RefinementReport::default();
        a.push(0, "below", IntentStatus::Satisfied, None);

        let mut b = RefinementReport::default();
        b.push(0, "pin", IntentStatus::Partial, None);

        a.merge(b);
        assert_eq!(a.satisfied, 1);
        assert_eq!(a.partial, 1);
        assert_eq!(a.results.len(), 2);
    }

    #[test]
    fn report_empty_default() {
        let report = RefinementReport::default();
        assert!(report.is_empty());
        assert_eq!(report.satisfied, 0);
    }

    #[test]
    fn intent_status_serde_snake_case() {
        let json = serde_json::to_string(&IntentStatus::NotFound).unwrap();
        assert_eq!(json, "\"not_found\"");
        let back: IntentStatus = serde_json::from_str("\"not_found\"").unwrap();
        assert_eq!(back, IntentStatus::NotFound);
    }
}

/// Phase 1 端到端集成测试：通过 `compute_layout_with_plan_and_overlay` 验证
/// 拓扑意图对 rank 排序的影响、环检测拒绝、FAS 保护、报告状态汇总。
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
    };
    use crate::layout::compute_layout_with_plan_and_overlay;
    use crate::layout::plan::LayoutPlan;
    use crate::profile::profile_for;
    use crate::types::DiagramType;

    /// 构建一个 Flowchart diagram（默认走 `flowchart` 布局 = SugiyamaV2 引擎）。
    fn flowchart_diagram(entities: &[&str], relations: &[(&str, &str)]) -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: entities
                .iter()
                .map(|id| Entity {
                    id: Identifier::new_unchecked(id),
                    label: id.to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                })
                .collect(),
            relations: relations
                .iter()
                .map(|(from, to)| Relation {
                    from: Identifier::new_unchecked(from),
                    to: Identifier::new_unchecked(to),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span,
                })
                .collect(),
            groups: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    /// 解析 Flowchart 的默认 LayoutPlan（layout_algo=flowchart, edge_routing=orthogonal）。
    fn flowchart_plan(diagram: &Diagram) -> LayoutPlan {
        LayoutPlan::resolve(diagram, profile_for(&diagram.diagram_type))
    }

    fn run_with_overlay(
        diagram: &Diagram,
        overlay: &LayoutIntentOverlay,
    ) -> (crate::layout::LayoutResult, RefinementReport) {
        let plan = flowchart_plan(diagram);
        let (result, report) =
            compute_layout_with_plan_and_overlay(diagram, &plan, Some(overlay)).unwrap();
        (result, report.unwrap())
    }

    // ─── below / above 影响 rank 排序 ───────────────────────

    #[test]
    fn below_intent_forces_a_below_b_in_rank() {
        // 无真实边；Below(A,B) 注入 B→A，使 rank(A) > rank(B)
        let diagram = flowchart_diagram(&["a", "b", "c"], &[]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };

        let (result, report) = run_with_overlay(&diagram, &overlay);
        let ranks = result.hints.sugiyama_ranks.as_ref().expect("ranks hint");
        let ra = ranks["a"];
        let rb = ranks["b"];
        assert!(
            ra > rb,
            "Below(a,b) should force rank(a) > rank(b), got rank(a)={ra}, rank(b)={rb}"
        );
        assert_eq!(report.satisfied, 1);
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].status, IntentStatus::Satisfied);
    }

    #[test]
    fn above_intent_forces_a_above_b_in_rank() {
        // 无真实边；Above(A,B) 注入 A→B，使 rank(A) < rank(B)
        let diagram = flowchart_diagram(&["a", "b", "c"], &[]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Above {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };

        let (result, report) = run_with_overlay(&diagram, &overlay);
        let ranks = result.hints.sugiyama_ranks.as_ref().expect("ranks hint");
        let ra = ranks["a"];
        let rb = ranks["b"];
        assert!(
            ra < rb,
            "Above(a,b) should force rank(a) < rank(b), got rank(a)={ra}, rank(b)={rb}"
        );
        assert_eq!(report.satisfied, 1);
        assert_eq!(report.results[0].status, IntentStatus::Satisfied);
    }

    #[test]
    fn below_intent_with_consistent_real_edge_still_satisfied() {
        // 真实边 B→A 已使 A 在 B 下方；Below(A,B) 注入 B→A（冗余但一致）
        let diagram = flowchart_diagram(&["a", "b"], &[("b", "a")]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };

        let (result, report) = run_with_overlay(&diagram, &overlay);
        let ranks = result.hints.sugiyama_ranks.as_ref().expect("ranks hint");
        assert!(ranks["a"] > ranks["b"], "real edge b→a already puts a below b");
        assert_eq!(report.satisfied, 1);
    }

    // ─── 环检测拒绝 ─────────────────────────────────────────

    #[test]
    fn below_intent_cycling_with_real_edge_is_conflicted() {
        // 真实边 A→B；Below(A,B) 注入 B→A → 环 A→B→A → Conflicted
        let diagram = flowchart_diagram(&["a", "b"], &[("a", "b")]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };

        let (result, report) = run_with_overlay(&diagram, &overlay);
        assert_eq!(report.conflicted, 1);
        assert_eq!(report.results[0].status, IntentStatus::Conflicted);
        // 意图被跳过，rank 由真实边 A→B 决定：rank(a) < rank(b)
        let ranks = result.hints.sugiyama_ranks.as_ref().expect("ranks hint");
        assert!(ranks["a"] < ranks["b"]);
    }

    #[test]
    fn above_intent_cycling_with_real_edge_is_conflicted() {
        // 真实边 B→A；Above(A,B) 注入 A→B → 环 B→A→B → Conflicted
        let diagram = flowchart_diagram(&["a", "b"], &[("b", "a")]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Above {
                from: "a".into(),
                to: "b".into(),
            }],
            geometric: vec![],
        };

        let (_result, report) = run_with_overlay(&diagram, &overlay);
        assert_eq!(report.conflicted, 1);
        assert_eq!(report.results[0].status, IntentStatus::Conflicted);
    }

    // ─── FAS 保护：真实边成环时意图边不被反转 ───────────────

    #[test]
    fn fas_protection_preserves_intent_edge_through_real_cycle() {
        // 真实边 A→B, B→A 构成 2-环；C, D 独立。
        // Below(C,D) 注入 D→C。FAS 仅反转真实边破环，意图边 D→C 保留。
        // 期望 rank(C) > rank(D) → Satisfied。
        let diagram = flowchart_diagram(
            &["a", "b", "c", "d"],
            &[("a", "b"), ("b", "a")],
        );
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "c".into(),
                to: "d".into(),
            }],
            geometric: vec![],
        };

        let (result, report) = run_with_overlay(&diagram, &overlay);
        let ranks = result.hints.sugiyama_ranks.as_ref().expect("ranks hint");
        let rc = ranks["c"];
        let rd = ranks["d"];
        assert!(
            rc > rd,
            "intent edge d→c must not be reversed by FAS; expected rank(c) > rank(d), got {rc}/{rd}"
        );
        assert_eq!(report.satisfied, 1);
    }

    // ─── NotFound / 空报告 / None overlay ───────────────────

    #[test]
    fn not_found_intent_marked_in_report() {
        let diagram = flowchart_diagram(&["a", "b"], &[]);
        let overlay = LayoutIntentOverlay {
            topology: vec![TopologyIntent::Below {
                from: "a".into(),
                to: "ghost".into(),
            }],
            geometric: vec![],
        };

        let (_result, report) = run_with_overlay(&diagram, &overlay);
        assert_eq!(report.not_found, 1);
        assert_eq!(report.results[0].status, IntentStatus::NotFound);
        assert!(report.results[0]
            .message
            .as_deref()
            .unwrap()
            .contains("ghost"));
    }

    #[test]
    fn no_overlay_returns_none_report() {
        let diagram = flowchart_diagram(&["a", "b"], &[("a", "b")]);
        let plan = flowchart_plan(&diagram);
        let (_result, report) =
            compute_layout_with_plan_and_overlay(&diagram, &plan, None).unwrap();
        assert!(report.is_none());
    }

    #[test]
    fn empty_overlay_returns_empty_report() {
        let diagram = flowchart_diagram(&["a", "b"], &[("a", "b")]);
        let overlay = LayoutIntentOverlay::default();
        let (_result, report) = run_with_overlay(&diagram, &overlay);
        assert!(report.is_empty());
        assert_eq!(report.satisfied + report.partial + report.conflicted + report.not_found, 0);
    }

    // ─── 多意图混合状态 ─────────────────────────────────────

    #[test]
    fn multiple_intents_produce_mixed_statuses() {
        // A→B 真实边
        // Below(C,D): 无环 → Satisfied
        // Below(A,B): 与 A→B 成环 → Conflicted
        // Below(A,"ghost"): 节点不存在 → NotFound
        let diagram = flowchart_diagram(&["a", "b", "c", "d"], &[("a", "b")]);
        let overlay = LayoutIntentOverlay {
            topology: vec![
                TopologyIntent::Below { from: "c".into(), to: "d".into() },
                TopologyIntent::Below { from: "a".into(), to: "b".into() },
                TopologyIntent::Below { from: "a".into(), to: "ghost".into() },
            ],
            geometric: vec![],
        };

        let (result, report) = run_with_overlay(&diagram, &overlay);
        assert_eq!(report.satisfied, 1);
        assert_eq!(report.conflicted, 1);
        assert_eq!(report.not_found, 1);
        assert_eq!(report.results.len(), 3);

        // 验证每条结果的 index 与状态对应
        let by_index: std::collections::HashMap<usize, IntentStatus> = report
            .results
            .iter()
            .map(|r| (r.index, r.status))
            .collect();
        assert_eq!(by_index[&0], IntentStatus::Satisfied, "Below(c,d) should be satisfied");
        assert_eq!(by_index[&1], IntentStatus::Conflicted, "Below(a,b) should be conflicted");
        assert_eq!(by_index[&2], IntentStatus::NotFound, "Below(a,ghost) should be not_found");

        // C 应在 D 下方
        let ranks = result.hints.sugiyama_ranks.as_ref().expect("ranks hint");
        assert!(ranks["c"] > ranks["d"]);
    }

    // ─── 矛盾意图去重 ───────────────────────────────────────

    #[test]
    fn contradictory_below_and_above_second_one_conflicted() {
        // Below(A,B) + Above(A,B) 互为反方向 → 后者 Conflicted
        let diagram = flowchart_diagram(&["a", "b"], &[]);
        let overlay = LayoutIntentOverlay {
            topology: vec![
                TopologyIntent::Below { from: "a".into(), to: "b".into() },
                TopologyIntent::Above { from: "a".into(), to: "b".into() },
            ],
            geometric: vec![],
        };

        let (_result, report) = run_with_overlay(&diagram, &overlay);
        assert_eq!(report.conflicted, 1);
        assert_eq!(report.satisfied, 1);
        // 第二条（index=1）被标记为 Conflicted
        let conflicted = report.results.iter().find(|r| r.status == IntentStatus::Conflicted).unwrap();
        assert_eq!(conflicted.index, 1);
    }
}
