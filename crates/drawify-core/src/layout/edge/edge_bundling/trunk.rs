//! Step 4: 通道分配（Trunk 定位）
//! Step 5: 分叉点计算
//!
//! 详见 `docs/architecture/布局优化/edge-bundling-research.md` §4.6、§4.7。
//!
//! ## Step 4: Trunk 定位
//!
//! 1. 主轴选择：bundle 内多数边为主方向 → 主干为该方向通道
//! 2. 主干坐标：组内对应方向段坐标中位数，量化到 8px 网格
//! 3. 主干范围：所有边在主轴方向投影的 min/max 坐标，外扩 trunk_margin
//!
//! ## Step 5: 分叉点计算
//!
//! - entry_i：from 端在主干上的最近投影点
//! - exit_i：to 端在主干上的最近投影点
//! - 分叉点按坐标排序，确保最小间距 fork_spacing

use crate::layout::geometry::{Point, Rect};
use crate::layout::NodeLayout;

use super::compatibility::{decompose_path, EdgeFeatures};
use super::clustering::BundleCandidate;
use super::types::{Axis, BundlingConfig, EdgeBundle};

/// 坐标比较容差
const EPS: f64 = 0.1;

/// 网格量化步长（与 grid_snap 一致）
const GRID_STEP: f64 = 8.0;

/// 主干范围外扩余量（像素）
const TRUNK_MARGIN: f64 = 24.0;

/// 为所有 bundle candidate 分配主干通道和分叉点，生成 EdgeBundle 列表。
///
/// 返回的 EdgeBundle 按 bundle 内最小 edge_index 升序排列。
pub fn allocate_trunks(
    candidates: &[BundleCandidate],
    features: &[EdgeFeatures],
    nodes: &std::collections::HashMap<String, NodeLayout>,
    config: &BundlingConfig,
) -> Vec<EdgeBundle> {
    let mut bundles = Vec::with_capacity(candidates.len());
    let mut next_bundle_id = 0usize;

    for candidate in candidates {
        if let Some(bundle) = allocate_single_trunk(
            next_bundle_id,
            candidate,
            features,
            nodes,
            config,
        ) {
            next_bundle_id += 1;
            bundles.push(bundle);
        }
    }

    bundles
}

/// 为单个 bundle candidate 分配主干通道和分叉点。
fn allocate_single_trunk(
    bundle_id: usize,
    candidate: &BundleCandidate,
    features: &[EdgeFeatures],
    nodes: &std::collections::HashMap<String, NodeLayout>,
    config: &BundlingConfig,
) -> Option<EdgeBundle> {
    let edges = &candidate.edges;
    if edges.len() < 2 {
        return None;
    }

    // ── Step 4.1: 主轴选择 ──
    let trunk_axis = choose_trunk_axis(edges, features);

    // ── Step 4.2: 主干坐标（中位数 + 网格量化）──
    let base_coord = compute_trunk_coordinate(edges, features, trunk_axis);

    // ── Step 4.2b: 碰撞避让——尝试偏移主干坐标避免穿障（§4.9.2）──
    // 依次尝试 base_coord, base_coord±GRID_STEP, base_coord±2*GRID_STEP, ...
    // 直到找到不穿障的坐标或达到最大尝试次数
    let trunk_coord = find_collision_free_coord(
        base_coord,
        edges,
        features,
        nodes,
        trunk_axis,
        config,
    )?;

    // ── Step 4.3: 主干范围 ──
    let (trunk_start, trunk_end) = compute_trunk_range(edges, features, trunk_axis, trunk_coord);

    // ── Step 5: 分叉点计算 ──
    let (entry_points, exit_points) = compute_fork_points(
        edges,
        features,
        trunk_axis,
        trunk_coord,
        trunk_start,
        trunk_end,
        config,
    );

    Some(EdgeBundle {
        id: bundle_id,
        edges: edges.clone(),
        trunk_axis,
        trunk_start,
        trunk_end,
        entry_points,
        exit_points,
    })
}

