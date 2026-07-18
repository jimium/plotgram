//! zu nei bu ju
//!
//! moved from two_phase.rs (A4, behavior unchanged).

use super::*;

pub(super) fn layout_intra_group(
    diagram: &Diagram,
    group_id: &str,
    members: &[String],
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    reversed: &HashSet<(String, String)>,
) -> IntraLayout {
    if members.is_empty() {
        return IntraLayout {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: vec![],
        };
    }

    if members.len() == 1 {
        let id = &members[0];
        let (w, h) = sizes.get(id).copied().unwrap_or((
            constants::DEFAULT_NODE_WIDTH,
            constants::DEFAULT_NODE_HEIGHT,
        ));
        return IntraLayout {
            nodes: HashMap::from([(
                id.clone(),
                NodeLayout {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                    ..Default::default()
                },
            )]),
            content_width: w,
            content_height: h,
            layers: vec![vec![id.clone()]],
        };
    }

    let member_set: HashSet<String> = members.iter().cloned().collect();
    let intra_map = synthetic_group_map(group_id, members);

    let hint = diagram
        .find_group(group_id)
        .map(|g| resolve_group_layout_hint(g, diagram.diagram_type.clone()))
        .unwrap_or(GroupLayoutHint::Auto);
    let mode = resolve_group_layout_mode(hint, members, graph, reversed);

    // Phase 3：复杂拓扑 / Sugiyama 模式委托 sugiyama_v2（hint 几何模式仍走本地路径）
    if mode == GroupLayoutMode::Sugiyama {
        return super::super::intra_sugiyama::layout_intra_with_sugiyama_v2(diagram, members);
    }

    let ranks = assign_ranks_for_mode(&mode, members, graph, reversed);
    let decl_index: HashMap<String, usize> = members
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    let layers = build_layers(&ranks, &decl_index);
    let mut ordered_layers =
        order_layers_group_aware(graph, &intra_map, &layers, reversed, &decl_index);

    let member_set: HashSet<String> = members.iter().cloned().collect();
    let space_budget = crate::layout::space_budget::SpaceBudget::from_diagram(diagram);
    let mut nodes = assign_coordinates_intra(
        graph,
        &ordered_layers,
        sizes,
        &member_set,
        Some(&space_budget),
        reversed,
    );

    center_group_hub_nodes(
        graph,
        &intra_map,
        &ordered_layers,
        sizes,
        &mut nodes,
        reversed,
    );
    align_client_nodes_to_hubs(
        graph,
        &intra_map,
        &ordered_layers,
        sizes,
        &mut nodes,
        reversed,
    );

    if mode == GroupLayoutMode::Vertical {
        align_nodes_in_column(&mut nodes);
    }

    normalize_to_origin(&mut nodes);
    let (mut content_width, mut content_height) = content_bbox(&nodes);

    // 过扁组（宽 >> 高）回退 Grid，改善 private_subnet 类单行布局
    const MIN_GROUP_ASPECT: f64 = 0.25;
    if mode == GroupLayoutMode::Horizontal
        && members.len() >= 3
        && content_width > f64::EPSILON
        && content_height < content_width * MIN_GROUP_ASPECT
    {
        let grid_mode = GroupLayoutMode::Grid;
        let ranks = assign_ranks_for_mode(&grid_mode, members, graph, reversed);
        let layers = build_layers(&ranks, &decl_index);
        ordered_layers =
            order_layers_group_aware(graph, &intra_map, &layers, reversed, &decl_index);
        nodes = assign_coordinates_intra(
            graph,
            &ordered_layers,
            sizes,
            &member_set,
            Some(&space_budget),
            reversed,
        );
        center_group_hub_nodes(
            graph,
            &intra_map,
            &ordered_layers,
            sizes,
            &mut nodes,
            reversed,
        );
        align_client_nodes_to_hubs(
            graph,
            &intra_map,
            &ordered_layers,
            sizes,
            &mut nodes,
            reversed,
        );
        normalize_to_origin(&mut nodes);
        (content_width, content_height) = content_bbox(&nodes);
    }

    IntraLayout {
        nodes,
        content_width,
        content_height,
        layers: ordered_layers.clone(),
    }
}

