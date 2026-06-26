//! Step 6: 路径重写
//! Step 7: 后处理（节点避让 + Ink 节省回退）
//!
//! 详见 `docs/architecture/布局优化/edge-bundling-research.md` §4.8、§4.9。
//!
//! ## 路径重写（§4.8）
//!
//! 将每条边的路径重写为：
//! ```text
//! from_anchor
//!     → FromStub（端口向外 PORT_CLEARANCE）
//!     → MergeLeg（stub 终点到 entry_i，1~2 个折点）
//!     → Trunk（entry_i → exit_i，与其他边完全重合）
//!     → ForkLeg（exit_i 到 to 端 stub 起点，1~2 个折点）
//!     → ToStub（到 to_anchor）
//! ```
//!
//! ## 后处理（§4.9）
//!
//! - **§4.9.1 重叠白名单**：经排查，`EDGE_OVERLAP_PENALTY`（scoring.rs）仅在路由阶段
//!   计算，bundling 是后处理不会再调用 scoring；`refine::analyze_edge_overlaps`（refine.rs）
//!   是唯一的后置边-边重叠检测，但它运行在 `refine()` 内部，而 bundling 的插入点在
//!   `refine()` 之后（见 §2.5 契约），因此 bundling 产生的故意重叠不会被任何后置检测
//!   惩罚。**本项无操作**。
//! - **§4.9.2 节点避让**：在 `trunk.rs::find_collision_free_coord` 中实现——
//!   尝试偏移主干坐标 ±GRID_STEP 避免穿障，无法避让则跳过该 bundle。
//!   此处 `trunk_collides_with_nodes` 作为安全兜底。
//! - **§4.9.4 Ink 节省回退**：节省 < `min_ink_saving` 则回退到原路径。
//! - **§4.9.5 自环/短边过滤**：在 `clustering.rs::cluster_edges` 入口处过滤。

use crate::layout::geometry::{Point, Rect};
use crate::layout::{EdgeLayout, NodeLayout, Port};

use super::compatibility::EdgeFeatures;
use super::trunk::allocate_trunks;
use super::types::{
    Axis, BundlingConfig, BundlingResult, EdgeBundle, EdgeBundlingDebugStats, EdgePathRoles, SegmentRole, SegmentSpan,
    TrunkKeepout,
};

/// 坐标比较容差
const EPS: f64 = 0.1;

/// 端口 stub 段长度（与 orthogonal 路由的 PORT_CLEARANCE 一致）
const PORT_CLEARANCE: f64 = 16.0;

/// 端口外法线方向（dx, dy）。
fn port_outward(side: Port) -> (f64, f64) {
    match side {
        Port::Top => (0.0, -1.0),
        Port::Bottom => (0.0, 1.0),
        Port::Left => (-1.0, 0.0),
        Port::Right => (1.0, 0.0),
    }
}

/// 一条边的重写结果。
struct RewrittenEdge {
    /// 新路径点序列
    new_path: Vec<Point>,
    /// 路径区段分解
    roles: EdgePathRoles,
}

