//! 端口 / slot 选择 + side 协调决策
//!
//! 从 `run.rs` 原样搬迁（A4 结构重构，行为不变）。

use super::super::*;
use crate::layout::edge::common::edge_geometry::{
    arrow_type_tag, canonical_pair, edge_line_style_signature, node_center, undirected_pair_key,
};
use std::collections::HashMap;

pub(crate) fn phase_port_slot(
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    group_ctx: &crate::layout::group::GroupRoutingContext,
    feedback_assignment: &feedback_side::FeedbackSideAssignment,
    cfg: &OrthoConfig,
    n: usize,
    s4_monitor_corridor: bool,
    horizontal: bool,
) -> (
    Vec<Port>,
    Vec<Port>,
    Vec<usize>,
    HashMap<(usize, bool), Endpoint>,
    crate::layout::edge::common::parallel_edges::ParallelGroups,
    std::collections::BTreeSet<String>,
) {
    // ── 1. 按无向节点对分组，并确定每条边的端口（连接边） ──
    let t1 = crate::layout::perf::Instant::now();
    let mut pair_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }

    let mut from_side = vec![Port::Bottom; n];
    let mut to_side = vec![Port::Top; n];
    let mut lane = vec![0usize; n];

    let mut pair_keys: Vec<String> = pair_groups.keys().cloned().collect();
    pair_keys.sort();
    for key in &pair_keys {
        let indices = &pair_groups[key];
        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());

        let (Some(a_nl), Some(b_nl)) = (nodes.get(can_from), nodes.get(can_to)) else {
            continue;
        };

        let (side_a, side_b) =
            choose_pair_sides_with_group(a_nl, b_nl, can_from, can_to, Some(group_ctx));

        for (l, &i) in indices.iter().enumerate() {
            let rel = &relations[i];
            if let Some(hint) = feedback_assignment.hints.get(&i) {
                from_side[i] = hint.from_side;
                to_side[i] = hint.to_side;
                lane[i] = hint.lane;
                continue;
            }
            if rel.from.as_str() == can_from {
                from_side[i] = side_a;
                to_side[i] = side_b;
            } else {
                from_side[i] = side_b;
                to_side[i] = side_a;
            }
            lane[i] = l;
        }
    }

    // ── 1b. 端口选择全局协调（同侧偏好，G8 修复） ──
    //
    // choose_pair_sides 逐对独立选端口，同一节点的多条边可能分散在不同侧出发，
    // 导致节点附近不必要的交叉。此阶段对每个节点的多条边做"同侧偏好"协调：
    // 统计各侧边数，让少数派边在几何可接受时切换到多数派侧。
    coordinate_port_sides(
        relations,
        nodes,
        &mut from_side,
        &mut to_side,
        Some(group_ctx),
    );
    // D4 P2：默认只「拒绝往超载侧合并」（见 coordinate 内）；主动分流需
    // PLOTGRAM_PORT_PRESSURE_RELIEVE=1（较激进，可能抬交叉，默认关）。
    if port_pressure_relieve_enabled() {
        relieve_overloaded_port_sides(relations, nodes, &mut from_side, &mut to_side);
    }
    apply_feedback_side_overrides(
        relations,
        feedback_assignment,
        &mut from_side,
        &mut to_side,
        &mut lane,
    );
    // S4.x：监控边同排侧廊被堵时改正对端口（须在 slot/endpoint 之前）
    if s4_monitor_corridor {
        feedback_side::apply_monitor_hub_escape_ports(
            relations,
            nodes,
            &mut from_side,
            &mut to_side,
            horizontal,
        );
    }
    align_fanin_target_sides(relations, nodes, &mut to_side);
    crate::perf_log!(
        "[perf]     step1_ports: {:.2}ms",
        t1.elapsed().as_secs_f64() * 1000.0
    );

    // ── 2. 为每个连接点分配磁吸 slot 坐标 ──
    //
    // 并线分组遵循三条设计规范：
    //   1. 不同箭头类型（Active/Passive/Bidirectional）不并线
    //   2. 不同线型（虚线/实线/dash pattern）不并线
    //   3. 仅当边「从同一节点出发」或「到达同一节点」时才并线（OR 语义）
    //      - 同源出边（都从 X 出发）→ 可并线
    //      - 同宿入边（都到达 X）→ 可并线
    //      - 一条出边 + 一条入边（在 X 上方向相反）→ 不并线
    //
    // 因此分组键 = (node_id, side, is_from, arrow_type, line_style)。
    // is_from 是端点级属性：同一条边在 from 端 is_from=true、在 to 端 is_from=false。
    // 同一 (node_id, side) 上可能存在多个并线子组：先为各子组分配互不重叠的
    // 锚点带中心（base_frac），再让子组内连接点围绕该中心按 DockingStrategy 分布。
    let mut bundling_endpoints: HashMap<String, Vec<Endpoint>> = HashMap::new();
    for i in 0..n {
        let rel = &relations[i];
        let from_id = rel.from.as_str();
        let to_id = rel.to.as_str();
        let (Some(from_nl), Some(to_nl)) = (nodes.get(from_id), nodes.get(to_id)) else {
            continue;
        };
        let from_center = node_center(from_nl);
        let to_center = node_center(to_nl);
        let fcx = from_center.x;
        let fcy = from_center.y;
        let tcx = to_center.x;
        let tcy = to_center.y;

        bundling_endpoints
            .entry(endpoint_bundling_key(from_id, from_side[i], true, rel))
            .or_default()
            .push(Endpoint {
                edge_index: i,
                is_from: true,
                target_x: tcx,
                target_y: tcy,
                lane: lane[i],
                node_id: from_id.to_string(),
                side: from_side[i],
                anchor: Point::zero(),
            });
        bundling_endpoints
            .entry(endpoint_bundling_key(to_id, to_side[i], false, rel))
            .or_default()
            .push(Endpoint {
                edge_index: i,
                is_from: false,
                target_x: fcx,
                target_y: fcy,
                lane: lane[i],
                node_id: to_id.to_string(),
                side: to_side[i],
                anchor: Point::zero(),
            });
    }

    // 按 (node_id, side) 聚合并线子组，便于在同一节点同一侧上为各子组分配互不重叠的锚点带
    let mut side_groups: HashMap<(String, Port), Vec<Vec<Endpoint>>> = HashMap::new();
    for (_, endpoints) in bundling_endpoints {
        if endpoints.is_empty() {
            continue;
        }
        let node_id = endpoints[0].node_id.clone();
        let side = endpoints[0].side;
        side_groups
            .entry((node_id, side))
            .or_default()
            .push(endpoints);
    }

    // endpoint_map: (edge_index, is_from) -> Endpoint (with anchor filled in)
    let mut endpoint_map: HashMap<(usize, bool), Endpoint> = HashMap::new();
    let mut side_group_keys: Vec<(String, Port)> = side_groups.keys().cloned().collect();
    side_group_keys.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (node_id, side) in side_group_keys {
        let Some(mut sub_groups) = side_groups.remove(&(node_id.clone(), side)) else {
            continue;
        };
        let Some(nl) = nodes.get(&node_id) else {
            continue;
        };
        let vertical_side = is_vertical_port(side);
        let edge_len = if vertical_side { nl.width } else { nl.height };

        // 子组内沿切线方向排序：上/下边按目标 x，左/右边按目标 y；同位置再按 lane
        for endpoints in sub_groups.iter_mut() {
            endpoints.sort_by(|p, q| {
                let pk = if vertical_side {
                    p.target_x
                } else {
                    p.target_y
                };
                let qk = if vertical_side {
                    q.target_x
                } else {
                    q.target_y
                };
                pk.partial_cmp(&qk)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(p.lane.cmp(&q.lane))
                    .then(p.edge_index.cmp(&q.edge_index))
            });
        }
        // 子组间按 (arrow_type, line_style, min_edge_index) 排序。
        // 排序键不含 is_from：同一 edge 在两端节点的 is_from 相反，若用 is_from
        // 排序会导致两端排名不一致 → base_frac 不同 → 路径非直线。min_edge_index
        // 作为稳定 tiebreaker，保证同一 edge 在两端子组中获得相同排名。
        sub_groups.sort_by(|a, b| {
            sub_group_sort_key(a, relations)
                .cmp(&sub_group_sort_key(b, relations))
                .then_with(|| {
                    a.iter()
                        .map(|e| e.edge_index)
                        .min()
                        .cmp(&b.iter().map(|e| e.edge_index).min())
                })
        });

        let k = sub_groups.len();
        // 本侧实际挂载端点数；≥ TRIG 时仅温和加大 Compact 组内 pitch（不改 Concentrate / 子组带）
        let side_load: usize = sub_groups.iter().map(|g| g.len()).sum();
        let high_pressure = port_pressure_slot_enabled() && side_load >= PORT_PRESSURE_TRIG;
        let compact_pitch = if high_pressure {
            pressure_aware_slot_pitch(cfg.slot_pitch, side_load, edge_len)
        } else {
            cfg.slot_pitch.min(COMPACT_SLOT_PITCH)
        };

        for (group_rank, endpoints) in sub_groups.iter().enumerate() {
            let count = endpoints.len();
            let strategy = choose_docking_strategy(count);
            let base_frac = if k <= 1 {
                0.5
            } else {
                slot_fraction(group_rank, k, edge_len, cfg.slot_pitch)
            };

            // V2：同侧入出混合时禁用 Concentrate 共心（多边汇流会抹掉子组带分离）。
            // 锚点分桶已含 is_from；共竖干间距由 lane / enforce_reverse_pair_min_gap 收口（O1）。
            let mixed_inout = k >= 2
                && sub_groups.iter().any(|g| g.iter().any(|e| e.is_from))
                && sub_groups.iter().any(|g| g.iter().any(|e| !e.is_from));
            let strategy = if mixed_inout && matches!(strategy, DockingStrategy::Concentrate) {
                DockingStrategy::Compact
            } else {
                strategy
            };

            for (rank, ep) in endpoints.iter().enumerate() {
                let frac = match strategy {
                    DockingStrategy::Single | DockingStrategy::Concentrate => base_frac,
                    DockingStrategy::Compact => {
                        slot_fraction_around(rank, count, edge_len, compact_pitch, base_frac)
                    }
                };
                let anchor = slot_anchor(nl, side, frac);
                endpoint_map.insert(
                    (ep.edge_index, ep.is_from),
                    Endpoint {
                        edge_index: ep.edge_index,
                        is_from: ep.is_from,
                        target_x: ep.target_x,
                        target_y: ep.target_y,
                        lane: ep.lane,
                        node_id: ep.node_id.clone(),
                        side: ep.side,
                        anchor,
                    },
                );
            }
        }
    }

    // 平行边切线偏移：仅 A↔B 正反向对对称错开；同向多边由 slot 分布处理。
    let parallel = crate::layout::edge::common::parallel_edges::group_parallel_edges(
        relations,
        crate::layout::constants::DEFAULT_EDGE_OFFSET,
    );
    let mut reverse_pairs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pair_groups: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }
    for (key, indices) in &pair_groups {
        if indices.len() < 2 {
            continue;
        }
        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());
        let mut has_forward = false;
        let mut has_backward = false;
        for &i in indices {
            let rel = &relations[i];
            if rel.from.as_str() == can_from && rel.to.as_str() == can_to {
                has_forward = true;
            } else {
                has_backward = true;
            }
        }
        if has_forward && has_backward {
            reverse_pairs.insert(key.clone());
        }
    }
    for ((edge_index, _), ep) in endpoint_map.iter_mut() {
        let rel = &relations[*edge_index];
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        if !reverse_pairs.contains(&key) {
            continue;
        }
        let offset = parallel.offsets[*edge_index];
        if offset.abs() < EPS {
            continue;
        }
        if is_vertical_port(ep.side) {
            ep.anchor.x += offset;
        } else {
            ep.anchor.y += offset;
        }
    }

    // ── 2b. 直连偏好对齐：正对端口边的 slot 锚点对齐修正 ──
    //
    // 问题场景：两个垂直/水平排列的节点，端口选择为正对(Bottom→Top/Left→Right)，
    // 但因两端同侧边数不同导致 slot frac 不对称，锚点不对齐，产生不必要的小弯折。
    // 例如：db_master(bottom有1条出边) → db_replica(top有2条入边)，
    // master端锚点居中，replica端锚点偏左/偏右，路径走Z字而非直线。
    //
    // 修正策略（移至 4c replan_slots 之后执行）：对正对端口且节点投影有重叠的边，
    // 将「自由度较高」端（该侧同方向仅1条边的Single端点）的锚点切线坐标调整为与另一端对齐，
    // 形成直线路径。两端都有多个边时若节点中心线高度对齐（<16px），也强制对齐。
    let t_align = crate::layout::perf::Instant::now();
    crate::perf_log!(
        "[perf]     step2b_straighten: {:.2}ms (moved to 4c)",
        t_align.elapsed().as_secs_f64() * 1000.0
    );

    (
        from_side,
        to_side,
        lane,
        endpoint_map,
        parallel,
        reverse_pairs,
    )
}

