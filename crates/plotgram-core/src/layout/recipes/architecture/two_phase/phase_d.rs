//! phase D postprocess
//!
//! moved from two_phase.rs (A4, behavior unchanged).

use super::*;

/// Phase D：节点落定后，单次物化组框（朴素容器 + RouteDemand side_gutters）。
///
/// G3：删除 EGB merge/equalize、sibling→expand→shrink、`apply_group_frame`。
/// 工程收口：corridor gutters 在同一次 `materialize` 写入组几何。
pub(super) fn phase_d_postprocess(
    diagram: &Diagram,
    nodes: &mut HashMap<String, NodeLayout>,
    groups: &mut HashMap<String, GroupLayout>,
    blocks: &[MacroBlock],
    block_row: &HashMap<String, usize>,
    graph: &GraphIndex,
    group_map: &GroupMap,
    sizes: &HashMap<String, (f64, f64)>,
    bounds_padding: GroupPadding,
    sizing: GroupSizingPolicy,
    reversed_edges: &HashSet<(String, String)>,
) -> LayoutResult {
    let layers = rebuild_layers_from_metadata(blocks, block_row);
    let facts = ArchDiagramFacts::from_diagram(diagram);
    // Phase 4.D：demand gaps / infra rebalance / same-rank y 已由 arch|intra builder
    // 的 MinSeparation + P1 objectives 覆盖，不再后处理扫节点。
    let _ = (&facts, graph, group_map, sizes, reversed_edges, sizing);
    clamp_to_canvas(nodes, sizes);

    // compose provisional 作废；seed → gutters → 单次 materialize。
    let _ = std::mem::take(groups);
    let t_egb = crate::layout::perf::Instant::now();
    let seed = crate::layout::kernel::common::group_bounds::compute_group_bounds_unrecorded(
        diagram,
        nodes,
        bounds_padding,
        crate::layout::kernel::common::group_bounds::container_padding_for_leaf(bounds_padding),
        None,
    );
    let mut side_gutters = estimate_side_gutters_with_hierarchy(diagram, nodes, &seed);
    {
        let corridor_model =
            crate::layout::demand::compute_corridor_model_from_groups(diagram, &seed);
        let pair_gaps =
            crate::layout::kernel::coordinate::group_ir::pair_gaps_from_corridor_demands(
                &corridor_model.demands,
                GROUP_GAP_X,
                crate::layout::demand::CORRIDOR_LANE_PITCH,
            );
        for ((a, b), need) in &pair_gaps {
            let half = (need - GROUP_GAP_X).max(0.0) * 0.5;
            if half <= 0.0 {
                continue;
            }
            for gid in [a.as_str(), b.as_str()] {
                let g = side_gutters.entry(gid.to_string()).or_default();
                g.left = g.left.max(half);
                g.right = g.right.max(half);
                g.top = g.top.max(half * 0.5);
                g.bottom = g.bottom.max(half * 0.5);
            }
        }
    }
    *groups = crate::layout::kernel::common::group_bounds::compute_group_bounds_with_side_gutters(
        diagram,
        nodes,
        bounds_padding,
        crate::layout::kernel::common::group_bounds::container_padding_for_leaf(bounds_padding),
        Some(&side_gutters),
    );
    let egb_ms = t_egb.elapsed().as_secs_f64() * 1000.0;

    let max_side_gutter = side_gutters
        .values()
        .flat_map(|g| [g.left, g.right, g.top, g.bottom])
        .fold(0.0_f64, f64::max);
    let gutter_budget_debug = crate::layout::GutterBudgetDebug {
        egb_ms,
        prs_ms: 0.0,
        prs_grew: false,
        max_side_gutter,
        canvas_area_delta_pct: 0.0,
    };

    let space_budget = crate::layout::demand::space_budget::SpaceBudget::from_diagram(diagram);

    let (total_width, total_height) =
        crate::layout::kernel::common::canvas_bounds::canvas_size(nodes, groups, PADDING);

    let sibling_corridors = crate::layout::group::build_sibling_corridors(diagram, groups);
    let corridors = crate::layout::group::merge_corridors(&sibling_corridors, groups);
    let group_routing = crate::layout::group::GroupRoutingHints {
        corridors,
        border_shell_pad: crate::layout::group::GROUP_BORDER_SHELL_PAD,
        side_gutters,
    };

    let sugiyama_ranks: HashMap<String, usize> = layers
        .iter()
        .enumerate()
        .flat_map(|(rank, layer)| layer.iter().map(move |id| (id.clone(), rank)))
        .collect();

    LayoutResult {
        nodes: std::mem::take(nodes),
        groups: std::mem::take(groups).into(),
        edges: vec![],
        total_width,
        total_height,
        hints: crate::layout::LayoutHints {
            edge_routing_style: crate::layout::EdgeRoutingStyle::Orthogonal,
            sugiyama_ranks: Some(sugiyama_ranks),
            group_routing: Some(group_routing),
            gutter_budget_debug: Some(gutter_budget_debug),
            space_budget: Some(space_budget),
            ..Default::default()
        },
    }
}

