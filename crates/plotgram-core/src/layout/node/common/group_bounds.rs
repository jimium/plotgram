//! 分组包围框计算

use std::collections::{HashMap, HashSet};

use crate::ast::Diagram;
use crate::layout::{
    GroupLayout, GroupLayoutWarning, GroupLayoutWarningKind, NodeLayout,
};

/// 分组包围框的 padding 配置
#[derive(Debug, Clone, Copy)]
pub struct GroupPadding {
    /// 水平内边距
    pub x: f64,
    /// 垂直内边距（上方，含标题区额外偏移）
    pub y_top: f64,
    /// 水平总增量（width += x_delta）
    pub x_delta: f64,
    /// 垂直总增量（height += y_delta）
    pub y_delta: f64,
}

impl GroupPadding {
    /// 统一 padding（x/y 相同），含标题区偏移
    pub fn uniform(padding: f64, header_height: f64) -> Self {
        Self {
            x: padding,
            y_top: padding + header_height,
            x_delta: padding * 2.0,
            y_delta: padding * 2.0 + header_height,
        }
    }

    /// force-directed 布局的分组内边距
    pub fn force_directed() -> Self {
        Self {
            x: 20.0,
            y_top: 36.0,
            x_delta: 40.0,
            y_delta: 56.0,
        }
    }
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

/// 计算分组的包围框
///
/// 支持嵌套分组：父组包围框 = 直接实体包围框 ∪ 所有子组包围框。
/// 按 `depth` 降序排序（叶子组先算，容器组后算），确保父组计算时
/// 子组结果已就绪。纯容器父组（无直接实体）也能从子组推导出包围框。
///
/// 容器组（无直接实体、仅有子组）使用 `container_padding`，避免与子组
/// padding 叠加导致内层空间紧张。有直接实体的组使用 `leaf_padding`。
pub fn compute_group_bounds(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    leaf_padding: GroupPadding,
) -> HashMap<String, GroupLayout> {
    compute_group_bounds_with_container_padding(
        diagram,
        nodes,
        leaf_padding,
        container_padding(leaf_padding),
    )
}

/// 容器组（无直接实体）的 padding：水平减半，垂直保留标题区但 padding 减半。
fn container_padding(leaf: GroupPadding) -> GroupPadding {
    // 底部 padding = y_delta - y_top（leaf 的底部纯 padding，不含 header）
    let leaf_bottom = leaf.y_delta - leaf.y_top;
    let container_x = leaf.x * 0.5;
    let container_y_top = leaf.y_top * 0.6;
    let container_bottom = leaf_bottom * 0.5;
    GroupPadding {
        x: container_x,
        y_top: container_y_top,
        x_delta: container_x * 2.0,
        y_delta: container_y_top + container_bottom,
    }
}

/// 与 [`compute_group_bounds`] 相同，但允许显式指定容器组 padding。
pub fn compute_group_bounds_with_container_padding(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    leaf_padding: GroupPadding,
    container_padding: GroupPadding,
) -> HashMap<String, GroupLayout> {
    // 按 depth 升序排序：叶子组（depth 大）先算，容器组（depth 小）后算。
    // 稳定排序保证同 depth 时按 diagram.groups 原始顺序，确定性输出。
    let mut sorted_groups: Vec<&crate::ast::Group> = diagram.groups.iter().collect();
    sorted_groups.sort_by(|a, b| a.depth.cmp(&b.depth).reverse());

    let mut groups: HashMap<String, GroupLayout> = HashMap::new();
    for group in &sorted_groups {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;

        // 1. 直接实体
        for eid in effective_entity_ids(group, diagram) {
            if let Some(nl) = nodes.get(eid.as_str()) {
                min_x = min_x.min(nl.x);
                min_y = min_y.min(nl.y);
                max_x = max_x.max(nl.x + nl.width);
                max_y = max_y.max(nl.y + nl.height);
            }
        }

        // 2. 递归子组：父组包围框必须包含所有子组包围框
        for child_gid in &group.child_group_ids {
            if let Some(child_gl) = groups.get(child_gid.as_str()) {
                min_x = min_x.min(child_gl.x);
                min_y = min_y.min(child_gl.y);
                max_x = max_x.max(child_gl.x + child_gl.width);
                max_y = max_y.max(child_gl.y + child_gl.height);
            }
        }

        if min_x < f64::MAX {
            // 容器组（无直接实体）使用更小的 padding，避免与子组 padding 叠加
            let has_direct_entities = !effective_entity_ids(group, diagram).is_empty();
            let padding = if has_direct_entities {
                leaf_padding
            } else {
                container_padding
            };
            groups.insert(
                group.id.as_str().to_string(),
                GroupLayout {
                    x: min_x - padding.x,
                    y: min_y - padding.y_top,
                    width: max_x - min_x + padding.x_delta,
                    height: max_y - min_y + padding.y_delta,
                    ..Default::default()
                },
            );
        }
    }
    groups
}

/// 检测分组布局问题：非嵌套分组包围框重叠、非组成员节点落入分组框内。
///
/// 返回警告列表（按 group_id → other_id 字典序排序，保证确定性）。
/// 嵌套分组（父子关系）的包围框自然包含，不报重叠警告。
///
/// 容差 `EPSILON`：重叠面积小于此值视为边界相切，不报。
pub fn detect_group_layout_warnings(
    diagram: &Diagram,
    nodes: &HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
) -> Vec<GroupLayoutWarning> {
    const EPSILON: f64 = 1.0;
    let mut warnings = Vec::new();

    // 构建 group_id → 所有后代实体 id 集合（含递归子组的实体）
    let mut group_descendants: HashMap<String, HashSet<String>> = HashMap::new();
    for group in &diagram.groups {
        let mut desc: HashSet<String> = group
            .entity_ids
            .iter()
            .map(|e| e.as_str().to_string())
            .collect();
        // 递归收集子组实体
        let mut stack: Vec<String> = group
            .child_group_ids
            .iter()
            .map(|g| g.as_str().to_string())
            .collect();
        while let Some(child) = stack.pop() {
            if let Some(child_group) = diagram.groups.iter().find(|g| g.id.as_str() == child) {
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

    // 构建 group 祖先链：group_id → 所有祖先 group id 集合（含自身）
    let mut group_ancestors: HashMap<String, HashSet<String>> = HashMap::new();
    for group in &diagram.groups {
        let mut ancestors = HashSet::new();
        ancestors.insert(group.id.as_str().to_string());
        let mut current = group.parent_id.as_ref().map(|p| p.as_str().to_string());
        while let Some(p) = current {
            ancestors.insert(p.clone());
            current = diagram
                .groups
                .iter()
                .find(|g| g.id.as_str() == p)
                .and_then(|g| g.parent_id.as_ref())
                .map(|p| p.as_str().to_string());
        }
        group_ancestors.insert(group.id.as_str().to_string(), ancestors);
    }

    // 1. 检测非嵌套分组包围框重叠
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
            // 跳过嵌套关系（父子）
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

    // 2. 检测非组成员节点落入分组框内
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

    // 确定性排序：按 (group_id, other_id) 字典序
    warnings.sort_by(|a, b| {
        a.group_id
            .cmp(&b.group_id)
            .then(a.other_id.cmp(&b.other_id))
            .then(a.kind.cmp(&b.kind))
    });
    warnings
}

/// 计算两个 GroupLayout 矩形的重叠面积
fn rect_overlap_area(a: &GroupLayout, b: &GroupLayout) -> f64 {
    let x_overlap = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let y_overlap = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    if x_overlap > 0.0 && y_overlap > 0.0 {
        x_overlap * y_overlap
    } else {
        0.0
    }
}

/// 计算 GroupLayout 矩形与 NodeLayout 矩形的重叠面积
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
    use crate::layout::{NodeLayout};
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
    fn detects_overlapping_sibling_groups() {
        let diagram = Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![entity("a1", Some("A")), entity("b1", Some("B"))],
            relations: vec![],
            groups: vec![group("A", vec!["a1"], None), group("B", vec!["b1"], None)],
            style_decls: vec![],
            source_info: crate::ast::SourceInfo { file: None, line_count: 0 },
            ..Default::default()
        };
        let nodes = HashMap::from([
            ("a1".to_string(), node_layout(0.0, 0.0, 100.0, 50.0)),
            ("b1".to_string(), node_layout(50.0, 0.0, 100.0, 50.0)),
        ]);
        // A 和 B 包围框明显重叠
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
            style_decls: vec![],
            source_info: crate::ast::SourceInfo { file: None, line_count: 0 },
            ..Default::default()
        };
        let nodes = HashMap::from([
            ("a1".to_string(), node_layout(0.0, 0.0, 100.0, 50.0)),
            ("a2".to_string(), node_layout(0.0, 200.0, 100.0, 50.0)),
            // foreign 节点落在 A 的包围框内（x 重叠，y 在 a1/a2 之间）
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
        // outer 包含 inner（嵌套），不应报重叠
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