/// 同宿 FanIn 若全部源节点位于目标同一侧，统一使用目标正对端口。
///
/// 逐边最近侧选择会把较远成员旋到 Left/Right，导致语义合流组在 S3 前被拆散。
/// 这里只处理明确的全上/全下关系；混合方向仍保留逐边端口选择。
fn align_fanin_target_sides(
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    to_side: &mut [Port],
) {
    let mut by_target: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (edge_index, relation) in relations.iter().enumerate() {
        by_target
            .entry(relation.to.as_str())
            .or_default()
            .push(edge_index);
    }
    for (target_id, members) in by_target {
        if let Some(common) = aligned_fanin_target_port(target_id, &members, relations, nodes) {
            for edge_index in members {
                if let Some(side) = to_side.get_mut(edge_index) {
                    *side = common;
                }
            }
        }
    }
}

pub(crate) fn aligned_fanin_target_port(
    target_id: &str,
    members: &[usize],
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
) -> Option<Port> {
    if members.len() != 2 {
        return None;
    }
    let target = nodes.get(target_id)?;
    let sources: Vec<(&str, &NodeLayout)> = members
        .iter()
        .filter_map(|&edge_index| {
            let relation = relations.get(edge_index)?;
            nodes
                .get(relation.from.as_str())
                .map(|node| (relation.from.as_str(), node))
        })
        .collect();
    if sources.len() != members.len() {
        return None;
    }
    let first_center_y = sources[0].1.y + sources[0].1.height / 2.0;
    if !sources
        .iter()
        .all(|(_, source)| (source.y + source.height / 2.0 - first_center_y).abs() <= 1.0)
    {
        return None;
    }
    let port = if sources
        .iter()
        .all(|(_, source)| source.y + source.height <= target.y + 0.5)
    {
        Port::Top
    } else if sources
        .iter()
        .all(|(_, source)| source.y >= target.y + target.height - 0.5)
    {
        Port::Bottom
    } else {
        return None;
    };

    let trunk_x = target.x + target.width / 2.0;
    let target_anchor = match port {
        Port::Top => Point::new(trunk_x, target.y),
        Port::Bottom => Point::new(trunk_x, target.y + target.height),
        _ => unreachable!(),
    };
    for (source_id, source) in &sources {
        let source_anchor = match port {
            Port::Top => Point::new(source.x + source.width / 2.0, source.y + source.height),
            Port::Bottom => Point::new(source.x + source.width / 2.0, source.y),
            _ => unreachable!(),
        };
        let join_y = source_anchor.y
            + if matches!(port, Port::Top) {
                PORT_CLEARANCE
            } else {
                -PORT_CLEARANCE
            };
        let path = [
            source_anchor,
            Point::new(source_anchor.x, join_y),
            Point::new(trunk_x, join_y),
            target_anchor,
        ];
        let blocked = path.windows(2).any(|segment| {
            nodes.iter().any(|(node_id, node)| {
                node_id.as_str() != *source_id
                    && node_id.as_str() != target_id
                    && crate::layout::geometry::Rect::from(node)
                        .expanded(NODE_OBSTACLE_PAD)
                        .segment_crosses_interior(segment[0], segment[1], 0.5)
            })
        });
        if blocked {
            return None;
        }
    }
    Some(port)
}

