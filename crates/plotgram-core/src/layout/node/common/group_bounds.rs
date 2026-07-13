//! 分组包围框计算

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::Diagram;
use crate::layout::{
    GroupLayout, GroupLayoutWarning, GroupLayoutWarningKind, NodeLayout,
};

/// 分组包围框的四侧内边距（各侧独立，支持非对称 gutter）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GroupPadding {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl GroupPadding {
    /// 统一 padding（四侧相同；`top` 额外含标题区 `header_height`）。
    pub fn uniform(padding: f64, header_height: f64) -> Self {
        Self {
            left: padding,
            right: padding,
            top: padding + header_height,
            bottom: padding,
        }
    }

    /// architecture_v2 默认非对称 padding。
    ///
    /// Phase D：收紧组壳（原 28/48/28 → 20/36/20），标题区仍由 `top` 覆盖
    ///（`GROUP_LABEL_HEIGHT=20`，内容顶隙 ≥16）。
    pub fn architecture_v2() -> Self {
        Self {
            left: 20.0,
            right: 20.0,
            top: 36.0,
            bottom: 20.0,
        }
    }

    /// force-directed 布局的分组内边距
    pub fn force_directed() -> Self {
        Self {
            left: 20.0,
            right: 20.0,
            top: 36.0,
            bottom: 20.0,
        }
    }

    pub fn horizontal_extent(&self) -> f64 {
        self.left + self.right
    }

    pub fn vertical_extent(&self) -> f64 {
        self.top + self.bottom
    }

    /// 逐侧取与 `SideGutter` 的较大值（base 与 EGB budget 合成）。
    pub fn max_per_side(self, budget: SideGutter) -> Self {
        Self {
            left: self.left.max(budget.left),
            right: self.right.max(budget.right),
            top: self.top.max(budget.top),
            bottom: self.bottom.max(budget.bottom),
        }
    }
}

