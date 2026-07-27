//! macro block
//!
//! moved from two_phase.rs (A4, behavior unchanged).

use super::*;

pub(super) fn build_macro_blocks(
    diagram: &Diagram,
    group_map: &GroupMap,
    sizes: &HashMap<String, (f64, f64)>,
    intra_by_group: &HashMap<String, IntraLayout>,
    super_members: &HashMap<String, Vec<String>>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    padding: &GroupPadding,
) -> Vec<MacroBlock> {
    let mut blocks = Vec::new();

    for gid in &group_map.top_groups {
        let intra = intra_by_group.get(gid).cloned().unwrap_or(IntraLayout {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: vec![],
        });
        blocks.push(MacroBlock {
            id: gid.clone(),
            is_group: true,
            width: intra.content_width + padding.horizontal_extent(),
            height: intra.content_height + padding.vertical_extent(),
            x: 0.0,
            y: 0.0,
            intra,
        });
    }

    // 无组节点：每个超级节点独立成块（同宏观 rank 时水平排列）
    let mut ungrouped_supers: Vec<String> = super_members
        .keys()
        .filter(|id| id.starts_with("@node:"))
        .cloned()
        .collect();
    ungrouped_supers.sort();

    for super_id in ungrouped_supers {
        let members = super_members.get(&super_id).cloned().unwrap_or_default();
        let intra = layout_ungrouped_cluster(diagram, &members, graph, sizes, reversed);
        blocks.push(MacroBlock {
            id: super_id,
            is_group: false,
            width: intra.content_width,
            height: intra.content_height,
            x: 0.0,
            y: 0.0,
            intra,
        });
    }

    blocks
}

/// 每条跨组边为层间距额外增加的像素
const CROSS_EDGE_LAYER_GAP_SCALE: f64 = 12.0;
/// 层间距额外增加的上限
const MAX_EXTRA_LAYER_GAP: f64 = 80.0;
/// 相邻 rank 组对跨组边为垂直间隙额外增加的像素
const CROSS_EDGE_PAIR_VERTICAL_GAP_SCALE: f64 = 10.0;
/// 组对垂直间隙额外增加的上限
const MAX_EXTRA_PAIR_VERTICAL_GAP: f64 = 56.0;
/// 每条同 rank 跨组边为组间距额外增加的像素（lane_budget）
const CROSS_EDGE_GROUP_GAP_SCALE: f64 = 8.0;
/// 组间距额外增加的上限（Phase 2：与 corridor_load 预算对齐，略抬高）
const MAX_EXTRA_GROUP_GAP: f64 = 56.0;

/// 计算相邻组块间的间距（Phase 2：lane_budget ↔ 跨组边负载）。
///
/// `gap = GROUP_GAP_X + min(edge_count × scale, max_extra)`。
/// 高负载 pair 预留更宽通道，降低事后 PRS 扩壳。
pub(super) fn adaptive_group_gap(pair_edge_count: usize) -> f64 {
    let extra = (pair_edge_count as f64 * CROSS_EDGE_GROUP_GAP_SCALE).min(MAX_EXTRA_GROUP_GAP);
    GROUP_GAP_X + extra
}

/// 同一 RankBand 内统一水平间距：取该行所有相邻 pair 的 lane_budget 最大值。
pub(super) fn band_uniform_gap(
    ordered_ids: &[String],
    pair_edge_counts: &HashMap<(String, String), usize>,
) -> f64 {
    if ordered_ids.len() < 2 {
        return GROUP_GAP_X;
    }
    let mut max_gap = GROUP_GAP_X;
    for w in ordered_ids.windows(2) {
        let pair = if w[0] <= w[1] {
            (w[0].clone(), w[1].clone())
        } else {
            (w[1].clone(), w[0].clone())
        };
        let count = pair_edge_counts.get(&pair).copied().unwrap_or(0);
        max_gap = max_gap.max(adaptive_group_gap(count));
    }
    max_gap
}

/// 相邻 macro rank 之间：取 rank 总跨组边密度与「上下行组对」最大边数的较大值，放大垂直通道。
pub(super) fn adaptive_vertical_rank_gap<B: crate::layout::recipes::architecture::group_sizing::GroupWidthBlock>(
    rank: usize,
    blocks: &[B],
    macro_ranks: &HashMap<String, usize>,
    cross_edge_counts: &HashMap<usize, usize>,
    pair_edge_counts: &HashMap<(String, String), usize>,
) -> f64 {
    let from_rank = cross_edge_counts
        .get(&rank)
        .map(|&c| (c as f64 * CROSS_EDGE_LAYER_GAP_SCALE).min(MAX_EXTRA_LAYER_GAP))
        .unwrap_or(0.0);

    let mut ids_a: Vec<String> = blocks
        .iter()
        .filter(|b| b.is_group_block() && macro_ranks.get(b.block_id()).copied() == Some(rank))
        .map(|b| b.block_id().to_string())
        .collect();
    let mut ids_b: Vec<String> = blocks
        .iter()
        .filter(|b| b.is_group_block() && macro_ranks.get(b.block_id()).copied() == Some(rank + 1))
        .map(|b| b.block_id().to_string())
        .collect();
    ids_a.sort();
    ids_b.sort();

    let mut pair_max = 0usize;
    for a in &ids_a {
        for b in &ids_b {
            let pair = if a <= b {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            };
            pair_max = pair_max.max(pair_edge_counts.get(&pair).copied().unwrap_or(0));
        }
    }
    let from_pair =
        (pair_max as f64 * CROSS_EDGE_PAIR_VERTICAL_GAP_SCALE).min(MAX_EXTRA_PAIR_VERTICAL_GAP);

    from_rank.max(from_pair)
}

