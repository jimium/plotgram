//! Sugiyama v2
//!
//! - greedy cycle removal
//! - network-simplex-style rank compaction
//! - weighted median + transpose ordering
//! - Brandes-Kopf-style four-pass coordinate assignment
//!
//! `flowchart` / `er` 为图类型门面算法，已拆分到 [`crate::layout::node::flowchart`] /
//! [`crate::layout::node::er`]，共享本模块引擎与 [`preset`] 参数集；
//! `sugiyama-v2` 保留为通用分层布局（高级选项）。

use crate::types::DiagramType;
use crate::ast::{Diagram};
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::{AlgorithmOptionSpec, LayoutResult, LayoutStrategy, NodeAlignConfig};

pub mod engine;
pub mod preset;

pub(super) mod graph;
pub(super) mod rank;
pub(super) mod order;
pub(super) mod coordinate;
pub(super) mod postprocess;

pub struct SugiyamaV2Layout {
    config: SugiyamaLayoutConfig,
}

impl SugiyamaV2Layout {
    pub fn new(config: SugiyamaLayoutConfig) -> Self {
        Self { config }
    }
}

impl Default for SugiyamaV2Layout {
    fn default() -> Self {
        Self::new(SugiyamaLayoutConfig::default())
    }
}

impl LayoutStrategy for SugiyamaV2Layout {
    fn name(&self) -> &'static str {
        "sugiyama-v2"
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[DiagramType::Flowchart, DiagramType::State, DiagramType::Er]
    }

    fn supports_custom(&self) -> bool {
        true
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
        engine::compute_with_preset(diagram, &preset::GENERIC_PRESET, self.config)
    }


    fn node_align_config(&self) -> NodeAlignConfig {
        NodeAlignConfig::default_sugiyama()
    }
}