// ─── Phase A: 组内布局 ───────────────────────────────────

/// 跨组边端口微调的基础位移（像素），作为动态计算的下限
const CROSS_GROUP_NUDGE_BASE: f64 = 16.0;
/// 跨组边端口微调的最大位移（像素）
const CROSS_GROUP_NUDGE_MAX: f64 = 48.0;
/// 跨组边端口微调占组内可用宽度的比例
const CROSS_GROUP_NUDGE_WIDTH_RATIO: f64 = 0.3;
/// 跨组边端口微调占目标距离的比例
const CROSS_GROUP_NUDGE_DIST_RATIO: f64 = 0.3;
/// 跨组边端口 y 对齐的最大位移（像素）
const CROSS_GROUP_Y_ALIGN_MAX: f64 = 20.0;
/// 跨组边端口 y 对齐的比例系数
const CROSS_GROUP_Y_ALIGN_RATIO: f64 = 0.5;

/// Phase C+: 两阶段 spacing 微调
///
/// 组框已定后，对涉及跨组边的组内节点朝跨组边方向做动态 x 微调，
/// 减少跨组边折弯。这是"先定组框再微调组内节点"的反转步骤。
///
/// 算法：
/// 1. P2.1: y 对齐——同 macro rank 内跨组边的两端节点 y 中心对齐
/// 2. 遍历每条跨组边 (from_super → to_super)
/// 3. 找到 from_super / to_super 中实际参与跨组边的节点
/// 4. 计算两端节点 x 中心的方向与距离
/// 5. 将组内节点朝该方向动态移动
/// 6. 对同组同向多条跨组边，按目标 x 排序后按比例分布
pub(super) fn nudge_intra_nodes_toward_cross_group_edges(
    nodes: &mut HashMap<String, NodeLayout>,
    groups: &HashMap<String, GroupLayout>,
    super_edges: &HashSet<(String, String)>,
    super_members: &HashMap<String, Vec<String>>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) {
    if super_edges.is_empty() {
        return;
    }

    // ── P2.1: y 对齐阶段 ──
    // 同 macro rank 内，跨组边的两端节点 y 中心对齐。
    // 仅对同 macro rank（同 y 行）的跨组边对端节点做 y 微调，
    // 避免不同 macro rank 的节点被错误拉扯。
    nudge_cross_group_y_alignment(nodes, super_edges, super_members, graph, reversed);

    // ── x 微调阶段（原有逻辑） ──
    let node_targets =
        collect_cross_group_node_targets(super_edges, super_members, graph, reversed, nodes);
    let group_node_targets =
        compute_group_node_targets(&node_targets, super_members, graph, reversed, nodes);
    apply_nudge_per_group(&group_node_targets, groups, nodes);
}