/// 统计每对相邻 macro rank 之间的跨组边数
///
/// 返回 `gap_rank -> cross_edge_count`，其中 `gap_rank = min(from_rank, to_rank)`，
/// 表示该 rank 到下一 rank 之间的跨组边密度。
pub(super) fn count_cross_edges_per_rank_gap(
    super_edges: &HashSet<(String, String)>,
    macro_ranks: &HashMap<String, usize>,
) -> HashMap<usize, usize> {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for (from, to) in super_edges {
        let from_rank = macro_ranks.get(from).copied().unwrap_or(0);
        let to_rank = macro_ranks.get(to).copied().unwrap_or(0);
        if from_rank == to_rank {
            continue;
        }
        let gap = from_rank.min(to_rank);
        *counts.entry(gap).or_insert(0) += 1;
    }
    counts
}

pub(super) fn position_macro_blocks(
    blocks: &mut [MacroBlock],
    macro_ranks: &HashMap<String, usize>,
    super_edges: &HashSet<(String, String)>,
    pair_edge_counts: &HashMap<(String, String), usize>,
    canvas_padding: f64,
    row_align: RowAlign,
    group_decl: &HashMap<String, usize>,
) -> HashMap<String, usize> {
    if blocks.is_empty() {
        return HashMap::new();
    }
    position_macro_blocks_stacked(
        blocks,
        macro_ranks,
        super_edges,
        pair_edge_counts,
        canvas_padding,
        row_align,
        group_decl,
    )
}

/// 逐 macro rank 纵向堆叠（单块 rank 独占一行）。
///
/// 返回 `block_id -> macro_rank`（视觉行 == macro rank，行为与改造前一致）。
fn position_macro_blocks_stacked(
    blocks: &mut [MacroBlock],
    macro_ranks: &HashMap<String, usize>,
    super_edges: &HashSet<(String, String)>,
    pair_edge_counts: &HashMap<(String, String), usize>,
    canvas_padding: f64,
    row_align: RowAlign,
    group_decl: &HashMap<String, usize>,
) -> HashMap<String, usize> {
    let max_rank = macro_ranks.values().copied().max().unwrap_or(0);
    let cross_edge_counts = count_cross_edges_per_rank_gap(super_edges, macro_ranks);
    let mut y_cursor = canvas_padding;

    for rank in 0..=max_rank {
        let mut rank_indices: Vec<usize> = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| macro_ranks.get(&b.id).copied().unwrap_or(0) == rank)
            .map(|(i, _)| i)
            .collect();
        rank_indices.sort_by(|&a, &b| {
            crate::layout::decl_order::cmp_by_decl_then_id(group_decl, &blocks[a].id, &blocks[b].id)
        });

        if rank_indices.is_empty() {
            continue;
        }

        let max_height = rank_indices
            .iter()
            .map(|&i| blocks[i].height)
            .fold(0.0_f64, f64::max);

        if rank_indices.len() == 1 {
            let i = rank_indices[0];
            blocks[i].x = canvas_padding;
            blocks[i].y = y_cursor;
        } else {
            // Iteration 2：band 内统一 lane_budget gap（取相邻 pair 最大值）
            let ordered_ids: Vec<String> =
                rank_indices.iter().map(|&i| blocks[i].id.clone()).collect();
            let gap = band_uniform_gap(&ordered_ids, pair_edge_counts);
            let mut x_cursor = canvas_padding;
            for (pos, &i) in rank_indices.iter().enumerate() {
                blocks[i].x = x_cursor;
                blocks[i].y = y_cursor;
                x_cursor += blocks[i].width;
                if pos + 1 < rank_indices.len() {
                    x_cursor += gap;
                }
            }
        }

        let extra_layer_gap = adaptive_vertical_rank_gap(
            rank,
            blocks,
            macro_ranks,
            &cross_edge_counts,
            pair_edge_counts,
        );
        let effective_layer_gap = LAYER_GAP + extra_layer_gap;

        y_cursor += max_height + effective_layer_gap;
    }

    if row_align == RowAlign::Center {
        center_rank_rows(macro_ranks, blocks.len(), |i| {
            (blocks[i].id.clone(), blocks[i].x, blocks[i].width)
        })
        .into_iter()
        .for_each(|(i, shift)| blocks[i].x += shift);
    }

    blocks
        .iter()
        .map(|b| (b.id.clone(), macro_ranks.get(&b.id).copied().unwrap_or(0)))
        .collect()
}

// ─── Phase C: 全局坐标回填（R3-5：见 `expand_global_layout`） ─

