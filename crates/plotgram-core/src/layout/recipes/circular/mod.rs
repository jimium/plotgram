//! 自适应圆形布局
//!
//! - 单双连通分量：所有节点均匀排列在一个圆上（状态图主路径）
//! - 多双连通分量：每个分量独立成圆，割点靠内侧（circo 风格）
//!
//! 边几何由 `edge_routing: circular` 独立计算。
//!
//! `state` 为图类型门面算法，共享本模块引擎；`circular` 保留为通用圆形布局（高级选项）。

pub mod common;

pub use common::{
    calculate_circle_radius, order_entities_on_circle, CircleGroup, CircularLayoutHints,
    SimpleGraph,
};

use crate::types::DiagramType;
use crate::ast::{Diagram};
use crate::layout::algorithm_config::{CircularLayoutConfig, CIRCULAR_LAYOUT_OPTIONS};
use crate::layout::kernel::recipe::LayoutRecipe;
use crate::layout::engines::common::group_bounds::{self, GroupPadding};
use crate::layout::pipeline::plan::ResolvedAlgoOptions;
use crate::layout::{AlgorithmOptionSpec, LayoutHints, LayoutResult, LayoutStrategy, NodeLayout};
use common::{
    circle_radius_for_bcc, find_articulation_points, find_biconnected_components,
    node_size_for, plan_multi_ring_groups, reorder_bcc_by_bfs, should_use_multi_ring,
    MAX_CIRCLE_RADIUS, CUTPOINT_OFFSET, APPLICABLE_TYPES,
};
use std::collections::HashMap;
use std::f64::consts::PI;

pub struct CircularLayout {
    config: CircularLayoutConfig,
}

impl CircularLayout {
    pub fn new(config: CircularLayoutConfig) -> Self {
        Self { config }
    }

    pub fn from_options(options: &ResolvedAlgoOptions) -> Self {
        Self::new(CircularLayoutConfig::from_options(options))
    }
}

impl Default for CircularLayout {
    fn default() -> Self {
        Self::new(CircularLayoutConfig::default())
    }
}

impl LayoutStrategy for CircularLayout {
    fn name(&self) -> &'static str {
        "circular"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        APPLICABLE_TYPES
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        CIRCULAR_LAYOUT_OPTIONS
    }

    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        let recipe = CircularRecipe { config: self.config };
        recipe.execute(diagram)
    }
}

// ─── Recipe 实现 ────────────────────────────────────────

/// 圆形布局配方。
///
/// 几何求解器：节点均匀排列在圆上（单环）或多 BCC 分圆（多环）。
struct CircularRecipe {
    config: CircularLayoutConfig,
}

/// 圆形布局问题 IR。
enum CircularProblem {
    Empty,
    SingleRing {
        entity_ids: Vec<String>,
        sizes: Vec<(f64, f64)>,
        order: Vec<usize>,
        radius: f64,
    },
    MultiRing {
        graph: SimpleGraph,
        bccs: Vec<Vec<usize>>,
        sizes: HashMap<String, (f64, f64)>,
    },
}

/// 圆形布局解。
struct CircularSolution {
    nodes: HashMap<String, NodeLayout>,
    circles: Vec<CircleGroup>,
    total_width: f64,
    total_height: f64,
}

impl LayoutRecipe for CircularRecipe {
    type Problem = CircularProblem;
    type Solution = CircularSolution;

