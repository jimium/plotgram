//! 平行边分组与偏移计算
//!
//! 同一对节点间的多条边需要分组并分配偏移量，避免视觉重叠。
//! 标签放置还覆盖 fan-in / fan-out（共享端点）边，避免汇入/扇出走廊上的标签互撞。

use crate::layout::constants::{
    DEFAULT_EDGE_OFFSET, DEFAULT_LABEL_PERP_OFFSET, DEFAULT_LEADER_LINE_MIN_LENGTH,
};
use crate::layout::geometry::Point;
use crate::layout::EdgeLabelLayout;
use super::edge_geometry::{
    build_edge_labels, canonical_pair, leader_anchor_on_path, parse_label_t, point_at_path_t,
    undirected_pair_key,
};
use super::label_avoidance::{label_metrics, leader_visible_length};

/// 标签外侧与边路径之间的空隙（标签半宽之外再留一点，避免贴线）。
const LABEL_SIDE_GAP: f64 = 6.0;

/// 同向平行边 / fan 边标签 t 的分布区间
const SAME_DIR_LABEL_T_MIN: f64 = 0.28;
const SAME_DIR_LABEL_T_MAX: f64 = 0.72;

/// 平行边分组结果
///
/// 仅保留每条边的偏移量；分组内部信息不对外暴露
/// （orthogonal / circular 路由器各自维护分组逻辑，需求不同）。
pub struct ParallelGroups {
    /// 每条边的垂直偏移量
    pub offsets: Vec<f64>,
}

/// 对平行边进行分组并计算偏移量
///
/// 返回每条边的偏移量。同一对节点间的多条边围绕基线对称分布；
/// 正反向边分别落在法线两侧。
pub fn group_parallel_edges(
    relations: &[crate::ast::Relation],
    edge_offset: f64,
) -> ParallelGroups {
    let n = relations.len();
    let mut pair_groups: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, rel) in relations.iter().enumerate() {
        let key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
        pair_groups.entry(key).or_default().push(i);
    }

    let mut offsets = vec![0.0; n];
    let mut pair_keys: Vec<String> = pair_groups.keys().cloned().collect();
    pair_keys.sort();
    for key in &pair_keys {
        let indices = &pair_groups[key];
        if indices.len() == 1 {
            continue;
        }

        let rel0 = &relations[indices[0]];
        let (can_from, can_to) = canonical_pair(rel0.from.as_str(), rel0.to.as_str());

        let mut forward = Vec::new();
        let mut backward = Vec::new();
        for &i in indices {
            let rel = &relations[i];
            if rel.from.as_str() == can_from && rel.to.as_str() == can_to {
                forward.push(i);
            } else {
                backward.push(i);
            }
        }

        if !forward.is_empty() && !backward.is_empty() {
            distribute_offsets(&mut offsets, &forward, edge_offset / 2.0);
            distribute_offsets(&mut offsets, &backward, -edge_offset / 2.0);
        } else {
            distribute_offsets(&mut offsets, indices, 0.0);
        }
    }

    ParallelGroups { offsets }
}