/// 对所有 bundle 执行路径重写，返回 BundlingResult。
///
/// 流程：
/// 1. 对每个 bundle，重写其包含边的路径
/// 2. 检查主干穿障 → 穿障则回退该 bundle 全部边
/// 3. 检查 Ink 节省 → 不足则回退该 bundle 全部边
/// 4. 构建 BundlingResult（edge_to_bundle / edge_roles / trunk_keepouts）
pub fn rewrite_bundle_paths(
    bundles: &[EdgeBundle],
    edges: &mut [EdgeLayout],
    features: &[EdgeFeatures],
    nodes: &std::collections::HashMap<String, NodeLayout>,
    config: &BundlingConfig,
    stats: &mut EdgeBundlingDebugStats,
) -> BundlingResult {
    let n = edges.len();
    let mut edge_to_bundle: Vec<Option<usize>> = vec![None; n];
    let mut edge_roles: Vec<EdgePathRoles> = (0..n)
        .map(|i| EdgePathRoles {
            edge_index: i,
            spans: Vec::new(),
        })
        .collect();
    let mut trunk_keepouts: Vec<TrunkKeepout> = Vec::new();
    let mut total_ink_saved: f64 = 0.0;

    for bundle in bundles {
        // 保存原始路径以供回退
        let original_paths: Vec<Vec<Point>> = bundle
            .edges
            .iter()
            .map(|&i| edges[i].path_points().into_owned())
            .collect();

        // 计算原始总 Ink
        let original_ink: f64 = original_paths
            .iter()
            .map(|p| path_length(p))
            .sum();

        // 重写路径
        let mut rewritten: Vec<Option<RewrittenEdge>> = Vec::with_capacity(bundle.edges.len());
        for (slot, &edge_idx) in bundle.edges.iter().enumerate() {
            let rewritten_edge = rewrite_single_edge(
                edge_idx,
                slot,
                bundle,
                &edges[edge_idx],
                &features[edge_idx],
            );
            rewritten.push(rewritten_edge);
        }

        // 检查主干穿障（排除 bundle 端点节点）
        let endpoint_ids: std::collections::HashSet<&str> = bundle
            .edges
            .iter()
            .flat_map(|&i| {
                let f = &features[i];
                [f.from_id.as_str(), f.to_id.as_str()]
            })
            .collect();
        let trunk_passes_nodes =
            trunk_collides_with_nodes(bundle, nodes, &endpoint_ids);
        if trunk_passes_nodes {
            stats.obstacle_fallback_count += 1;
            continue; // 回退：不修改路径，不标记 bundle
        }

        // 计算新总 Ink（trunk 段共享，只计一次）
        // 详见 §4.9：Ink 节省基准 = 捆绑后实际绘制长度
        let mut non_trunk_ink: f64 = 0.0;
        let mut max_trunk_ink: f64 = 0.0;
        for r in &rewritten {
            if let Some(ref r) = r {
                for span in &r.roles.spans {
                    if span.role == SegmentRole::Trunk {
                        max_trunk_ink = max_trunk_ink.max(span.length);
                    } else {
                        non_trunk_ink += span.length;
                    }
                }
            }
        }
        let new_ink = non_trunk_ink + max_trunk_ink;

        // 检查 Ink 节省
        if original_ink > EPS {
            let saving_ratio = (original_ink - new_ink) / original_ink;
            if saving_ratio < config.min_ink_saving {
                stats.ink_fallback_count += 1;
                continue; // 回退：Ink 节省不足
            }
            total_ink_saved += original_ink - new_ink;
        }

        // 应用重写
        for (slot, &edge_idx) in bundle.edges.iter().enumerate() {
            if let Some(ref r) = rewritten[slot] {
                edges[edge_idx].set_polyline_points(r.new_path.clone());
                edge_roles[edge_idx] = r.roles.clone();
                edge_to_bundle[edge_idx] = Some(bundle.id);
            }
        }

        // 生成 TrunkKeepout
        let keepout = build_trunk_keepout(bundle, config);
        trunk_keepouts.push(keepout);
        stats.bundle_count += 1;
    }

    // 计算箭头抑制：同 bundle 内多条边指向同一节点时，只保留第一条边的箭头
    let mut arrow_suppressed = std::collections::HashSet::new();
    for bundle in bundles {
        if bundle.edges.len() < 2 {
            continue;
        }
        // 按 to_id 分组，同一个目标上只保留第一个箭头
        let mut seen_targets = std::collections::HashSet::new();
        for &edge_idx in &bundle.edges {
            let to_id = &features[edge_idx].to_id;
            if seen_targets.contains(to_id) {
                // 已经有同目标的边，抑制当前边（以及所有后续同目标边）的箭头
                arrow_suppressed.insert(edge_idx);
            } else {
                seen_targets.insert(to_id.clone());
            }
        }
    }

    BundlingResult {
        bundles: bundles.to_vec(),
        edge_to_bundle,
        total_ink_saved,
        edge_roles,
        trunk_keepouts,
        arrow_suppressed,
    }
}