/// 收集每个节点的跨组边目标信息：(node_id → Vec<target_cx>)。
/// target_cx 为跨组边对端节点的中心 x。
///
/// 排序保证迭代顺序确定（HashSet 迭代顺序随机），
/// 否则 node_targets 中每个 Vec<f64> 顺序随机，
/// f64 求和非结合性会导致 avg_target 1 ULP 差异 → desired_x 排序 tie → 最终位置抖动
pub(super) fn collect_cross_group_node_targets(
    super_edges: &HashSet<(String, String)>,
    super_members: &HashMap<String, Vec<String>>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    nodes: &HashMap<String, NodeLayout>,
) -> HashMap<String, Vec<f64>> {
    let mut node_targets: HashMap<String, Vec<f64>> = HashMap::new();
    let mut super_edges_sorted: Vec<&(String, String)> = super_edges.iter().collect();
    super_edges_sorted.sort();

    for (from_super, to_super) in super_edges_sorted {
        let from_members = match super_members.get(from_super) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };
        let to_members = match super_members.get(to_super) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };

        for from_node in from_members {
            let succs = graph.out_edges.get(from_node).cloned().unwrap_or_default();
            for succ in &succs {
                if !is_effective_edge(from_node, succ, reversed) {
                    continue;
                }
                if !to_members.contains(succ) {
                    continue;
                }
                let Some(to_nl) = nodes.get(succ) else {
                    continue;
                };
                let to_cx = to_nl.x + to_nl.width / 2.0;
                node_targets
                    .entry(from_node.clone())
                    .or_default()
                    .push(to_cx);
                // 反向：succ 也要朝 from_node 方向微调
                let Some(from_nl) = nodes.get(from_node) else {
                    continue;
                };
                let from_cx = from_nl.x + from_nl.width / 2.0;
                node_targets.entry(succ.clone()).or_default().push(from_cx);
            }
        }
    }
    node_targets
}

/// 按组收集同组节点，用于同向多边排序分布。
/// 跳过组内 hub（有组内后继的节点，如 gateway → services），
/// 它们需要保持居中于组内子节点，不应被跨组边拉开。
pub(super) fn compute_group_node_targets(
    node_targets: &HashMap<String, Vec<f64>>,
    super_members: &HashMap<String, Vec<String>>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
    nodes: &HashMap<String, NodeLayout>,
) -> HashMap<String, Vec<(String, f64, f64)>> {
    // (node_id, current_cx, avg_target_cx)
    let mut group_node_targets: HashMap<String, Vec<(String, f64, f64)>> = HashMap::new();

    for (node_id, targets) in node_targets {
        let Some(nl) = nodes.get(node_id) else {
            continue;
        };

        let group_id = super_members
            .iter()
            .find(|(_, members)| members.contains(node_id))
            .map(|(gid, _)| gid.clone());
        let Some(ref gid) = group_id else {
            continue;
        };
        let Some(group_members) = super_members.get(gid) else {
            continue;
        };
        let has_intra_successors = graph
            .out_edges
            .get(node_id)
            .map(|succs| {
                succs
                    .iter()
                    .any(|s| group_members.contains(s) && is_effective_edge(node_id, s, reversed))
            })
            .unwrap_or(false);
        if has_intra_successors {
            continue;
        }

        let current_cx = nl.x + nl.width / 2.0;
        let avg_target = targets.iter().sum::<f64>() / targets.len() as f64;

        group_node_targets.entry(gid.clone()).or_default().push((
            node_id.clone(),
            current_cx,
            avg_target,
        ));
    }
    group_node_targets
}