/// 基于一组边索引分配偏移量，围绕 base 值居中展开
pub fn distribute_offsets(offsets: &mut [f64], indices: &[usize], base: f64) {
    let n = indices.len();
    for (j, &i) in indices.iter().enumerate() {
        let centered = j as f64 - (n - 1) as f64 / 2.0;
        offsets[i] = base + centered * DEFAULT_EDGE_OFFSET;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SiblingKind {
    /// 同一无向节点对（含正反向）
    UndirectedPair,
    /// 多源汇入同一 `to`
    FanIn,
    /// 同一 `from` 扇出到多宿
    FanOut,
}

/// 收集与本边共享标签避让关系的兄弟边（稳定排序）。
///
/// 优先级：无向对 > 更大的 fan-in/fan-out 组（并列时偏 fan-in）。
fn label_sibling_group(
    relations: &[crate::ast::Relation],
    edge_index: usize,
) -> (Vec<usize>, SiblingKind) {
    let rel = &relations[edge_index];
    let pair_key = undirected_pair_key(rel.from.as_str(), rel.to.as_str());
    let mut pair_siblings: Vec<usize> = relations
        .iter()
        .enumerate()
        .filter(|(_, r)| undirected_pair_key(r.from.as_str(), r.to.as_str()) == pair_key)
        .map(|(i, _)| i)
        .collect();
    pair_siblings.sort_unstable();
    if pair_siblings.len() >= 2 {
        return (pair_siblings, SiblingKind::UndirectedPair);
    }

    let mut fan_in: Vec<usize> = relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.to.as_str() == rel.to.as_str())
        .map(|(i, _)| i)
        .collect();
    fan_in.sort_by(|&a, &b| {
        relations[a]
            .from
            .as_str()
            .cmp(relations[b].from.as_str())
            .then_with(|| a.cmp(&b))
    });

    let mut fan_out: Vec<usize> = relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.from.as_str() == rel.from.as_str())
        .map(|(i, _)| i)
        .collect();
    fan_out.sort_by(|&a, &b| {
        relations[a]
            .to
            .as_str()
            .cmp(relations[b].to.as_str())
            .then_with(|| a.cmp(&b))
    });

    let fan_in_ok = fan_in.len() >= 2;
    let fan_out_ok = fan_out.len() >= 2;
    match (fan_in_ok, fan_out_ok) {
        (true, true) if fan_out.len() > fan_in.len() => (fan_out, SiblingKind::FanOut),
        (true, _) => (fan_in, SiblingKind::FanIn),
        (false, true) => (fan_out, SiblingKind::FanOut),
        (false, false) => (pair_siblings, SiblingKind::UndirectedPair),
    }
}

/// 同源扇出 / 同宿汇入兄弟（稳定排序）。用于 UndirectedPair 之上叠加 Fan 分离。
fn same_endpoint_fan_siblings(
    relations: &[crate::ast::Relation],
    edge_index: usize,
) -> Option<(Vec<usize>, SiblingKind)> {
    let rel = &relations[edge_index];

    let mut fan_out: Vec<usize> = relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.from.as_str() == rel.from.as_str())
        .map(|(i, _)| i)
        .collect();
    fan_out.sort_by(|&a, &b| {
        relations[a]
            .to
            .as_str()
            .cmp(relations[b].to.as_str())
            .then_with(|| a.cmp(&b))
    });
    if fan_out.len() >= 2 {
        return Some((fan_out, SiblingKind::FanOut));
    }

    let mut fan_in: Vec<usize> = relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.to.as_str() == rel.to.as_str())
        .map(|(i, _)| i)
        .collect();
    fan_in.sort_by(|&a, &b| {
        relations[a]
            .from
            .as_str()
            .cmp(relations[b].from.as_str())
            .then_with(|| a.cmp(&b))
    });
    if fan_in.len() >= 2 {
        return Some((fan_in, SiblingKind::FanIn));
    }
    None
}

