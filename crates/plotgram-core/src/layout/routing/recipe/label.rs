//! 统一标签求解器（doc16 §4.6 / §14 / R3 Slice 3.3 / R9 Slice 9a）。
//!
//! 几何冻结后由 [`LabelSolver`] 统一放置边标签并消解冲突：
//!
//! ```text
//! place (plan-based)  →  solve (conflict resolution)  →  assignment
//! ```
//!
//! - [`LabelSolver::place`]：按 [`EdgeLabelPlan`] 在冻结几何上放置初始标签位置。
//! - [`LabelSolver::solve`]：统一冲突消解入口（候选打分 + 迭代推开 + leader line +
//!   merge dedupe hard contract）。所有 family 在 geometry freeze 后经此入口。
//!
//! ## R9 Slice 9a 扩展
//!
//! 将 `label_candidate.rs`（候选枚举+打分）+ `label_avoidance.rs`（迭代推开）整合进
//! LabelSolver 框架。[`LabelProblem`] 承载问题输入，[`LabelAssignment`] 承载解输出。
//! 算法沿用既有实现（同顺序、同逻辑），结构重组为 LabelProblem → solve → assignment。
//!
//! ## byte-identical 保证（R3 原始契约）
//!
//! 标签放置对渲染字节敏感，迁移必须严格等价：
//! - `middle_t` / `offset` 由各 Recipe 的 solve 计算（与原 router 同源，见
//!   [`EdgeLabelPlan`]），放进逐边计划。
//! - 路径取点函数由**冻结后的** [`PathGeometry`] 变体重建，与原 router「先算 geometry
//!   再取 path_pts」完全一致：
//!   - `Straight` / `Polyline` → [`point_at_path_t`]（按弧长）
//!   - `Bezier` → [`cubic_bezier_point`]
//! - 复用既有 [`build_edge_labels`]（label / head_label / tail_label + 切线角度），
//!   不改任何标签生成逻辑。

use crate::ast::Diagram;
use crate::layout::geometry::Point;
use crate::layout::routing::common::edge_geometry::{
    build_edge_labels, cubic_bezier_point, point_at_path_t,
};
use crate::layout::routing::common::label_avoidance::{
    aabb_overlap, dedupe_labels_on_declared_merges, resolve_label_overlaps_with_config,
};
use crate::layout::routing::common::label_candidate::LabelPlacementConfig;
use crate::layout::routing::model::FrozenRouteGeometry;
use crate::layout::routing::RouteAnnotationSet;
use crate::layout::types::{EdgeLabelLayout, EdgeLayout, GroupLayout, NodeLayout, PathGeometry};
use std::collections::HashMap;

/// 一条边的标签放置计划（family-neutral，owned）。
///
/// 由各 Recipe 的 solve 产出，与声明序对齐。`None`（在 [`super::RecipeSolution`] 的
/// `label_plans` 中）表示该边被抑制（空边），不产出标签。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeLabelPlan {
    /// 中段标签在路径上的参数位置（通常 `parse_label_t(rel)`，特殊布局可自定义）。
    pub middle_t: f64,
    /// 标签相对路径点的偏移（法线方向等）。
    pub offset: Point,
    /// 显式采样折线：若为 `Some`，标签沿该折线按弧长（[`point_at_path_t`]）放置，
    /// **忽略**几何变体重建。用于 spline 家族——其渲染几何为 `Bezier`，但标签历史上
    /// 沿采样折线放置（`sample_bezier`），二者取点不同，须显式携带以保字节一致。
    pub sample_path: Option<Vec<Point>>,
    /// 固定锚点：若为 `Some`，标签中心 = anchor + offset（切线角 0），**忽略**几何
    /// 取点与 `sample_path`。用于自环边（Slice D1）——其标签历史上锚定环 apex
    /// 而非沿路径 t 取点，须显式携带以保字节一致。
    pub anchor: Option<Point>,
}

