//! EGB：布局前边感知 gutter 预算（architecture 专用）。

use std::collections::BTreeMap;

use crate::ast::Diagram;
use crate::layout::edge::common::label_avoidance::estimate_label_width;
use crate::layout::group::constants::{GROUP_BORDER_SHELL_PAD, PORT_STUB_CLEARANCE};
use crate::layout::group::hierarchy::{
    ancestor_set_excluding_self, build_group_hierarchy, lowest_common_ancestor,
    path_from_leaf_up_to_including, GroupHierarchy,
};
use crate::layout::group::constants::EPS;
use crate::layout::node::common::group_bounds::{GutterSide, SideGutter};
use crate::layout::{GroupLayout, NodeLayout};
use std::collections::HashMap;

/// 与走廊车道间距对齐（见 `corridor_route::CORRIDOR_LANE_PITCH`）。
const LANE_PITCH: f64 = 18.0;
const GUTTER_MAX: f64 = 56.0;
const SOFT_PRIMARY: f64 = 0.7;
const SOFT_SECONDARY: f64 = 0.3;

#[derive(Debug, Clone, Copy, Default)]
struct LaneDemand {
    lanes: f64,
    label_w: f64,
}

/// 估计每个 group 四侧 gutter 预算。
pub fn estimate_side_gutters(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    base_groups: &HashMap<String, GroupLayout>,
    hierarchy: &GroupHierarchy,
) -> BTreeMap<String, SideGutter> {
    let mut demand: BTreeMap<(String, GutterSide), LaneDemand> = BTreeMap::new();

    let mut edges: Vec<(String, String, f64)> = diagram
        .relations
        .iter()
        .map(|r| {
            let from = r.from.as_str().to_string();
            let to = r.to.as_str().to_string();
            let label_w = r
                .label
                .as_ref()
                .map(|t| estimate_label_width(t))
                .unwrap_or(0.0);
            (from, to, label_w)
        })
        .collect();
    edges.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    for (from_id, to_id, label_w) in edges {
        let Some(cu) = node_center(nodes, &from_id) else {
            continue;
        };
        let Some(cv) = node_center(nodes, &to_id) else {
            continue;
        };

        let gu = hierarchy.node_leaf_group.get(&from_id).map(|s| s.as_str());
        let gv = hierarchy.node_leaf_group.get(&to_id).map(|s| s.as_str());

        if gu == gv {
            continue;
        }

        if gu.is_some() && gv.is_some() {
            let gu = gu.unwrap();
            let gv = gv.unwrap();
            if let Some(lca) = lowest_common_ancestor(gu, gv, &hierarchy.parent_of) {
                for g in path_from_leaf_up_to_including(gu, &lca, &hierarchy.parent_of) {
                    accrue_on_group(
                        &mut demand,
                        &g,
                        cu,
                        cv,
                        base_groups,
                        label_w,
                        1.0,
                    );
                }
                for g in path_from_leaf_up_to_including(gv, &lca, &hierarchy.parent_of) {
                    accrue_on_group(
                        &mut demand,
                        &g,
                        cv,
                        cu,
                        base_groups,
                        label_w,
                        1.0,
                    );
                }
            }
        }

        if let (Some(gu), Some(gv)) = (gu, gv) {
            let exit_u = exit_groups(gu, gv, hierarchy);
            let exit_v = exit_groups(gv, gu, hierarchy);
            for g in exit_u {
                accrue_on_group(&mut demand, &g, cu, cv, base_groups, label_w, 1.0);
            }
            for g in exit_v {
                accrue_on_group(&mut demand, &g, cv, cu, base_groups, label_w, 1.0);
            }
        }
    }

    demand_to_side_gutters(demand)
}

/// 便捷入口：内部构建 hierarchy。
pub fn estimate_side_gutters_with_hierarchy(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    base_groups: &HashMap<String, GroupLayout>,
) -> BTreeMap<String, SideGutter> {
    let hierarchy = build_group_hierarchy(diagram, base_groups);
    estimate_side_gutters(diagram, nodes, base_groups, &hierarchy)
}