/// 平行 / 反向 / fan-in / fan-out 边的标签初始放置：`(middle_t, 法向偏移)`。
///
/// - **正反向对**：两侧外偏，t 保持中段；若同 from/to 另有 ≥2 条同向边，再叠加 Fan 的 t 错开与外推。
/// - **同向多边 / fan**：外偏 + 按稳定序错开 t，避免水平走廊上宽标签互撞。
/// - 用户显式设置 `label_position` 时保留其 t，仍施加侧向偏移。
pub fn label_placement_for_parallel_edge(
    rel: &crate::ast::Relation,
    edge_index: usize,
    relations: &[crate::ast::Relation],
    parallel_offsets: &[f64],
    path: &[Point],
) -> (f64, Point) {
    let user_set_t = rel.attributes.style.contains_key("label_position");
    let mut middle_t = parse_label_t(rel);

    let (siblings, kind) = label_sibling_group(relations, edge_index);
    if siblings.len() < 2 {
        return (middle_t, Point::new(0.0, -6.0));
    }

    let rank = siblings.iter().position(|&i| i == edge_index).unwrap_or(0);
    let n = siblings.len();
    // UndirectedPair 抢占 Fan 分组时，仍叠加同源/同宿扇出分离
    let fan_overlay = if kind == SiblingKind::UndirectedPair {
        same_endpoint_fan_siblings(relations, edge_index)
    } else {
        None
    };

    if !user_set_t {
        match kind {
            SiblingKind::UndirectedPair => {
                let (can_from, can_to) = canonical_pair(rel.from.as_str(), rel.to.as_str());
                let mut forward = Vec::new();
                let mut backward = Vec::new();
                for &i in &siblings {
                    let r = &relations[i];
                    if r.from.as_str() == can_from && r.to.as_str() == can_to {
                        forward.push(i);
                    } else {
                        backward.push(i);
                    }
                }
                let is_reverse_pair = !forward.is_empty() && !backward.is_empty();
                if is_reverse_pair {
                    let is_forward = rel.from.as_str() == can_from && rel.to.as_str() == can_to;
                    let same_dir = if is_forward { &forward } else { &backward };
                    if same_dir.len() > 1 {
                        let dir_rank = same_dir.iter().position(|&i| i == edge_index).unwrap_or(0);
                        let span = SAME_DIR_LABEL_T_MAX - SAME_DIR_LABEL_T_MIN;
                        middle_t = SAME_DIR_LABEL_T_MIN
                            + (dir_rank as f64 / (same_dir.len() - 1) as f64) * span * 0.5;
                    } else {
                        middle_t = 0.5;
                    }
                } else {
                    let span = SAME_DIR_LABEL_T_MAX - SAME_DIR_LABEL_T_MIN;
                    middle_t = SAME_DIR_LABEL_T_MIN + (rank as f64 / (n - 1) as f64) * span;
                }
                // 扇出叠加：跨不同无向对的同向边错开 t，避免都钉在 0.5
                if let Some((ref fan, _)) = fan_overlay {
                    let fan_rank = fan.iter().position(|&i| i == edge_index).unwrap_or(0);
                    let fan_n = fan.len();
                    if fan_n >= 2 {
                        let span = SAME_DIR_LABEL_T_MAX - SAME_DIR_LABEL_T_MIN;
                        middle_t =
                            SAME_DIR_LABEL_T_MIN + (fan_rank as f64 / (fan_n - 1) as f64) * span;
                    }
                }
            }
            SiblingKind::FanIn | SiblingKind::FanOut => {
                // 汇入/扇出：优先落在最长水平走廊中点
                if let Some((t_on_h, _)) = longest_horizontal_segment_t(path) {
                    middle_t = t_on_h.clamp(0.2, 0.8);
                } else {
                    let span = SAME_DIR_LABEL_T_MAX - SAME_DIR_LABEL_T_MIN;
                    middle_t = SAME_DIR_LABEL_T_MIN + (rank as f64 / (n - 1) as f64) * span;
                }
            }
        }
    }

    if path.len() < 2 {
        return (middle_t, Point::new(0.0, 0.0));
    }

    let normal = canonical_path_normal(path, middle_t);
    let offset_scalar = parallel_offsets.get(edge_index).copied().unwrap_or(0.0);
    let sign = if offset_scalar.abs() > 0.1 {
        offset_scalar.signum()
    } else {
        // fan / 同向：按稳定序交替法向侧（水平走廊 → 上下分开）
        if rank % 2 == 0 {
            1.0
        } else {
            -1.0
        }
    };

    let dist = side_offset_distance(rel);
    let mut label_offset = match kind {
        SiblingKind::UndirectedPair => {
            Point::new(normal.x * sign * dist, normal.y * sign * dist)
        }
        SiblingKind::FanIn | SiblingKind::FanOut => {
            // 按稳定序：靠前→左、靠后→右，把标签推到两侧空白
            let outward_x = fan_outward_x(rank, n);
            if longest_horizontal_segment_t(path).is_some() || normal.y.abs() >= normal.x.abs() {
                Point::new(outward_x * dist, 0.0)
            } else {
                // 无水平走廊时退回法向交替
                Point::new(normal.x * sign * dist, normal.y * sign * dist)
            }
        }
    };

    // UndirectedPair 之上叠加 Fan：沿扇出方向外推，避免多条正向标签同侧同 x
    if let Some((ref fan, _)) = fan_overlay {
        let fan_rank = fan.iter().position(|&i| i == edge_index).unwrap_or(0);
        let fan_n = fan.len();
        if fan_n >= 2 {
            let outward_x = fan_outward_x(fan_rank, fan_n);
            if longest_horizontal_segment_t(path).is_some() || normal.y.abs() >= normal.x.abs() {
                // 水平走廊 / 竖直法向：X 向外推开，保留正反向对的法向侧偏 Y 分量
                label_offset = Point::new(outward_x * dist, label_offset.y);
            } else {
                // 竖直走廊（法向以 X 为主）：用扇出符号覆盖同侧贴合
                label_offset = Point::new(outward_x * dist, label_offset.y * 0.35);
            }
        }
    }

    (middle_t, label_offset)
}