/// 对每组：计算动态微调（按 gid 排序保证确定性）。
/// 动态位移上限基于组宽和固定上限取小；
/// 按 desired_x 排序后强制保持最小间距，避免重叠；
/// 最后 clamp 到组框（先排序边界再 clamp，避免 release 下 f64::clamp panic）。
pub(super) fn apply_nudge_per_group(
    group_node_targets: &HashMap<String, Vec<(String, f64, f64)>>,
    groups: &HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
) {
    let mut group_ids: Vec<String> = group_node_targets.keys().cloned().collect();
    group_ids.sort();
    for gid in group_ids {
        let Some(entries) = group_node_targets.get(&gid) else {
            continue;
        };
        let Some(gl) = groups.get(&gid) else {
            continue;
        };
        let pad = GroupPadding::architecture().left;
        let group_min_x = gl.x + pad;
        let group_max_x = gl.x + gl.width - pad;
        let available_width = (group_max_x - group_min_x).max(0.0);

        // 动态位移上限：基于组宽和固定上限取小
        let width_based_cap = available_width * CROSS_GROUP_NUDGE_WIDTH_RATIO;
        let dynamic_cap = width_based_cap
            .min(CROSS_GROUP_NUDGE_MAX)
            .max(CROSS_GROUP_NUDGE_BASE);

        // 计算每个节点的期望新 x（左上角），保持原有顺序
        let mut planned: Vec<(String, f64, f64)> = entries
            .iter()
            .map(|(node_id, current_cx, target_cx)| {
                let nl = nodes.get(node_id).unwrap();
                let direction = target_cx - current_cx;
                let sign = direction.signum();
                let abs_dir = direction.abs();
                let proportional = (abs_dir * CROSS_GROUP_NUDGE_DIST_RATIO).min(dynamic_cap);
                let min_move = CROSS_GROUP_NUDGE_BASE.min(abs_dir);
                let delta = if abs_dir < f64::EPSILON {
                    0.0
                } else {
                    sign * proportional.max(min_move)
                };
                let desired_x = nl.x + delta;
                (node_id.clone(), nl.width, desired_x)
            })
            .collect();

        // 按 desired_x 排序，强制保持最小间距，避免重叠
        // 加 node_id tie-breaker，避免 desired_x 相同时保持 HashMap 迭代顺序（非确定）
        planned.sort_by(|a, b| {
            a.2.partial_cmp(&b.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let min_gap = super::super::layout::constants::NODE_GAP;
        let n = planned.len();
        for i in 1..n {
            let prev_right = planned[i - 1].2 + planned[i - 1].1;
            if planned[i].2 < prev_right + min_gap {
                planned[i].2 = prev_right + min_gap;
            }
        }
        // 反向再扫一次，防止右溢出导致左重叠
        for i in (0..n.saturating_sub(1)).rev() {
            let next_left = planned[i + 1].2;
            if planned[i].2 + planned[i].1 > next_left - min_gap {
                planned[i].2 = next_left - min_gap - planned[i].1;
            }
        }

        // 应用最终位置，clamp 到组框。
        // Fit 下节点宽 ≈ 组内可用宽时，浮点误差可能使 max < min；
        // 先排序边界再 clamp，避免 release 下 f64::clamp panic。
        for (node_id, width, desired_x) in planned {
            let Some(nl) = nodes.get_mut(&node_id) else {
                continue;
            };
            let node_min_x = group_min_x;
            let node_max_x = group_max_x - width;
            let lo = node_min_x.min(node_max_x);
            let hi = node_min_x.max(node_max_x);
            nl.x = desired_x.clamp(lo, hi);
        }
    }
}

/// P2.1: 跨组边端口 y 对齐
///
/// 对同 macro rank 内跨组边的两端节点做 y 中心对齐微调。
/// 当两个节点通过跨组边连接且处于同一 y 行（y 中心差 < LAYER_GAP）
/// 时，将两者 y 中心向中间值靠拢，减少跨组边的折弯数。
///
/// 仅微调，不改变节点所在层——位移上限为 `CROSS_GROUP_Y_ALIGN_MAX`。
pub(super) fn nudge_cross_group_y_alignment(
    nodes: &mut HashMap<String, NodeLayout>,
    super_edges: &HashSet<(String, String)>,
    super_members: &HashMap<String, Vec<String>>,
    graph: &GraphIndex,
    reversed: &HashSet<(String, String)>,
) {
    // 收集每对跨组边端点的 y 对齐目标：(node_id → Vec<target_cy>)
    let mut node_y_targets: HashMap<String, Vec<f64>> = HashMap::new();

    // 排序保证迭代顺序确定
    let mut super_edges_sorted: Vec<&(String, String)> = super_edges.iter().collect();
    super_edges_sorted.sort();

    for (from_super, to_super) in &super_edges_sorted {
        let from_members = match super_members.get(from_super) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };
        let to_members = match super_members.get(to_super) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };

        for from_node in from_members {
            let succs = graph.out_edges.get(from_node).cloned().unwrap_or_default();
            for succ in &succs {
                if !is_effective_edge(from_node, succ, reversed) {
                    continue;
                }
                if !to_members.contains(succ) {
                    continue;
                }
                let Some(from_nl) = nodes.get(from_node) else {
                    continue;
                };
                let Some(to_nl) = nodes.get(succ) else {
                    continue;
                };

                let from_cy = from_nl.y + from_nl.height / 2.0;
                let to_cy = to_nl.y + to_nl.height / 2.0;

                // 仅对同 y 行的节点做 y 对齐（y 中心差 < LAYER_GAP）
                if (from_cy - to_cy).abs() < LAYER_GAP {
                    node_y_targets
                        .entry(from_node.clone())
                        .or_default()
                        .push(to_cy);
                    node_y_targets
                        .entry(succ.clone())
                        .or_default()
                        .push(from_cy);
                }
            }
        }
    }

    // 按 node_id 排序保证确定性
    let mut sorted_targets: Vec<_> = node_y_targets.into_iter().collect();
    sorted_targets.sort_by(|a, b| a.0.cmp(&b.0));

    for (node_id, targets) in sorted_targets {
        let Some(nl) = nodes.get_mut(&node_id) else {
            continue;
        };
        let current_cy = nl.y + nl.height / 2.0;
        let avg_target_cy = targets.iter().sum::<f64>() / targets.len() as f64;
        let direction = avg_target_cy - current_cy;
        let abs_dir = direction.abs();
        if abs_dir < f64::EPSILON {
            continue;
        }
        let delta = (abs_dir * CROSS_GROUP_Y_ALIGN_RATIO)
            .min(CROSS_GROUP_Y_ALIGN_MAX)
            .copysign(direction);
        nl.y += delta;
    }
}

/// 从元数据重建全局层列表，供基础设施行居中使用
///
/// 旧版 `rebuild_layers_from_positions` 从 y 坐标反推层（依赖 4px epsilon，
/// 相邻层 y 接近时会误合并）。本版直接从**视觉行号**（P1-4 shelf 装箱产出，
/// gate 关闭时等于 macro rank）+ intra layers 元数据重建，确定性且无 epsilon 依赖。
pub(super) fn rebuild_layers_from_metadata(
    blocks: &[MacroBlock],
    block_row: &HashMap<String, usize>,
) -> Vec<Vec<String>> {
    if blocks.is_empty() {
        return vec![];
    }

    let max_row = block_row.values().copied().max().unwrap_or(0);

    // 收集每个视觉行下的 block，按 id 排序保证确定性
    let mut rank_blocks: Vec<Vec<usize>> = vec![Vec::new(); max_row + 1];
    for (i, b) in blocks.iter().enumerate() {
        let r = block_row.get(&b.id).copied().unwrap_or(0);
        rank_blocks[r].push(i);
    }
    for indices in &mut rank_blocks {
        indices.sort_by(|&a, &b| blocks[a].id.cmp(&blocks[b].id));
    }

    // 同一视觉行内，各 block 的 intra layer 0 对齐、layer 1 对齐……
    // 不同视觉行产出独立的全局层
    let mut global_layers: Vec<Vec<String>> = Vec::new();
    for indices in &rank_blocks {
        if indices.is_empty() {
            continue;
        }
        let max_intra_layers = indices
            .iter()
            .map(|&i| blocks[i].intra.layers.len())
            .max()
            .unwrap_or(0);
        for intra_idx in 0..max_intra_layers {
            let mut layer: Vec<String> = Vec::new();
            for &bi in indices {
                if let Some(intra_layer) = blocks[bi].intra.layers.get(intra_idx) {
                    layer.extend(intra_layer.iter().cloned());
                }
            }
            if !layer.is_empty() {
                global_layers.push(layer);
            }
        }
    }

    global_layers
}

// ─── 测试 ────────────────────────────────────────────────