/// 标签冲突求解问题（doc16 §14.2 / R9 Slice 9a）。
///
/// 承载 geometry freeze 后的标签冲突消解所需全部输入：已放置标签的边集合 +
/// 节点/分组障碍 + 图种策略 + 可选 merge annotation（hard dedupe contract）。
pub struct LabelProblem<'a> {
    /// 已组装好标签的边集合（标签位置由 [`LabelSolver::place`] 或 router 初版放置）。
    pub edges: &'a mut [EdgeLayout],
    /// 节点障碍（label 不得覆盖节点 bbox）。
    pub nodes: &'a HashMap<String, NodeLayout>,
    /// 分组障碍（label 不得越 group shell）。
    pub groups: &'a HashMap<String, GroupLayout>,
    /// 图种相关的候选打分策略。
    pub config: LabelPlacementConfig,
    /// merge annotation（若存在，dedupe 为 hard contract：同 trunk 同文案只保留一份）。
    pub merge_annotations: Option<&'a RouteAnnotationSet>,
}

/// 标签冲突求解结果（doc16 §14.3 / Slice F1.4）。
#[derive(Debug, Clone, Default)]
pub struct LabelAssignment {
    /// 冲突消解后仍存在的 label-label 重叠对数（0 = 全部消解），
    /// 按固定 (edge_idx, label_idx) 序真实统计。
    pub conflicts_remaining: usize,
    /// 显式退化清单：(edge_idx, reason)。残余 label-label 重叠 =
    /// `"label_overlap_residual"`；标签仍压节点（无可行候选）=
    /// `"label_node_overlap"`。按 edge_idx 升序、同边按 reason 先后序。
    pub degraded: Vec<(usize, &'static str)>,
    /// 最终标签状态的确定性指纹：中心量化 0.01px + 文案的 FNV-1a hash。
    /// 同输入两次 solve 必须一致（单测钉死）。
    pub signature: u64,
}

/// 冻结几何后的统一标签放置器 + 冲突求解器。
pub struct LabelSolver;

impl LabelSolver {
    /// 统一标签冲突消解入口（doc16 §14 / R9 Slice 9a）。
    ///
    /// 在 geometry freeze 后、final audit 前调用。内部按固定顺序执行：
    /// 1. 候选位打分选优（Phase 1：消除 label-node 硬冲突）
    /// 2. 迭代推开（Phase 2：label-label / label-obstacle 冲突）
    /// 3. Leader line 归属引导（Phase 3）
    /// 4. Merge dedupe hard contract（同 trunk 同文案只保留一份）
    ///
    /// 算法沿用 `label_avoidance.rs` 既有实现（同顺序、同逻辑），
    /// 本入口为所有 family 的统一调用点。
    pub fn solve(problem: LabelProblem<'_>) -> LabelAssignment {
        // Phase 1-3：候选打分 + 迭代推开 + leader line（既有算法，同顺序）。
        resolve_label_overlaps_with_config(
            problem.edges,
            problem.nodes,
            problem.groups,
            problem.config,
        );
        // Phase 4：merge dedupe hard contract（§14.2 硬约束）。
        if let Some(annotations) = problem.merge_annotations {
            dedupe_labels_on_declared_merges(problem.edges, annotations);
        }
        // Slice F1.4：按固定 (edge_idx, label_idx) 序真实统计残余冲突 +
        // 显式退化清单 + 确定性 signature。
        let (conflicts_remaining, degraded) =
            audit_residual_conflicts(problem.edges, problem.nodes);
        let signature = assignment_signature(problem.edges);
        LabelAssignment {
            conflicts_remaining,
            degraded,
            signature,
        }
    }

