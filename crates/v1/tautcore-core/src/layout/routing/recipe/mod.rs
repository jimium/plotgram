//! 路由 Recipe 与统一驱动适配器（doc16 §5.1 / R3 Slice 3）。
//!
//! doc16 目标管线把「一个边路由家族」表达为 [`RoutingRecipe`]：
//!
//! ```text
//! compile   : Diagram + 已冻结布局 → 家族专属 Draft（端点/端口/标签计划等已编译事实）
//! solve     : Draft → family-neutral RouteSolution（+ 逐边标签计划）
//! materialize: RouteSolution → 几何（由 GeometryMaterializer 唯一写者物化）
//! ```
//!
//! [`RecipeRouter`] 是泛型适配器，实现既有 [`RoutingRecipeDyn`]：内部串起
//! compile → solve → materialize → audit → freeze → LabelSolver::place（plan-based
//! 初始放置）→ finalize_edges。标签冲突消解统一由 Coordinator 在唯一 freeze
//! 之后执行（Slice F1，标签唯一最后写者）。
//! registry 只需把 `Box::new(StraightRouting)` 换成 `Box::new(RecipeRouter::new(StraightRecipe))`，
//! pipeline 与全部 post-route 阶段零改动 → 输出字节不变。
//!
//! ## Slice 3 范围与红线
//!
//! - **严格行为不变**：Recipe 只把现有 router 逻辑重构到 compile/solve 接口后面；
//!   几何物化走 Slice 2 的 [`GeometryMaterializer`]，标签走 [`label::LabelSolver`]，
//!   最终 SVG 与迁移前基线字节一致。
//! - §2 确定性：逐边集合按声明序（`StableEdgeId` 下标）显式排序，不依赖 HashMap 迭代序。
//! - compile 不读取 `DiagramType`（`DiagramType` 相关行为如 mindmap 清标签、ER 标签 t
//!   由 solve/finalize 处理）。

pub mod bezier;
pub mod circular;
pub mod label;
pub mod organic;
pub mod spline;
pub mod straight;

pub use bezier::BezierRecipe;
pub use circular::CircularRecipe;
pub use label::{EdgeLabelPlan, LabelAssignment, LabelProblem, LabelSolver};
pub use organic::OrganicRecipe;
pub use spline::SplineRecipe;
pub use straight::StraightRecipe;

use crate::ast::Diagram;
use crate::layout::algorithm_config::AlgorithmOptionSpec;
use crate::layout::routing::common::routing_skeleton::finalize_edges;
use crate::layout::routing::model::{
    EmptyRouteReason, GeometryMaterializer, RouteAuditor, RoutePath, RouteSolution,
};
use crate::layout::routing::model::prepared::PreparedRoutingInput;
use crate::layout::snap::grid_snap::EdgeSnapConfig;
use crate::layout::types::{EdgeLayout, LayoutResult, Port};
use crate::layout::{RoutingRecipeDyn, RoutingProduct};
use crate::types::DiagramType;

/// 一次 solve 的完整产物：family-neutral 解 + 逐边标签计划。
///
/// `label_plans` 与 `solution.paths` 按声明序对齐（下标 == `StableEdgeId`）；
/// `None` 表示该边被抑制（如端点缺失的空边），不产出标签。
///
/// Slice D1：历史上的逐边预制覆盖旁路（自环边绕开 materializer 直接携带完整
/// `EdgeLayout`）已删除：自环拓扑由 `solve_self_loop` 产 `RoutePath`，标签经
/// [`EdgeLabelPlan::anchor`] 在冻结几何上放置，所有边统一走 materialize→audit→freeze。
pub struct RecipeSolution {
    pub solution: RouteSolution,
    pub label_plans: Vec<Option<EdgeLabelPlan>>,
    /// Phase 6 / A6：正交内核统计（含 `degraded_count`）；非正交家族为 `None`。
    pub orthogonal_debug: Option<crate::layout::OrthoDebugStats>,
}

/// 一个边路由家族的编译语义（doc16 §5.1）。
///
/// Slice 3 的 Recipe 是**无状态**的纯映射；`compile`/`solve` 对 straight/bezier/spline
/// 等简单家族不会失败（端点缺失退化为空路径，而非错误），故此处不引入 `Result` 包装。
pub trait RoutingRecipe {
    /// 家族专属的已编译 Draft（借用 diagram / 已冻结布局）。
    type Draft<'a>;

    /// 家族名（与既有 router `name()` 一致，用于 registry / 诊断）。
    fn name(&self) -> &'static str;