fn exit_groups(from_leaf: &str, to_leaf: &str, hierarchy: &GroupHierarchy) -> Vec<String> {
    let mut to_set = ancestor_set_excluding_self(to_leaf, &hierarchy.group_ancestors);
    to_set.insert(to_leaf.to_string());

    let mut out = Vec::new();
    let mut cur = Some(from_leaf.to_string());
    while let Some(g) = cur {
        if !to_set.contains(&g) {
            out.push(g.clone());
        }
        cur = hierarchy.parent_of.get(&g).cloned();
    }
    out.sort();
    out
}

fn accrue_on_group(
    demand: &mut BTreeMap<(String, GutterSide), LaneDemand>,
    group_id: &str,
    from_center: (f64, f64),
    to_center: (f64, f64),
    base_groups: &HashMap<String, GroupLayout>,
    label_w: f64,
    weight: f64,
) {
    let Some(gl) = base_groups.get(group_id) else {
        return;
    };
    for (side, frac) in soft_dominant_side(from_center, gl, to_center) {
        let entry = demand
            .entry((group_id.to_string(), side))
            .or_default();
        entry.lanes += weight * frac;
        if matches!(side, GutterSide::Left | GutterSide::Right) {
            entry.label_w = entry.label_w.max(label_w * frac);
        }
    }
}

fn demand_to_side_gutters(
    demand: BTreeMap<(String, GutterSide), LaneDemand>,
) -> BTreeMap<String, SideGutter> {
    let mut out: BTreeMap<String, SideGutter> = BTreeMap::new();
    for ((gid, side), d) in demand {
        let lanes_ceil = d.lanes.ceil().max(1.0) as u32;
        let gutter = (GROUP_BORDER_SHELL_PAD
            + PORT_STUB_CLEARANCE
            + lanes_ceil.saturating_sub(1) as f64 * LANE_PITCH
            + d.label_w)
            .min(GUTTER_MAX);
        out.entry(gid).or_default().set_side(side, gutter);
    }
    // Iteration 2：左右 / 上下对称优先，避免「一侧挤、一侧空」
    for gutter in out.values_mut() {
        let lr = gutter.left.max(gutter.right);
        gutter.left = lr;
        gutter.right = lr;
        let tb = gutter.top.max(gutter.bottom);
        gutter.top = tb;
        gutter.bottom = tb;
    }
    out
}

fn node_center(nodes: &HashMap<String, NodeLayout>, id: &str) -> Option<(f64, f64)> {
    nodes.get(id).map(|n| (n.x + n.width / 2.0, n.y + n.height / 2.0))
}