/// 尝试在 base_coord 附近找到不穿障的主干坐标。
///
/// 依次尝试 base_coord, base_coord±GRID_STEP, base_coord±2*GRID_STEP, ...
/// 最多尝试 `max_attempts` 次（每侧 max_attempts/2 次）。
/// 返回 None 表示无法找到不穿障的坐标 → bundle 被跳过。
fn find_collision_free_coord(
    base_coord: f64,
    edges: &[usize],
    features: &[EdgeFeatures],
    nodes: &std::collections::HashMap<String, NodeLayout>,
    axis: Axis,
    _config: &BundlingConfig,
) -> Option<f64> {
    const MAX_ATTEMPTS: usize = 8; // 每侧最多尝试 4 次（±4 × 8px = ±32px）

    // 收集 bundle 的端点节点 ID，碰撞检测时排除这些节点
    // （主干本就连接这些节点，靠近它们是正常的）
    let endpoint_ids: std::collections::HashSet<&str> = edges
        .iter()
        .flat_map(|&i| {
            let f = &features[i];
            [f.from_id.as_str(), f.to_id.as_str()]
        })
        .collect();

    for step in 0..=MAX_ATTEMPTS {
        // 交替尝试正负偏移：0, +1, -1, +2, -2, ...
        let offsets: Vec<f64> = match step {
            0 => vec![0.0],
            _ => vec![step as f64 * GRID_STEP, -(step as f64) * GRID_STEP],
        };
        for offset in &offsets {
            let coord = quantize(base_coord + offset, GRID_STEP);
            let (trunk_start, trunk_end) =
                compute_trunk_range(edges, features, axis, coord);
            if !trunk_segment_collides(trunk_start, trunk_end, nodes, &endpoint_ids) {
                return Some(coord);
            }
        }
    }
    None
}

/// 检查主干段是否穿过任何节点（排除 bundle 端点节点）。
fn trunk_segment_collides(
    trunk_start: Point,
    trunk_end: Point,
    nodes: &std::collections::HashMap<String, NodeLayout>,
    excluded_ids: &std::collections::HashSet<&str>,
) -> bool {
    for (id, nl) in nodes {
        if excluded_ids.contains(id.as_str()) {
            continue;
        }
        if Rect::from(nl).intersects_segment(trunk_start, trunk_end, 0.0) {
            return true;
        }
    }
    false
}

/// Step 4.1: 选择主干主轴。
///
/// 统计 bundle 内每条边路径中水平段和垂直段的总长度，
/// 取总长度更大的的方向作为主干主轴。
fn choose_trunk_axis(edges: &[usize], features: &[EdgeFeatures]) -> Axis {
    let mut h_len: f64 = 0.0;
    let mut v_len: f64 = 0.0;

    for &edge_idx in edges {
        let segments = decompose_path(edge_idx, &features[edge_idx].path_points);
        for seg in &segments {
            match seg.axis {
                Axis::Horizontal => h_len += seg.length,
                Axis::Vertical => v_len += seg.length,
            }
        }
    }

    if h_len >= v_len {
        Axis::Horizontal
    } else {
        Axis::Vertical
    }
}