/// 递归版组内布局：支持嵌套分组
///
/// - 叶子组（无子组）：走 `layout_intra_group` 原逻辑
/// - 容器组（有子组）：递归布局每个子组，然后将子组视为宏观块做组间定位
///
/// 容器组的 IntraLayout 包含所有后代节点的局部坐标（相对容器组内容区原点），
/// layers 反映宏观层级（同 macro rank 的子组 intra layer 对齐）。
pub(super) fn layout_intra_group_recursive(
    diagram: &Diagram,
    group_id: &str,
    group_tree: &GroupTree,
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    reversed: &HashSet<(String, String)>,
    padding: &GroupPadding,
) -> IntraLayout {
    let children = group_tree.children_of(group_id);
    let direct_entities = group_tree.entities_of(group_id).to_vec();

    // 叶子组：走原逻辑
    if children.is_empty() {
        let all_members = group_tree.descendant_entities(group_id);
        return layout_intra_group(diagram, group_id, &all_members, graph, sizes, reversed);
    }

    // 容器组：递归布局子组 + 直接实体
    // 1. 递归布局每个子组
    let mut child_intras: HashMap<String, IntraLayout> = HashMap::new();
    for child_id in children {
        let child_intra = layout_intra_group_recursive(
            diagram, child_id, group_tree, graph, sizes, reversed, padding,
        );
        child_intras.insert(child_id.clone(), child_intra);
    }

    // 2. 直接实体作为"无组节点块"布局（若有）
    let direct_intra = if direct_entities.is_empty() {
        None
    } else {
        Some(layout_ungrouped_cluster(
            diagram,
            &direct_entities,
            graph,
            sizes,
            reversed,
        ))
    };

    // 3. 构建宏观块（子组块 + 直接实体块）
    let mut blocks: Vec<IntraMacroBlock> = Vec::new();
    for child_id in children {
        let intra = child_intras.get(child_id).cloned().unwrap_or(IntraLayout {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: vec![],
        });
        blocks.push(IntraMacroBlock {
            id: child_id.clone(),
            is_group: true,
            width: intra.content_width + padding.horizontal_extent(),
            height: intra.content_height + padding.vertical_extent(),
            x: 0.0,
            y: 0.0,
            intra,
        });
    }
    if let Some(di) = &direct_intra {
        blocks.push(IntraMacroBlock {
            id: format!("@direct:{group_id}"),
            is_group: false,
            width: di.content_width,
            height: di.content_height,
            x: 0.0,
            y: 0.0,
            intra: di.clone(),
        });
    }

    // 4. 构建超级节点图（基于跨子组边）
    let (super_members, super_edges, pair_edge_counts, edge_weights) =
        build_super_graph_for_group(group_id, group_tree, graph, reversed);
    let group_decl = crate::layout::decl_order::group_sibling_decl_index(diagram);
    // 约束边映射到组内超级节点级别
    let node_to_super: HashMap<&str, &str> = super_members
        .iter()
        .flat_map(|(super_id, members)| {
            members.iter().map(move |m| (m.as_str(), super_id.as_str()))
        })
        .collect();
    let constraint_super_edges: HashSet<(String, String)> = diagram
        .constraints
        .iter()
        .filter_map(|c| {
            let from_super = node_to_super.get(c.from.as_str())?;
            let to_super = node_to_super.get(c.to.as_str())?;
            if from_super != to_super {
                Some((from_super.to_string(), to_super.to_string()))
            } else {
                None
            }
        })
        .collect();
    let macro_ranks = assign_super_macro_ranks(
        &super_members,
        &super_edges,
        &edge_weights,
        &graph.node_ids,
        &group_decl,
        &constraint_super_edges,
    );

    // 4.5 嵌套 sibling：Phase 1 起 Equal 仅由 L1 执行；此处只保留 content-fit 初值。
    let _child_group_ids: Vec<String> = children.to_vec();

    // 5. 宏观定位（初值左对齐；L1 完成 Center）
    position_intra_macro_blocks(
        &mut blocks,
        &macro_ranks,
        &super_edges,
        &pair_edge_counts,
        RowAlign::Start,
    );

    // 6. 合并为单个 IntraLayout
    compose_intra_layout_recursive(group_id, &blocks, padding, &child_intras, &direct_intra)
}

