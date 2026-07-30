//! Pack nodes by rank into frames.

use plotgram_engine_api::LayoutError;
use plotgram_model::attr::AttrMap;
use plotgram_model::geometry::Rect;
use plotgram_model::graph::Graph;
use plotgram_model::result::NodePlacement;
use plotgram_model::sizes::NodeSizes;

use super::rank::RankMap;

/// Hierarchical layout plugin.
#[derive(Debug, Default, Clone, Copy)]
pub struct HierarchicalLayout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    TopToBottom,
    LeftToRight,
}

fn direction_from_options(options: &AttrMap) -> Direction {
    match options.get("direction").and_then(|v| v.as_str()) {
        Some("left-to-right") | Some("ltr") => Direction::LeftToRight,
        _ => Direction::TopToBottom,
    }
}

pub fn place_nodes(
    graph: &Graph,
    sizes: &NodeSizes,
    options: &AttrMap,
    ranks: &RankMap,
) -> Result<Vec<NodePlacement>, LayoutError> {
    let dir = direction_from_options(options);
    let layer_gap = options
        .get("layer_gap")
        .and_then(|v| v.as_f64())
        .unwrap_or(40.0);
    let node_gap = options
        .get("node_gap")
        .and_then(|v| v.as_f64())
        .unwrap_or(24.0);

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
            let (x, y) = match dir {
                Direction::TopToBottom => {
                    layer_thickness = layer_thickness.max(size.height);
                    let p = (cursor_cross, cursor_main);
                    cursor_cross += size.width + node_gap;
                    p
                }
                Direction::LeftToRight => {
                    layer_thickness = layer_thickness.max(size.width);
                    let p = (cursor_main, cursor_cross);
                    cursor_cross += size.height + node_gap;
                    p
                }
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
