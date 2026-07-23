//! 状态图布局（`layout_algo: state`）。
//!
//! 按 Greedy FAS 反转边比例自动选择布局策略：
//! - `r < 0.3`：准 DAG，走 Sugiyama-v2（TB）+ 正交路由
//! - `r ≥ 0.3`：高回环率，保留 circular
//! - 用户显式 `layout: circular` 时强制 circular

use crate::ast::Diagram;
use crate::layout::algorithm_config::{CircularLayoutConfig, SugiyamaLayoutConfig};
use crate::layout::kernel::recipe::LayoutRecipe;
use crate::layout::node::circular::CircularLayout;
use crate::layout::node::common::acyclic::greedy_fas;
use crate::layout::node::sugiyama_v2::{engine, preset};
use crate::layout::plan::{diagram_algorithm_name, ResolvedAlgoOptions};
use crate::layout::{AlgorithmOptionSpec, EdgeRoutingStyle, LayoutResult, LayoutStrategy, NodeAlignConfig};
use crate::types::standard_attr_keys::diagram;
use crate::types::DiagramType;
use std::collections::HashMap;

/// FAS 反转边比例低于此阈值时走 Sugiyama。
pub const SUGIYAMA_REVERSAL_THRESHOLD: f64 = 0.3;

/// 状态图布局（`layout_algo: state`）。
pub struct StateLayout {
    circular_config: CircularLayoutConfig,
    sugiyama_config: SugiyamaLayoutConfig,
}

impl StateLayout {
    pub fn new(
        circular_config: CircularLayoutConfig,
        sugiyama_config: SugiyamaLayoutConfig,
    ) -> Self {
        Self {
            circular_config,
            sugiyama_config,
        }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(
            CircularLayoutConfig::from_options(options),
            SugiyamaLayoutConfig::from_options(options),
        )
    }
}

impl Default for StateLayout {
    fn default() -> Self {
        Self::new(
            CircularLayoutConfig::default(),
            SugiyamaLayoutConfig::default(),
        )
    }
}

impl LayoutStrategy for StateLayout {
    fn name(&self) -> &'static str {
        "state"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[DiagramType::State]
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        crate::layout::algorithm_config::SUGIYAMA_LAYOUT_OPTIONS
    }

    fn supported_directions(&self) -> &'static [&'static str] {
        const SUPPORTED_DIRECTIONS: &[&str] = &[
            crate::types::attr_constants::direction::TOP_TO_BOTTOM,
            crate::types::attr_constants::direction::LEFT_TO_RIGHT,
        ];
        SUPPORTED_DIRECTIONS
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        let recipe = StateRecipe {
            circular_config: self.circular_config,
            sugiyama_config: self.sugiyama_config,
        };
        recipe.execute(diagram)
    }

    fn node_align_config(&self) -> NodeAlignConfig {
        NodeAlignConfig::default_sugiyama()
    }
}

// ─── Recipe 实现 ────────────────────────────────────────

/// 状态图布局配方。
///
/// 委托配方：根据 FAS 反转率选择 Circular 或 Sugiyama。
struct StateRecipe {
    circular_config: CircularLayoutConfig,
    sugiyama_config: SugiyamaLayoutConfig,
}

/// 状态图问题 IR。
enum StateProblem {
    /// Circular 布局路径
    Circular,
    /// Sugiyama 布局路径：LayeredKernel 产出的分层草稿
    Sugiyama(crate::layout::node::sugiyama_v2::layered_kernel::LayeredDraft),
}

/// 状态图求解结果。
struct StateSolution {
    result: LayoutResult,
}

impl LayoutRecipe for StateRecipe {
    type Problem = StateProblem;
    type Solution = StateSolution;

    fn name(&self) -> &'static str {
        "state"
    }

    fn compile(&self, diagram: &Diagram) -> StateProblem {
        if user_requested_circular(diagram) || !should_use_sugiyama(diagram) {
            StateProblem::Circular
        } else {
            let draft = crate::layout::node::sugiyama_v2::layered_kernel::LayeredKernel::compute(
                diagram,
                &preset::STATE_PRESET,
                self.sugiyama_config,
            );
            StateProblem::Sugiyama(draft)
        }
    }

    fn solve(&self, problem: &StateProblem) -> StateSolution {
        match problem {
            StateProblem::Sugiyama(draft) => {
                let (nodes, solved_problem) =
                    crate::layout::node::sugiyama_v2::coordinate::assign_coordinates_brandes_koepf(
                        &draft.dag,
                        &draft.proper_graph,
                        &draft.layers,
                        &draft.sizes,
                        draft.horizontal,
                        &draft.preset,
                        &draft.per_layer_gaps,
                        draft.has_order_bias,
                        &draft.end_ids,
                    );
                // solve 产出节点坐标，完整 LayoutResult 由 execute 组装
                StateSolution {
                    result: LayoutResult {
                        nodes,
                        groups: HashMap::new(),
                        edges: vec![],
                        total_width: 0.0,
                        total_height: 0.0,
                        hints: Default::default(),
                    },
                }
            }
            StateProblem::Circular => StateSolution {
                result: LayoutResult {
                    nodes: HashMap::new(),
                    groups: HashMap::new(),
                    edges: vec![],
                    total_width: 0.0,
                    total_height: 0.0,
                    hints: Default::default(),
                },
            },
        }
    }

    fn product(&self, _solution: &StateSolution, _diagram: &Diagram) -> LayoutResult {
        unreachable!("product called directly; use execute()")
    }

    fn execute(&self, diagram: &Diagram) -> LayoutResult {
        if user_requested_circular(diagram) || !should_use_sugiyama(diagram) {
            let mut result = CircularLayout::new(self.circular_config).compute(diagram);
            result.hints.edge_routing_style = EdgeRoutingStyle::Curved;
            return result;
        }

        let mut result = engine::compute_with_preset(
            diagram,
            &preset::STATE_PRESET,
            self.sugiyama_config,
        );
        result.hints.edge_routing_style = EdgeRoutingStyle::Orthogonal;
        result
    }
}