/// 容器组内宏观块定位（复用顶层 position_macro_blocks 逻辑，但 padding=0）
pub(super) fn position_intra_macro_blocks(
    blocks: &mut [IntraMacroBlock],
    macro_ranks: &HashMap<String, usize>,
    super_edges: &HashSet<(String, String)>,
    pair_edge_counts: &HashMap<(String, String), usize>,
    row_align: RowAlign,
) {
    if blocks.is_empty() {
        return;
    }

    let max_rank = macro_ranks.values().copied().max().unwrap_or(0);
    let cross_edge_counts = count_cross_edges_per_rank_gap(super_edges, macro_ranks);
    let mut y_cursor = 0.0;

    for rank in 0..=max_rank {
        let mut rank_indices: Vec<usize> = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| macro_ranks.get(&b.id).copied().unwrap_or(0) == rank)
            .map(|(i, _)| i)
            .collect();
        rank_indices.sort_by(|&a, &b| blocks[a].id.cmp(&blocks[b].id));

        if rank_indices.is_empty() {
            continue;
        }

        let max_height = rank_indices
            .iter()
            .map(|&i| blocks[i].height)
            .fold(0.0_f64, f64::max);

        if rank_indices.len() == 1 {
            let i = rank_indices[0];
            blocks[i].x = 0.0;
            blocks[i].y = y_cursor;
        } else {
            // Iteration 2：band 内统一 lane_budget gap
            let ordered_ids: Vec<String> =
                rank_indices.iter().map(|&i| blocks[i].id.clone()).collect();
            let gap = band_uniform_gap(&ordered_ids, pair_edge_counts);
            let mut x_cursor = 0.0;
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
}

/// 计算每个宏观块的行居中偏移量。
///
/// 按 rank 分行，行宽 = 行内块的最大右边界 - origin；
/// 最宽行保持不动，窄行整体右移 `(max_row_width - row_width) / 2`。
/// 返回 `(block_index, shift_x)` 列表（shift 为 0 的块不返回）。
pub(super) fn center_rank_rows(
    macro_ranks: &HashMap<String, usize>,
    block_count: usize,
    block_info: impl Fn(usize) -> (String, f64, f64),
) -> Vec<(usize, f64)> {
    // rank → (行右边界, 行内块索引)
    let mut rows: HashMap<usize, (f64, Vec<usize>)> = HashMap::new();
    for i in 0..block_count {
        let (id, x, width) = block_info(i);
        let rank = macro_ranks.get(&id).copied().unwrap_or(0);
        let entry = rows.entry(rank).or_insert((f64::NEG_INFINITY, Vec::new()));
        entry.0 = entry.0.max(x + width);
        entry.1.push(i);
    }

    let max_extent = rows
        .values()
        .map(|(extent, _)| *extent)
        .fold(f64::NEG_INFINITY, f64::max);
    if !max_extent.is_finite() {
        return Vec::new();
    }

    let mut shifts = Vec::new();
    let mut ranks: Vec<usize> = rows.keys().copied().collect();
    ranks.sort_unstable();
    for rank in ranks {
        let (extent, indices) = &rows[&rank];
        let shift = (max_extent - extent) / 2.0;
        if shift > f64::EPSILON {
            for &i in indices {
                shifts.push((i, shift));
            }
        }
    }
    shifts
}

/// 合并容器组内的宏观块为单个 IntraLayout
///
/// - 节点坐标：block.x + padding.x + local.x（组块）或 block.x + local.x（直接实体块）
/// - layers：按 macro rank 顺序，同 rank 内对齐各 block 的 intra layer
pub(super) fn compose_intra_layout_recursive(
    _group_id: &str,
    blocks: &[IntraMacroBlock],
    padding: &GroupPadding,
    _child_intras: &HashMap<String, IntraLayout>,
    _direct_intra: &Option<IntraLayout>,
) -> IntraLayout {
    let mut nodes: HashMap<String, NodeLayout> = HashMap::new();
    let mut max_x = 0.0_f64;
    let mut max_y = 0.0_f64;

    for block in blocks {
        let (offset_x, offset_y) = if block.is_group {
            (block.x + padding.left, block.y + padding.top)
        } else {
            (block.x, block.y)
        };
        for (nid, local) in &block.intra.nodes {
            let nx = offset_x + local.x;
            let ny = offset_y + local.y;
            max_x = max_x.max(nx + local.width);
            max_y = max_y.max(ny + local.height);
            nodes.insert(
                nid.clone(),
                NodeLayout {
                    x: nx,
                    y: ny,
                    width: local.width,
                    height: local.height,
                    ..Default::default()
                },
            );
        }
    }

    // 重建 layers：按 block 的 y 顺序，合并 y 接近的 block 的 intra layers
    // 简化策略：直接按 block 顺序拼接 intra.layers（宏观定位已保证 y 不重叠）
    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut sorted_blocks: Vec<&IntraMacroBlock> = blocks.iter().collect();
    sorted_blocks.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.id.cmp(&b.id))
    });

    for block in &sorted_blocks {
        for intra_layer in &block.intra.layers {
            // 过滤掉不属于当前块的节点（防御性）
            let filtered: Vec<String> = intra_layer
                .iter()
                .filter(|n| nodes.contains_key(*n))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                layers.push(filtered);
            }
        }
    }

    IntraLayout {
        nodes,
        content_width: max_x,
        content_height: max_y,
        layers,
    }
}

