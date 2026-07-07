//! 边路由阶段的分组障碍上下文（构建一次，scorer / path / 后处理共用）。

use std::collections::{HashMap, HashSet};

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::{GroupLayout, LayoutResult};

use super::border_shell::group_segment_violates_border_shell;
use super::config::GroupRoutingProfile;
use super::constants::{GROUP_BORDER_SHELL_PAD, EPS};
use super::corridor::{merge_corridors, CorridorAxis, GroupCorridor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiblingOrientation {
    Horizontal,
    Vertical,
}

const GROUP_GAP_THRESHOLD: f64 = 48.0;

/// 布局阶段产出的分组路由提示。
#[derive(Debug, Clone)]
pub struct GroupRoutingHints {
    pub corridors: Vec<GroupCorridor>,
    pub border_shell_pad: f64,
}

impl Default for GroupRoutingHints {
    fn default() -> Self {
        Self {
            corridors: Vec::new(),
            border_shell_pad: GROUP_BORDER_SHELL_PAD,
        }
    }
}

impl GroupRoutingHints {
    pub fn from_groups(groups: &HashMap<String, GroupLayout>) -> Self {
        Self {
            corridors: super::corridor::build_corridors_from_groups(groups),
            border_shell_pad: GROUP_BORDER_SHELL_PAD,
        }
    }

    pub fn with_corridors(corridors: Vec<GroupCorridor>) -> Self {
        Self {
            corridors,
            border_shell_pad: GROUP_BORDER_SHELL_PAD,
        }
    }
}

/// 边路由阶段的分组障碍上下文。
#[derive(Debug, Clone)]
pub struct GroupRoutingContext {
    pub groups: HashMap<String, GroupLayout>,
    pub node_to_groups: HashMap<String, Vec<String>>,
    pub border_shell_pad: f64,
    pub stub_clearance: f64,
    pub corridor_misalignment_penalty: f64,
    pub repulse_max_rounds: usize,
    pub corridors: Vec<GroupCorridor>,
    pub node_leaf_group: HashMap<String, String>,
    pub sibling_sets: Vec<Vec<String>>,
    pub sibling_orientation: HashMap<(String, String), SiblingOrientation>,
    pub group_ancestors: HashMap<String, Vec<String>>,
}

impl GroupRoutingContext {
    pub fn from_layout(diagram: &Diagram, result: &LayoutResult, algo: &str) -> Self {
        let profile = GroupRoutingProfile::for_algo(algo);
        let node_to_groups = build_node_to_groups(diagram);
        let corridors = if let Some(hints) = &result.hints.group_routing {
            merge_corridors(&hints.corridors, &result.groups)
        } else {
            super::corridor::build_corridors_from_groups(&result.groups)
        };
        let (node_leaf_group, sibling_sets, sibling_orientation, group_ancestors) =
            build_group_hierarchy(diagram, &result.groups);
        Self {
            groups: result.groups.clone(),
            node_to_groups,
            border_shell_pad: profile.border_shell_pad,
            stub_clearance: profile.stub_clearance,
            corridor_misalignment_penalty: profile.corridor_misalignment_penalty,
            repulse_max_rounds: profile.repulse_max_rounds,
            corridors,
            node_leaf_group,
            sibling_sets,
            sibling_orientation,
            group_ancestors,
        }
    }

    pub fn routing_hints(&self) -> GroupRoutingHints {
        GroupRoutingHints {
            corridors: self.corridors.clone(),
            border_shell_pad: self.border_shell_pad,
        }
    }

    pub fn node_leaf_group(&self, node_id: &str) -> Option<&str> {
        self.node_leaf_group.get(node_id).map(|s| s.as_str())
    }

    pub fn is_same_leaf_group(&self, a: &str, b: &str) -> bool {
        match (self.node_leaf_group(a), self.node_leaf_group(b)) {
            (Some(ga), Some(gb)) => ga == gb,
            (None, None) => true,
            _ => false,
        }
    }

    pub fn sibling_orientation(&self, ga: &str, gb: &str) -> Option<SiblingOrientation> {
        let key = if ga <= gb {
            (ga.to_string(), gb.to_string())
        } else {
            (gb.to_string(), ga.to_string())
        };
        self.sibling_orientation.get(&key).copied()
    }

    pub fn corridor_between_groups(
        &self,
        ga: &str,
        gb: &str,
        axis: CorridorAxis,
    ) -> Option<&GroupCorridor> {
        self.corridors.iter().find(|c| {
            c.axis == axis
                && ((c.group_a == ga && c.group_b == gb) || (c.group_a == gb && c.group_b == ga))
        })
    }

    pub fn endpoint_group_set<'a>(
        &'a self,
        from_id: &str,
        to_id: &str,
    ) -> HashSet<&'a str> {
        let mut set: HashSet<&'a str> = self
            .node_to_groups
            .get(from_id)
            .into_iter()
            .flatten()
            .chain(self.node_to_groups.get(to_id).into_iter().flatten())
            .map(|s| s.as_str())
            .collect();
        if let Some(ancestors) = self.node_leaf_group(from_id).and_then(|g| self.group_ancestors.get(g)) {
            for a in ancestors {
                set.insert(a.as_str());
            }
        }
        if let Some(ancestors) = self.node_leaf_group(to_id).and_then(|g| self.group_ancestors.get(g)) {
            for a in ancestors {
                set.insert(a.as_str());
            }
        }
        set
    }

    pub fn segment_violates_border_shell(
        &self,
        path: &[Point],
        segment_index: usize,
        gl: &GroupLayout,
        endpoint_in_group: bool,
    ) -> bool {
        group_segment_violates_border_shell(
            path,
            segment_index,
            gl,
            self.border_shell_pad,
            endpoint_in_group,
            self.stub_clearance,
        )
    }
}