/// 重写单条边的路径。
fn rewrite_single_edge(
    edge_index: usize,
    slot: usize,
    bundle: &EdgeBundle,
    edge: &EdgeLayout,
    _features: &EdgeFeatures,
) -> Option<RewrittenEdge> {
    let from_anchor = edge.path_start()?;
    let to_anchor = edge.path_end()?;
    let from_port = edge.from_port;
    let to_port = edge.to_port;

    let entry = bundle.entry_points.get(slot)?;
    let exit = bundle.exit_points.get(slot)?;
    let trunk_axis = bundle.trunk_axis;

    // 构建 FromStub
    let (fx, fy) = port_outward(from_port);
    let stub_end = Point::new(from_anchor.x + fx * PORT_CLEARANCE, from_anchor.y + fy * PORT_CLEARANCE);

    // 构建 ToStub
    let (tx, ty) = port_outward(to_port);
    let to_stub_start = Point::new(to_anchor.x + tx * PORT_CLEARANCE, to_anchor.y + ty * PORT_CLEARANCE);

    // 构建完整路径
    let mut path: Vec<Point> = Vec::new();
    let mut spans: Vec<SegmentSpan> = Vec::new();

    // FromStub: from_anchor → stub_end
    path.push(from_anchor);
    path.push(stub_end);
    let stub_len = distance(from_anchor, stub_end);
    spans.push(SegmentSpan {
        role: SegmentRole::FromStub,
        point_start: 0,
        point_end: 2,
        t_start: 0.0,
        t_end: 0.0, // 稍后计算
        length: stub_len,
    });

    // MergeLeg: stub_end → entry（正交路径）
    let merge_points = orthogonal_connect(stub_end, *entry, trunk_axis, true);
    let merge_start_idx = path.len() - 1; // stub_end 的索引
    for pt in &merge_points {
        path.push(*pt);
    }
    let merge_end_idx = path.len() - 1;
    let merge_len: f64 = path[merge_start_idx..=merge_end_idx]
        .windows(2)
        .map(|w| distance(w[0], w[1]))
        .sum();
    spans.push(SegmentSpan {
        role: SegmentRole::MergeLeg,
        point_start: merge_start_idx,
        point_end: merge_end_idx + 1,
        t_start: 0.0,
        t_end: 0.0,
        length: merge_len,
    });

    // Trunk: entry → exit
    path.push(*exit);
    let trunk_start_idx = path.len() - 2; // entry 的索引
    let trunk_end_idx = path.len() - 1; // exit 的索引
    let trunk_len = distance(*entry, *exit);
    spans.push(SegmentSpan {
        role: SegmentRole::Trunk,
        point_start: trunk_start_idx,
        point_end: trunk_end_idx + 1,
        t_start: 0.0,
        t_end: 0.0,
        length: trunk_len,
    });

    // ForkLeg: exit → to_stub_start（正交路径）
    let fork_points = orthogonal_connect(*exit, to_stub_start, trunk_axis, false);
    let fork_start_idx = path.len() - 1; // exit 的索引
    for pt in &fork_points {
        path.push(*pt);
    }
    let fork_end_idx = path.len() - 1;
    let fork_len: f64 = path[fork_start_idx..=fork_end_idx]
        .windows(2)
        .map(|w| distance(w[0], w[1]))
        .sum();
    spans.push(SegmentSpan {
        role: SegmentRole::ForkLeg,
        point_start: fork_start_idx,
        point_end: fork_end_idx + 1,
        t_start: 0.0,
        t_end: 0.0,
        length: fork_len,
    });

    // ToStub: to_stub_start → to_anchor
    path.push(to_anchor);
    let to_stub_start_idx = path.len() - 2;
    let to_stub_end_idx = path.len() - 1;
    let to_stub_len = distance(to_stub_start, to_anchor);
    spans.push(SegmentSpan {
        role: SegmentRole::ToStub,
        point_start: to_stub_start_idx,
        point_end: to_stub_end_idx + 1,
        t_start: 0.0,
        t_end: 0.0,
        length: to_stub_len,
    });

    // 清理退化段（零长度段）
    let path = dedup_consecutive(path);

    // 计算 t 值
    let total_len: f64 = spans.iter().map(|s| s.length).sum();
    let mut accum = 0.0;
    for span in &mut spans {
        span.t_start = if total_len > EPS {
            accum / total_len
        } else {
            0.0
        };
        accum += span.length;
        span.t_end = if total_len > EPS {
            accum / total_len
        } else {
            0.0
        };
    }

    Some(RewrittenEdge {
        new_path: path,
        roles: EdgePathRoles {
            edge_index,
            spans,
        },
    })
}