/// 单个 group 的四侧 gutter 预算（EGB 产出）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SideGutter {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl SideGutter {
    pub fn set_side(&mut self, side: GutterSide, value: f64) {
        let v = self.get_side_mut(side);
        *v = (*v).max(value);
    }

    pub fn get_side(self, side: GutterSide) -> f64 {
        match side {
            GutterSide::Left => self.left,
            GutterSide::Right => self.right,
            GutterSide::Top => self.top,
            GutterSide::Bottom => self.bottom,
        }
    }

    fn get_side_mut(&mut self, side: GutterSide) -> &mut f64 {
        match side {
            GutterSide::Left => &mut self.left,
            GutterSide::Right => &mut self.right,
            GutterSide::Top => &mut self.top,
            GutterSide::Bottom => &mut self.bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GutterSide {
    Left,
    Right,
    Top,
    Bottom,
}

/// 分组的有效成员实体 id（优先 `group.entity_ids`，否则从 `entity.group_id` 推导）。
fn effective_entity_ids(group: &crate::ast::Group, diagram: &Diagram) -> Vec<String> {
    if !group.entity_ids.is_empty() {
        return group.entity_ids.iter().map(|id| id.to_string()).collect();
    }
    let gid = group.id.as_str();
    let mut ids: Vec<String> = diagram
        .entities
        .iter()
        .filter(|e| e.group_id.as_ref().is_some_and(|g| g.as_str() == gid))
        .map(|e| e.id.to_string())
        .collect();
    ids.sort();
    ids
}

fn is_container_group(group: &crate::ast::Group, diagram: &Diagram) -> bool {
    effective_entity_ids(group, diagram).is_empty()
}

fn resolve_padding(
    leaf_padding: GroupPadding,
    container_padding: GroupPadding,
    group: &crate::ast::Group,
    diagram: &Diagram,
    side_gutters: Option<&BTreeMap<String, SideGutter>>,
) -> GroupPadding {
    let base = if is_container_group(group, diagram) {
        container_padding
    } else {
        leaf_padding
    };
    let budget = side_gutters
        .and_then(|m| m.get(group.id.as_str()))
        .copied()
        .unwrap_or_default();
    let mut merged = base.max_per_side(budget);
    // 顶层叶子：仅当左右 EGB 都达到完整出口量级才水平对称；
    // 单侧有边不把空侧拉齐（避免「假设很多边」预留）。
    if group.parent_id.is_none() && !is_container_group(group, diagram) {
        let both_horizontal = budget.left >= base.left && budget.right >= base.right
            && budget.left > f64::EPSILON
            && budget.right > f64::EPSILON;
        if both_horizontal {
            let h = merged.left.max(merged.right);
            merged.left = h;
            merged.right = h;
        }
    }
    merged
}

/// 计算分组的包围框
pub fn compute_group_bounds(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    leaf_padding: GroupPadding,
) -> HashMap<String, GroupLayout> {
    compute_group_bounds_with_side_gutters(
        diagram,
        nodes,
        leaf_padding,
        container_padding(leaf_padding),
        None,
    )
}

/// 与 [`compute_group_bounds`] 相同，但叠加 EGB 产出的逐组 `side_gutters`。
pub fn compute_group_bounds_with_side_gutters(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    leaf_padding: GroupPadding,
    container_pad: GroupPadding,
    side_gutters: Option<&BTreeMap<String, SideGutter>>,
) -> HashMap<String, GroupLayout> {
    compute_group_bounds_inner(
        diagram,
        nodes,
        leaf_padding,
        container_pad,
        side_gutters,
    )
}

/// 容器组 padding（由叶子 padding 推导，供 GroupFrame 重算）。
pub fn container_padding_for_leaf(leaf: GroupPadding) -> GroupPadding {
    container_padding(leaf)
}

/// 容器组（无直接实体）的 padding：水平减半，垂直保留标题区但 padding 减半。
fn container_padding(leaf: GroupPadding) -> GroupPadding {
    GroupPadding {
        left: leaf.left * 0.5,
        right: leaf.right * 0.5,
        top: leaf.top * 0.6,
        bottom: leaf.bottom * 0.5,
    }
}

/// 与 [`compute_group_bounds`] 相同，但允许显式指定容器组 padding。
pub fn compute_group_bounds_with_container_padding(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    leaf_padding: GroupPadding,
    container_padding: GroupPadding,
) -> HashMap<String, GroupLayout> {
    compute_group_bounds_inner(
        diagram,
        nodes,
        leaf_padding,
        container_padding,
        None,
    )
}

fn compute_group_bounds_inner(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    leaf_padding: GroupPadding,
    container_padding: GroupPadding,
    side_gutters: Option<&BTreeMap<String, SideGutter>>,
) -> HashMap<String, GroupLayout> {
    let mut sorted_groups: Vec<&crate::ast::Group> = diagram.groups.iter().collect();
    sorted_groups.sort_by(|a, b| a.depth.cmp(&b.depth).reverse());

    let mut groups: HashMap<String, GroupLayout> = HashMap::new();
    for group in &sorted_groups {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;

        for eid in effective_entity_ids(group, diagram) {
            if let Some(nl) = nodes.get(eid.as_str()) {
                min_x = min_x.min(nl.x);
                min_y = min_y.min(nl.y);
                max_x = max_x.max(nl.x + nl.width);
                max_y = max_y.max(nl.y + nl.height);
            }
        }

        for child_gid in &group.child_group_ids {
            if let Some(child_gl) = groups.get(child_gid.as_str()) {
                min_x = min_x.min(child_gl.x);
                min_y = min_y.min(child_gl.y);
                max_x = max_x.max(child_gl.x + child_gl.width);
                max_y = max_y.max(child_gl.y + child_gl.height);
            }
        }

        if min_x < f64::MAX {
            let padding = resolve_padding(
                leaf_padding,
                container_padding,
                group,
                diagram,
                side_gutters,
            );
            groups.insert(
                group.id.as_str().to_string(),
                GroupLayout {
                    x: min_x - padding.left,
                    y: min_y - padding.top,
                    width: max_x - min_x + padding.horizontal_extent(),
                    height: max_y - min_y + padding.vertical_extent(),
                    ..Default::default()
                },
            );
        }
    }
    groups
}

/// 检测分组布局问题：非嵌套分组包围框重叠、非组成员节点落入分组框内。
pub fn detect_group_layout_warnings(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
) -> Vec<GroupLayoutWarning> {
    const EPSILON: f64 = 1.0;
    let mut warnings = Vec::new();

    // 预构建索引,避免在嵌套循环中反复 `diagram.groups.iter().find(...)`(O(n) → O(1))
    // - group_by_id: 由 group id 查 Group 引用(用于 descendants 的 BFS 展开)
    // - parent_of: 由 group id 查直接父 group id(用于 ancestors 的链式上溯)
    let group_by_id: HashMap<&str, &crate::ast::Group> = diagram
        .groups
        .iter()
        .map(|g| (g.id.as_str(), g))
        .collect();
    let parent_of: HashMap<&str, Option<&str>> = diagram
        .groups
        .iter()
        .map(|g| (g.id.as_str(), g.parent_id.as_ref().map(|p| p.as_str())))
        .collect();

    let mut group_descendants: HashMap<String, HashSet<String>> = HashMap::new();
    for group in &diagram.groups {
        let mut desc: HashSet<String> = group
            .entity_ids
            .iter()
            .map(|e| e.as_str().to_string())
            .collect();
        let mut stack: Vec<String> = group
            .child_group_ids
            .iter()
            .map(|g| g.as_str().to_string())
            .collect();
        while let Some(child) = stack.pop() {
            if let Some(child_group) = group_by_id.get(child.as_str()) {
                desc.extend(child_group.entity_ids.iter().map(|e| e.as_str().to_string()));
                stack.extend(
                    child_group
                        .child_group_ids
                        .iter()
                        .map(|g| g.as_str().to_string()),
                );
            }
        }
        group_descendants.insert(group.id.as_str().to_string(), desc);
    }

    let mut group_ancestors: HashMap<String, HashSet<String>> = HashMap::new();
    for group in &diagram.groups {
        let mut ancestors = HashSet::new();
        ancestors.insert(group.id.as_str().to_string());
        let mut current = group.parent_id.as_ref().map(|p| p.as_str().to_string());
        while let Some(p) = current {
            ancestors.insert(p.clone());
            current = parent_of
                .get(p.as_str())
                .copied()
                .flatten()
                .map(|p| p.to_string());
        }
        group_ancestors.insert(group.id.as_str().to_string(), ancestors);
    }

    let mut sorted_groups: Vec<&crate::ast::Group> = diagram.groups.iter().collect();
    sorted_groups.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    for i in 0..sorted_groups.len() {
        for j in (i + 1)..sorted_groups.len() {
            let ga = &sorted_groups[i];
            let gb = &sorted_groups[j];
            let (Some(la), Some(lb)) = (groups.get(ga.id.as_str()), groups.get(gb.id.as_str()))
            else {
                continue;
            };
            let a_ancestors = group_ancestors.get(ga.id.as_str());
            let b_ancestors = group_ancestors.get(gb.id.as_str());
            let nested = a_ancestors
                .map(|s| s.contains(gb.id.as_str()))
                .unwrap_or(false)
                || b_ancestors
                    .map(|s| s.contains(ga.id.as_str()))
                    .unwrap_or(false);
            if nested {
                continue;
            }
            let area = rect_overlap_area(la, lb);
            if area > EPSILON {
                warnings.push(GroupLayoutWarning {
                    kind: GroupLayoutWarningKind::GroupOverlap,
                    group_id: ga.id.as_str().to_string(),
                    other_id: gb.id.as_str().to_string(),
                    overlap_area: area,
                });
            }
        }
    }

    for group in &diagram.groups {
        let Some(gl) = groups.get(group.id.as_str()) else {
            continue;
        };
        let members = group_descendants
            .get(group.id.as_str())
            .cloned()
            .unwrap_or_default();
        for entity in &diagram.entities {
            let eid = entity.id.as_str();
            if members.contains(eid) {
                continue;
            }
            let Some(nl) = nodes.get(eid) else { continue };
            let area = rect_overlap_area_node(gl, nl);
            if area > EPSILON {
                warnings.push(GroupLayoutWarning {
                    kind: GroupLayoutWarningKind::ForeignNodeInside,
                    group_id: group.id.as_str().to_string(),
                    other_id: eid.to_string(),
                    overlap_area: area,
                });
            }
        }
    }

    warnings.sort_by(|a, b| {
        a.group_id
            .cmp(&b.group_id)
            .then(a.other_id.cmp(&b.other_id))
            .then(a.kind.cmp(&b.kind))
    });
    warnings
}

fn rect_overlap_area(a: &GroupLayout, b: &GroupLayout) -> f64 {
    let x_overlap = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let y_overlap = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    if x_overlap > 0.0 && y_overlap > 0.0 {
        x_overlap * y_overlap
    } else {
        0.0
    }
}

fn rect_overlap_area_node(g: &GroupLayout, n: &NodeLayout) -> f64 {
    let x_overlap = (g.x + g.width).min(n.x + n.width) - g.x.max(n.x);
    let y_overlap = (g.y + g.height).min(n.y + n.height) - g.y.max(n.y);
    if x_overlap > 0.0 && y_overlap > 0.0 {
        x_overlap * y_overlap
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AttributeMap, Entity, Group, Identifier, Span};
    use crate::layout::NodeLayout;
    use crate::types::DiagramType;

    fn span() -> Span {
        Span::dummy()
    }

    fn entity(id: &str, group: Option<&str>) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            group_id: group.map(|g| Identifier::new_unchecked(g)),
            span: span(),
        }
    }

    fn group(id: &str, entity_ids: Vec<&str>, parent: Option<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: AttributeMap::default(),
            parent_id: parent.map(|p| Identifier::new_unchecked(p)),
            depth: if parent.is_some() { 1 } else { 0 },
            entity_ids: entity_ids
                .into_iter()
                .map(|e| Identifier::new_unchecked(e))
                .collect(),
            child_group_ids: vec![],
            span: span(),
        }
    }

    fn node_layout(x: f64, y: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x,
            y,
            width: w,
            height: h,
            ..Default::default()
        }
    }

    fn group_layout(x: f64, y: f64, w: f64, h: f64) -> GroupLayout {
        GroupLayout {
            x,
            y,
            width: w,
            height: h,
            ..Default::default()
        }
    }

    #[test]
    fn uniform_padding_matches_legacy_extents() {
        let p = GroupPadding::uniform(20.0, 16.0);
        assert_eq!(p.left, 20.0);
        assert_eq!(p.right, 20.0);
        assert_eq!(p.top, 36.0);
        assert_eq!(p.bottom, 20.0);
        assert_eq!(p.horizontal_extent(), 40.0);
        assert_eq!(p.vertical_extent(), 56.0);
    }

    #[test]
    fn side_gutter_overrides_container_half_padding() {
        let leaf = GroupPadding::architecture_v2();
        let container = container_padding(leaf);
        assert!(container.left < leaf.left);
        let merged = container.max_per_side(SideGutter {
            left: 60.0,
            ..Default::default()
        });
        assert_eq!(merged.left, 60.0);
    }

    #[test]
    fn detects_overlapping_sibling_groups() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![entity("a1", Some("A")), entity("b1", Some("B"))],
            relations: vec![],
            groups: vec![group("A", vec!["a1"], None), group("B", vec!["b1"], None)],
            constraints: vec![],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo { file: None, line_count: 0 },
            ..Default::default()
        };
        let nodes = HashMap::from([
            ("a1".to_string(), node_layout(0.0, 0.0, 100.0, 50.0)),
            ("b1".to_string(), node_layout(50.0, 0.0, 100.0, 50.0)),
        ]);
        let groups = HashMap::from([
            ("A".to_string(), group_layout(-10.0, -10.0, 120.0, 70.0)),
            ("B".to_string(), group_layout(40.0, -10.0, 120.0, 70.0)),
        ]);

        let warnings = detect_group_layout_warnings(&diagram, &nodes, &groups);
        assert!(
            warnings
                .iter()
                .any(|w| w.kind == GroupLayoutWarningKind::GroupOverlap
                    && ((w.group_id == "A" && w.other_id == "B")
                        || (w.group_id == "B" && w.other_id == "A"))),
            "should detect A/B overlap, got: {:?}",
            warnings
        );
    }

    #[test]
    fn detects_foreign_node_inside_group() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                entity("a1", Some("A")),
                entity("a2", Some("A")),
                entity("foreign", None),
            ],
            relations: vec![],
            groups: vec![group("A", vec!["a1", "a2"], None)],
            constraints: vec![],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo { file: None, line_count: 0 },
            ..Default::default()
        };
        let nodes = HashMap::from([
            ("a1".to_string(), node_layout(0.0, 0.0, 100.0, 50.0)),
            ("a2".to_string(), node_layout(0.0, 200.0, 100.0, 50.0)),
            ("foreign".to_string(), node_layout(10.0, 100.0, 80.0, 40.0)),
        ]);
        let groups = HashMap::from([(
            "A".to_string(),
            group_layout(-10.0, -10.0, 120.0, 270.0),
        )]);

        let warnings = detect_group_layout_warnings(&diagram, &nodes, &groups);
        assert!(
            warnings
                .iter()
                .any(|w| w.kind == GroupLayoutWarningKind::ForeignNodeInside
                    && w.group_id == "A"
                    && w.other_id == "foreign"),
            "should detect foreign node inside A, got: {:?}",
            warnings
        );
    }

    #[test]
    fn does_not_warn_for_nested_groups() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![entity("a1", Some("inner"))],
            relations: vec![],
            constraints: vec![],
            groups: vec![
                group("outer", vec![], None),
                Group {
                    id: Identifier::new_unchecked("inner"),
                    label: "inner".to_string(),
                    attributes: AttributeMap::default(),
                    parent_id: Some(Identifier::new_unchecked("outer")),
                    depth: 1,
                    entity_ids: vec![Identifier::new_unchecked("a1")],
                    child_group_ids: vec![],
                    span: span(),
                },
            ],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo { file: None, line_count: 0 },
            ..Default::default()
        };
        let nodes = HashMap::from([("a1".to_string(), node_layout(0.0, 0.0, 100.0, 50.0))]);
        let groups = HashMap::from([
            ("outer".to_string(), group_layout(-20.0, -20.0, 140.0, 90.0)),
            ("inner".to_string(), group_layout(-10.0, -10.0, 120.0, 70.0)),
        ]);

        let warnings = detect_group_layout_warnings(&diagram, &nodes, &groups);
        assert!(
            !warnings
                .iter()
                .any(|w| w.kind == GroupLayoutWarningKind::GroupOverlap),
            "nested groups should not warn, got: {:?}",
            warnings
        );
    }

    #[test]
    fn no_warnings_when_groups_disjoint() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![entity("a1", Some("A")), entity("b1", Some("B"))],
            relations: vec![],
            groups: vec![group("A", vec!["a1"], None), group("B", vec!["b1"], None)],
            constraints: vec![],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo { file: None, line_count: 0 },
            ..Default::default()
        };
        let nodes = HashMap::from([
            ("a1".to_string(), node_layout(0.0, 0.0, 100.0, 50.0)),
            ("b1".to_string(), node_layout(500.0, 0.0, 100.0, 50.0)),
        ]);
        let groups = HashMap::from([
            ("A".to_string(), group_layout(-10.0, -10.0, 120.0, 70.0)),
            ("B".to_string(), group_layout(490.0, -10.0, 120.0, 70.0)),
        ]);

        let warnings = detect_group_layout_warnings(&diagram, &nodes, &groups);
        assert!(
            warnings.is_empty(),
            "disjoint groups should not warn, got: {:?}",
            warnings
        );
    }
}