/// fan 组内水平外偏符号：两端向外，中间交替。
fn fan_outward_x(rank: usize, n: usize) -> f64 {
    if n <= 1 {
        return 0.0;
    }
    if rank == 0 {
        -1.0
    } else if rank + 1 == n {
        1.0
    } else if rank % 2 == 0 {
        -1.0
    } else {
        1.0
    }
}

/// 标签中心相对路径的侧向偏移量：半宽 + 空隙，至少 `DEFAULT_LABEL_PERP_OFFSET`。
fn side_offset_distance(rel: &crate::ast::Relation) -> f64 {
    let text = rel.label.as_deref().unwrap_or("");
    if text.is_empty() {
        return DEFAULT_LABEL_PERP_OFFSET;
    }
    let (w, _) = label_metrics(text);
    (w * 0.5 + LABEL_SIDE_GAP).max(DEFAULT_LABEL_PERP_OFFSET)
}

/// 最长水平段的弧长中点对应的路径参数 t，以及该段长度。
fn longest_horizontal_segment_t(path: &[Point]) -> Option<(f64, f64)> {
    if path.len() < 2 {
        return None;
    }
    let mut total = 0.0;
    let mut segs: Vec<(f64, f64, f64, bool)> = Vec::new(); // (start_len, end_len, len, is_h)
    for w in path.windows(2) {
        let dx = w[1].x - w[0].x;
        let dy = w[1].y - w[0].y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            continue;
        }
        let is_h = dx.abs() >= dy.abs();
        segs.push((total, total + len, len, is_h));
        total += len;
    }
    if total < 1e-6 {
        return None;
    }
    let best = segs
        .iter()
        .filter(|s| s.3)
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))?;
    if best.2 < 16.0 {
        return None; // 太短的水平折不配作走廊
    }
    let mid = (best.0 + best.1) * 0.5;
    Some((mid / total, best.2))
}

/// 在正交路径上构建平行边感知的边标签，并在侧向偏置足够时挂上引线锚点。
pub fn build_parallel_aware_edge_labels(
    rel: &crate::ast::Relation,
    edge_index: usize,
    relations: &[crate::ast::Relation],
    parallel_offsets: &[f64],
    path: &[Point],
) -> Vec<EdgeLabelLayout> {
    let (middle_t, offset) =
        label_placement_for_parallel_edge(rel, edge_index, relations, parallel_offsets, path);
    let mut labels = build_edge_labels(rel, middle_t, offset, |t| point_at_path_t(path, t));
    attach_leaders_if_offset(&mut labels, path);
    labels
}

/// 便捷入口：内部计算 parallel offsets 后构建标签。
pub fn build_parallel_aware_edge_labels_auto(
    rel: &crate::ast::Relation,
    edge_index: usize,
    relations: &[crate::ast::Relation],
    path: &[Point],
) -> Vec<EdgeLabelLayout> {
    let parallel = group_parallel_edges(relations, DEFAULT_EDGE_OFFSET);
    build_parallel_aware_edge_labels(rel, edge_index, relations, &parallel.offsets, path)
}

fn attach_leaders_if_offset(labels: &mut [EdgeLabelLayout], path: &[Point]) {
    if path.len() < 2 {
        return;
    }
    for label in labels.iter_mut() {
        let anchor = leader_anchor_on_path(path, label.center);
        if leader_visible_length(label.center, label.size, anchor) >= DEFAULT_LEADER_LINE_MIN_LENGTH
        {
            label.leader_to = Some(anchor);
        }
    }
}