/// 取一个并线子组的排序键 `(arrow_tag, line_style, min_edge_index)`。
///
/// 排序键**不含** `is_from`：同一 edge 在 from 端 `is_from=true`、在 to 端
/// `is_from=false`，若将 is_from 纳入排序，两端子组排名会不一致，导致同一
/// edge 在两端获得不同的 `base_frac`，路径出现弯折。用 `min_edge_index` 做
/// 稳定 tiebreaker 可保证同一 edge 在两端子组中获得相同排名 → 相同 base_frac
/// → 直线路径。
fn sub_group_sort_key(
    endpoints: &[Endpoint],
    relations: &[crate::ast::Relation],
) -> (&'static str, String, usize) {
    let min_edge = endpoints.iter().map(|e| e.edge_index).min().unwrap_or(0);
    let rel = &relations[min_edge];
    (
        arrow_type_tag(&rel.arrow),
        edge_line_style_signature(rel),
        min_edge,
    )
}

// ═══════════════════════════════════════════════════════════
//  P1-3: 回环边侧向通道覆盖（在端口协调后强制执行）
// ═══════════════════════════════════════════════════════════

fn apply_feedback_side_overrides(
    relations: &[crate::ast::Relation],
    assignment: &feedback_side::FeedbackSideAssignment,
    from_side: &mut [Port],
    to_side: &mut [Port],
    lane: &mut [usize],
) {
    for (&edge_index, hint) in &assignment.hints {
        if edge_index >= relations.len() {
            continue;
        }
        from_side[edge_index] = hint.from_side;
        to_side[edge_index] = hint.to_side;
        lane[edge_index] = hint.lane;
    }
}

