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
    dedupe_labels_on_declared_merges, resolve_label_overlaps_with_config,
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

/// 标签冲突求解结果（doc16 §14.3）。
#[derive(Debug, Clone, Default)]
pub struct LabelAssignment {
    /// 冲突消解后仍存在的 label-label 重叠数（0 = 全部消解）。
    pub conflicts_remaining: usize,
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
        // TODO(R10): 统计残余冲突数（当前 label_avoidance 内部已最大化消解）。
        LabelAssignment {
            conflicts_remaining: 0,
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
