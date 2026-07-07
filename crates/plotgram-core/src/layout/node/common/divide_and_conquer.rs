//! 分治布局通用框架：数据结构 + 策略 trait
//!
//! 本模块抽取自 `architecture_v2::two_phase`，为 flowchart / architecture 等场景
//! 提供统一的分治布局类型基础。各场景自行实现 [`IntraGroupLayouter`] 和
//! [`GroupArrangement`] 两个 trait，完成"组内布局 → 组间排列 → 合并"的调度。
//!
//! # 设计约束
//!
//! - **组内布局由外部决定**：`IntraGroupLayouter` trait 由各场景实现，内部可调用
//!   `LayoutStrategy` 或特化逻辑（如 architecture_v2 的 hub 居中、client 对齐）
//! - **组间排列可注入**：`GroupArrangement` trait 支持堆叠（flowchart）、
//!   super macro rank（architecture）等不同策略
//! - **确定性**：`GroupTree` 的所有方法对同输入产出同输出，不依赖 HashMap 迭代顺序

use crate::ast::Diagram;
use crate::layout::NodeLayout;
use std::collections::{HashMap, HashSet};

// ─── 组内布局结果 ─────────────────────────────────────────

/// 组内局部布局结果（原点在内容区左上角）
///
/// 抽取自 `architecture_v2::two_phase::IntraLayout`，供所有分治布局场景共用。
#[derive(Clone, Debug)]
pub struct IntraLayout {
    /// 组内节点的局部坐标（相对组内容区原点）
    pub nodes: HashMap<String, NodeLayout>,
    /// 组内内容区宽度
    pub content_width: f64,
    /// 组内内容区高度
    pub content_height: f64,
    /// 组内层结构（按 y 自上而下），供全局层重建使用
    pub layers: Vec<Vec<String>>,
}

impl IntraLayout {
    /// 空布局
    pub fn empty() -> Self {
        Self {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: Vec::new(),
        }
    }

    /// 单节点布局
    pub fn single(id: &str, width: f64, height: f64) -> Self {
        Self {
            nodes: HashMap::from([(
                id.to_string(),
                NodeLayout {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                    ..Default::default()
                },
            )]),
            content_width: width,
            content_height: height,
            layers: vec![vec![id.to_string()]],
        }
    }
}

impl Default for IntraLayout {
    fn default() -> Self {
        Self::empty()
    }
}

// ─── 分组树 ───────────────────────────────────────────────

/// 分组树：保留完整嵌套层级，供递归布局使用
///
/// 抽取自 `architecture_v2::two_phase::GroupTree`。与扁平化的
/// `node_to_top_group` 映射互补，本结构保留父子关系，使递归组内布局
/// 能对容器组递归布局子组。
#[derive(Debug, Clone)]
pub struct GroupTree {
    /// 组 → 直接子组 ID 列表（已确定性排序）
    group_children: HashMap<String, Vec<String>>,
    /// 组 → 直接实体 ID 列表（已确定性排序）
    group_entities: HashMap<String, Vec<String>>,
}

impl GroupTree {
    /// 从 Diagram 构建分组树
    ///
    /// 所有子组列表和实体列表均按 ID 排序，保证确定性
    /// （遵循 AGENTS.md 第 2 条：不依赖 HashMap 迭代顺序）。
    pub fn build(diagram: &Diagram) -> Self {
        let mut group_children: HashMap<String, Vec<String>> = HashMap::new();
        let mut group_entities: HashMap<String, Vec<String>> = HashMap::new();

        // 收集子组关系
        for group in &diagram.groups {
            if let Some(ref parent) = group.parent_id {
                group_children
                    .entry(parent.as_str().to_string())
                    .or_default()
                    .push(group.id.as_str().to_string());
            }
            // 确保每个组都有 entry（即使无子组）
            group_children
                .entry(group.id.as_str().to_string())
                .or_default();
            group_entities
                .entry(group.id.as_str().to_string())
                .or_default();
        }

        // 收集直接实体
        for entity in &diagram.entities {
            if let Some(ref gid) = entity.group_id {
                group_entities
                    .entry(gid.as_str().to_string())
                    .or_default()
                    .push(entity.id.as_str().to_string());
            }
        }

        // 确定性排序
        for children in group_children.values_mut() {
            children.sort();
        }
        for entities in group_entities.values_mut() {
            entities.sort();
        }

        Self {
            group_children,
            group_entities,
        }
    }