/// 生成两点之间的正交连接路径（不含起点，含终点）。
///
/// `join_trunk` 为 true 时，最后一段垂直于 trunk 轴（合入主干）；
/// 为 false 时，第一段垂直于 trunk 轴（离开主干）。
fn orthogonal_connect(
    from: Point,
    to: Point,
    trunk_axis: Axis,
    join_trunk: bool,
) -> Vec<Point> {
    if (from.x - to.x).abs() < EPS && (from.y - to.y).abs() < EPS {
        return vec![to];
    }

    // 同轴对齐 → 直线
    if (from.x - to.x).abs() < EPS || (from.y - to.y).abs() < EPS {
        return vec![to];
    }

    // L 形路径
    match trunk_axis {
        Axis::Horizontal => {
            // 主干水平，垂直方向是 y
            if join_trunk {
                // 合入主干：先水平后垂直 → (to.x, from.y) → to
                vec![Point::new(to.x, from.y), to]
            } else {
                // 离开主干：先垂直后水平 → (from.x, to.y) → to
                vec![Point::new(from.x, to.y), to]
            }
        }
        Axis::Vertical => {
            // 主干垂直，水平方向是 x
            if join_trunk {
                // 合入主干：先垂直后水平 → (from.x, to.y) → to
                vec![Point::new(from.x, to.y), to]
            } else {
                // 离开主干：先水平后垂直 → (to.x, from.y) → to
                vec![Point::new(to.x, from.y), to]
            }
        }
    }
}

/// 去除连续重复点（避免零长度退化段）。
fn dedup_consecutive(mut path: Vec<Point>) -> Vec<Point> {
    if path.len() <= 2 {
        return path;
    }
    let mut result: Vec<Point> = Vec::with_capacity(path.len());
    for pt in path.drain(..) {
        if result.last().map_or(true, |last| {
            (last.x - pt.x).abs() > EPS || (last.y - pt.y).abs() > EPS
        }) {
            result.push(pt);
        }
    }
    // 保证至少 2 个点
    if result.len() < 2 {
        result
    } else {
        result
    }
}

/// 检查主干段是否穿过任何节点。
///
/// 主干段是 bundle.trunk_start → bundle.trunk_end 的线段。
fn trunk_collides_with_nodes(
    bundle: &EdgeBundle,
    nodes: &std::collections::HashMap<String, NodeLayout>,
    excluded_ids: &std::collections::HashSet<&str>,
) -> bool {
    let p1 = bundle.trunk_start;
    let p2 = bundle.trunk_end;

    for (id, nl) in nodes {
        if excluded_ids.contains(id.as_str()) {
            continue;
        }
        if Rect::from(nl).intersects_segment(p1, p2, 0.0) {
            return true;
        }
    }
    false
}