    /// 适用的内置图类型（转发给 [`RoutingRecipeDyn::applicable_diagram_types`]）。
    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        &[]
    }

    /// 是否支持 Custom 图类型。
    fn supports_custom(&self) -> bool {
        false
    }


    /// 支持的 DSL 配置块 option 列表。
    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        &[]
    }

    /// 边 waypoint snap 配置。
    fn edge_snap_config(&self) -> EdgeSnapConfig {
        EdgeSnapConfig::disabled()
    }

    /// 编译：从 diagram 声明语义 + 已冻结布局构造家族专属 Draft。
    fn compile<'a>(&self, diagram: &'a Diagram, result: &'a LayoutResult) -> Self::Draft<'a>;

    /// Phase 0：带正式 [`RoutingConfig`](crate::layout::routing::config::RoutingConfig) 的编译。
    /// 默认忽略 config，等价于 [`compile`](Self::compile)；正交家族注入 `OrthoConfig.routing`。
    fn compile_with_config<'a>(
        &self,
        diagram: &'a Diagram,
        result: &'a LayoutResult,
        _routing_config: crate::layout::routing::config::RoutingConfig,
    ) -> Self::Draft<'a> {
        self.compile(diagram, result)
    }

    /// Phase 5 / D4-4：从 PreparedRoutingInput 编译（可携带富契约）。
    fn compile_from_prepared<'a>(
        &self,
        input: &PreparedRoutingInput<'a>,
        result: &'a LayoutResult,
    ) -> Self::Draft<'a> {
        self.compile_with_config(input.diagram, result, input.config)
    }

    /// 求解：Draft → family-neutral [`RecipeSolution`]。
    fn solve(&self, draft: &Self::Draft<'_>) -> RecipeSolution;

    /// Slice F2c：preserve 求解——`draft` 编译自 seeded edges（preserve 边已携带
    /// prev 冻结几何），仅重解 `preserve` 之外的边。默认 `None`（family 不支持
    /// 逐边 preserve，调用方回退全量 [`solve`](Self::solve)）。
    fn solve_preserving(
        &self,
        _draft: &Self::Draft<'_>,
        _preserve: &std::collections::HashSet<usize>,
    ) -> Option<RecipeSolution> {
        None
    }

    /// 短路判定：某些家族（如 circular）在无可路由结构时直接返回原 `result`（不改
    /// `edges`），跳过物化 / 求解 / 收尾。默认从不短路。
    fn should_skip(&self, _draft: &Self::Draft<'_>) -> bool {
        false
    }

    /// 收尾：把物化好的 `edges` 写回 `result`。默认走
    /// [`finalize_edges`]（mindmap 清标签 + `resolve_label_overlaps`）。
    fn finalize(
        &self,
        result: LayoutResult,
        edges: Vec<EdgeLayout>,
        diagram: &Diagram,
    ) -> LayoutResult {
        finalize_edges(result, edges, diagram)
    }

}

/// 泛型适配器：把 [`RoutingRecipe`] 包装成既有 [`RoutingRecipeDyn`]。
///
/// 承载 compile → solve → materialize → audit → freeze → LabelSolver::place → finalize_edges
/// 的统一驱动，使各家族的具体 Recipe 不再各自实现 `route()`。
pub struct RecipeRouter<R: RoutingRecipe> {
    recipe: R,
}

impl<R: RoutingRecipe> RecipeRouter<R> {
    pub fn new(recipe: R) -> Self {
        Self { recipe }
    }

    /// 从 input.frozen + input.hints 构造临时 LayoutResult（仅供内部算法消费，
    /// 不暴露给 trait 边界）。
    fn temp_result(&self, input: &PreparedRoutingInput<'_>) -> LayoutResult {
        LayoutResult {
            nodes: input.frozen.nodes().clone(),
            groups: input.frozen.groups().clone().into(),
            edges: Vec::new(),
            total_width: input.canvas.width,
            total_height: input.canvas.height,
            hints: input.hints.clone(),
        }
    }