/// 从 diagram 确定性构建 node → group id 列表。
pub fn build_node_to_groups(diagram: &Diagram) -> HashMap<String, Vec<String>> {
    let mut node_to_groups: HashMap<String, Vec<String>> = HashMap::new();
    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        let mut member_ids: Vec<String> = group
            .entity_ids
            .iter()
            .map(|id| id.as_str().to_string())
            .collect();
        member_ids.sort();
        for member in member_ids {
            node_to_groups.entry(member).or_default().push(gid.clone());
        }
    }
    for groups_list in node_to_groups.values_mut() {
        groups_list.sort();
    }
    node_to_groups
}

fn build_group_hierarchy(
    diagram: &Diagram,
    groups: &HashMap<String, GroupLayout>,
) -> (
    HashMap<String, String>,
    Vec<Vec<String>>,
    HashMap<(String, String), SiblingOrientation>,
    HashMap<String, Vec<String>>,
) {
    let mut parent_of: HashMap<String, String> = HashMap::new();
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut group_depth: HashMap<String, u8> = HashMap::new();

    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        group_depth.insert(gid.clone(), group.depth);
        if let Some(pid) = &group.parent_id {
            let pid_str = pid.as_str().to_string();
            parent_of.insert(gid.clone(), pid_str.clone());
            children_of.entry(pid_str).or_default().push(gid);
        }
    }

    for children in children_of.values_mut() {
        children.sort();
    }

    let mut sibling_sets: Vec<Vec<String>> = Vec::new();
    for children in children_of.values() {
        if children.len() >= 2 {
            sibling_sets.push(children.clone());
        }
    }
    sibling_sets.sort();

    let mut sibling_orientation: HashMap<(String, String), SiblingOrientation> = HashMap::new();
    for siblings in &sibling_sets {
        for i in 0..siblings.len() {
            for j in (i + 1)..siblings.len() {
                let ga = &siblings[i];
                let gb = &siblings[j];
                if let (Some(gla), Some(glb)) = (groups.get(ga), groups.get(gb)) {
                    let ox = range_overlap(gla.x, gla.x + gla.width, glb.x, glb.x + glb.width);
                    let oy = range_overlap(gla.y, gla.y + gla.height, glb.y, glb.y + glb.height);
                    let min_h = gla.height.min(glb.height);
                    let min_w = gla.width.min(glb.width);

                    let orient = if oy >= 0.5 * min_h - EPS && ox <= GROUP_GAP_THRESHOLD {
                        SiblingOrientation::Horizontal
                    } else if ox >= 0.5 * min_w - EPS && oy <= GROUP_GAP_THRESHOLD {
                        SiblingOrientation::Vertical
                    } else {
                        let dx = (gla.x + gla.width / 2.0) - (glb.x + glb.width / 2.0);
                        let dy = (gla.y + gla.height / 2.0) - (glb.y + glb.height / 2.0);
                        if dy.abs() >= dx.abs() {
                            SiblingOrientation::Vertical
                        } else {
                            SiblingOrientation::Horizontal
                        }
                    };
                    let key = if ga <= gb {
                        (ga.clone(), gb.clone())
                    } else {
                        (gb.clone(), ga.clone())
                    };
                    sibling_orientation.insert(key, orient);
                }
            }
        }
    }

    let mut group_ancestors: HashMap<String, Vec<String>> = HashMap::new();
    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        let mut ancestors = Vec::new();
        let mut current = parent_of.get(&gid).cloned();
        while let Some(p) = current {
            ancestors.push(p.clone());
            current = parent_of.get(&p).cloned();
        }
        group_ancestors.insert(gid, ancestors);
    }

    let mut node_leaf_group: HashMap<String, String> = HashMap::new();
    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        for eid in &group.entity_ids {
            let eid_str = eid.as_str().to_string();
            let existing_depth = node_leaf_group
                .get(&eid_str)
                .and_then(|g| group_depth.get(g))
                .copied()
                .unwrap_or(0);
            if group.depth >= existing_depth {
                node_leaf_group.insert(eid_str, gid.clone());
            }
        }
    }

    (node_leaf_group, sibling_sets, sibling_orientation, group_ancestors)
}