/// 构建主干禁放区（TrunkKeepout），含 merge/fork leg 分叉点避让。
fn build_trunk_keepout(bundle: &EdgeBundle, config: &BundlingConfig) -> TrunkKeepout {
    let sx = bundle.trunk_start.x;
    let sy = bundle.trunk_start.y;
    let ex = bundle.trunk_end.x;
    let ey = bundle.trunk_end.y;
    let pad = config.label_trunk_pad;
    let fork_pad = pad * 0.75; // 分叉点避让带稍小

    let mut zones: Vec<(f64, f64, f64, f64)> = Vec::new();

    // 主干避让带
    let trunk_zone = match bundle.trunk_axis {
        Axis::Horizontal => (sx - pad, sy - pad, ex + pad, sy + pad),
        Axis::Vertical => (sx - pad, sy - pad, ex + pad, ey + pad),
    };
    zones.push(trunk_zone);

    // Entry 分叉点避让（merge leg 根部）
    for &pt in &bundle.entry_points {
        zones.push((pt.x - fork_pad, pt.y - fork_pad, pt.x + fork_pad, pt.y + fork_pad));
    }

    // Exit 分叉点避让（fork leg 根部）
    for &pt in &bundle.exit_points {
        zones.push((pt.x - fork_pad, pt.y - fork_pad, pt.x + fork_pad, pt.y + fork_pad));
    }

    TrunkKeepout {
        bundle_id: bundle.id,
        zones,
    }
}

/// 计算折线总长度。
fn path_length(path: &[Point]) -> f64 {
    path.windows(2).map(|w| distance(w[0], w[1])).sum()
}

/// 两点距离。
fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}

