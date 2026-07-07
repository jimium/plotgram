//! 边路由测试公共 fixture
//!
//! 提供 straight / bezier / spline / orthogonal 四个 router 测试共享的
//! Diagram + LayoutResult 构造工具，消除各 router 文件中重复的
//! `create_test_diagram_with_layout` / `create_test_setup` helper。
//!
//! circular router 的测试需要 `CircularLayoutHints`，场景特殊，不在此覆盖。

use crate::ast::{ArrowType, AttributeMap, Diagram, Entity, Identifier, Relation, SourceInfo, Span};
use crate::layout::{LayoutResult, NodeLayout};
use crate::types::DiagramType;
use std::collections::HashMap;

/// 默认测试节点尺寸
pub const DEFAULT_NODE_WIDTH: f64 = 160.0;
pub const DEFAULT_NODE_HEIGHT: f64 = 50.0;

/// 默认测试画布尺寸
pub const DEFAULT_CANVAS_WIDTH: f64 = 500.0;
pub const DEFAULT_CANVAS_HEIGHT: f64 = 500.0;

/// 创建一个默认尺寸的 NodeLayout（160×50）
pub fn make_node_layout(x: f64, y: f64) -> NodeLayout {
    NodeLayout {
        x,
        y,
        width: DEFAULT_NODE_WIDTH,
        height: DEFAULT_NODE_HEIGHT,
        ..Default::default()
    }
}

/// 构造测试用 Diagram + LayoutResult（参数化版本）
///
/// - `entities`: `(id, x, y)` 三元组
/// - `relations`: `(from, to, label)` 三元组
///
/// 节点尺寸默认 160×50，画布 500×500，图表类型 Flowchart。
/// 所有 Entity 的 label 与 id 相同，Relation 的 arrow 为 Active。
pub fn make_diagram_with_layout(
    entities: Vec<(&str, f64, f64)>,
    relations: Vec<(&str, &str, Option<&str>)>,
) -> (Diagram, LayoutResult) {
    let span = Span::dummy();

    let nodes: HashMap<String, NodeLayout> = entities
        .iter()
        .map(|(id, x, y)| (id.to_string(), make_node_layout(*x, *y)))
        .collect();

    let diagram = Diagram {
        diagram_type: DiagramType::Flowchart,
        attributes: Vec::new(),
        entities: entities
            .into_iter()
            .map(|(id, _x, _y)| Entity {
                id: Identifier::new_unchecked(id),
                label: id.to_string(),
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            })
            .collect(),
        relations: relations
            .into_iter()
            .map(|(from, to, label)| Relation {
                from: Identifier::new_unchecked(from),
                to: Identifier::new_unchecked(to),
                arrow: ArrowType::Active,
                label: label.map(|s| s.to_string()),
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            })
            .collect(),
        groups: Vec::new(),
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 1,
        },
        ..Default::default()
    };

    let result = LayoutResult {
        nodes,
        groups: HashMap::new(),
        edges: vec![],
        total_width: DEFAULT_CANVAS_WIDTH,
        total_height: DEFAULT_CANVAS_HEIGHT,
        hints: Default::default(),
    };

    (diagram, result)
}

/// 构造网格场景（`rows`×`cols` 节点均匀分布），用于避障 / 多边交叉测试。
///
/// 节点按 `n{row}{col}` 命名，水平间距 200px，垂直间距 120px。
/// 水平相邻节点连边（`n00 → n01 → ...`），中间行的非端点节点会成为障碍。
pub fn make_diagram_grid(rows: usize, cols: usize) -> (Diagram, LayoutResult) {
    let spacing_x = 200.0;
    let spacing_y = 120.0;

    let mut entities: Vec<(String, f64, f64)> = Vec::new();
    let mut relations: Vec<(String, String)> = Vec::new();

    for r in 0..rows {
        for c in 0..cols {
            let id = format!("n{r}{c}");
            let x = c as f64 * spacing_x + 40.0;
            let y = r as f64 * spacing_y + 40.0;
            entities.push((id, x, y));
        }
    }

    // 水平相邻节点连边
    for r in 0..rows {
        for c in 0..cols.saturating_sub(1) {
            let from = format!("n{r}{c}");
            let to = format!("n{r}{}", c + 1);
            relations.push((from, to));
        }
    }

    let span = Span::dummy();
    let nodes: HashMap<String, NodeLayout> = entities
        .iter()
        .map(|(id, x, y)| (id.clone(), make_node_layout(*x, *y)))
        .collect();

    let diagram = Diagram {
        diagram_type: DiagramType::Flowchart,
        attributes: Vec::new(),
        entities: entities
            .into_iter()
            .map(|(id, _x, _y)| Entity {
                id: Identifier::new_unchecked(&id),
                label: id,
                attributes: AttributeMap::default(),
                group_id: None,
                span,
            })
            .collect(),
        relations: relations
            .into_iter()
            .map(|(from, to)| Relation {
                from: Identifier::new_unchecked(&from),
                to: Identifier::new_unchecked(&to),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span,
            })
            .collect(),
        groups: Vec::new(),
        style_decls: vec![],
        source_info: SourceInfo {
            file: None,
            line_count: 1,
        },
        ..Default::default()
    };

    let result = LayoutResult {
        nodes,
        groups: HashMap::new(),
        edges: vec![],
        total_width: DEFAULT_CANVAS_WIDTH,
        total_height: DEFAULT_CANVAS_HEIGHT,
        hints: Default::default(),
    };

    (diagram, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_two_nodes() {
        let (diagram, result) = make_diagram_with_layout(
            vec![("a", 40.0, 40.0), ("b", 260.0, 40.0)],
            vec![("a", "b", None)],
        );
        assert_eq!(diagram.entities.len(), 2);
        assert_eq!(diagram.relations.len(), 1);
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 0);
    }

    #[test]
    fn fixture_grid_3x3() {
        let (diagram, result) = make_diagram_grid(3, 3);
        assert_eq!(diagram.entities.len(), 9);
        assert_eq!(diagram.relations.len(), 6); // 3 rows × 2 horizontal edges
        assert_eq!(result.nodes.len(), 9);
    }
}
