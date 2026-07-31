//! Pack nodes by rank into frames.

use plotgram_engine_api::LayoutError;
use plotgram_model::geometry::Rect;
use plotgram_model::graph::Graph;
use plotgram_model::result::NodePlacement;
use plotgram_model::sizes::NodeSizes;

use super::params::HierarchicalParams;
use super::rank::RankMap;

/// Hierarchical layout plugin.
#[derive(Debug, Default, Clone, Copy)]
pub struct HierarchicalLayout;

pub fn place_nodes(
    graph: &Graph,
    sizes: &NodeSizes,
    params: &HierarchicalParams,
    ranks: &RankMap,
) -> Result<Vec<NodePlacement>, LayoutError> {
    let vertical = params.orientation.is_vertical();
    let layer_gap = params.layer_gap;
    let node_gap = params.node_gap;

    let ids = graph.all_node_ids();
    let mut by_rank: std::collections::BTreeMap<u32, Vec<String>> =
        std::collections::BTreeMap::new();
    for id in &ids {
        let r = ranks.get(id).copied().unwrap_or(0);
        by_rank.entry(r).or_default().push(id.clone());
    }

    let mut placements = Vec::with_capacity(ids.len());
    let mut cursor_main = 0.0_f64;

    for (_rank, members) in by_rank {
        let mut layer_thickness = 0.0_f64;
        let mut cursor_cross = 0.0_f64;

        for id in &members {
            let size = sizes.get(id).ok_or_else(|| {
                LayoutError::from(plotgram_model::MissingNodeSize {
                    node_id: id.clone(),
                })
            })?;
            // BT / RL flips belong to an Orientation Stage later; stub packs TB / LR axes.
            let (x, y) = if vertical {
                layer_thickness = layer_thickness.max(size.height);
                let p = (cursor_cross, cursor_main);
                cursor_cross += size.width + node_gap;
                p
            } else {
                layer_thickness = layer_thickness.max(size.width);
                let p = (cursor_main, cursor_cross);
                cursor_cross += size.height + node_gap;
                p
            };
            placements.push(NodePlacement {
                id: id.clone(),
                frame: Rect::from_origin_size(x, y, size),
            });
        }

        cursor_main += layer_thickness + layer_gap;
    }

    // Stable order: match declaration order of ids
    let index: std::collections::BTreeMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    placements.sort_by_key(|p| index.get(p.id.as_str()).copied().unwrap_or(usize::MAX));

    Ok(placements)
}
