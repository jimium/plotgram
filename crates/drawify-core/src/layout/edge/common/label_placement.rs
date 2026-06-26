//! 标签放置策略统一接口
//!
//! 不同布局对标签避让有不同需求：
//! - 结构化布局（Sugiyama/architecture）使用 AABB 轴对齐推开，适合矩形节点网格
//! - 圆形布局使用径向推开，标签沿圆心放射方向远离节点，保持弧形美感
//!
//! `LabelPlacer` trait 抽象此差异，`ChainedPlacer` 允许串联多个策略。
//! 各边路由算法在完成初始标签定位后调用对应 placer 消除重叠。
//!
//! P0 重构：标签文本与位置均从 `edge.labels[0]` 获取（LayoutResult 自洽），
//! 不再需要传入 `relations`。

use crate::layout::constants::DEFAULT_MAX_LABEL_ITERATIONS;
use crate::layout::geometry::Point;
use crate::layout::{EdgeLayout, GroupLayout, NodeLayout};
use crate::layout::edge::common::edge_geometry::node_center;
use std::collections::HashMap;

/// 标签避让上下文：提供节点与分组障碍信息
pub struct LabelContext<'a> {
    pub nodes: &'a HashMap<String, NodeLayout>,
    pub groups: &'a HashMap<String, GroupLayout>,
}

impl<'a> LabelContext<'a> {
    pub fn new(
        nodes: &'a HashMap<String, NodeLayout>,
        groups: &'a HashMap<String, GroupLayout>,
    ) -> Self {
        Self { nodes, groups }
    }
}

/// 标签放置策略 trait
///
/// 实现方在 `place` 中读取 `edge.label_pos()` 并就地调整，
/// 消除标签-标签、标签-节点、标签-分组之间的重叠。
pub trait LabelPlacer: std::fmt::Debug {
    fn place(&self, edges: &mut [EdgeLayout], ctx: &LabelContext);
}

// ─── AABB 轴对齐推开策略 ─────────────────────────────────

/// 轴对齐推开策略（默认）
///
/// 标签-标签、标签-节点、标签-分组均沿坐标轴最小重叠方向推开。
/// 适用于 Sugiyama 分层、architecture、force_directed 等矩形节点布局。
#[derive(Debug)]
pub struct AxisAlignedPlacer {
    pub max_iterations: usize,
}

impl Default for AxisAlignedPlacer {
    fn default() -> Self {
        Self {
            max_iterations: DEFAULT_MAX_LABEL_ITERATIONS,
        }
    }
}

impl LabelPlacer for AxisAlignedPlacer {
    fn place(&self, edges: &mut [EdgeLayout], ctx: &LabelContext) {
        crate::layout::edge::common::label_avoidance::resolve_label_overlaps(
            edges,
            ctx.nodes,
            ctx.groups,
        );
    }
}

// ─── 径向推开策略（circular 专用）─────────────────────────

/// 径向推开策略
///
/// 标签-标签沿两标签中心连线方向推开；标签-节点沿节点中心到标签的径向方向推开。
/// 适用于 circular 等弧形布局，保持标签沿圆周放射分布的美感。
#[derive(Debug)]
pub struct RadialPlacer {
    pub max_iterations: usize,
    /// 标签-标签推开力度（0~1，越大越激进）
    pub label_push_factor: f64,
    /// 标签-节点径向推开额外距离
    pub node_push_margin: f64,
}

impl Default for RadialPlacer {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            label_push_factor: 0.55,
            node_push_margin: 28.0,
        }
    }
}

impl LabelPlacer for RadialPlacer {
    fn place(&self, edges: &mut [EdgeLayout], ctx: &LabelContext) {
        resolve_label_collisions_radial(edges, self.max_iterations, self.label_push_factor);
        push_labels_away_from_nodes_radial(edges, ctx.nodes, self.node_push_margin);
    }
}

// ─── 串联策略 ────────────────────────────────────────────

/// 串联多个 placer，按顺序执行
///
/// 例如 circular 可串联 `RadialPlacer` + `AxisAlignedPlacer`，
/// 先做径向推开再做轴对齐微调。
#[derive(Debug)]
pub struct ChainedPlacer {
    pub placers: Vec<Box<dyn LabelPlacer>>,
}

