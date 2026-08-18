//! R3-4：收缩元数据与 expand。

use crate::layout::atlas::dialect::contraction::weak::ArrangementMode;
use crate::layout::kernel::common::divide_and_conquer::IntraLayout;
use crate::layout::NodeLayout;
use std::collections::{BTreeMap, HashMap};

/// Weak（及后续 Strong）共用的收缩壳：组内 Intra + Super 映射。
#[derive(Debug, Clone)]
pub struct ContractionMeta {
    pub intras: BTreeMap<String, IntraLayout>,
    /// Super id 稳定序（拓扑序）。
    pub super_ids: Vec<String>,
    pub entity_to_super: BTreeMap<String, String>,
    pub mode: ArrangementMode,
}

/// `member.xy = super[gid].xy + intra.nodes[id].xy`（Intra 未归零原点约定不变）。
pub fn expand_contraction(
    meta: &ContractionMeta,
    super_nodes: &HashMap<String, NodeLayout>,
) -> HashMap<String, NodeLayout> {
    let mut nodes = HashMap::new();
    for gid in &meta.super_ids {
        let Some(intra) = meta.intras.get(gid) else {
            continue;
        };
        let (x_off, y_off) = super_nodes
            .get(gid)
            .map(|n| (n.x, n.y))
            .unwrap_or((0.0, 0.0));
        let mut member_ids: Vec<&String> = intra.nodes.keys().collect();
        member_ids.sort();
        for id in member_ids {
            let Some(local) = intra.nodes.get(id) else {
                continue;
            };
            let mut global = local.clone();
            global.x += x_off;
            global.y += y_off;
            nodes.insert(id.clone(), global);
        }
    }
    nodes
}