    fn name(&self) -> &'static str {
        "circular"
    }

    fn compile(&self, diagram: &Diagram) -> CircularProblem {
        let node_count = diagram.entities.len();
        if node_count == 0 {
            return CircularProblem::Empty;
        }

        let graph = SimpleGraph::from_diagram(diagram);
        let bccs = find_biconnected_components(&graph);

        if should_use_multi_ring(&graph, &bccs) {
            let sizes: HashMap<String, (f64, f64)> = diagram
                .entities
                .iter()
                .map(|e| {
                    let (w, h) = node_size_for(diagram, e);
                    (e.id.as_str().to_string(), crate::layout::styled_node_size(e, w, h))
                })
                .collect();
            CircularProblem::MultiRing { graph, bccs, sizes }
        } else {
            let order = order_entities_on_circle(diagram);
            let entity_ids: Vec<String> = diagram
                .entities
                .iter()
                .map(|e| e.id.as_str().to_string())
                .collect();
            let sizes: Vec<(f64, f64)> = order
                .iter()
                .map(|&idx| node_size_for(diagram, &diagram.entities[idx]))
                .collect();
            let radius = calculate_circle_radius(&sizes);
            CircularProblem::SingleRing { entity_ids, sizes, order, radius }
        }
    }

    fn solve(&self, problem: &CircularProblem) -> CircularSolution {
        match problem {
            CircularProblem::Empty => CircularSolution {
                nodes: HashMap::new(),
                circles: vec![],
                total_width: self.config.padding * 2.0,
                total_height: self.config.padding * 2.0,
            },
            CircularProblem::SingleRing { entity_ids, sizes, order, radius } => {
                self.solve_single_ring(entity_ids, sizes, order, *radius)
            }
            CircularProblem::MultiRing { graph, bccs, sizes } => {
                self.solve_multi_ring(graph, bccs, sizes)
            }
        }
    }

    fn product(&self, solution: &CircularSolution, diagram: &Diagram) -> LayoutResult {
        if solution.nodes.is_empty() {
            return LayoutResult {
                nodes: HashMap::new(),
                groups: HashMap::new(),
                edges: vec![],
                total_width: solution.total_width,
                total_height: solution.total_height,
                hints: LayoutHints {
                    edge_routing_style: crate::layout::EdgeRoutingStyle::Curved,
                    ..Default::default()
                },
            };
        }

        let groups = group_bounds::compute_group_bounds(
            diagram,
            &solution.nodes,
            GroupPadding::uniform(self.config.group_padding, 16.0),
        );

        LayoutResult {
            nodes: solution.nodes.clone(),
            groups,
            edges: vec![],
            total_width: solution.total_width,
            total_height: solution.total_height,
            hints: CircularLayoutHints {
                circles: solution.circles.clone(),
            }
            .into_layout_hints(),
        }
    }
}

impl CircularRecipe {
    fn solve_single_ring(
        &self,
        entity_ids: &[String],
        sizes: &[(f64, f64)],
        order: &[usize],
        radius: f64,
    ) -> CircularSolution {
        let node_count = order.len();
        let center = (self.config.padding + radius, self.config.padding + radius);

        let mut nodes = HashMap::new();
        for (i, &entity_idx) in order.iter().enumerate() {
            let (width, height) = sizes[i];
            let angle = -PI / 2.0 + (2.0 * PI * i as f64) / node_count as f64;
            let node_center_x = center.0 + radius * angle.cos();
            let node_center_y = center.1 + radius * angle.sin();

            nodes.insert(
                entity_ids[entity_idx].clone(),
                NodeLayout {
                    x: node_center_x - width / 2.0,
                    y: node_center_y - height / 2.0,
                    width,
                    height,
                    ..Default::default()
                },
            );
        }

        CircularSolution {
            nodes,
            circles: vec![CircleGroup {
                center,
                radius,
                entity_indices: order.to_vec(),
            }],
            total_width: center.0 * 2.0 + self.config.padding,
            total_height: center.1 * 2.0 + self.config.padding,
        }
    }