fn range_overlap(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> f64 {
    (a_max.min(b_max) - a_min.max(b_min)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeMap, Diagram, Entity, Group, Identifier, Span};
    use crate::layout::{GroupLayout, LayoutHints, LayoutResult};
    use crate::types::DiagramType;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn node_to_groups_is_deterministic() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![Entity {
                id: Identifier::new_unchecked("n"),
                label: "n".into(),
                attributes: AttributeMap::default(),
                group_id: None,
                span: span(),
            }],
            groups: vec![Group {
                id: Identifier::new_unchecked("g"),
                label: "G".into(),
                attributes: AttributeMap::default(),
                parent_id: None,
                depth: 0,
                entity_ids: vec![Identifier::new_unchecked("n")],
                child_group_ids: vec![],
                span: span(),
            }],
            relations: vec![],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo {
                file: None,
                line_count: 0,
            },
            ..Default::default()
        };
        let map = build_node_to_groups(&diagram);
        assert_eq!(map.get("n").map(|v| v.as_slice()), Some(&["g".to_string()][..]));
    }

    #[test]
    fn from_layout_merges_injected_corridors() {
        let diagram = Diagram::default();
        let mut groups = HashMap::new();
        groups.insert(
            "a".into(),
            GroupLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
        );
        groups.insert(
            "b".into(),
            GroupLayout {
                x: 140.0,
                y: 0.0,
                width: 80.0,
                height: 80.0,
            },
        );
        let hints = GroupRoutingHints::with_corridors(vec![super::super::corridor::GroupCorridor {
            axis: super::super::corridor::CorridorAxis::Vertical,
            coord: 130.0,
            span_min: 0.0,
            span_max: 80.0,
            group_a: "a".into(),
            group_b: "b".into(),
        }]);
        let result = LayoutResult {
            nodes: HashMap::new(),
            groups,
            edges: vec![],
            total_width: 0.0,
            total_height: 0.0,
            hints: LayoutHints {
                group_routing: Some(hints),
                ..Default::default()
            },
        };
        let ctx = GroupRoutingContext::from_layout(&diagram, &result, "flowchart");
        assert_eq!(ctx.corridors.len(), 1);
        assert!((ctx.corridors[0].coord - 130.0).abs() < 0.01);
        assert_eq!(ctx.corridor_misalignment_penalty, 80.0);
    }
}