    /// 共用尾部（Slice F2c 抽取，route / route_preserving 共享）：
    /// materialize → audit → freeze → LabelSolver::place → finalize → RoutingProduct。
    fn drive_solution(
        &self,
        mut temp_result: LayoutResult,
        solved: RecipeSolution,
        diagram: &Diagram,
    ) -> RoutingProduct {
        let RecipeSolution {
            mut solution,
            label_plans,
            orthogonal_debug,
        } = solved;

        // 几何唯一写者物化 → 只读审计 → 冻结（Slice D2：无审计旁路）。
        let materialized = GeometryMaterializer::materialize(&solution);
        let audited = match RouteAuditor::audit_and_advance(materialized) {
            Ok(audited) => audited,
            Err((report, _materialized)) => {
                // debug：直接暴露，禁止静默绕过 auditor（doc16 §4.6）。
                debug_assert!(
                    false,
                    "recipe {} 物化几何审计失败: {report:?}",
                    self.recipe.name()
                );
                // release：降级记账——违规边清为声明性空边后重物化，仍必经 auditor。
                for v in &report.violations {
                    if let Some(path) = solution.paths.get_mut(v.edge.index()) {
                        *path = RoutePath::Empty(EmptyRouteReason::Suppressed);
                    }
                    solution.diagnostics.notes.push(format!(
                        "audit degraded: edge {} suppressed ({:?})",
                        v.edge.index(),
                        v.kind
                    ));
                }
                RouteAuditor::audit_and_advance(GeometryMaterializer::materialize(&solution))
                    .expect("违规边降级为声明性空边后审计必通过")
            }
        };
        let frozen = audited.freeze();

        // 标签：在冻结几何之后按声明序放置。
        let placed = LabelSolver::place(diagram, &label_plans, &frozen);

        // 组装 EdgeLayout：几何来自冻结几何，端口来自 solution.ports。
        let entries = frozen.entries();
        let mut edges: Vec<EdgeLayout> = Vec::with_capacity(entries.len());
        for (i, (_id, geometry)) in entries.iter().enumerate() {
            let (from_port, to_port) = solution
                .ports
                .get(i)
                .map(|e| (e.from_port, e.to_port))
                .unwrap_or((Port::Bottom, Port::Top));
            edges.push(EdgeLayout {
                geometry: geometry.clone(),
                labels: placed.get(i).cloned().unwrap_or_default(),
                from_port,
                to_port,
            });
        }

        // 标签冲突消解由 Coordinator 在唯一 freeze 之后统一执行（Slice F1）。

        // finalize：把物化好的 edges 写回 temp_result。
        temp_result = self.recipe.finalize(temp_result, edges, diagram);

        // 提取 RoutingProduct（edges + hints delta）。
        // Phase 6：优先消费 solve 带回的 ortho stats（勿再读空的 temp_result.hints）。
        let orthogonal_debug = orthogonal_debug.or(temp_result.hints.orthogonal_debug);
        RoutingProduct {
            edges: temp_result.edges,
            group_routing: temp_result.hints.group_routing,
            route_annotations: temp_result.hints.route_annotations,
            orthogonal_debug,
        }
    }
}

impl<R: RoutingRecipe> RoutingRecipeDyn for RecipeRouter<R> {
    fn name(&self) -> &'static str {
        self.recipe.name()
    }

    fn applicable_diagram_types(&self) -> &'static [DiagramType] {
        self.recipe.applicable_diagram_types()
    }

    fn supports_custom(&self) -> bool {
        self.recipe.supports_custom()
    }

    fn option_specs(&self) -> &'static [AlgorithmOptionSpec] {
        self.recipe.option_specs()
    }

    fn edge_snap_config(&self) -> EdgeSnapConfig {
        self.recipe.edge_snap_config()
    }

    fn route(&self, input: &PreparedRoutingInput<'_>) -> RoutingProduct {
        let mut temp_result = self.temp_result(input);

        // compile + solve 借用 temp_result；在移动 result 进 finalize 前必须结束借用。
        let solved = {
            let draft = self.recipe.compile_from_prepared(input, &temp_result);
            if self.recipe.should_skip(&draft) {
                None
            } else {
                Some(self.recipe.solve(&draft))
            }
        };
        match solved {
            Some(s) => self.drive_solution(temp_result, s, input.diagram),
            None => RoutingProduct {
                edges: Vec::new(),
                group_routing: temp_result.hints.group_routing.take(),
                route_annotations: None,
                orthogonal_debug: None,
            },
        }
    }

    /// Slice F2c：增量路由——seeded edges 携带 preserve 边的 prev 冻结几何，
    /// 经 `solve_preserving` 仅重解 dirty 边，尾部与 `route` 共用
    /// materialize → audit → freeze → place → finalize。
    fn route_preserving(
        &self,
        input: &PreparedRoutingInput<'_>,
        seeded_edges: Vec<EdgeLayout>,
        preserve: &std::collections::HashSet<usize>,
    ) -> Option<RoutingProduct> {
        let mut temp_result = self.temp_result(input);
        temp_result.edges = seeded_edges;

        let solved = {
            let draft = self.recipe.compile_from_prepared(input, &temp_result);
            if self.recipe.should_skip(&draft) {
                return None;
            }
            self.recipe.solve_preserving(&draft, preserve)?
        };
        // seeded edges 已被 solve 消费（经 draft.result 克隆），drive 会用冻结几何重建 edges。
        temp_result.edges = Vec::new();
        Some(self.drive_solution(temp_result, solved, input.diagram))
    }
}