    fn solve_multi_ring(
        &self,
        graph: &SimpleGraph,
        bccs: &[Vec<usize>],
        sizes: &HashMap<String, (f64, f64)>,
    ) -> CircularSolution {
        let articulation = find_articulation_points(graph);

        let mut nodes = HashMap::new();
        let mut circles = Vec::new();
        let mut cursor_x = self.config.padding;

        let groups_to_layout = plan_multi_ring_groups(graph, bccs);
        let mut placed_articulation: HashMap<usize, (f64, f64)> = HashMap::new();

        for group in &groups_to_layout {
            if group.is_empty() {
                continue;
            }

            let ordered = reorder_bcc_by_bfs(group, graph);
            let radius = circle_radius_for_bcc(&ordered, sizes, graph).min(MAX_CIRCLE_RADIUS);

            let anchor_articulation = ordered
                .iter()
                .find(|&&idx| placed_articulation.contains_key(&idx));

            let center = if let Some(&anchor_idx) = anchor_articulation {
                let (ax, ay) = placed_articulation[&anchor_idx];
                (ax + radius, ay)
            } else {
                (cursor_x + radius + self.config.padding, radius + self.config.padding)
            };

            for (i, &node_idx) in ordered.iter().enumerate() {
                if articulation.contains(&node_idx) && placed_articulation.contains_key(&node_idx) {
                    continue;
                }

                let node_id = &graph.node_ids[node_idx];
                let (width, height) = sizes
                    .get(node_id)
                    .copied()
                    .unwrap_or((common::DEFAULT_NODE_WIDTH, common::DEFAULT_NODE_HEIGHT));

                let r = if articulation.contains(&node_idx) {
                    (radius - CUTPOINT_OFFSET).max(radius * 0.5)
                } else {
                    radius
                };

                let angle = -PI / 2.0 + (2.0 * PI * i as f64) / ordered.len() as f64;
                let cx = center.0 + r * angle.cos();
                let cy = center.1 + r * angle.sin();

                nodes.insert(
                    node_id.clone(),
                    NodeLayout {
                        x: cx - width / 2.0,
                        y: cy - height / 2.0,
                        width,
                        height,
                        ..Default::default()
                    },
                );

                if articulation.contains(&node_idx) {
                    placed_articulation.insert(node_idx, (cx, cy));
                }
            }

            circles.push(CircleGroup {
                center,
                radius,
                entity_indices: ordered,
            });

            cursor_x = center.0 + radius;
        }

        let total_width =
            nodes.values().map(|n| n.x + n.width).fold(0.0_f64, f64::max) + self.config.padding;
        let total_height =
            nodes.values().map(|n| n.y + n.height).fold(0.0_f64, f64::max) + self.config.padding;

        CircularSolution {
            nodes,
            circles,
            total_width,
            total_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, Entity, Identifier, Relation, SourceInfo,
        Span, TextValue,
    };
    use crate::types::DiagramType;

    fn create_test_diagram(
        diagram_type: DiagramType,
        entities: Vec<(&str, &str, &str)>,
        relations: Vec<(&str, &str)>,
    ) -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type,
            attributes: Vec::new(),
            entities: entities
                .into_iter()
                .map(|(id, label, ty)| {
                    let mut attrs = AttributeMap::default();
                    if !ty.is_empty() {
                        attrs
                            .standard
                            .insert("type".to_string(), AttributeValue::String(TextValue::unquoted(ty.to_string())));
                    }
                    Entity {
                        id: Identifier::new_unchecked(id),
                        label: label.to_string(),
                        attributes: attrs,
                        group_id: None,
                        span,
                    }
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
            groups: Vec::new(),
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
    fn test_circular_layout_empty() {
        let diagram = Diagram::new(
            DiagramType::Flowchart,
            SourceInfo {
                file: None,
                line_count: 1,
            },
        );
        let result = CircularLayout::default().compute(&diagram);
        assert!(result.nodes.is_empty());
        assert!(result.edges.is_empty());
    }

    #[test]
    fn test_circular_layout_places_nodes_on_circle() {
        let diagram = create_test_diagram(
            DiagramType::Flowchart,
            vec![("a", "A", ""), ("b", "B", ""), ("c", "C", "")],
            vec![],
        );
        let result = CircularLayout::default().compute(&diagram);
        assert_eq!(result.nodes.len(), 3);
        assert_eq!(result.hints.circular.as_ref().unwrap().circles.len(), 1);
    }

    #[test]
    fn test_state_layout_uses_smaller_initial_node() {
        let diagram = create_test_diagram(
            DiagramType::State,
            vec![("init", "", "initial"), ("running", "运行中", "state")],
            vec![("init", "running")],
        );
        let result = CircularLayout::default().compute(&diagram);
        let init = result.nodes.get("init").unwrap();
        let running = result.nodes.get("running").unwrap();
        assert!(init.width < running.width);
        assert!(init.height < running.height);
    }

    #[test]
    fn test_circular_layout_does_not_produce_edges() {
        let diagram = create_test_diagram(
            DiagramType::State,
            vec![("a", "A", "state"), ("b", "B", "state"), ("c", "C", "state")],
            vec![("a", "b"), ("b", "c")],
        );
        let result = CircularLayout::default().compute(&diagram);
        assert!(result.edges.is_empty());
    }

    #[test]
    fn test_multi_ring_for_disconnected_components() {
        let diagram = create_test_diagram(
            DiagramType::Flowchart,
            vec![("a", "A", ""), ("b", "B", ""), ("c", "C", ""), ("d", "D", "")],
            vec![("a", "b"), ("c", "d")],
        );
        let result = CircularLayout::default().compute(&diagram);
        assert_eq!(result.nodes.len(), 4);
        assert_eq!(result.hints.circular.as_ref().unwrap().circles.len(), 2);
    }

    #[test]
    fn test_calculate_circle_radius_grows_with_count() {
        let r3 = calculate_circle_radius(&[(80.0, 44.0); 3]);
        let r10 = calculate_circle_radius(&[(80.0, 44.0); 10]);
        assert!(r10 > r3);
    }
}
