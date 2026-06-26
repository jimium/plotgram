//! 图索引与分组映射。

use crate::ast::Diagram;
use crate::layout::node::common::graph_index::DirectedGraphIndex;
use crate::layout::node::common::group_map;
use std::collections::HashMap;

/// 架构图布局使用的有向图索引（过滤 Passive 边）。
pub(in super::super) type GraphIndex = DirectedGraphIndex;

pub(in super::super) struct GroupMap {
    pub(in super::super) node_to_top_group: HashMap<String, String>,
    pub(in super::super) top_group_members: HashMap<String, Vec<String>>,
    pub(in super::super) top_groups: Vec<String>,
    pub(in super::super) ungrouped: Vec<String>,
}

pub(in super::super) fn build_group_map(diagram: &Diagram) -> GroupMap {
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