/// 路径在 t 处的单位法向，并翻转到规范半球（nx>0，或 nx≈0 时 ny>0），
/// 使正反向边共享同一世界系法向基准。
fn canonical_path_normal(path: &[Point], t: f64) -> Point {
    let t0 = (t - 0.01).max(0.0);
    let t1 = (t + 0.01).min(1.0);
    let p0 = point_at_path_t(path, t0);
    let p1 = point_at_path_t(path, t1);
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let mut nx = -dy / len;
    let mut ny = dx / len;
    if nx < -1e-9 || (nx.abs() <= 1e-9 && ny < 0.0) {
        nx = -nx;
        ny = -ny;
    }
    Point::new(nx, ny)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Identifier, Relation, Span};
    use crate::layout::routing::common::label_avoidance::aabb_overlap;

    fn rel(from: &str, to: &str, label: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: Some(label.to_string()),
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn reverse_pair_labels_go_to_opposite_sides() {
        let relations = vec![
            rel("client", "api", "HTTP 请求"),
            rel("api", "client", "JSON 响应"),
        ];
        let parallel = group_parallel_edges(&relations, DEFAULT_EDGE_OFFSET);
        let path_down = vec![Point::new(0.0, 0.0), Point::new(0.0, 100.0)];
        let path_up = vec![Point::new(0.0, 100.0), Point::new(0.0, 0.0)];

        let (t0, off0) = label_placement_for_parallel_edge(
            &relations[0],
            0,
            &relations,
            &parallel.offsets,
            &path_down,
        );
        let (t1, off1) = label_placement_for_parallel_edge(
            &relations[1],
            1,
            &relations,
            &parallel.offsets,
            &path_up,
        );

        assert!((t0 - 0.5).abs() < 1e-9);
        assert!((t1 - 0.5).abs() < 1e-9);
        assert!(off0.x.abs() > 20.0);
        assert!((off0.x - off1.x).abs() > 40.0);
    }

    #[test]
    fn same_direction_parallels_stagger_t() {
        let relations = vec![
            rel("a", "b", "one"),
            rel("a", "b", "two"),
            rel("a", "b", "three"),
        ];
        let parallel = group_parallel_edges(&relations, DEFAULT_EDGE_OFFSET);
        let path = vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)];

        let ts: Vec<f64> = (0..3)
            .map(|i| {
                label_placement_for_parallel_edge(
                    &relations[i],
                    i,
                    &relations,
                    &parallel.offsets,
                    &path,
                )
                .0
            })
            .collect();

        assert!((ts[0] - ts[1]).abs() > 0.1);
        assert!((ts[1] - ts[2]).abs() > 0.1);
        assert!(ts[0] < ts[1] && ts[1] < ts[2]);
    }

    #[test]
    fn fan_in_labels_separate_on_horizontal_corridor() {
        // app_db/log_server → kafka：共享 to，水平走廊上宽标签不得重叠
        let relations = vec![
            rel("app_db", "kafka", "Binlog 同步"),
            rel("log_server", "kafka", "用户日志收集"),
        ];
        let parallel = group_parallel_edges(&relations, DEFAULT_EDGE_OFFSET);
        // 走廊横向间距需接近真实布局（~45px+），标签半宽外偏后才不重叠
        let path0 = vec![
            Point::new(205.0, 100.0),
            Point::new(205.0, 180.0),
            Point::new(235.0, 180.0),
            Point::new(235.0, 220.0),
        ];
        let path1 = vec![
            Point::new(310.0, 100.0),
            Point::new(310.0, 180.0),
            Point::new(280.0, 180.0),
            Point::new(280.0, 220.0),
        ];

        let labels0 =
            build_parallel_aware_edge_labels(&relations[0], 0, &relations, &parallel.offsets, &path0);
        let labels1 =
            build_parallel_aware_edge_labels(&relations[1], 1, &relations, &parallel.offsets, &path1);

        // 左扇叶应偏左、右扇叶应偏右
        assert!(
            labels0[0].center.x < 205.0,
            "left fan label should go outward left, got {:?}",
            labels0[0].center
        );
        assert!(
            labels1[0].center.x > 280.0,
            "right fan label should go outward right, got {:?}",
            labels1[0].center
        );

        let b0 = (
            labels0[0].center.x - labels0[0].size.0 / 2.0,
            labels0[0].center.y - labels0[0].size.1 / 2.0,
            labels0[0].center.x + labels0[0].size.0 / 2.0,
            labels0[0].center.y + labels0[0].size.1 / 2.0,
        );
        let b1 = (
            labels1[0].center.x - labels1[0].size.0 / 2.0,
            labels1[0].center.y - labels1[0].size.1 / 2.0,
            labels1[0].center.x + labels1[0].size.0 / 2.0,
            labels1[0].center.y + labels1[0].size.1 / 2.0,
        );
        assert!(
            aabb_overlap(&b0, &b1).is_none(),
            "fan-in labels overlap: {:?} vs {:?}",
            labels0[0].center,
            labels1[0].center
        );
    }

    #[test]
    fn fan_out_labels_separate_on_horizontal_corridor() {
        let relations = vec![
            rel("kafka", "flink", "实时流消费"),
            rel("kafka", "spark", "离线批量消费"),
        ];
        let parallel = group_parallel_edges(&relations, DEFAULT_EDGE_OFFSET);
        let path0 = vec![
            Point::new(235.0, 260.0),
            Point::new(235.0, 322.0),
            Point::new(180.0, 322.0),
            Point::new(180.0, 380.0),
        ];
        let path1 = vec![
            Point::new(251.0, 260.0),
            Point::new(251.0, 322.0),
            Point::new(320.0, 322.0),
            Point::new(320.0, 380.0),
        ];

        let labels0 =
            build_parallel_aware_edge_labels(&relations[0], 0, &relations, &parallel.offsets, &path0);
        let labels1 =
            build_parallel_aware_edge_labels(&relations[1], 1, &relations, &parallel.offsets, &path1);

        assert!(labels0[0].center.x < labels1[0].center.x);

        let b0 = (
            labels0[0].center.x - labels0[0].size.0 / 2.0,
            labels0[0].center.y - labels0[0].size.1 / 2.0,
            labels0[0].center.x + labels0[0].size.0 / 2.0,
            labels0[0].center.y + labels0[0].size.1 / 2.0,
        );
        let b1 = (
            labels1[0].center.x - labels1[0].size.0 / 2.0,
            labels1[0].center.y - labels1[0].size.1 / 2.0,
            labels1[0].center.x + labels1[0].size.0 / 2.0,
            labels1[0].center.y + labels1[0].size.1 / 2.0,
        );
        assert!(
            aabb_overlap(&b0, &b1).is_none(),
            "fan-out labels overlap: {:?} vs {:?}",
            labels0[0].center,
            labels1[0].center
        );
    }

    #[test]
    fn undirected_pair_plus_fan_out_staggers_forward_labels() {
        // auth→db / auth→cache 各有回边：UndirectedPair 抢占 FanOut 时仍应叠加 t 错开与外推
        let relations = vec![
            rel("auth", "db", "查询用户信息"),
            rel("db", "auth", "返回用户记录"),
            rel("auth", "cache", "存储 Token"),
            rel("cache", "auth", "返回缓存结果"),
        ];
        let parallel = group_parallel_edges(&relations, DEFAULT_EDGE_OFFSET);
        let path_db = vec![Point::new(200.0, 300.0), Point::new(200.0, 420.0)];
        let path_cache = vec![Point::new(280.0, 300.0), Point::new(280.0, 420.0)];

        let (t_db, off_db) = label_placement_for_parallel_edge(
            &relations[0],
            0,
            &relations,
            &parallel.offsets,
            &path_db,
        );
        let (t_cache, off_cache) = label_placement_for_parallel_edge(
            &relations[2],
            2,
            &relations,
            &parallel.offsets,
            &path_cache,
        );

        assert!(
            (t_db - t_cache).abs() > 0.1,
            "fan overlay should stagger t: {t_db} vs {t_cache}"
        );
        assert!(
            off_db.x.signum() != off_cache.x.signum() || (off_db.x - off_cache.x).abs() > 20.0,
            "fan overlay should push outward: {:?} vs {:?}",
            off_db,
            off_cache
        );

        let labels_db = build_parallel_aware_edge_labels(
            &relations[0],
            0,
            &relations,
            &parallel.offsets,
            &path_db,
        );
        let labels_cache = build_parallel_aware_edge_labels(
            &relations[2],
            2,
            &relations,
            &parallel.offsets,
            &path_cache,
        );
        let b0 = (
            labels_db[0].center.x - labels_db[0].size.0 / 2.0,
            labels_db[0].center.y - labels_db[0].size.1 / 2.0,
            labels_db[0].center.x + labels_db[0].size.0 / 2.0,
            labels_db[0].center.y + labels_db[0].size.1 / 2.0,
        );
        let b1 = (
            labels_cache[0].center.x - labels_cache[0].size.0 / 2.0,
            labels_cache[0].center.y - labels_cache[0].size.1 / 2.0,
            labels_cache[0].center.x + labels_cache[0].size.0 / 2.0,
            labels_cache[0].center.y + labels_cache[0].size.1 / 2.0,
        );
        assert!(
            aabb_overlap(&b0, &b1).is_none(),
            "pair+fan labels overlap: {:?} vs {:?}",
            labels_db[0].center,
            labels_cache[0].center
        );
    }
}