    /// 按声明序为每条边放置标签。
    ///
    /// - `plans[i] == None` → 该边无标签（空边）。
    /// - `plans[i] == Some(plan)` → 用 `diagram.relations[i]` 的 label/head/tail
    ///   在冻结几何上放置，取点函数由几何变体重建。
    ///
    /// 返回值按声明序对齐，`out[i]` 是第 i 条边的标签列表。
    pub fn place(
        diagram: &Diagram,
        plans: &[Option<EdgeLabelPlan>],
        frozen: &FrozenRouteGeometry,
    ) -> Vec<Vec<EdgeLabelLayout>> {
        let entries = frozen.entries();
        let mut out: Vec<Vec<EdgeLabelLayout>> = Vec::with_capacity(plans.len());
        for (i, plan) in plans.iter().enumerate() {
            let labels = match (plan, diagram.relations.get(i), entries.get(i)) {
                (Some(plan), Some(rel), Some((_, geometry))) => {
                    place_on_geometry(rel, plan, geometry)
                }
                _ => Vec::new(),
            };
            out.push(labels);
        }
        out
    }
}

/// 在单条边的冻结几何上放置标签，取点函数由几何变体决定。
fn place_on_geometry(
    rel: &crate::ast::Relation,
    plan: &EdgeLabelPlan,
    geometry: &PathGeometry,
) -> Vec<EdgeLabelLayout> {
    // 固定锚点（自环）：取点函数恒返回 anchor（center=anchor+offset，rotation=0），
    // 与遗留 `route_self_loop` 的 `|_| apex` 字节一致。
    if let Some(anchor) = plan.anchor {
        return build_edge_labels(rel, plan.middle_t, plan.offset, |_| anchor);
    }
    // 计划显式给定采样折线时，沿其按弧长放置（spline：几何 Bezier，标签走采样折线）。
    if let Some(sample) = &plan.sample_path {
        return build_edge_labels(rel, plan.middle_t, plan.offset, |t| point_at_path_t(sample, t));
    }
    match geometry {
        PathGeometry::Straight { start, end } => {
            let pts = [*start, *end];
            build_edge_labels(rel, plan.middle_t, plan.offset, |t| point_at_path_t(&pts, t))
        }
        PathGeometry::Polyline { points } => {
            build_edge_labels(rel, plan.middle_t, plan.offset, |t| {
                point_at_path_t(points, t)
            })
        }
        PathGeometry::Bezier {
            start,
            end,
            controls,
        } => build_edge_labels(rel, plan.middle_t, plan.offset, |t| {
            cubic_bezier_point(*start, controls[0], controls[1], *end, t)
        }),
    }
}

/// Slice F1.4：solve 后按固定 (edge_idx, label_idx) 序真实统计残余冲突。
///
/// 返回 (残余 label-label 重叠对数, 退化清单)。退化清单按 edge_idx 升序，
/// 同一边同一 reason 只记一次；标签仍压节点 bbox 视为无可行候选。
fn audit_residual_conflicts(
    edges: &[EdgeLayout],
    nodes: &HashMap<String, NodeLayout>,
) -> (usize, Vec<(usize, &'static str)>) {
    // 固定序收集所有非空标签 bbox：(edge_idx, bbox)。
    let mut label_boxes: Vec<(usize, (f64, f64, f64, f64))> = Vec::new();
    for (edge_idx, edge) in edges.iter().enumerate() {
        for label in &edge.labels {
            if label.text.is_empty() || label.size.0 <= 0.0 || label.size.1 <= 0.0 {
                continue;
            }
            let (w, h) = label.size;
            label_boxes.push((
                edge_idx,
                (
                    label.center.x - w * 0.5,
                    label.center.y - h * 0.5,
                    label.center.x + w * 0.5,
                    label.center.y + h * 0.5,
                ),
            ));
        }
    }

    // 节点障碍按 id 排序（确定性，不依赖 HashMap 迭代序）。
    let mut node_ids: Vec<&String> = nodes.keys().collect();
    node_ids.sort();
    let node_boxes: Vec<(f64, f64, f64, f64)> = node_ids
        .iter()
        .map(|id| {
            let nl = &nodes[*id];
            (nl.x, nl.y, nl.x + nl.width, nl.y + nl.height)
        })
        .collect();

    let mut conflicts = 0usize;
    let mut degraded: Vec<(usize, &'static str)> = Vec::new();
    for i in 0..label_boxes.len() {
        for j in (i + 1)..label_boxes.len() {
            if aabb_overlap(&label_boxes[i].1, &label_boxes[j].1).is_some() {
                conflicts += 1;
                for &(edge_idx, _) in [&label_boxes[i], &label_boxes[j]] {
                    if !degraded.contains(&(edge_idx, "label_overlap_residual")) {
                        degraded.push((edge_idx, "label_overlap_residual"));
                    }
                }
            }
        }
        for nb in &node_boxes {
            if aabb_overlap(&label_boxes[i].1, nb).is_some() {
                let entry = (label_boxes[i].0, "label_node_overlap");
                if !degraded.contains(&entry) {
                    degraded.push(entry);
                }
                break;
            }
        }
    }
    degraded.sort();
    (conflicts, degraded)
}

/// Slice F1.4：最终标签状态的确定性指纹。
///
/// 按固定 (edge_idx, label_idx) 序逐字段混入：文案字节 + 中心坐标量化 0.01px。
/// FNV-1a 64（确定性算法，无随机种子，WASM 兼容）。
fn assignment_signature(edges: &[EdgeLayout]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    };
    for (edge_idx, edge) in edges.iter().enumerate() {
        for (label_idx, label) in edge.labels.iter().enumerate() {
            eat(&(edge_idx as u64).to_le_bytes());
            eat(&(label_idx as u64).to_le_bytes());
            eat(label.text.as_bytes());
            // 量化 0.01px，避免浮点尾差影响指纹。
            eat(&(((label.center.x * 100.0).round()) as i64).to_le_bytes());
            eat(&(((label.center.y * 100.0).round()) as i64).to_le_bytes());
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge_with_label(text: &str, center: Point) -> EdgeLayout {
        EdgeLayout {
            geometry: PathGeometry::Straight {
                start: Point::new(center.x - 50.0, center.y + 20.0),
                end: Point::new(center.x + 50.0, center.y + 20.0),
            },
            labels: vec![EdgeLabelLayout::new(text, center)],
            from_port: crate::layout::types::Port::Bottom,
            to_port: crate::layout::types::Port::Top,
        }
    }

    /// F1.4 退出判据：同输入两次 solve，signature / conflicts / degraded 必须一致。
    #[test]
    fn label_assignment_signature_stable_for_same_input() {
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let groups: HashMap<String, GroupLayout> = HashMap::new();
        let make_edges = || {
            vec![
                edge_with_label("alpha", Point::new(100.0, 50.0)),
                edge_with_label("beta", Point::new(300.0, 50.0)),
            ]
        };

        let mut edges_a = make_edges();
        let a = LabelSolver::solve(LabelProblem {
            edges: &mut edges_a,
            nodes: &nodes,
            groups: &groups,
            config: LabelPlacementConfig::default(),
            merge_annotations: None,
        });
        let mut edges_b = make_edges();
        let b = LabelSolver::solve(LabelProblem {
            edges: &mut edges_b,
            nodes: &nodes,
            groups: &groups,
            config: LabelPlacementConfig::default(),
            merge_annotations: None,
        });

        assert_eq!(a.signature, b.signature, "同输入两次 solve signature 必须一致");
        assert_eq!(a.conflicts_remaining, b.conflicts_remaining);
        assert_eq!(a.degraded, b.degraded);
        assert_ne!(a.signature, 0, "有标签时 signature 不应为初始值 0");
    }

    /// 残余重叠真实统计：人为构造两个重叠标签直接过 audit（不经推开）。
    #[test]
    fn residual_conflict_audit_counts_overlaps() {
        let nodes: HashMap<String, NodeLayout> = HashMap::new();
        let edges = vec![
            edge_with_label("one", Point::new(100.0, 50.0)),
            edge_with_label("two", Point::new(102.0, 51.0)),
        ];
        let (conflicts, degraded) = audit_residual_conflicts(&edges, &nodes);
        assert_eq!(conflicts, 1, "重叠标签应计为 1 对");
        assert_eq!(
            degraded,
            vec![(0, "label_overlap_residual"), (1, "label_overlap_residual")]
        );
    }

    /// 标签压节点（无可行候选）计入退化清单。
    #[test]
    fn node_overlap_marks_degraded() {
        let mut nodes: HashMap<String, NodeLayout> = HashMap::new();
        nodes.insert(
            "n".to_string(),
            NodeLayout {
                x: 80.0,
                y: 30.0,
                width: 60.0,
                height: 40.0,
                ..Default::default()
            },
        );
        let edges = vec![edge_with_label("stuck", Point::new(100.0, 50.0))];
        let (_, degraded) = audit_residual_conflicts(&edges, &nodes);
        assert!(degraded.contains(&(0, "label_node_overlap")));
    }
}