fn soft_dominant_side(
    from_center: (f64, f64),
    group: &GroupLayout,
    to_center: (f64, f64),
) -> [(GutterSide, f64); 2] {
    let cx = group.x + group.width / 2.0;
    let cy = group.y + group.height / 2.0;
    let dx = to_center.0 - cx;
    let dy = to_center.1 - cy;
    let ax = dx.abs();
    let ay = dy.abs();
    let total = ax + ay;
    if total < EPS {
        return [
            (GutterSide::Left, 0.5),
            (GutterSide::Right, 0.5),
        ];
    }
    if ax >= ay {
        let primary = if dx < 0.0 {
            GutterSide::Left
        } else {
            GutterSide::Right
        };
        let secondary = if dy < 0.0 {
            GutterSide::Top
        } else {
            GutterSide::Bottom
        };
        [(primary, SOFT_PRIMARY), (secondary, SOFT_SECONDARY)]
    } else {
        let primary = if dy < 0.0 {
            GutterSide::Top
        } else {
            GutterSide::Bottom
        };
        let secondary = if dx < 0.0 {
            GutterSide::Left
        } else {
            GutterSide::Right
        };
        [(primary, SOFT_PRIMARY), (secondary, SOFT_SECONDARY)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Diagram, Entity, Group, Identifier, Relation, Span};
    use crate::layout::node::common::group_bounds::{compute_group_bounds, GroupPadding};
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

    fn nested_cloud_diagram() -> Diagram {
        Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities: vec![
                entity("biz", "private_subnet"),
                entity("mq", "data_subnet"),
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("biz"),
                to: Identifier::new_unchecked("mq"),
                arrow: ArrowType::Active,
                label: Some("event".to_string()),
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: span(),
            }],
            groups: vec![
                Group {
                    id: Identifier::new_unchecked("cloud"),
                    label: "cloud".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: None,
                    depth: 0,
                    entity_ids: vec![],
                    child_group_ids: vec![
                        Identifier::new_unchecked("private_subnet"),
                        Identifier::new_unchecked("data_subnet"),
                    ],
                    span: span(),
                },
                Group {
                    id: Identifier::new_unchecked("private_subnet"),
                    label: "private".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: Some(Identifier::new_unchecked("cloud")),
                    depth: 1,
                    entity_ids: vec![Identifier::new_unchecked("biz")],
                    child_group_ids: vec![],
                    span: span(),
                },
                Group {
                    id: Identifier::new_unchecked("data_subnet"),
                    label: "data".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: Some(Identifier::new_unchecked("cloud")),
                    depth: 1,
                    entity_ids: vec![Identifier::new_unchecked("mq")],
                    child_group_ids: vec![],
                    span: span(),
                },
            ],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 0,
            },
            ..Default::default()
        }
    }

    #[test]
    fn lca_cross_subnet_increases_cloud_gutter() {
        let diagram = nested_cloud_diagram();
        let nodes = HashMap::from([
            (
                "biz".to_string(),
                NodeLayout {
                    x: 80.0,
                    y: 200.0,
                    width: 100.0,
                    height: 40.0,
                    ..Default::default()
                },
            ),
            (
                "mq".to_string(),
                NodeLayout {
                    x: 80.0,
                    y: 80.0,
                    width: 100.0,
                    height: 40.0,
                    ..Default::default()
                },
            ),
        ]);
        let pad = GroupPadding::architecture_v2();
        let base_groups = compute_group_bounds(&diagram, &nodes, pad);
        let gutters = estimate_side_gutters_with_hierarchy(&diagram, &nodes, &base_groups);
        let cloud = gutters.get("cloud").copied().unwrap_or_default();
        assert!(
            cloud.left > pad.left || cloud.right > pad.right || cloud.top > pad.top,
            "cloud should have non-zero gutter budget, got {:?}",
            cloud
        );
    }

    #[test]
    fn same_leaf_edge_produces_no_gutter() {
        let diagram = Diagram {
            diagram_type: DiagramType::Architecture,
            attributes: vec![],
            entities: vec![entity("a", "g1"), entity("b", "g1")],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: span(),
            }],
            groups: vec![Group {
                id: Identifier::new_unchecked("g1"),
                label: "g1".to_string(),
                attributes: AttributeMap::default(),
                parent_id: None,
                depth: 0,
                entity_ids: vec![
                    Identifier::new_unchecked("a"),
                    Identifier::new_unchecked("b"),
                ],
                child_group_ids: vec![],
                span: span(),
            }],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 0,
            },
            ..Default::default()
        };
        let nodes = HashMap::from([
            (
                "a".to_string(),
                NodeLayout {
                    x: 0.0,
                    y: 0.0,
                    width: 50.0,
                    height: 30.0,
                    ..Default::default()
                },
            ),
            (
                "b".to_string(),
                NodeLayout {
                    x: 80.0,
                    y: 0.0,
                    width: 50.0,
                    height: 30.0,
                    ..Default::default()
                },
            ),
        ]);
        let pad = GroupPadding::uniform(20.0, 16.0);
        let base = compute_group_bounds(&diagram, &nodes, pad);
        let gutters = estimate_side_gutters_with_hierarchy(&diagram, &nodes, &base);
        assert!(gutters.is_empty());
    }

    #[test]
    fn estimate_is_deterministic() {
        let diagram = nested_cloud_diagram();
        let nodes = HashMap::from([
            (
                "biz".to_string(),
                NodeLayout {
                    x: 80.0,
                    y: 200.0,
                    width: 100.0,
                    height: 40.0,
                    ..Default::default()
                },
            ),
            (
                "mq".to_string(),
                NodeLayout {
                    x: 80.0,
                    y: 80.0,
                    width: 100.0,
                    height: 40.0,
                    ..Default::default()
                },
            ),
        ]);
        let pad = GroupPadding::architecture_v2();
        let base = compute_group_bounds(&diagram, &nodes, pad);
        let a = estimate_side_gutters_with_hierarchy(&diagram, &nodes, &base);
        let b = estimate_side_gutters_with_hierarchy(&diagram, &nodes, &base);
        assert_eq!(a, b);
    }
}
