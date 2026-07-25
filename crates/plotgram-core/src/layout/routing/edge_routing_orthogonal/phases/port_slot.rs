//! 端口 / slot 选择 + side 协调决策
//!
//! 从 `run.rs` 原样搬迁（A4 结构重构，行为不变）。

use super::super::*;
use crate::layout::routing::common::edge_geometry::{
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
    shape_polygons: &HashMap<String, Vec<Point>>,
) -> (
    Vec<Port>,
    Vec<Port>,
    Vec<usize>,
    HashMap<(usize, bool), Endpoint>,
    crate::layout::routing::common::parallel_edges::ParallelGroups,
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

    // ── 1b. 端口 side 单一写者（Slice 5） ──
    //
    // 端口约束求解器是 pre-route 端口 side 的**唯一写者**：feedback 回环边
    // 锁定、monitor 侧向逃逸、fanin 对齐、反向 stub 预测均已并入求解器。
    // 上方 choose 循环仅保留为 lane 初始化（side 被求解器覆写）。
    // 原散落的 coordinate / relieve / feedback_override / monitor / fanin pass 已删除。
    dbg_ports("after_choose", relations, &from_side, &to_side);
    {
        let input = super::super::port_solver::PortSolverInput {
            relations,
            nodes,
            group_ctx,
            feedback: feedback_assignment,
            s4_monitor_corridor,
            horizontal,
            port_solver_v2: cfg.routing.port_solver_v2,
        };
        let assignment = super::super::port_solver::solve_port_assignment(&input);
        from_side = assignment.from_side;
        to_side = assignment.to_side;
    }
    dbg_ports("after_port_solver", relations, &from_side, &to_side);
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
        let high_pressure = cfg.routing.port_pressure_slot && side_load >= PORT_PRESSURE_TRIG;
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
                // B 族（ISS-003）：非矩形形状锚点吸附到真实轮廓
                let anchor = match shape_polygons.get(&node_id) {
                    Some(poly) => super::super::shape_boundary::snap_anchor_to_boundary(anchor, side, poly),
                    None => anchor,
                };
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
    let parallel = crate::layout::routing::common::parallel_edges::group_parallel_edges(
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

/// 临时调试：PLOTGRAM_PORT_DEBUG=1 时打印含 "revise" 的边的端口流转。
fn dbg_ports(
    label: &str,
    relations: &[crate::ast::Relation],
    from_side: &[Port],
    to_side: &[Port],
) {
    if !std::env::var("PLOTGRAM_PORT_DEBUG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return;
    }
    for (i, rel) in relations.iter().enumerate() {
        if rel.from.as_str().contains("revise") || rel.to.as_str().contains("revise") {
            eprintln!(
                "[port_dbg] {:24} edge[{}] {} -> {} | from={:?} to={:?}",
                label, i, rel.from, rel.to, from_side[i], to_side[i]
            );
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

/// 与 `demand` 归一化 `REF_PORT` 对齐：单侧 ≥4 视为超载。
const PORT_PRESSURE_TRIG: usize = 4;


/// 高压侧 Compact pitch：从 16px 向 `slot_pitch` 温和靠拢，避免一步拉到 40 抬 stub 交叉。
fn pressure_aware_slot_pitch(base_slot_pitch: f64, side_load: usize, edge_len: f64) -> f64 {
    let overflow = side_load.saturating_sub(PORT_PRESSURE_TRIG);
    let room = (base_slot_pitch - COMPACT_SLOT_PITCH).max(0.0);
    // load=4 → +25% room；load=8 → +85% room
    let t = 0.25 + 0.15 * (overflow.min(4) as f64);
    let pitched = COMPACT_SLOT_PITCH + room * t;
    pitched.min(edge_len * 0.2).max(COMPACT_SLOT_PITCH)
}