// ═══════════════════════════════════════════════════════════
//  P0-3: 端口选择全局协调（同侧偏好）
// ═══════════════════════════════════════════════════════════

/// 端口选择全局协调：对每个节点的多条边做"同侧偏好"协调。
///
/// `choose_pair_sides` 逐对独立选端口，同一节点的多条边可能分散在不同侧出发，
/// 导致节点附近不必要的交叉。此函数统计各侧边数，让少数派边在几何可接受时
/// 切换到多数派侧。
///
/// 协调以 pair_group 为最小单元（保持组内端口对一致性），出边/入边分开协调。
/// 确定性：节点按 node_id 排序，多数派 tiebreak 用最小 edge_index。
fn coordinate_port_sides(
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &mut [Port],
    to_side: &mut [Port],
    _group_ctx: Option<&crate::layout::group::GroupRoutingContext>,
) {
    use std::collections::{BTreeMap, BTreeSet};
    let n = relations.len();
    if n == 0 {
        return;
    }

    // 1. 重建 pair_groups: pair_key -> (can_from, can_to, edge_indices)
    let mut pair_info: BTreeMap<String, (String, String, Vec<usize>)> = BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        let (can_from, can_to) = canonical_pair(rel.from.as_str(), rel.to.as_str());
        pair_info
            .entry(key)
            .or_insert_with(|| (can_from.to_string(), can_to.to_string(), Vec::new()))
            .2
            .push(i);
    }

    // 2. 收集每个节点的端口信息: node_id -> Vec<(pair_key, edge_index, is_from, side_on_node)>
    let mut node_ports: BTreeMap<String, Vec<(String, usize, bool, Port)>> = BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        node_ports.entry(rel.from.to_string()).or_default().push((
            key.clone(),
            i,
            true,
            from_side[i],
        ));
        node_ports
            .entry(rel.to.to_string())
            .or_default()
            .push((key.clone(), i, false, to_side[i]));
    }

    // 3. 按确定性顺序协调每个节点（已切换的 pair_group 不再处理，避免振荡）
    let mut switched_pairs: BTreeSet<String> = BTreeSet::new();

    for (node_id, ports) in &node_ports {
        let Some(node_nl) = nodes.get(node_id) else {
            continue;
        };

        // 分离出边和入边（排除已切换的 pair_group）
        let mut out_ports: Vec<&(String, usize, bool, Port)> = Vec::new();
        let mut in_ports: Vec<&(String, usize, bool, Port)> = Vec::new();
        for entry in ports {
            if switched_pairs.contains(&entry.0) {
                continue;
            }
            if entry.2 {
                out_ports.push(entry);
            } else {
                in_ports.push(entry);
            }
        }

        // 协调出边（≥2 条才有协调意义）
        if out_ports.len() >= 2 {
            if let Some(majority_side) = find_majority_side(&out_ports) {
                let maj_count = out_ports
                    .iter()
                    .filter(|e| e.3 == majority_side)
                    .count();
                // Phase 4：多数派侧已超载时不再把少数派拉过去（避免更挤）
                if !(port_pressure_side_enabled() && maj_count >= PORT_PRESSURE_TRIG) {
                    for entry in &out_ports {
                        let pair_key = &entry.0;
                        let side = entry.3;
                        if side == majority_side || switched_pairs.contains(pair_key.as_str()) {
                            continue;
                        }
                        if let Some(other_nl) = pair_other_node(pair_key, node_id, &pair_info, nodes)
                        {
                            if side_acceptable(node_nl, other_nl, majority_side) {
                                switch_pair_side(
                                    pair_key,
                                    node_id,
                                    majority_side,
                                    &pair_info,
                                    relations,
                                    from_side,
                                    to_side,
                                );
                                switched_pairs.insert(pair_key.clone());
                            }
                        }
                    }
                }
            }
        }

        // 协调入边
        if in_ports.len() >= 2 {
            if let Some(majority_side) = find_majority_side(&in_ports) {
                let maj_count = in_ports
                    .iter()
                    .filter(|e| e.3 == majority_side)
                    .count();
                if !(port_pressure_side_enabled() && maj_count >= PORT_PRESSURE_TRIG) {
                    for entry in &in_ports {
                        let pair_key = &entry.0;
                        let side = entry.3;
                        if side == majority_side || switched_pairs.contains(pair_key.as_str()) {
                            continue;
                        }
                        if let Some(other_nl) = pair_other_node(pair_key, node_id, &pair_info, nodes)
                        {
                            if side_acceptable(node_nl, other_nl, majority_side) {
                                switch_pair_side(
                                    pair_key,
                                    node_id,
                                    majority_side,
                                    &pair_info,
                                    relations,
                                    from_side,
                                    to_side,
                                );
                                switched_pairs.insert(pair_key.clone());
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 与 `demand` 归一化 `REF_PORT` 对齐：单侧 ≥4 视为超载。
const PORT_PRESSURE_TRIG: usize = 4;

fn port_pressure_side_enabled() -> bool {
    !std::env::var("PLOTGRAM_PORT_PRESSURE_SIDE")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

/// 主动把超载侧边挪到邻侧；默认关（校准见 microservice 交叉 +2）。
fn port_pressure_relieve_enabled() -> bool {
    std::env::var("PLOTGRAM_PORT_PRESSURE_RELIEVE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}


/// 同侧 slot 加大错开；默认开（不换侧，只拉开）。
fn port_pressure_slot_enabled() -> bool {
    !std::env::var("PLOTGRAM_PORT_PRESSURE_SLOT")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

/// 高压侧 Compact pitch：从 16px 向 `slot_pitch` 温和靠拢，避免一步拉到 40 抬 stub 交叉。
fn pressure_aware_slot_pitch(base_slot_pitch: f64, side_load: usize, edge_len: f64) -> f64 {
    let overflow = side_load.saturating_sub(PORT_PRESSURE_TRIG);
    let room = (base_slot_pitch - COMPACT_SLOT_PITCH).max(0.0);
    // load=4 → +25% room；load=8 → +85% room
    let t = 0.25 + 0.15 * (overflow.min(4) as f64);
    let pitched = COMPACT_SLOT_PITCH + room * t;
    pitched.min(edge_len * 0.2).max(COMPACT_SLOT_PITCH)
}


/// D4 P2：对超载 `(node, side)` 把多余边 soft 分流到几何可接受的邻侧。
///
/// 在 `coordinate_port_sides` 之后、feedback 覆盖之前执行；默认关，见
/// [`port_pressure_relieve_enabled`]。
fn relieve_overloaded_port_sides(
    relations: &[crate::ast::Relation],
    nodes: &HashMap<String, NodeLayout>,
    from_side: &mut [Port],
    to_side: &mut [Port],
) {
    if !port_pressure_side_enabled() || relations.is_empty() {
        return;
    }
    use std::collections::{BTreeMap, BTreeSet};

    let mut pair_info: BTreeMap<String, (String, String, Vec<usize>)> = BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        let (can_from, can_to) = canonical_pair(rel.from.as_str(), rel.to.as_str());
        pair_info
            .entry(key)
            .or_insert_with(|| (can_from.to_string(), can_to.to_string(), Vec::new()))
            .2
            .push(i);
    }

    // node -> Vec<(pair_key, edge_index, is_from, side)>
    let mut node_ports: BTreeMap<String, Vec<(String, usize, bool, Port)>> = BTreeMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        node_ports.entry(rel.from.to_string()).or_default().push((
            key.clone(),
            i,
            true,
            from_side[i],
        ));
        node_ports.entry(rel.to.to_string()).or_default().push((
            key.clone(),
            i,
            false,
            to_side[i],
        ));
    }

    let port_order = [Port::Top, Port::Bottom, Port::Left, Port::Right];
    let mut switched_pairs: BTreeSet<String> = BTreeSet::new();
    let mut relieved = 0usize;

    for (node_id, ports) in &node_ports {
        let Some(node_nl) = nodes.get(node_id) else {
            continue;
        };
        for is_from in [true, false] {
            let mut by_side: BTreeMap<Port, Vec<(String, usize)>> = BTreeMap::new();
            for (pair_key, ei, from_flag, side) in ports {
                if *from_flag != is_from {
                    continue;
                }
                by_side
                    .entry(*side)
                    .or_default()
                    .push((pair_key.clone(), *ei));
            }
            for (over_side, mut edges) in by_side {
                if edges.len() < PORT_PRESSURE_TRIG {
                    continue;
                }
                // 后分配的边优先挪开（确定性：edge_index 降序）
                edges.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                let mut load = edges.len();
                for (pair_key, _ei) in &edges {
                    if load < PORT_PRESSURE_TRIG {
                        break;
                    }
                    if switched_pairs.contains(pair_key.as_str()) {
                        continue;
                    }
                    let Some(other_nl) =
                        pair_other_node(pair_key, node_id, &pair_info, nodes)
                    else {
                        continue;
                    };
                    let mut moved = false;
                    for &alt in &port_order {
                        if alt == over_side {
                            continue;
                        }
                        if !side_acceptable(node_nl, other_nl, alt) {
                            continue;
                        }
                        switch_pair_side(
                            pair_key,
                            node_id,
                            alt,
                            &pair_info,
                            relations,
                            from_side,
                            to_side,
                        );
                        switched_pairs.insert(pair_key.clone());
                        load -= 1;
                        relieved += 1;
                        moved = true;
                        break;
                    }
                    let _ = moved;
                }
            }
        }
    }

    if std::env::var_os("PLOTGRAM_DEBUG_PORT_PRESSURE").is_some() {
        eprintln!(
            "[port-pressure] relieved_pairs={} trig={}",
            relieved, PORT_PRESSURE_TRIG
        );
    }
}

/// 查找多数派端口。tiebreak：count 降序 → 最小 edge_index 升序 → 固定端口顺序。
fn find_majority_side(ports: &[&(String, usize, bool, Port)]) -> Option<Port> {
    let port_order = [Port::Top, Port::Bottom, Port::Left, Port::Right];
    let mut counts: [(usize, usize); 4] = [(0, usize::MAX); 4]; // (count, min_edge_index)
    for entry in ports {
        let edge_index = entry.1;
        let side = entry.3;
        for (idx, p) in port_order.iter().enumerate() {
            if side == *p {
                counts[idx].0 += 1;
                counts[idx].1 = counts[idx].1.min(edge_index);
                break;
            }
        }
    }
    let mut best_idx: Option<usize> = None;
    for (idx, (count, min_edge)) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let is_better = match best_idx {
            None => true,
            Some(bi) => {
                let (bc, be) = counts[bi];
                count > &bc
                    || (*count == bc && min_edge < &be)
                    || (*count == bc && min_edge == &be && idx < bi)
            }
        };
        if is_better {
            best_idx = Some(idx);
        }
    }
    best_idx.map(|idx| port_order[idx])
}

/// 获取 pair_group 中 node_id 之外另一个节点的布局
fn pair_other_node<'a>(
    pair_key: &str,
    node_id: &str,
    pair_info: &std::collections::BTreeMap<String, (String, String, Vec<usize>)>,
    nodes: &'a HashMap<String, NodeLayout>,
) -> Option<&'a NodeLayout> {
    let (can_from, can_to, _) = pair_info.get(pair_key)?;
    let other_id = if can_from == node_id {
        can_to.as_str()
    } else {
        can_from.as_str()
    };
    nodes.get(other_id)
}

/// 切换 pair_group 中 node_id 侧的端口为 new_side，保持组内端口对一致性。
fn switch_pair_side(
    pair_key: &str,
    node_id: &str,
    new_side: Port,
    pair_info: &std::collections::BTreeMap<String, (String, String, Vec<usize>)>,
    relations: &[crate::ast::Relation],
    from_side: &mut [Port],
    to_side: &mut [Port],
) {
    let Some((can_from, _can_to, edge_indices)) = pair_info.get(pair_key) else {
        return;
    };
    for &i in edge_indices {
        let rel = &relations[i];
        let is_can_from_from = rel.from.as_str() == can_from.as_str();
        if can_from == node_id {
            // node_id 的端口是 side_a
            if is_can_from_from {
                from_side[i] = new_side;
            } else {
                to_side[i] = new_side;
            }
        } else {
            // node_id == can_to，端口是 side_b
            if is_can_from_from {
                to_side[i] = new_side;
            } else {
                from_side[i] = new_side;
            }
        }
    }
}

/// 判断 `side` 作为 `from` 节点连接 `to` 节点的端口是否几何可接受。
///
/// 复用 `choose_pair_sides` 的阈值逻辑（`slot.rs` `dy.abs() >= dx.abs() * 0.4`）。
/// 若该方向的对端节点位移比例低于阈值，则代价过高、不可接受。
fn side_acceptable(from: &NodeLayout, to: &NodeLayout, side: Port) -> bool {
    let fc = node_center(from);
    let tc = node_center(to);
    let dx = tc.x - fc.x;
    let dy = tc.y - fc.y;
    let ox = range_overlap_local(from.x, from.x + from.width, to.x, to.x + to.width);
    let oy = range_overlap_local(from.y, from.y + from.height, to.y, to.y + to.height);

    match side {
        Port::Top | Port::Bottom => {
            if oy > EPS && ox <= EPS {
                return false;
            }
            let direction_ok = match side {
                Port::Bottom => dy > EPS,
                Port::Top => dy < -EPS,
                _ => unreachable!(),
            };
            if !direction_ok {
                return false;
            }
            if ox <= EPS && oy <= EPS {
                return dy.abs() >= dx.abs() * 0.4 - EPS;
            }
            if ox > EPS && oy > EPS {
                return dy.abs() >= dx.abs() - EPS;
            }
            true
        }
        Port::Left | Port::Right => {
            if ox > EPS && oy <= EPS {
                return false;
            }
            let direction_ok = match side {
                Port::Right => dx > EPS,
                Port::Left => dx < -EPS,
                _ => unreachable!(),
            };
            if !direction_ok {
                return false;
            }
            if ox <= EPS && oy <= EPS {
                return dx.abs() >= dy.abs() * 0.4 - EPS;
            }
            if ox > EPS && oy > EPS {
                return dx.abs() >= dy.abs() - EPS;
            }
            true
        }
    }
}
