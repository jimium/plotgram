//! 路由 / 渲染权威分组包围框。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::node::common::group_bounds::{self, GroupPadding};
use crate::layout::{GroupLayout, NodeLayout};

/// 按布局算法选择 `GroupPadding` 配置。
pub fn routing_group_padding(algo: &str, group_padding: f64) -> GroupPadding {
    match algo {
        "architecture" => GroupPadding {
            x: 28.0,
            y_top: 48.0,
            x_delta: 56.0,
            y_delta: 76.0,
        },
        "force-directed" => GroupPadding::force_directed(),
        _ => GroupPadding::uniform(group_padding, 16.0),
    }
}

/// 路由 / 渲染权威分组包围框（与 `compute_group_bounds` 语义一致）。
pub fn finalize_routing_groups(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    algo: &str,
    group_padding: f64,
) -> HashMap<String, GroupLayout> {
    let leaf_padding = routing_group_padding(algo, group_padding);
    group_bounds::compute_group_bounds(diagram, nodes, leaf_padding)
}

/// debug 构建：组成员节点应落在 group rect 内（容差内）。
#[cfg(debug_assertions)]
pub fn debug_assert_routing_groups_contain_members(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
) {
    const MEMBER_EPS: f64 = 2.0;
    for group in &diagram.groups {
        let Some(gl) = groups.get(group.id.as_str()) else {
            continue;
        };
        for eid in &group.entity_ids {
            let Some(nl) = nodes.get(eid.as_str()) else {
                continue;
            };
            debug_assert!(
                nl.x >= gl.x - MEMBER_EPS
                    && nl.y >= gl.y - MEMBER_EPS
                    && nl.x + nl.width <= gl.x + gl.width + MEMBER_EPS
                    && nl.y + nl.height <= gl.y + gl.height + MEMBER_EPS,
                "entity {} outside routing group {} rect",
                eid.as_str(),
                group.id.as_str()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeMap, Diagram, Entity, Group, Identifier, Span};
    use crate::layout::node::common::group_bounds::{self, GroupPadding};
    use crate::layout::{NodeLayout};
    use crate::types::DiagramType;

    fn span() -> Span {
        Span::dummy()
    }

    fn entity(id: &str, group: &str) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            group_id: Some(Identifier::new_unchecked(group)),
            span: span(),
        }
    }

    fn group(id: &str, entity_ids: Vec<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            parent_id: None,
            depth: 0,
            entity_ids: entity_ids
                .into_iter()
                .map(|e| Identifier::new_unchecked(e))
                .collect(),
            child_group_ids: vec![],
            span: span(),
        }
    }

    #[test]
    fn finalize_matches_compute_group_bounds() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![entity("a", "g1"), entity("b", "g1")],
            relations: vec![],
            groups: vec![group("g1", vec!["a", "b"])],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 0,
            },
            ..Default::default()
        };
        let mut nodes = HashMap::new();
        nodes.insert(
            "a".to_string(),
            NodeLayout {
                x: 50.0,
                y: 60.0,
                width: 80.0,
                height: 40.0,
                ..Default::default()
            },
        );
        nodes.insert(
            "b".to_string(),
            NodeLayout {
                x: 50.0,
                y: 120.0,
                width: 80.0,
                height: 40.0,
                ..Default::default()
            },
        );
        let padding = 28.0;
        let gp = GroupPadding::uniform(padding, 16.0);
        let expected = group_bounds::compute_group_bounds(&diagram, &nodes, gp);
        let actual = finalize_routing_groups(&diagram, &nodes, "flowchart", padding);
        assert_eq!(expected.len(), actual.len());
        for (id, gl) in &expected {
            let other = actual.get(id).expect("missing group");
            assert!((gl.x - other.x).abs() < 0.01, "x mismatch for {id}");
            assert!((gl.y - other.y).abs() < 0.01, "y mismatch for {id}");
            assert!((gl.width - other.width).abs() < 0.01, "w mismatch for {id}");
            assert!((gl.height - other.height).abs() < 0.01, "h mismatch for {id}");
        }
    }
}
