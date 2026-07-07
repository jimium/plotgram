//! 节点 → 顶层组的映射工具。
//!
//! 抽取自 `architecture_v2::layout::build_group_map` 与
//! `intent::geometric::build_node_to_top_group`，消除两处重复实现。

use crate::ast::Diagram;
use std::collections::HashMap;

/// 构建节点 → 顶层组的映射（用于跨组检测）。
///
/// 遍历 `diagram.groups` 解析父子链，将每个实体映射到其所属的顶层组 ID。
/// 无组的实体不出现在返回映射中。
pub fn build_node_to_top_group(diagram: &Diagram) -> HashMap<String, String> {
    let mut group_parent: HashMap<String, String> = HashMap::new();
    for group in &diagram.groups {
        if let Some(ref parent) = group.parent_id {
            group_parent.insert(group.id.as_str().to_string(), parent.as_str().to_string());
        }
    }

    let mut group_to_top: HashMap<String, String> = HashMap::new();
    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        let mut top = gid.clone();
        while let Some(parent) = group_parent.get(&top) {
            top = parent.clone();
        }
        group_to_top.insert(gid, top);
    }

    let mut node_to_top = HashMap::new();
    for entity in &diagram.entities {
        if let Some(ref gid) = entity.group_id {
            let gs = gid.as_str().to_string();
            let top = group_to_top.get(&gs).cloned().unwrap_or(gs);
            node_to_top.insert(entity.id.as_str().to_string(), top);
        }
    }
    node_to_top
}