/// Step 4.2: 计算主干坐标（中位数 + 网格量化）。
///
/// 水平主干取 y 中位数，垂直主干取 x 中位数。
/// 中位数来自每条边对应方向段的层坐标。
fn compute_trunk_coordinate(
    edges: &[usize],
    features: &[EdgeFeatures],
    axis: Axis,
) -> f64 {
    // 收集每条边对应方向段的层坐标（取该方向最长段）
    let mut coords: Vec<f64> = Vec::with_capacity(edges.len());
    for &edge_idx in edges {
        let segments = decompose_path(edge_idx, &features[edge_idx].path_points);
        let best = segments
            .iter()
            .filter(|s| s.axis == axis)
            .max_by(|a, b| {
                a.length
                    .partial_cmp(&b.length)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some(seg) = best {
            coords.push(seg.layer);
        }
    }

    // 若无匹配段，退化为 from/to 中心坐标的中位数
    if coords.is_empty() {
        for &edge_idx in edges {
            let feat = &features[edge_idx];
            let coord = match axis {
                Axis::Horizontal => (feat.from_center.y + feat.to_center.y) / 2.0,
                Axis::Vertical => (feat.from_center.x + feat.to_center.x) / 2.0,
            };
            coords.push(coord);
        }
    }

    if coords.is_empty() {
        return 0.0;
    }

    // 中位数
    coords.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if coords.len() % 2 == 1 {
        coords[coords.len() / 2]
    } else {
        (coords[coords.len() / 2 - 1] + coords[coords.len() / 2]) / 2.0
    };

    // 量化到 8px 网格
    quantize(median, GRID_STEP)
}

/// 量化坐标到网格步长。
fn quantize(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

/// Step 4.3: 计算主干范围（起止点）。
///
/// 主干沿主轴方向延伸：
/// - 水平主干：x 从 min(path端点x) - margin 到 max(path端点x) + margin，y = trunk_coord
/// - 垂直主干：y 从 min(path端点y) - margin 到 max(path端点y) + margin，x = trunk_coord
///
/// 使用路径端点（from_anchor / to_anchor）而非节点中心，确保主干范围
/// 覆盖实际路径接入点，避免 entry/exit 落在节点内部。
fn compute_trunk_range(
    edges: &[usize],
    features: &[EdgeFeatures],
    axis: Axis,
    trunk_coord: f64,
) -> (Point, Point) {
    let mut min_proj = f64::INFINITY;
    let mut max_proj = f64::NEG_INFINITY;

    for &edge_idx in edges {
        let feat = &features[edge_idx];
        // 使用路径端点（from_anchor / to_anchor）而非节点中心
        let path = &feat.path_points;
        if path.is_empty() {
            continue;
        }
        let from_anchor = path[0];
        let to_anchor = path[path.len() - 1];
        let (proj1, proj2) = match axis {
            Axis::Horizontal => (from_anchor.x, to_anchor.x),
            Axis::Vertical => (from_anchor.y, to_anchor.y),
        };
        min_proj = min_proj.min(proj1).min(proj2);
        max_proj = max_proj.max(proj1).max(proj2);
    }

    min_proj -= TRUNK_MARGIN;
    max_proj += TRUNK_MARGIN;

    match axis {
        Axis::Horizontal => (Point::new(min_proj, trunk_coord), Point::new(max_proj, trunk_coord)),
        Axis::Vertical => (Point::new(trunk_coord, min_proj), Point::new(trunk_coord, max_proj)),
    }
}

/// Step 5: 计算分叉点（entry / exit）。
///
/// 详见 §4.7：
/// - entry_i：from 端在主干上的投影点，向 to 方向偏移 `fork_distance`
/// - exit_i：to 端在主干上的投影点，向 from 方向偏移 `fork_distance`
/// - 按 from/to 节点的垂直坐标排序，确保 fork_spacing 间距
///
/// 偏移 `fork_distance` 的目的：使 entry/exit 落在 stub 终点之外，
/// 避免 merge leg / fork leg 反向（否则 Ink saving 为负 → 触发回退）。
fn compute_fork_points(
    edges: &[usize],
    features: &[EdgeFeatures],
    axis: Axis,
    trunk_coord: f64,
    trunk_start: Point,
    trunk_end: Point,
    config: &BundlingConfig,
) -> (Vec<Point>, Vec<Point>) {
    // 主轴方向的范围
    let (axis_min, axis_max) = match axis {
        Axis::Horizontal => (trunk_start.x, trunk_end.x),
        Axis::Vertical => (trunk_start.y, trunk_end.y),
    };

    // 计算每条边的原始 entry/exit 投影
    let mut entry_data: Vec<(usize, f64, f64)> = Vec::with_capacity(edges.len()); // (edge_idx, perp_coord, axis_proj)
    let mut exit_data: Vec<(usize, f64, f64)> = Vec::with_capacity(edges.len());

    // 先计算多数边的行进方向，确保主干方向一致
    let mut forward_count = 0usize;
    for &edge_idx in edges {
        let feat = &features[edge_idx];
        let path = &feat.path_points;
        if path.is_empty() {
            continue;
        }
        let from_anchor = path[0];
        let to_anchor = path[path.len() - 1];
        let (from_proj, to_proj) = match axis {
            Axis::Horizontal => (from_anchor.x, to_anchor.x),
            Axis::Vertical => (from_anchor.y, to_anchor.y),
        };
        if to_proj >= from_proj {
            forward_count += 1;
        }
    }
    let majority_forward = forward_count * 2 >= edges.len();
    let majority_dir_sign = if majority_forward { 1.0 } else { -1.0 };

    for &edge_idx in edges {
        let feat = &features[edge_idx];
        // 使用路径端点（from_anchor / to_anchor）而非节点中心。
        let path = &feat.path_points;
        if path.is_empty() {
            continue;
        }
        let from_anchor = path[0];
        let to_anchor = path[path.len() - 1];
        let (from_proj, from_perp, to_proj, to_perp) = match axis {
            Axis::Horizontal => (
                from_anchor.x,
                from_anchor.y,
                to_anchor.x,
                to_anchor.y,
            ),
            Axis::Vertical => (
                from_anchor.y,
                from_anchor.x,
                to_anchor.y,
                to_anchor.x,
            ),
        };

        // §4.7: 使用多数边方向，确保 bundle 内主干方向一致
        let dir_sign = majority_dir_sign;
        let entry_proj = (from_proj + dir_sign * config.fork_distance).clamp(axis_min, axis_max);
        let exit_proj = (to_proj - dir_sign * config.fork_distance).clamp(axis_min, axis_max);

        entry_data.push((edge_idx, from_perp, entry_proj));
        exit_data.push((edge_idx, to_perp, exit_proj));
    }

    // 按垂直坐标排序（确定性：perp_coord 升序 → edge_index 升序）
    entry_data.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    exit_data.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });

    // 提取轴上坐标并确保 fork_spacing 间距
    // 同源/同宿边额外增加间距，避免视觉重叠
    let entry_axis_coords = enforce_fork_spacing_with_groups(
        &entry_data,
        features,
        true, // entry: 按 from_id 分组
        config.fork_spacing,
        axis_min,
        axis_max,
    );
    let exit_axis_coords = enforce_fork_spacing_with_groups(
        &exit_data,
        features,
        false, // exit: 按 to_id 分组
        config.fork_spacing,
        axis_min,
        axis_max,
    );

    // 转换为 Point 坐标
    let entry_points: Vec<Point> = entry_axis_coords
        .iter()
        .map(|&coord| match axis {
            Axis::Horizontal => Point::new(coord, trunk_coord),
            Axis::Vertical => Point::new(trunk_coord, coord),
        })
        .collect();
    let exit_points: Vec<Point> = exit_axis_coords
        .iter()
        .map(|&coord| match axis {
            Axis::Horizontal => Point::new(coord, trunk_coord),
            Axis::Vertical => Point::new(trunk_coord, coord),
        })
        .collect();

    (entry_points, exit_points)
}

/// 同源/同宿感知的分叉点间距调整。
///
/// 当多条边共享同一 from_id（entry）或 to_id（exit）时，
/// 它们的分叉点应该在视觉上更分散，避免密集重叠。
/// 对同组边使用 `spacing * 2` 的间距，跨组边使用普通 `spacing`。
fn enforce_fork_spacing_with_groups(
    data: &[(usize, f64, f64)], // (edge_idx, perp_coord, axis_proj)
    features: &[EdgeFeatures],
    use_from: bool, // true → 按 from_id 分组, false → 按 to_id 分组
    spacing: f64,
    min_bound: f64,
    max_bound: f64,
) -> Vec<f64> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<f64> = data.iter().map(|d| d.2).collect();

    // 分组：按 from_id 或 to_id 分组
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, &(edge_idx, _, _)) in data.iter().enumerate() {
        let key = if use_from {
            features[edge_idx].from_id.clone()
        } else {
            features[edge_idx].to_id.clone()
        };
        if let Some(&g) = seen.get(&key) {
            groups[g].push(i);
        } else {
            seen.insert(key, groups.len());
            groups.push(vec![i]);
        }
    }

    // 对同组内（≥2 条边）使用加倍间距
    let group_spacing = spacing * 2.0;

    for _iteration in 0..16 {
        let mut adjusted = false;
        for i in 1..result.len() {
            let gap = result[i] - result[i - 1];
            // 检查 i-1 和 i 是否同组，同组使用加倍间距
            let same_group = groups.iter().any(|g| g.contains(&(i - 1)) && g.contains(&i));
            let min_gap = if same_group { group_spacing } else { spacing };
            if gap < min_gap - EPS {
                let deficit = min_gap - gap;
                let push = deficit / 2.0;
                result[i - 1] -= push;
                result[i] += push;
                adjusted = true;
            }
        }
        if !adjusted {
            break;
        }
    }

    for coord in &mut result {
        *coord = coord.clamp(min_bound, max_bound);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ArrowType, AttributeMap, Identifier, Relation, Span,
    };
    use crate::layout::NodeLayout;
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

    fn make_node_layout(cx: f64, cy: f64) -> NodeLayout {
        NodeLayout {
            x: cx - 40.0,
            y: cy - 20.0,
            width: 80.0,
            height: 40.0,
        }
    }

    fn make_nodes(ids: &[&str], centers: &[(f64, f64)]) -> HashMap<String, NodeLayout> {
        let mut nodes = HashMap::new();
        for (i, id) in ids.iter().enumerate() {
            nodes.insert(id.to_string(), make_node_layout(centers[i].0, centers[i].1));
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

    #[test]
    fn horizontal_trunk_for_horizontal_edges() {
        // 节点垂直间距 80px（40px gap），使 trunk 中位坐标不穿障
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (200.0, 0.0), (0.0, 80.0), (200.0, 80.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(240.0, 0.0)],
            vec![Point::new(40.0, 80.0), Point::new(240.0, 80.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let candidate = BundleCandidate { edges: vec![0, 1] };
        let config = BundlingConfig::default();
        let bundles = allocate_trunks(&[candidate], &features, &nodes, &config);
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].trunk_axis, Axis::Horizontal);
        // 主干 y 坐标应在 0~80 之间（中位数附近，无穿障偏移）
        let trunk_y = bundles[0].trunk_start.y;
        assert!(trunk_y >= 0.0 && trunk_y <= 80.0, "trunk_y={}", trunk_y);
    }

    #[test]
    fn vertical_trunk_for_vertical_edges() {
        // 节点水平间距 120px（40px gap），使 trunk 中位坐标不穿障
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (0.0, 200.0), (120.0, 0.0), (120.0, 200.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(0.0, 20.0), Point::new(0.0, 220.0)],
            vec![Point::new(120.0, 20.0), Point::new(120.0, 220.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let candidate = BundleCandidate { edges: vec![0, 1] };
        let config = BundlingConfig::default();
        let bundles = allocate_trunks(&[candidate], &features, &nodes, &config);
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].trunk_axis, Axis::Vertical);
        // 主干 x 坐标应在 0~120 之间
        let trunk_x = bundles[0].trunk_start.x;
        assert!(trunk_x >= 0.0 && trunk_x <= 120.0, "trunk_x={}", trunk_x);
    }

    #[test]
    fn trunk_range_includes_all_edges_with_margin() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (200.0, 0.0), (0.0, 80.0), (200.0, 80.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(240.0, 0.0)],
            vec![Point::new(40.0, 80.0), Point::new(240.0, 80.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let candidate = BundleCandidate { edges: vec![0, 1] };
        let config = BundlingConfig::default();
        let bundles = allocate_trunks(&[candidate], &features, &nodes, &config);
        let bundle = &bundles[0];

        // 主干 x 范围应包含路径端点 x（40~240）并外扩 TRUNK_MARGIN
        let start_x = bundle.trunk_start.x;
        let end_x = bundle.trunk_end.x;
        assert!(start_x <= 40.0, "trunk_start_x should be <= 40, got {}", start_x);
        assert!(end_x >= 240.0, "trunk_end_x should be >= 240, got {}", end_x);
        assert!((end_x - start_x) >= 200.0 + 2.0 * TRUNK_MARGIN);
    }

    #[test]
    fn entry_exit_points_on_trunk() {
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (200.0, 0.0), (0.0, 80.0), (200.0, 80.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(240.0, 0.0)],
            vec![Point::new(40.0, 80.0), Point::new(240.0, 80.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let candidate = BundleCandidate { edges: vec![0, 1] };
        let config = BundlingConfig::default();
        let bundles = allocate_trunks(&[candidate], &features, &nodes, &config);
        let bundle = &bundles[0];

        let trunk_y = bundle.trunk_start.y;
        // 所有 entry/exit 点的 y 坐标应等于 trunk_y
        for pt in &bundle.entry_points {
            assert!((pt.y - trunk_y).abs() < EPS, "entry point y should be trunk_y");
        }
        for pt in &bundle.exit_points {
            assert!((pt.y - trunk_y).abs() < EPS, "exit point y should be trunk_y");
        }
    }

    #[test]
    fn fork_points_respect_min_spacing() {
        // 两条 from 端 x 坐标非常接近的边，节点垂直间距 80px 避免穿障
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(100.0, 0.0), (300.0, 0.0), (101.0, 80.0), (300.0, 80.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(140.0, 0.0), Point::new(340.0, 0.0)],
            vec![Point::new(141.0, 80.0), Point::new(340.0, 80.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let candidate = BundleCandidate { edges: vec![0, 1] };
        let config = BundlingConfig {
            fork_spacing: 16.0,
            ..Default::default()
        };
        let bundles = allocate_trunks(&[candidate], &features, &nodes, &config);
        let bundle = &bundles[0];

        // entry 点间距应 ≥ fork_spacing
        let entry_gap = (bundle.entry_points[0].x - bundle.entry_points[1].x).abs();
        assert!(
            entry_gap >= config.fork_spacing - EPS,
            "entry gap {} should be >= fork_spacing {}",
            entry_gap,
            config.fork_spacing
        );
    }

    #[test]
    fn trunk_coordinate_quantized_to_grid() {
        // 节点垂直间距 80px，中位坐标 40 已量化
        let nodes = make_nodes(&["A", "B", "C", "D"], &[(0.0, 0.0), (200.0, 0.0), (0.0, 80.0), (200.0, 80.0)]);
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(240.0, 0.0)],
            vec![Point::new(40.0, 80.0), Point::new(240.0, 80.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let candidate = BundleCandidate { edges: vec![0, 1] };
        let config = BundlingConfig::default();
        let bundles = allocate_trunks(&[candidate], &features, &nodes, &config);
        let trunk_y = bundles[0].trunk_start.y;
        // 应量化到 8px 网格
        assert!(
            (trunk_y % GRID_STEP).abs() < EPS,
            "trunk_y {} should be multiple of {}",
            trunk_y,
            GRID_STEP
        );
    }

    #[test]
    fn bundle_id_assigned_sequentially() {
        let nodes = make_nodes(
            &["A", "B", "C", "D", "E", "F"],
            &[(0.0, 0.0), (200.0, 0.0), (0.0, 80.0), (200.0, 80.0), (0.0, 160.0), (200.0, 160.0)],
        );
        let rels = vec![
            make_relation("A", "B", ArrowType::Active),
            make_relation("C", "D", ArrowType::Active),
            make_relation("E", "F", ArrowType::Passive), // 不同箭头类型 → 不同 bundle
        ];
        let paths = vec![
            vec![Point::new(40.0, 0.0), Point::new(240.0, 0.0)],
            vec![Point::new(40.0, 80.0), Point::new(240.0, 80.0)],
            vec![Point::new(40.0, 160.0), Point::new(240.0, 160.0)],
        ];
        let features: Vec<EdgeFeatures> = rels
            .iter()
            .enumerate()
            .map(|(i, rel)| make_features(i, rel, &nodes, &paths[i]))
            .collect();

        let candidates = vec![
            BundleCandidate { edges: vec![0, 1] },
            BundleCandidate { edges: vec![2] }, // 单条边会被过滤
        ];
        let config = BundlingConfig::default();
        let bundles = allocate_trunks(&candidates, &features, &nodes, &config);
        // 只有第一个 candidate 有效（2 条边），第二个被过滤（单条边）
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].id, 0);
    }
}