    /// 组的直接子组（空切片若无）
    pub fn children_of(&self, gid: &str) -> &[String] {
        self.group_children
            .get(gid)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// 组的直接实体（空切片若无）
    pub fn entities_of(&self, gid: &str) -> &[String] {
        self.group_entities
            .get(gid)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// 组的所有后代实体（递归，用于构建超级节点图的成员）
    pub fn descendant_entities(&self, gid: &str) -> Vec<String> {
        let mut out: Vec<String> = self.entities_of(gid).to_vec();
        for child in self.children_of(gid) {
            out.extend(self.descendant_entities(child));
        }
        out
    }

    /// 所有顶层组（parent_id 为 None 的组），按 ID 排序保证确定性
    pub fn top_groups(&self) -> Vec<String> {
        // 顶层组 = 在 group_children 中有 entry 但不是任何组的 child
        let mut all_children: HashSet<&str> = HashSet::new();
        for children in self.group_children.values() {
            for c in children {
                all_children.insert(c.as_str());
            }
        }
        let mut tops: Vec<String> = self
            .group_children
            .keys()
            .filter(|g| !all_children.contains(g.as_str()))
            .cloned()
            .collect();
        tops.sort();
        tops
    }
}

// ─── 跨 group 边 ──────────────────────────────────────────

/// 跨 group 边：连接不同 group（或 group 与无组节点）的边
///
/// 用于分治布局后，将跨 group 边交给边路由引擎处理。
#[derive(Debug, Clone)]
pub struct CrossGroupEdge {
    /// 起点 entity_id
    pub from: String,
    /// 终点 entity_id
    pub to: String,
    /// 起点所属的顶层 group（None 表示无组）
    pub from_group: Option<String>,
    /// 终点所属的顶层 group（None 表示无组）
    pub to_group: Option<String>,
}

// ─── 策略 trait ───────────────────────────────────────────

/// 组内布局策略
///
/// 由各场景实现，决定如何对一个 group 的内部节点进行布局。
///
/// # 实现方
///
/// - `architecture_v2`：内部调用 `layout_intra_group_recursive`（含 hub 居中、
///   client 对齐等特化优化）
/// - `flowchart`：内部调用 `FlowchartLayout::compute(子 Diagram)`（走 LayoutStrategy 入口）
///
/// # 为什么不直接用 `LayoutStrategy` trait
///
/// `LayoutStrategy::compute` 接受 `&Diagram`，但分治布局的输入通常是已预处理
/// 的图数据（如 `GraphIndex`、去环边集等）。各场景的预处理流程不同，强行统一
/// 为 `&Diagram` 会丢失预处理信息。因此定义此更具体的 trait，由各场景自行
/// 决定内部是否调用 `LayoutStrategy`。
pub trait IntraGroupLayouter {
    /// 对指定 group 的内部节点进行布局，返回局部坐标
    ///
    /// - `group_id`：组 ID（顶层组）
    /// - `members`：组的所有后代实体 ID（已扁平化）
    fn layout_intra(&self, group_id: &str, members: &[String]) -> IntraLayout;
}

/// 组间排列策略
///
/// 决定如何排列各 group 的全局位置。由各场景实现。
///
/// # 实现方
///
/// - `StackingArrangement`（flowchart）：拓扑排序 + 垂直/水平堆叠
/// - `SuperMacroRankArrangement`（architecture）：复用现有 `assign_super_macro_ranks`
pub trait GroupArrangement {
    /// 计算每个 group 的全局偏移 `(x_offset, y_offset)`
    ///
    /// - `group_ids`：参与排列的 group ID 列表（已确定性排序）
    /// - `intra_layouts`：各 group 的组内布局结果
    /// - `cross_edges`：跨 group 边（用于计算通道间距等）
    fn arrange(
        &self,
        group_ids: &[String],
        intra_layouts: &HashMap<String, IntraLayout>,
        cross_edges: &[CrossGroupEdge],
    ) -> HashMap<String, (f64, f64)>;
}

// ─── 通用辅助函数 ─────────────────────────────────────────

/// 从 group 的 IntraLayout 和全局偏移，计算 group 的包围框
///
/// group 包围框 = 内容区 + padding
pub fn group_bounds_from_intra(
    intra: &IntraLayout,
    (x_offset, y_offset): (f64, f64),
    padding_x: f64,
    padding_y_top: f64,
    padding_y_bottom: f64,
) -> (f64, f64, f64, f64) {
    (
        x_offset,
        y_offset,
        intra.content_width + padding_x * 2.0,
        intra.content_height + padding_y_top + padding_y_bottom,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Entity, Group, Identifier, Span};

    fn entity(id: &str, group: Option<&str>) -> Entity {
        Entity {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: crate::ast::AttributeMap::default(),
            group_id: group.map(|g| Identifier::new_unchecked(g)),
            span: Span::dummy(),
        }
    }

    fn group(id: &str, parent: Option<&str>) -> Group {
        Group {
            id: Identifier::new_unchecked(id),
            label: id.to_string(),
            attributes: crate::ast::AttributeMap::default(),
            parent_id: parent.map(|p| Identifier::new_unchecked(p)),
            depth: 0,
            entity_ids: vec![],
            child_group_ids: vec![],
            span: Span::dummy(),
        }
    }

    #[test]
    fn group_tree_build_and_query() {
        let diagram = Diagram {
            entities: vec![
                entity("a", Some("g1")),
                entity("b", Some("g2")),
                entity("c", Some("g2")),
                entity("d", None),
            ],
            groups: vec![
                group("g1", None),
                group("g2", None),
                group("g3", Some("g1")), // 嵌套
            ],
            ..Default::default()
        };

        let tree = GroupTree::build(&diagram);

        // 顶层组（确定性排序）
        let tops = tree.top_groups();
        assert_eq!(tops, vec!["g1".to_string(), "g2".to_string()]);

        // 直接子组
        assert_eq!(tree.children_of("g1"), &["g3".to_string()]);
        assert_eq!(tree.children_of("g2"), &[] as &[String]);

        // 直接实体（g1 有直接实体 a，g2 有 b/c）
        assert_eq!(tree.entities_of("g1"), &["a".to_string()]);
        assert_eq!(tree.entities_of("g2"), &["b".to_string(), "c".to_string()]);

        // 后代实体（g1 的后代 = 直接实体 a + 子组 g3 的后代）
        assert_eq!(tree.descendant_entities("g1"), &["a".to_string()]);
        assert_eq!(
            tree.descendant_entities("g2"),
            &["b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn intra_layout_empty_and_single() {
        let empty = IntraLayout::empty();
        assert!(empty.nodes.is_empty());
        assert_eq!(empty.content_width, 0.0);

        let single = IntraLayout::single("x", 100.0, 50.0);
        assert_eq!(single.nodes.len(), 1);
        assert_eq!(single.content_width, 100.0);
        assert_eq!(single.content_height, 50.0);
        assert_eq!(single.layers.len(), 1);
    }

    #[test]
    fn cross_group_edge_fields() {
        let edge = CrossGroupEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            from_group: Some("g1".to_string()),
            to_group: Some("g2".to_string()),
        };
        assert_eq!(edge.from, "a");
        assert_eq!(edge.to_group, Some("g2".to_string()));
    }
}