/// 无组节点簇的局部水平布局（如 db + mq）
pub(super) fn layout_ungrouped_cluster(
    diagram: &Diagram,
    members: &[String],
    graph: &GraphIndex,
    sizes: &HashMap<String, (f64, f64)>,
    reversed: &HashSet<(String, String)>,
) -> IntraLayout {
    if members.is_empty() {
        return IntraLayout {
            nodes: HashMap::new(),
            content_width: 0.0,
            content_height: 0.0,
            layers: vec![],
        };
    }

    if members.len() == 1 {
        return layout_intra_group(diagram, "@solo", members, graph, sizes, reversed);
    }

    let ranks = assign_intra_ranks(members, graph, reversed);
    let decl_index: HashMap<String, usize> = members
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    let layers = build_layers(&ranks, &decl_index);
    let member_set: HashSet<String> = members.iter().cloned().collect();

    let mut nodes = assign_coordinates_intra(
        graph,
        &layers,
        sizes,
        &member_set,
        Some(&crate::layout::space_budget::SpaceBudget::from_diagram(
            diagram,
        )),
        reversed,
    );
    normalize_to_origin(&mut nodes);
    let (content_width, content_height) = content_bbox(&nodes);

    IntraLayout {
        nodes,
        content_width,
        content_height,
        layers,
    }
}

pub(super) fn synthetic_group_map(group_id: &str, members: &[String]) -> GroupMap {
    let mut node_to_top_group = HashMap::new();
    for member in members {
        node_to_top_group.insert(member.clone(), group_id.to_string());
    }

    GroupMap {
        node_to_top_group,
        top_group_members: HashMap::from([(group_id.to_string(), members.to_vec())]),
        top_groups: vec![group_id.to_string()],
        ungrouped: vec![],
    }
}

/// 组内坐标分配：局部原点，邻接拉力仅限组内成员
pub(super) fn assign_coordinates_intra(
    graph: &GraphIndex,
    layers: &[Vec<String>],
    sizes: &HashMap<String, (f64, f64)>,
    member_set: &HashSet<String>,
    budget: Option<&crate::layout::space_budget::SpaceBudget>,
    reversed: &HashSet<(String, String)>,
) -> HashMap<String, NodeLayout> {
    let mut nodes = HashMap::new();

    let layer_heights: Vec<f64> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .map(|node| {
                    sizes
                        .get(node)
                        .map(|(_, h)| *h)
                        .unwrap_or(constants::DEFAULT_NODE_HEIGHT)
                })
                .fold(0.0_f64, f64::max)
        })
        .collect();

    let mut layer_y_offsets = vec![0.0];
    for i in 1..layers.len() {
        layer_y_offsets.push(layer_y_offsets[i - 1] + layer_heights[i - 1] + INTRA_LAYER_GAP);
    }

    for (layer_idx, layer) in layers.iter().enumerate() {
        let y_center = layer_y_offsets[layer_idx] + layer_heights[layer_idx] / 2.0;
        let mut positions = uniform_initial_positions(layer, sizes);

        let upper_x = if layer_idx > 0 {
            Some(layer_centers_from_placed(
                &layers[layer_idx - 1],
                &nodes,
                sizes,
            ))
        } else {
            None
        };
        let lower_x = if layer_idx + 1 < layers.len() {
            Some(layer_centers_from_placed(
                &layers[layer_idx + 1],
                &nodes,
                sizes,
            ))
        } else {
            None
        };

        for _ in 0..6 {
            if let Some(ref upper) = upper_x {
                pull_toward_neighbors(
                    layer,
                    &mut positions,
                    upper,
                    graph,
                    reversed,
                    Some(member_set),
                    true,
                    NEIGHBOR_PULL_FACTOR,
                );
            }
            if let Some(ref lower) = lower_x {
                pull_toward_neighbors(
                    layer,
                    &mut positions,
                    lower,
                    graph,
                    reversed,
                    Some(member_set),
                    false,
                    NEIGHBOR_PULL_FACTOR,
                );
            }
        }

        let adjusted = if let Some(b) = budget {
            resolve_x_overlaps_with_gaps(layer, &positions, sizes, |a, c| b.min_gap(a, c))
        } else {
            resolve_x_overlaps(layer, &positions, sizes)
        };

        for (i, node) in layer.iter().enumerate() {
            let (width, height) = sizes.get(node).copied().unwrap_or((
                constants::DEFAULT_NODE_WIDTH,
                constants::DEFAULT_NODE_HEIGHT,
            ));
            let x_center = adjusted[i];
            nodes.insert(
                node.clone(),
                NodeLayout {
                    x: x_center - width / 2.0,
                    y: y_center - height / 2.0,
                    width,
                    height,
                    ..Default::default()
                },
            );
        }
    }

    nodes
}