impl ChainedPlacer {
    pub fn new(placers: Vec<Box<dyn LabelPlacer>>) -> Self {
        Self { placers }
    }
}

impl LabelPlacer for ChainedPlacer {
    fn place(&self, edges: &mut [EdgeLayout], ctx: &LabelContext) {
        for placer in &self.placers {
            placer.place(edges, ctx);
        }
    }
}

// ─── 径向推开实现 ────────────────────────────────────────

/// 标签-标签径向碰撞消除
///
/// 沿两标签中心连线方向推开，保持弧形布局的放射美感。
fn resolve_label_collisions_radial(
    edges: &mut [EdgeLayout],
    max_iterations: usize,
    push_factor: f64,
) {
    let n = edges.len();
    if n <= 1 {
        return;
    }

    for _ in 0..max_iterations {
        let mut moved = false;
        for i in 0..n {
            if !edges[i].has_label() {
                continue;
            }
            for j in (i + 1)..n {
                if !edges[j].has_label() {
                    continue;
                }
                let bi = edges[i].label_bbox();
                let bj = edges[j].label_bbox();
                if let Some((ox, oy)) = separation_vector(bi, bj) {
                    let mut pi = edges[i].label_pos();
                    pi.x -= ox * push_factor;
                    pi.y -= oy * push_factor;
                    edges[i].set_label_pos(pi);

                    let mut pj = edges[j].label_pos();
                    pj.x += ox * push_factor;
                    pj.y += oy * push_factor;
                    edges[j].set_label_pos(pj);
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
}

/// 标签-节点径向推开
///
/// 标签沿节点中心到标签位置的径向方向被推开，距离不足时向外推。
fn push_labels_away_from_nodes_radial(
    edges: &mut [EdgeLayout],
    nodes: &HashMap<String, NodeLayout>,
    margin: f64,
) {
    // 按 id 排序保证迭代顺序确定（HashMap 迭代顺序随机），
    // 否则位移累积顺序不同导致标签位置抖动
    let mut node_ids: Vec<&String> = nodes.keys().collect();
    node_ids.sort();

    for edge in edges.iter_mut() {
        if !edge.has_label() {
            continue;
        }
        for nid in &node_ids {
            let nl = &nodes[*nid];
            let c = node_center(nl);
            let pos = edge.label_pos();
            let dx = pos.x - c.x;
            let dy = pos.y - c.y;
            let dist = (dx * dx + dy * dy).sqrt();
            let min_dist = nl.width.max(nl.height) * 0.55 + margin;
            if dist < min_dist && dist > 0.5 {
                let push = (min_dist - dist) * 1.15;
                let mut new_pos = pos;
                new_pos.x += dx / dist * push;
                new_pos.y += dy / dist * push;
                edge.set_label_pos(new_pos);
            }
        }
    }
}

/// 两 AABB 的径向分离向量
///
/// 返回从 b 指向 a 的单位向量乘以推开力度。重叠时返回 Some，
/// 否则返回 None。
fn separation_vector(
    a: (f64, f64, f64, f64),
    b: (f64, f64, f64, f64),
) -> Option<(f64, f64)> {
    let overlap_x = (a.2.min(b.2) - a.0.max(b.0)).max(0.0);
    let overlap_y = (a.3.min(b.3) - a.1.max(b.1)).max(0.0);
    if overlap_x <= 0.0 || overlap_y <= 0.0 {
        return None;
    }
    let acx = (a.0 + a.2) / 2.0;
    let acy = (a.1 + a.3) / 2.0;
    let bcx = (b.0 + b.2) / 2.0;
    let bcy = (b.1 + b.3) / 2.0;
    let dx = acx - bcx;
    let dy = acy - bcy;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return Some((overlap_x.max(overlap_y), 0.0));
    }
    let push = overlap_x.max(overlap_y) * 0.6 + 4.0;
    Some((dx / len * push, dy / len * push))
}

// ─── 测试 ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::edge::common::label_avoidance::{aabb_overlap, label_bbox};
    use crate::layout::{EdgeLabelLayout, EdgeLayout, GroupLayout, NodeLayout, PathGeometry, Port};

    fn labeled_edge(label_center: Point) -> EdgeLayout {
        EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(0.0, 0.0),
                end: Point::new(100.0, 0.0),
            },
            labels: vec![EdgeLabelLayout::new("X", label_center)],
            from_port: Port::Bottom,
            to_port: Port::Top,
        }
    }

    #[test]
    fn radial_placer_separates_overlapping_labels() {
        let mut edges = vec![
            labeled_edge(Point::new(50.0, 50.0)),
            labeled_edge(Point::new(52.0, 52.0)),
        ];
        let empty_nodes = HashMap::new();
        let empty_groups = HashMap::new();
        let ctx = LabelContext::new(&empty_nodes, &empty_groups);

        let initial_dist = ((edges[0].label_pos().x - edges[1].label_pos().x).powi(2)
            + (edges[0].label_pos().y - edges[1].label_pos().y).powi(2))
        .sqrt();

        RadialPlacer::default().place(&mut edges, &ctx);

        let final_dist = ((edges[0].label_pos().x - edges[1].label_pos().x).powi(2)
            + (edges[0].label_pos().y - edges[1].label_pos().y).powi(2))
        .sqrt();

        assert!(
            final_dist > initial_dist,
            "radial placer should separate overlapping labels: {} -> {}",
            initial_dist,
            final_dist
        );
    }

    #[test]
    fn radial_placer_pushes_label_from_node() {
        // 标签紧贴节点中心，应被径向推开
        let mut edges = vec![labeled_edge(Point::new(105.0, 50.0))];
        let mut nodes = HashMap::new();
        nodes.insert(
            "n1".to_string(),
            NodeLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
                ..Default::default()
            },
        );
        let empty_groups = HashMap::new();
        let ctx = LabelContext::new(&nodes, &empty_groups);

        let initial_x = edges[0].label_pos().x;
        RadialPlacer::default().place(&mut edges, &ctx);
        // 标签应被向右推（远离节点中心 50,50）
        assert!(
            edges[0].label_pos().x > initial_x,
            "radial placer should push label away from node center: {} -> {}",
            initial_x,
            edges[0].label_pos().x
        );
    }

    #[test]
    fn chained_placer_runs_all() {
        let mut edges = vec![
            labeled_edge(Point::new(50.0, 50.0)),
            labeled_edge(Point::new(52.0, 52.0)),
        ];
        let empty_nodes = HashMap::new();
        let empty_groups = HashMap::new();
        let ctx = LabelContext::new(&empty_nodes, &empty_groups);

        let chained = ChainedPlacer::new(vec![
            Box::new(RadialPlacer::default()),
            Box::new(AxisAlignedPlacer::default()),
        ]);
        chained.place(&mut edges, &ctx);

        // 两个 placer 都应执行，标签应被分开
        let dist = ((edges[0].label_pos().x - edges[1].label_pos().x).powi(2)
            + (edges[0].label_pos().y - edges[1].label_pos().y).powi(2))
        .sqrt();
        assert!(dist > 5.0, "chained placer should separate labels, dist={}", dist);
    }

    #[test]
    fn axis_aligned_placer_delegates_to_resolve_label_overlaps() {
        let mut edges = vec![EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(0.0, 0.0),
                end: Point::new(100.0, 0.0),
            },
            labels: vec![EdgeLabelLayout::new("发布订单事件", Point::new(200.0, 430.0))],
            from_port: Port::Bottom,
            to_port: Port::Top,
        }];
        let nodes = HashMap::new();
        let mut groups = HashMap::new();
        groups.insert(
            "backend".to_string(),
            GroupLayout {
                x: 12.0,
                y: 174.0,
                width: 368.0,
                height: 256.0,
                ..Default::default()
            },
        );
        let ctx = LabelContext::new(&nodes, &groups);

        AxisAlignedPlacer::default().place(&mut edges, &ctx);

        let bbox = label_bbox(&edges[0], "");
        let group_bbox = (12.0, 174.0, 380.0, 430.0);
        assert!(
            aabb_overlap(&bbox, &group_bbox).is_none(),
            "axis-aligned placer should avoid group border: bbox={:?} group={:?}",
            bbox,
            group_bbox
        );
    }
}