/// 用户显式指定 `layout: circular` 时不做自动覆盖。
fn user_requested_circular(diagram: &Diagram) -> bool {
    diagram_algorithm_name(diagram, diagram::LAYOUT) == Some("circular")
}

/// Greedy FAS 反转边比例 `r` 低于阈值时走 Sugiyama。
fn should_use_sugiyama(diagram: &Diagram) -> bool {
    fas_reversal_ratio(diagram) < SUGIYAMA_REVERSAL_THRESHOLD
}

/// 计算 Greedy FAS 反转边比例。
pub fn fas_reversal_ratio(diagram: &Diagram) -> f64 {
    let total = diagram.relations.len();
    if total == 0 {
        return 0.0;
    }

    let mut nodes: Vec<String> = diagram
        .entities
        .iter()
        .map(|e| e.id.as_str().to_string())
        .collect();
    nodes.sort();

    let mut out_neighbors: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_neighbors: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &diagram.relations {
        let from = rel.from.as_str().to_string();
        let to = rel.to.as_str().to_string();
        out_neighbors.entry(from.clone()).or_default().push(to.clone());
        in_neighbors.entry(to).or_default().push(from);
    }
    for list in out_neighbors.values_mut() {
        list.sort();
    }
    for list in in_neighbors.values_mut() {
        list.sort();
    }

    let reversed = greedy_fas(&nodes, &out_neighbors, &in_neighbors);
    reversed.len() as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span,
    };

    fn state_diagram(relations: Vec<(&str, &str)>) -> Diagram {
        let span = Span::dummy();
        let mut entity_ids: Vec<String> = relations
            .iter()
            .flat_map(|(a, b)| [a.to_string(), b.to_string()])
            .collect();
        entity_ids.sort();
        entity_ids.dedup();

        Diagram {
            diagram_type: DiagramType::State,
            attributes: vec![],
            entities: entity_ids
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
                .into_iter()
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
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        }
    }

    #[test]
    fn payment_flow_ratio_below_threshold() {
        let diagram = state_diagram(vec![
            ("init", "pending"),
            ("pending", "processing"),
            ("processing", "success"),
            ("processing", "failed"),
            ("failed", "retry"),
            ("retry", "processing"),
            ("success", "refunding"),
            ("refunding", "refunded"),
            ("refunding", "success"),
        ]);
        assert!(fas_reversal_ratio(&diagram) < SUGIYAMA_REVERSAL_THRESHOLD);
        assert!(should_use_sugiyama(&diagram));
    }

    #[test]
    fn dense_cycles_ratio_above_threshold() {
        let diagram = state_diagram(vec![
            ("a", "b"),
            ("b", "c"),
            ("c", "a"),
            ("c", "b"),
            ("a", "a"),
        ]);
        assert!(fas_reversal_ratio(&diagram) >= SUGIYAMA_REVERSAL_THRESHOLD);
        assert!(!should_use_sugiyama(&diagram));
    }

    #[test]
    fn payment_flow_layout_is_top_to_bottom() {
        use crate::pipeline::{parse, prepare};
        use crate::prepare::StyleRequest;

        let source = include_str!("../../../../../../showcase/state/product.payment-flow.pgm");
        let raw = parse(source).expect("parse payment-flow");
        let prepared = prepare(raw, &StyleRequest::default()).expect("prepare");
        let result = StateLayout::default().compute(&prepared.diagram);

        let init_y = result.nodes["init"].y;
        let success_y = result.nodes["success"].y;
        let expired_y = result.nodes["expired"].y;
        assert!(
            init_y < success_y && success_y <= expired_y,
            "init={init_y} success={success_y} expired={expired_y}"
        );
        assert_eq!(result.hints.edge_routing_style, crate::layout::EdgeRoutingStyle::Orthogonal);
    }
}
