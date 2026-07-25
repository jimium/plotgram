//! Phase 3：架构图组内复杂拓扑委托 sugiyama_v2。

use std::collections::{HashMap, HashSet};

use crate::ast::{Diagram, DiagramAttribute, Entity, Relation};
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::kernel::common::divide_and_conquer::IntraLayout;
use crate::layout::kernel::layered::{engine, preset};
use crate::layout::NodeLayout;

/// 构建仅含 members 的子 Diagram，并调用 sugiyama_v2 `ARCHITECTURE_PRESET`。
pub(super) fn layout_intra_with_sugiyama_v2(
    diagram: &Diagram,
    members: &[String],
) -> IntraLayout {
    if members.is_empty() {
        return IntraLayout {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: vec![],
        };
    }

    let sub = build_sub_diagram(diagram, members);
    let result = engine::compute_with_preset(
        &sub,
        &preset::ARCHITECTURE_PRESET,
        SugiyamaLayoutConfig::default(),
    );

    let mut nodes: HashMap<String, NodeLayout> = result
        .nodes
        .into_iter()
        .filter(|(id, _)| members.iter().any(|m| m == id))
        .collect();

    // 平移到原点，便于 two_phase 组内合成
    let min_x = nodes
        .values()
        .map(|n| n.x)
        .fold(f64::INFINITY, f64::min);
    let min_y = nodes
        .values()
        .map(|n| n.y)
        .fold(f64::INFINITY, f64::min);
    if min_x.is_finite() && min_y.is_finite() {
        for n in nodes.values_mut() {
            n.x -= min_x;
            n.y -= min_y;
        }
    }

    let content_width = nodes
        .values()
        .map(|n| n.x + n.width)
        .fold(0.0_f64, f64::max);
    let content_height = nodes
        .values()
        .map(|n| n.y + n.height)
        .fold(0.0_f64, f64::max);

    let mut layers: Vec<Vec<String>> = Vec::new();
    if let Some(ranks) = result.hints.sugiyama_ranks {
        let mut by_rank: HashMap<usize, Vec<String>> = HashMap::new();
        for id in members {
            let rank = ranks.get(id).copied().unwrap_or(0);
            by_rank.entry(rank).or_default().push(id.clone());
        }
        let mut rank_keys: Vec<usize> = by_rank.keys().copied().collect();
        rank_keys.sort_unstable();
        for r in rank_keys {
            if let Some(mut layer) = by_rank.remove(&r) {
                layer.sort();
                layers.push(layer);
            }
        }
    }
    if layers.is_empty() {
        let mut ids = members.to_vec();
        ids.sort();
        layers.push(ids);
    }

    IntraLayout {
        nodes,
        content_width,
        content_height,
        layers,
    }
}

fn build_sub_diagram(diagram: &Diagram, members: &[String]) -> Diagram {
    let member_set: HashSet<&str> = members.iter().map(|s| s.as_str()).collect();

    let entities: Vec<Entity> = diagram
        .entities
        .iter()
        .filter(|e| member_set.contains(e.id.as_str()))
        .map(|e| {
            let mut cloned = e.clone();
            cloned.group_id = None;
            cloned
        })
        .collect();

    let relations: Vec<Relation> = diagram
        .relations
        .iter()
        .filter(|r| {
            member_set.contains(r.from.as_str()) && member_set.contains(r.to.as_str())
        })
        .cloned()
        .collect();

    let constraints: Vec<crate::ast::Constraint> = diagram
        .constraints
        .iter()
        .filter(|c| {
            member_set.contains(c.from.as_str()) && member_set.contains(c.to.as_str())
        })
        .cloned()
        .collect();

    let attributes: Vec<DiagramAttribute> = diagram.attributes.clone();

    Diagram {
        diagram_type: diagram.diagram_type.clone(),
        attributes,
        entities,
        relations,
        groups: vec![],
        constraints,
        style_decls: vec![],
        doc_comment: None,
        source_info: Default::default(),
    }
}
