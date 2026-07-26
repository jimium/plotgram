//! 图索引与分组映射。

use crate::ast::{Diagram, Relation};
use crate::layout::kernel::common::graph_index::DirectedGraphIndex;
use crate::layout::kernel::common::group_map;
use crate::types::DiagramType;
use std::collections::HashMap;

/// 架构图布局使用的有向图索引（过滤 Passive 边）。
pub(crate) type GraphIndex = DirectedGraphIndex;

/// 架构图坐标求解所需的图级事实（解耦对 `&Diagram` 的直接依赖）。
#[derive(Clone)]
pub(crate) struct ArchDiagramFacts {
    pub diagram_type: DiagramType,
    pub relations: Vec<Relation>,
    pub has_groups: bool,
}

impl ArchDiagramFacts {
    pub fn from_diagram(diagram: &Diagram) -> Self {
        Self {
            diagram_type: diagram.diagram_type.clone(),
            relations: diagram.relations.clone(),
            has_groups: !diagram.groups.is_empty(),
        }
    }
}

pub(crate) struct GroupMap {
    pub(crate) node_to_top_group: HashMap<String, String>,
    pub(crate) top_group_members: HashMap<String, Vec<String>>,
    pub(crate) top_groups: Vec<String>,
    pub(crate) ungrouped: Vec<String>,
}

pub(crate) fn build_group_map(diagram: &Diagram) -> GroupMap {
    let mut top_groups = Vec::new();
    for group in &diagram.groups {
        if group.parent_id.is_none() {
            top_groups.push(group.id.as_str().to_string());
        }
    }

    let node_to_top_group = group_map::build_node_to_top_group(diagram);

    let mut top_group_members: HashMap<String, Vec<String>> = HashMap::new();
    let mut ungrouped = Vec::new();

    for entity in &diagram.entities {
        let eid = entity.id.as_str().to_string();
        if let Some(top) = node_to_top_group.get(&eid) {
            top_group_members.entry(top.clone()).or_default().push(eid);
        } else {
            ungrouped.push(eid);
        }
    }

    GroupMap {
        node_to_top_group,
        top_group_members,
        top_groups,
        ungrouped,
    }
}