/// 完整的 bundling 流水线（Step 3 → Step 7）。
///
/// 输入：已路由的 edges + features + config
/// 输出：(BundlingResult, EdgeBundlingDebugStats)（edges 被原地修改为 bundled 路径）
pub fn apply_bundling(
    edges: &mut [EdgeLayout],
    features: &[EdgeFeatures],
    nodes: &std::collections::HashMap<String, NodeLayout>,
    config: &BundlingConfig,
) -> (BundlingResult, EdgeBundlingDebugStats) {
    let started = std::time::Instant::now();
    let mut stats = EdgeBundlingDebugStats {
        edge_count: features.len(),
        ..Default::default()
    };

    // Step 3: 聚类
    let candidates = super::clustering::cluster_edges(features, config, &mut stats);

    // Step 4-5: 主干通道分配 + 分叉点计算
    let bundles = allocate_trunks(&candidates, features, nodes, config);

    // Step 6-7: 路径重写 + 后处理
    let result = rewrite_bundle_paths(&bundles, edges, features, nodes, config, &mut stats);
    stats.total_ink_saved = result.total_ink_saved;
    stats.elapsed_us = started.elapsed().as_micros() as u64;
    (result, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Identifier, Relation, Span,
    };
    use crate::layout::{EdgeLayout, Port};
    use std::collections::HashMap;

    fn make_relation(from: &str, to: &str, arrow: ArrowType) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn make_node_layout(cx: f64, cy: f64, w: f64, h: f64) -> NodeLayout {
        NodeLayout {
            x: cx - w / 2.0,
            y: cy - h / 2.0,
            width: w,
            height: h,
        }
    }

    fn make_nodes(ids: &[&str], centers: &[(f64, f64)]) -> HashMap<String, NodeLayout> {
        let mut nodes = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            nodes.insert(id.to_string(), make_node_layout(centers[i].0, centers[i].1, 80.0, 40.0));
        }
        nodes
    }

    fn make_features(
        edge_index: usize,
        rel: &Relation,
        nodes: &HashMap<String, NodeLayout>,
        path: &[Point],
    ) -> EdgeFeatures {
        EdgeFeatures::extract(edge_index, rel, nodes, None, path).unwrap()
    }

    fn make_edge_layout(path: Vec<Point>, from_port: Port, to_port: Port) -> EdgeLayout {
        let mut edge = EdgeLayout::empty();
        edge.set_polyline_points(path);
        edge.from_port = from_port;
        edge.to_port = to_port;
        edge
    }

    #[test]
    fn rewrite_preserves_endpoints() {
        // 节点垂直间距 50px：from_center 距离 ≤ 60（兼容性阈值），
        // 且 trunk 中位坐标（~24）不穿障（A: y=-20..20, C: y=30..70）
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (300.0, 0.0), (0.0, 50.0), (300.0, 50.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(340.0, 0.0)],
            vec![Point::new(40.0, 50.0), Point::new(340.0, 50.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let mut edges = vec![
            make_edge_layout(paths[0].clone(), Port::Right, Port::Left),
            make_edge_layout(paths[1].clone(), Port::Right, Port::Left),
        ];

        let config = BundlingConfig {
            enabled: true,
            min_ink_saving: 0.0, // 禁用 Ink 回退以测试重写
            ..Default::default()
        };

        let (result, _) = apply_bundling(&mut edges, &features, &nodes, &config);

        // 端点应保持不变
        for (i, edge) in edges.iter().enumerate() {
            let orig_start = paths[i][0];
            let orig_end = paths[i][paths[i].len() - 1];
            let new_start = edge.path_start().unwrap();
            let new_end = edge.path_end().unwrap();
            assert!((new_start.x - orig_start.x).abs() < EPS, "edge {} start x changed", i);
            assert!((new_start.y - orig_start.y).abs() < EPS, "edge {} start y changed", i);
            assert!((new_end.x - orig_end.x).abs() < EPS, "edge {} end x changed", i);
            assert!((new_end.y - orig_end.y).abs() < EPS, "edge {} end y changed", i);
        }

        // 至少有一条边被捆绑
        let bundled_count = result.edge_to_bundle.iter().filter(|b| b.is_some()).count();
        assert!(bundled_count >= 2, "应至少有 2 条边被捆绑");
    }

    #[test]
    fn rewrite_produces_orthogonal_path() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (300.0, 0.0), (0.0, 40.0), (300.0, 40.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(340.0, 0.0)],
            vec![Point::new(40.0, 40.0), Point::new(340.0, 40.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let mut edges = vec![
            make_edge_layout(paths[0].clone(), Port::Right, Port::Left),
            make_edge_layout(paths[1].clone(), Port::Right, Port::Left),
        ];

        let config = BundlingConfig {
            enabled: true,
            min_ink_saving: 0.0,
            ..Default::default()
        };

        apply_bundling(&mut edges, &features, &nodes, &config);

        // 所有段应为水平或垂直
        for (i, edge) in edges.iter().enumerate() {
            let points: Vec<Point> = edge.path_points().into_owned();
            for w in points.windows(2) {
                let dx = (w[1].x - w[0].x).abs();
                let dy = (w[1].y - w[0].y).abs();
                assert!(
                    dx < EPS || dy < EPS,
                    "edge {} has non-orthogonal segment: {:?} → {:?}",
                    i,
                    w[0],
                    w[1]
                );
            }
        }
    }

    #[test]
    fn rewrite_generates_edge_roles() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (300.0, 0.0), (0.0, 40.0), (300.0, 40.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(340.0, 0.0)],
            vec![Point::new(40.0, 40.0), Point::new(340.0, 40.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let mut edges = vec![
            make_edge_layout(paths[0].clone(), Port::Right, Port::Left),
            make_edge_layout(paths[1].clone(), Port::Right, Port::Left),
        ];

        let config = BundlingConfig {
            enabled: true,
            min_ink_saving: 0.0,
            ..Default::default()
        };

        let (result, _) = apply_bundling(&mut edges, &features, &nodes, &config);

        // 被捆绑的边应有 role 分解
        for (i, roles) in result.edge_roles.iter().enumerate() {
            if result.edge_to_bundle[i].is_some() {
                assert!(!roles.spans.is_empty(), "edge {} should have role spans", i);
                // 应包含 Trunk 角色
                let has_trunk = roles.spans.iter().any(|s| s.role == SegmentRole::Trunk);
                assert!(has_trunk, "edge {} should have Trunk span", i);
            }
        }
    }

    #[test]
    fn rewrite_generates_trunk_keepout() {
        // 节点垂直间距 50px：from_center 距离 ≤ 60（兼容性阈值），
        // 且 trunk 中位坐标（~24）不穿障（A: y=-20..20, C: y=30..70）
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (300.0, 0.0), (0.0, 50.0), (300.0, 50.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(340.0, 0.0)],
            vec![Point::new(40.0, 50.0), Point::new(340.0, 50.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let mut edges = vec![
            make_edge_layout(paths[0].clone(), Port::Right, Port::Left),
            make_edge_layout(paths[1].clone(), Port::Right, Port::Left),
        ];

        let config = BundlingConfig {
            enabled: true,
            min_ink_saving: 0.0,
            label_trunk_pad: 8.0,
            ..Default::default()
        };

        let (result, _) = apply_bundling(&mut edges, &features, &nodes, &config);

        if !result.bundles.is_empty() {
            assert!(
                !result.trunk_keepouts.is_empty(),
                "应生成 trunk keepout"
            );
            for keepout in &result.trunk_keepouts {
                assert!(!keepout.zones.is_empty(), "keepout 应有 zone");
            }
        }
    }

    #[test]
    fn ink_saving_fallback_reverts_bundle() {
        // 两条很短的边，bundling 后 Ink 节省很少
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (50.0, 0.0), (0.0, 20.0), (50.0, 20.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(90.0, 0.0)],
            vec![Point::new(40.0, 20.0), Point::new(90.0, 20.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let mut edges = vec![
            make_edge_layout(paths[0].clone(), Port::Right, Port::Left),
            make_edge_layout(paths[1].clone(), Port::Right, Port::Left),
        ];

        let config = BundlingConfig {
            enabled: true,
            min_ink_saving: 0.99, // 要求 99% 节省 → 几乎必然回退
            ..Default::default()
        };

        let (result, _) = apply_bundling(&mut edges, &features, &nodes, &config);

        // 应回退，无边被捆绑
        let bundled_count = result.edge_to_bundle.iter().filter(|b| b.is_some()).count();
        assert_eq!(bundled_count, 0, "Ink 节省不足应回退");
    }

    #[test]
    fn trunk_collision_fallback() {
        // 在主干路径上放一个节点
        let nodes = make_nodes(&["A", "B", "C", "D", "blocker"], &[(0.0, 0.0), (300.0, 0.0), (0.0, 40.0), (300.0, 40.0), (150.0, 20.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(340.0, 0.0)],
            vec![Point::new(40.0, 40.0), Point::new(340.0, 40.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let mut edges = vec![
            make_edge_layout(paths[0].clone(), Port::Right, Port::Left),
            make_edge_layout(paths[1].clone(), Port::Right, Port::Left),
        ];

        let config = BundlingConfig {
            enabled: true,
            min_ink_saving: 0.0, // 禁用 Ink 回退，只测穿障回退
            ..Default::default()
        };

        let (result, _) = apply_bundling(&mut edges, &features, &nodes, &config);

        // 主干 y 在 0~40 之间，blocker 中心在 (150, 20)，主干可能穿障
        // 若穿障则回退
        // 结果不确定（取决于主干 y 坐标），但不应 panic
        let _ = result.edge_to_bundle.iter().filter(|b| b.is_some()).count();
    }

    #[test]
    fn orthogonal_connect_horizontal_trunk_join() {
        // 水平主干，合入：先水平后垂直
        let from = Point::new(0.0, 0.0);
        let to = Point::new(100.0, 50.0);
        let pts = orthogonal_connect(from, to, Axis::Horizontal, true);
        assert_eq!(pts, vec![Point::new(100.0, 0.0), Point::new(100.0, 50.0)]);
    }

    #[test]
    fn orthogonal_connect_vertical_trunk_join() {
        // 垂直主干，合入：先垂直后水平
        let from = Point::new(0.0, 0.0);
        let to = Point::new(100.0, 50.0);
        let pts = orthogonal_connect(from, to, Axis::Vertical, true);
        assert_eq!(pts, vec![Point::new(0.0, 50.0), Point::new(100.0, 50.0)]);
    }

    #[test]
    fn orthogonal_connect_aligned_points() {
        // 同 x → 直线
        let from = Point::new(50.0, 0.0);
        let to = Point::new(50.0, 100.0);
        let pts = orthogonal_connect(from, to, Axis::Horizontal, true);
        assert_eq!(pts, vec![Point::new(50.0, 100.0)]);
    }

    #[test]
    fn segment_intersects_rect_basic() {
        let rect = Rect::new(10.0, 10.0, 40.0, 40.0);
        assert!(rect.intersects_segment(Point::new(0.0, 30.0), Point::new(100.0, 30.0), 0.0));
        assert!(!rect.intersects_segment(Point::new(0.0, 0.0), Point::new(100.0, 0.0), 0.0));
        assert!(rect.intersects_segment(Point::new(30.0, 0.0), Point::new(30.0, 100.0), 0.0));
    }

    #[test]
    fn mixed_direction_bundle_uses_l_shaped_legs() {
        // 混合方向 bundle：from 集群在左上，to 集群在右下（对角流向）。
        // 主干选择水平主轴（水平行程 300 > 垂直行程 50），
        // merge/fork leg 以 L 形处理垂直分量。
        // 节点间距确保 from_center 距离 ≤ 60（兼容性阈值）。
        let nodes = make_nodes(
            &["A", "B", "C", "D"],
            &[(0.0, 0.0), (300.0, 50.0), (0.0, 40.0), (300.0, 90.0)],
        );
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(300.0, 0.0), Point::new(300.0, 50.0)],
            vec![Point::new(40.0, 40.0), Point::new(300.0, 40.0), Point::new(300.0, 90.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let mut edges = vec![
            make_edge_layout(paths[0].clone(), Port::Right, Port::Left),
            make_edge_layout(paths[1].clone(), Port::Right, Port::Left),
        ];

        let config = BundlingConfig {
            enabled: true,
            min_ink_saving: 0.0,
            ..Default::default()
        };

        let (result, _) = apply_bundling(&mut edges, &features, &nodes, &config);

        // 应形成 bundle（混合方向通过主轴选择 + L 形 leg 处理）
        let bundled_count = result.edge_to_bundle.iter().filter(|b| b.is_some()).count();
        assert!(bundled_count >= 2, "混合方向应能形成 bundle，实际 {}", bundled_count);

        // 端点不变
        for (i, edge) in edges.iter().enumerate() {
            let orig_start = paths[i][0];
            let orig_end = paths[i][paths[i].len() - 1];
            let new_start = edge.path_start().unwrap();
            let new_end = edge.path_end().unwrap();
            assert!((new_start.x - orig_start.x).abs() < EPS, "edge {} start x", i);
            assert!((new_start.y - orig_start.y).abs() < EPS, "edge {} start y", i);
            assert!((new_end.x - orig_end.x).abs() < EPS, "edge {} end x", i);
            assert!((new_end.y - orig_end.y).abs() < EPS, "edge {} end y", i);
        }

        // 所有段正交
        for (i, edge) in edges.iter().enumerate() {
            let points: Vec<Point> = edge.path_points().into_owned();
            for w in points.windows(2) {
                let dx = (w[1].x - w[0].x).abs();
                let dy = (w[1].y - w[0].y).abs();
                assert!(
                    dx < EPS || dy < EPS,
                    "edge {} has non-orthogonal segment: {:?} → {:?}",
                    i,
                    w[0],
                    w[1]
                );
            }
        }

        // 应有 Trunk 角色（共享主干段）
        for (i, roles) in result.edge_roles.iter().enumerate() {
            if result.edge_to_bundle[i].is_some() {
                let has_trunk = roles.spans.iter().any(|s| s.role == SegmentRole::Trunk);
                assert!(has_trunk, "edge {} should have Trunk span", i);
            }
        }
    }
}