// ─── Tests ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, AttributeValue, Diagram, Entity, Identifier, Relation, SourceInfo,
        Span,
    };
    use crate::types::DiagramType;
    use crate::prepare::StyleRequest;
    use crate::layout::NodeLayout;
    use crate::layout::GroupLayout;
    use crate::layout::node::flowchart::FlowchartLayout;
    use crate::pipeline::{parse, prepare};
    use petgraph::graph::{DiGraph, NodeIndex};
    use std::collections::{HashMap, HashSet};

    fn test_diagram(entities: Vec<&str>, relations: Vec<(&str, &str)>) -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: entities
                .into_iter()
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
    fn v2_layout_builds_nodes() {
        let diagram = test_diagram(
            vec!["a", "b", "c", "d"],
            vec![("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")],
        );

        let result = SugiyamaV2Layout::default().compute(&diagram);
        assert_eq!(result.nodes.len(), 4);
        assert!(result.total_width > preset::GENERIC_PRESET.padding * 2.0);
        assert!(result.total_height > preset::GENERIC_PRESET.padding * 2.0);
    }

    #[test]
    fn flowchart_facade_matches_flowchart_preset() {
        let diagram = test_diagram(vec!["a", "b"], vec![("a", "b")]);
        let facade = FlowchartLayout::default().compute(&diagram);
        let direct = engine::compute_with_preset(
            &diagram,
            &preset::FLOWCHART_PRESET,
            SugiyamaLayoutConfig::default(),
        );
        assert_eq!(layout_signature(&facade), layout_signature(&direct));
    }

    #[test]
    fn network_simplex_compacts_ranks() {
        let diagram = test_diagram(
            vec!["a", "b", "c", "d"],
            vec![("a", "b"), ("b", "c"), ("d", "c")],
        );

        let g = graph::build_graph(&diagram);
        let reversed = graph::greedy_cycle_reversal(&g);
        let dag = graph::build_dag(&g, &reversed);
        let component = rank::weak_components(&dag).remove(0);
        let component_set = component.iter().copied().collect::<HashSet<_>>();
        let initial = rank::longest_path_ranks(&dag, &component_set);
        let optimized = rank::assign_ranks_network_simplex_style(&dag)
            .into_iter()
            .map(|(node, rank)| (node, rank as i32))
            .collect::<HashMap<_, _>>();

        assert!(total_rank_cost(&dag, &optimized) < total_rank_cost(&dag, &initial));
        let c = dag.node_indices().find(|node| dag[*node] == "c").unwrap();
        let d = dag.node_indices().find(|node| dag[*node] == "d").unwrap();
        assert_eq!(optimized[&c] - optimized[&d], 1);
    }

    #[test]
    fn network_simplex_style_ranking_returns_monotonic_layers() {
        let diagram = test_diagram(
            vec!["a", "b", "c", "d", "e"],
            vec![("a", "c"), ("b", "c"), ("c", "d"), ("c", "e")],
        );
        let g = graph::build_graph(&diagram);
        let reversed = graph::greedy_cycle_reversal(&g);
        let dag = graph::build_dag(&g, &reversed);
        let ranks = rank::assign_ranks_network_simplex_style(&dag);
        for edge in dag.edge_indices() {
            let (from, to) = dag.edge_endpoints(edge).unwrap();
            assert!(ranks[&to] > ranks[&from]);
        }
    }

    #[test]
    fn proper_layer_graph_splits_long_edges_into_dummy_chain() {
        let diagram = test_diagram(
            vec!["a", "b", "c", "d"],
            vec![("a", "b"), ("b", "c"), ("c", "d"), ("a", "d")],
        );
        let g = graph::build_graph(&diagram);
        let reversed = graph::greedy_cycle_reversal(&g);
        let dag = graph::build_dag(&g, &reversed);
        let ranks = rank::assign_ranks_network_simplex_style(&dag);
        let proper = graph::build_proper_layer_graph(
            &diagram,
            &dag,
            &ranks,
            &preset::FLOWCHART_PRESET,
        );

        let dummy_segments = proper
            .graph
            .node_indices()
            .filter_map(|node| match &proper.graph[node].kind {
                graph::LayerNodeKind::Dummy {
                    source,
                    target,
                    segment,
                } => Some((source.index(), target.index(), *segment)),
                graph::LayerNodeKind::Real(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(dummy_segments.len(), 2);

        for edge in proper.graph.edge_indices() {
            let (from, to) = proper.graph.edge_endpoints(edge).unwrap();
            let from_rank = proper.graph[from].rank;
            let to_rank = proper.graph[to].rank;
            assert_eq!(to_rank, from_rank + 1);
        }
    }

    #[test]
    fn brandes_koepf_aligns_dummy_chain_vertically() {
        let diagram = test_diagram(
            vec!["a", "b", "c", "d"],
            vec![("a", "b"), ("b", "c"), ("c", "d"), ("a", "d")],
        );
        let g = graph::build_graph(&diagram);
        let reversed = graph::greedy_cycle_reversal(&g);
        let dag = graph::build_dag(&g, &reversed);
        let ranks = rank::assign_ranks_network_simplex_style(&dag);
        let proper = graph::build_proper_layer_graph(
            &diagram,
            &dag,
            &ranks,
            &preset::FLOWCHART_PRESET,
        );
        let layers = order::order_layers_weighted_median(
            &proper.graph,
            proper.layers.clone(),
            preset::FLOWCHART_PRESET.ordering_sweeps,
            preset::FLOWCHART_PRESET.long_edge_barycenter_weight,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        let centers = coordinate::assign_layer_centers_brandes_koepf(
            &proper.graph,
            &layers,
            &proper.sizes,
            &preset::FLOWCHART_PRESET,
            &std::collections::HashSet::new(),
        );

        let mut chain = proper
            .graph
            .node_indices()
            .filter_map(|node| match proper.graph[node].kind {
                graph::LayerNodeKind::Dummy { source, target, .. }
                    if dag[source] == "a" && dag[target] == "d" =>
                {
                    Some((proper.graph[node].rank, centers[&node]))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        chain.sort_by_key(|(rank, _)| *rank);
        assert_eq!(chain.len(), 2);
        assert!((chain[0].1 - chain[1].1).abs() < 1e-6);
    }

    #[test]
    fn v2_layout_avoids_overlap_for_bottom_siblings() {
        let mut diagram = test_diagram(
            vec!["client", "gateway", "auth", "db", "cache"],
            vec![
                ("client", "gateway"),
                ("gateway", "auth"),
                ("auth", "db"),
                ("auth", "cache"),
                ("client", "auth"),
                ("gateway", "cache"),
            ],
        );
        for entity in &mut diagram.entities {
            if entity.id.as_str() == "db" || entity.id.as_str() == "cache" {
                entity
                    .attributes
                    .style
                    .insert("width".to_string(), AttributeValue::Number(260.0));
            }
        }

        let result = SugiyamaV2Layout::default().compute(&diagram);
        let db = &result.nodes["db"];
        let cache = &result.nodes["cache"];
        assert!(!rectangles_overlap(db, cache));
    }

    #[test]
    fn v2_layout_is_deterministic_across_runs() {
        let diagram = test_diagram(
            vec!["client", "gateway", "auth", "db", "cache"],
            vec![
                ("client", "gateway"),
                ("gateway", "auth"),
                ("auth", "db"),
                ("auth", "cache"),
                ("client", "auth"),
                ("gateway", "cache"),
            ],
        );

        let baseline = layout_signature(&SugiyamaV2Layout::default().compute(&diagram));
        for _ in 0..4 {
            let current = layout_signature(&SugiyamaV2Layout::default().compute(&diagram));
            assert_eq!(current, baseline);
        }
    }

    #[test]
    fn v2_layout_is_deterministic_from_source_pipeline() {
        let source = r#"diagram flowchart {
            title: "用户认证流程"
            config {
                layout: sugiyama-v2
                direction: top-to-bottom
            }

            entity client "移动客户端" { type: client }
            entity gateway "API 网关" { type: gateway }
            entity auth "认证服务" { type: service }
            entity db "用户数据库" { type: database }
            entity cache "Token 缓存" { type: cache }

            client -> gateway "HTTPS 请求"
            gateway -> auth "转发认证请求"
            auth -> db "查询用户信息"
            db --> auth "返回用户记录"
            auth -> cache "存储 Token"
            cache --> auth "返回缓存结果"
            auth --> gateway "认证结果"
            gateway --> client "响应"
        }"#;

        let baseline = pipeline_layout_signature(source);
        for _ in 0..4 {
            let current = pipeline_layout_signature(source);
            assert_eq!(current, baseline);
        }
    }

    #[test]
    fn group_bias_clusters_same_group_nodes_in_layer() {
        use crate::ast::Group;
        // hub → {a1, a2 (group A), b1, b2 (group B)}
        // 4 个后继原本在同一层，apply_group_rank_constraints 将 group A/B
        // 分到不重叠的 rank 窗口，消除 group 包围框重叠。
        let span = Span::dummy();
        let mk_entity = |id: &str, gid: Option<&str>| Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            group_id: gid.map(|g| Identifier::new_unchecked(g)),
            span,
        };
        let mk_relation = |from: &str, to: &str| Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                mk_entity("hub", None),
                mk_entity("a1", Some("A")),
                mk_entity("a2", Some("A")),
                mk_entity("b1", Some("B")),
                mk_entity("b2", Some("B")),
            ],
            relations: vec![
                mk_relation("hub", "a1"),
                mk_relation("hub", "a2"),
                mk_relation("hub", "b1"),
                mk_relation("hub", "b2"),
            ],
            groups: vec![
                Group {
                    id: Identifier::new_unchecked("A"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![Identifier::new_unchecked("a1"), Identifier::new_unchecked("a2")],
                    child_group_ids: vec![],
                    span,
                },
                Group {
                    id: Identifier::new_unchecked("B"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![Identifier::new_unchecked("b1"), Identifier::new_unchecked("b2")],
                    child_group_ids: vec![],
                    span,
                },
            ],
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        };

        let result = SugiyamaV2Layout::default().compute(&diagram);

        // group A 和 group B 的包围框不应重叠
        let group_a = &result.groups["A"];
        let group_b = &result.groups["B"];
        assert!(
            !rectangles_overlap_groups(group_a, group_b),
            "A and B should not overlap: A={:?} B={:?}",
            group_a,
            group_b
        );
    }

    #[test]
    fn group_rank_constraints_prevent_group_overlap() {
        use crate::ast::Group;
        // 3 个 group，group 间有边和回边
        // A: start → gate_a
        // B: review → gate_b
        // C: approve → end
        // 跨 group 边：gate_a → review, gate_b → approve, gate_b → start（回边）
        let span = Span::dummy();
        let mk_entity = |id: &str, gid: Option<&str>| Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            group_id: gid.map(|g| Identifier::new_unchecked(g)),
            span,
        };
        let mk_relation = |from: &str, to: &str| Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span,
        };
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                mk_entity("start", Some("A")),
                mk_entity("gate_a", Some("A")),
                mk_entity("review", Some("B")),
                mk_entity("gate_b", Some("B")),
                mk_entity("approve", Some("C")),
                mk_entity("end", Some("C")),
            ],
            relations: vec![
                mk_relation("start", "gate_a"),
                mk_relation("gate_a", "review"),
                mk_relation("review", "gate_b"),
                mk_relation("gate_b", "approve"),
                mk_relation("approve", "end"),
                mk_relation("gate_b", "start"), // 回边
            ],
            groups: vec![
                Group {
                    id: Identifier::new_unchecked("A"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![
                        Identifier::new_unchecked("start"),
                        Identifier::new_unchecked("gate_a"),
                    ],
                    child_group_ids: vec![],
                    span,
                },
                Group {
                    id: Identifier::new_unchecked("B"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![
                        Identifier::new_unchecked("review"),
                        Identifier::new_unchecked("gate_b"),
                    ],
                    child_group_ids: vec![],
                    span,
                },
                Group {
                    id: Identifier::new_unchecked("C"),
                    label: "C".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![
                        Identifier::new_unchecked("approve"),
                        Identifier::new_unchecked("end"),
                    ],
                    child_group_ids: vec![],
                    span,
                },
            ],
            style_decls: vec![],
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
            ..Default::default()
        };

        let result = SugiyamaV2Layout::default().compute(&diagram);

        // 检查 group 包围框不重叠
        let group_a = &result.groups["A"];
        let group_b = &result.groups["B"];
        let group_c = &result.groups["C"];

        assert!(
            !rectangles_overlap_groups(group_a, group_b),
            "A and B overlap: A={:?} B={:?}",
            group_a,
            group_b
        );
        assert!(
            !rectangles_overlap_groups(group_b, group_c),
            "B and C overlap: B={:?} C={:?}",
            group_b,
            group_c
        );
        assert!(
            !rectangles_overlap_groups(group_a, group_c),
            "A and C overlap: A={:?} C={:?}",
            group_a,
            group_c
        );
    }

    fn total_rank_cost(
        dag: &DiGraph<String, ()>,
        ranks: &HashMap<NodeIndex, i32>,
    ) -> i32 {
        dag.edge_indices()
            .map(|edge| {
                let (from, to) = dag.edge_endpoints(edge).unwrap();
                ranks[&to] - ranks[&from]
            })
            .sum()
    }

    fn layout_signature(result: &LayoutResult) -> Vec<(String, i64, i64, i64, i64)> {
        let mut nodes = result
            .nodes
            .iter()
            .map(|(id, node)| {
                (
                    id.clone(),
                    quantize(node.x),
                    quantize(node.y),
                    quantize(node.width),
                    quantize(node.height),
                )
            })
            .collect::<Vec<_>>();
        nodes.sort_by(|left, right| left.0.cmp(&right.0));
        nodes
    }

    fn quantize(value: f64) -> i64 {
        (value * 1000.0).round() as i64
    }

    fn pipeline_layout_signature(source: &str) -> Vec<(String, i64, i64, i64, i64)> {
        let raw = parse(source).unwrap();
        let prepared = prepare(raw, &StyleRequest::default()).unwrap().diagram;
        let result = SugiyamaV2Layout::default().compute(prepared.inner());
        layout_signature(&result)
    }

    fn rectangles_overlap(left: &NodeLayout, right: &NodeLayout) -> bool {
        left.x < right.x + right.width
            && left.x + left.width > right.x
            && left.y < right.y + right.height
            && left.y + left.height > right.y
    }

    fn rectangles_overlap_groups(a: &GroupLayout, b: &GroupLayout) -> bool {
        a.x < b.x + b.width
            && a.x + a.width > b.x
            && a.y < b.y + b.height
            && a.y + a.height > b.y
    }

    #[test]
    fn test_normalize_padding_syncs_groups_and_total_size() {
        // 回归 P15：normalize_layout_result_to_padding 必须同步移动 nodes + groups + total_size
        use crate::layout::LayoutResult;
        use crate::layout::LayoutHints;
        const EPS: f64 = 1e-9;

        // 场景 1：节点 min < padding，需要平移
        let mut result = LayoutResult {
            nodes: {
                let mut m = HashMap::new();
                m.insert(
                    "n1".to_string(),
                    NodeLayout { x: 2.0, y: 2.0, width: 40.0, height: 20.0 },
                );
                m
            },
            groups: {
                let mut m = HashMap::new();
                m.insert(
                    "g1".to_string(),
                    GroupLayout { x: 2.0, y: 2.0, width: 60.0, height: 40.0 },
                );
                m
            },
            edges: vec![],
            total_width: 42.0,
            total_height: 22.0,
            hints: LayoutHints::default(),
        };
        let padding = 10.0;
        postprocess::normalize_layout_result_to_padding(&mut result, padding);

        // 节点应移动 dx=8, dy=8
        let n1 = result.nodes.get("n1").unwrap();
        assert!((n1.x - 10.0).abs() < EPS, "node x should be 10, got {}", n1.x);
        assert!((n1.y - 10.0).abs() < EPS, "node y should be 10, got {}", n1.y);
        // group 应同步移动
        let g1 = result.groups.get("g1").unwrap();
        assert!((g1.x - 10.0).abs() < EPS, "group x should be 10, got {}", g1.x);
        assert!((g1.y - 10.0).abs() < EPS, "group y should be 10, got {}", g1.y);
        // total_width/height 应各 +8
        assert!((result.total_width - 50.0).abs() < EPS, "total_width should be 50, got {}", result.total_width);
        assert!((result.total_height - 30.0).abs() < EPS, "total_height should be 30, got {}", result.total_height);

        // 场景 2：节点已 >= padding，不应移动
        let mut result2 = LayoutResult {
            nodes: {
                let mut m = HashMap::new();
                m.insert(
                    "n1".to_string(),
                    NodeLayout { x: 20.0, y: 20.0, width: 40.0, height: 20.0 },
                );
                m
            },
            groups: {
                let mut m = HashMap::new();
                m.insert(
                    "g1".to_string(),
                    GroupLayout { x: 20.0, y: 20.0, width: 60.0, height: 40.0 },
                );
                m
            },
            edges: vec![],
            total_width: 60.0,
            total_height: 40.0,
            hints: LayoutHints::default(),
        };
        postprocess::normalize_layout_result_to_padding(&mut result2, padding);
        let n1 = result2.nodes.get("n1").unwrap();
        assert!((n1.x - 20.0).abs() < EPS, "node should not move, x={}", n1.x);
        assert!((n1.y - 20.0).abs() < EPS, "node should not move, y={}", n1.y);
        let g1 = result2.groups.get("g1").unwrap();
        assert!((g1.x - 20.0).abs() < EPS, "group should not move, x={}", g1.x);
        assert!((result2.total_width - 60.0).abs() < EPS, "total_width should not change");
    }

    /// V3b：n.user-auth 同层 db+cache 挂到 auth → 组质心对齐；auth/gateway/client 冻结。
    #[test]
    fn v3b_user_auth_pendants_pack_under_auth() {
        let source = include_str!("../../../../../../showcase/flowchart/product.user-auth.pgm");
        let output = crate::pipeline::parse_prepare_validate(
            source,
            &StyleRequest::default(),
        );
        let prepared = output.diagram.expect("valid diagram");
        let layout = crate::layout::compute_layout_with_plan(
            prepared.inner(),
            prepared.layout_plan(),
        )
        .expect("layout");

        let cx = |id: &str| {
            let n = layout.nodes.get(id).unwrap();
            n.x + n.width / 2.0
        };
        let auth = cx("auth");
        let db = cx("db");
        let cache = cx("cache");
        let centroid = (db + cache) / 2.0;
        assert!(
            (centroid - auth).abs() <= 2.0,
            "db+cache 组质心应对齐 auth，auth={auth} db={db} cache={cache} centroid={centroid}"
        );
        // 同层间距 ≥ flowchart base node_gap（密度可能更大，这里只查无重叠）
        let db_n = layout.nodes.get("db").unwrap();
        let cache_n = layout.nodes.get("cache").unwrap();
        let edge_gap = if cache_n.x >= db_n.x + db_n.width {
            cache_n.x - (db_n.x + db_n.width)
        } else if db_n.x >= cache_n.x + cache_n.width {
            db_n.x - (cache_n.x + cache_n.width)
        } else {
            -1.0
        };
        assert!(
            edge_gap + 1e-6 >= preset::FLOWCHART_PRESET.node_gap,
            "db/cache 边距应 ≥ node_gap，got {edge_gap}"
        );

        // 主干相对稳定：auth 与 gateway 同心（既有链），且非 pendant
        assert!(
            (cx("gateway") - auth).abs() <= 2.0,
            "gateway 应仍与 auth 对齐"
        );
    }

    #[test]
    fn feedback_hub_same_layer_as_primary_pred() {
        // order-approval 拓扑：ISS-006 + ISS-007
        let src = r#"diagram flowchart {
    entity[start] submit "S"
    entity[process] review "R"
    entity[decision] check "C"
    entity[process] finance "F"
    entity[process] approved "A"
    entity[process] rejected "X"
    entity[end] done "D"
    submit -> review
    review -> check
    check -> finance
    check -> approved
    finance -> approved
    finance -> rejected
    approved -> done
    rejected -> submit
}"#;
        let raw = parse(src).unwrap();
        let prepared = prepare(raw, &StyleRequest::default()).unwrap().diagram;
        let result = FlowchartLayout::default().compute(prepared.inner());
        let finance = &result.nodes["finance"];
        let rejected = &result.nodes["rejected"];
        let approved = &result.nodes["approved"];
        let done = &result.nodes["done"];
        // ISS-006: rejected 与 finance 同高，且在 finance 左侧
        assert!(
            (rejected.y - finance.y).abs() < 1.0,
            "rejected should be same row as finance: rejected.y={} finance.y={}",
            rejected.y, finance.y
        );
        assert!(
            rejected.x < finance.x,
            "rejected should be left of finance: rejected.x={} finance.x={}",
            rejected.x, finance.x
        );
        // ISS-007（修订）: end 不再同层旁置，作为主链延续放在最下方
        assert!(
            done.y > approved.y,
            "done should be below approved: done.y={} approved.y={}",
            done.y, approved.y
        );
        // 主干拉直：check → finance → approved → done 垂直成一列
        let check = &result.nodes["check"];
        let check_cx = check.x + check.width / 2.0;
        let finance_cx = finance.x + finance.width / 2.0;
        let approved_cx = approved.x + approved.width / 2.0;
        let done_cx = done.x + done.width / 2.0;
        assert!(
            (finance_cx - check_cx).abs() < 1.0,
            "finance should be vertically under check: check_cx={} finance_cx={}",
            check_cx, finance_cx
        );
        assert!(
            (approved_cx - finance_cx).abs() < 1.0,
            "approved should be vertically under finance: finance_cx={} approved_cx={}",
            finance_cx, approved_cx
        );
        assert!(
            (done_cx - approved_cx).abs() < 1.0,
            "done should be vertically under approved: approved_cx={} done_cx={}",
            approved_cx, done_cx
        );
    }

    #[test]
    fn local_end_placed_below_single_pred_not_max_rank() {
        // PR 架构评审：comment -> done 与 update -> collect 为并行分支，end 应局部收束而非沉底。
        let src = include_str!("../../../../../../showcase/flowchart/demo.pr-architecture-review.pgm");
        let raw = parse(src).unwrap();
        let prepared = prepare(raw, &StyleRequest::default()).unwrap().diagram;
        let result = FlowchartLayout::default().compute(prepared.inner());
        let comment = &result.nodes["comment"];
        let collect = &result.nodes["collect"];
        let done = &result.nodes["done"];
        let generate = &result.nodes["generate"];

        assert!(
            (done.y - collect.y).abs() < 1.0,
            "done should share rank with collect (branch locals): done.y={} collect.y={}",
            done.y,
            collect.y
        );
        assert!(
            done.y < generate.y,
            "done should not sink below generate loop body: done.y={} generate.y={}",
            done.y,
            generate.y
        );
        let comment_cx = comment.x + comment.width / 2.0;
        let done_cx = done.x + done.width / 2.0;
        assert!(
            (done_cx - comment_cx).abs() < 1.0,
            "done should align under comment: comment_cx={} done_cx={}",
            comment_cx,
            done_cx
        );
    }
}
