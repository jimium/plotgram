# 20 - 路由 Recipe / Kernel 实施审查与后续收敛计划

> 日期：2026-07-25  
> 审查对象：[`16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md`](./16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md)  
> 代码基线：当前工作区 `developv2-bigchange`（含未提交改动）  
> 审查口径：以实际调用链和最终写权为准，不以文件名、注释中的 Slice 状态或类型是否存在作为完成证明  
> 结论状态：**架构骨架与若干局部求解器已落地，但原方案的终态目标尚未达成**

---

## 0. 执行结论

本轮实施不是“只有空壳”：六个 family 都已有显式 Recipe，简单 family 已走 `RouteSolution → GeometryMaterializer → audit → freeze → LabelSolver`，正交侧也落地了端口 side 求解、统一资源视图、单一冲突重路由主流程、lane/bundle 的 solve/materialize 分离、统一标签入口与 Coordinator 入口。

但当前仍是**新模型包裹旧可变产品管线**，还不是方案要求的“稳定输入 + 窄 IR + 唯一物化 + 固定轮次修复 + 真冻结”：

1. 正式动态接口仍是 `route(&Diagram, LayoutResult) -> LayoutResult`，`FrozenNodeProduct` 只是前后指纹断言，不是 router 的类型级输入。
2. `PreparedRoutingInput` 在 runner 中只计算 signature；Recipe/solver 并未真正消费它，富 `RoutingContract` 的多数 intent 仍为空。
3. 正交 Recipe 的 `compile` 只是克隆整个 `LayoutResult`，`solve` 再调用旧 `route_orthogonal_inner`，随后把已经生成的几何 `lift` 回 `RouteSolution`。
4. `GeometryMaterializer` 只在 Recipe 局部生命周期内是唯一写者；全局仍有大量 `set_polyline_points`、直接 `geometry =` 和 `polyline_points_mut` 写点。
5. 正交 D 段虽从 runner 搬入 finalizer，但仍在 snap 之后原地执行 sanitize、端口/车道分离、穿节点/穿组修复、trunk 修复和 crossing reduction；repair intent 目前只是伴随记账。
6. Coordinator 没有消费 audit report，也没有固定轮次 repair/re-solve；`max_repair_rounds` 只是保留字段。
7. 正式 route 后仍可能由 route feedback、group frame、PRS 等路径移动节点并再次 `router.route`，因此“最终路由只发生在 node freeze 后”尚不成立。
8. 正式算法仍受多个环境变量控制，`RoutingPlan` 不能完整复现一次运行。
9. 增量路由仍以一次运行内的 relation 下标为 identity，没有 previous solution、资源依赖图和 dirty component 扩张。

因此，当前更准确的里程碑定义是：

> **完成了 R0–R9 的结构搭桥和部分算法收敛；R10/R11 以及 R1/R2/R7/R8 的终态写权切换尚未完成。**

---

## 1. 审查方法与验证证据

本次审查同时使用以下证据：

- 从 `LayoutPipeline::run_routing_pipeline` 追踪真实时序：
  `pre-route → coordinator → route feedback → repulse/group frame/PRS/budget → snap → orthogonal finalizer → audit → canvas transform`。
- 搜索所有正式接口、几何写点、节点写点、环境变量与稳定 identity。
- 对照方案 §20 的 R0–R11 和 §25 的 15 条最终完成定义。
- 读取最新基准：
  `benchmarks/baselines/2026-07-25-011309-r12c.json`。
- 执行当前代码验证：
  `cargo test -p plotgram-core --lib`，结果 **972 passed，0 failed**。

补充说明：

- 最新基准的 38/38 样例均 `det=true`，确定性红线当前有证据支持。
- 最新基准仍有硬正确性残余：
  - `showcase/architecture/product.cdn-cache.pgm`：`edge_crosses_group_interior = 1`。
  - `showcase/er/product.saas-schema.pgm`：`edge_crosses_group_interior = 1`。
  - `showcase/architecture/stress.layout-stress-flat-mesh.pgm`：`edge_through_node = 1`。
- `git diff --check` 发现当前工作区已有 1 个 EOF 空行和 architecture 文档 4 处 trailing whitespace；它们不是本架构审查的功能结论，但说明提交前还需做一次卫生检查。

---

## 2. 已经实质达成的部分

### 2.1 六个 family 都有显式 Recipe

注册表已统一构造：

- `StraightRecipe`
- `BezierRecipe`
- `SplineRecipe`
- `CircularRecipe`
- `OrthogonalRecipe`
- `OrganicRecipe`

证据：

- `crates/plotgram-core/src/layout/pipeline/registry.rs:53-79`
- `crates/plotgram-core/src/layout/routing/recipe/mod.rs:25-39`

这满足了“每个 geometry family 有显式 Recipe”的结构目标，也删除了 family 注册层的多套动态接口。

### 2.2 简单 family 已经产出结构化 RouteSolution

straight / bezier / spline / circular / organic 的 Recipe 会产出：

- `RoutePath`
- `EndpointAssignment`
- `DegradedReason`
- `EdgeLabelPlan`

Bezier、Circular、Organic 的 family fallback 已在主要分支中记录 typed `DegradedReason`，不再完全静默地把曲线变成折线。

证据：

- `routing/recipe/bezier.rs:240-297`
- `routing/recipe/circular.rs:319-354`
- `routing/recipe/organic.rs:349-433`
- `routing/model/solution.rs:18-205`

### 2.3 geometry typestate 与统一标签入口已经建立

局部 Recipe 生命周期已经具备：

```text
RouteSolution
  → GeometryMaterializer
  → RouteAuditor
  → FrozenRouteGeometry
  → LabelSolver
```

证据：

- `routing/recipe/mod.rs:164-240`
- `routing/model/materialize.rs:113-197`
- `routing/model/audit.rs:71-153`
- `routing/recipe/label.rs:85-141`

这套类型可作为后续全局收口的基础，不需要推倒重写。

### 2.4 正交求解内部已有确定性收敛

已经落地的有效能力包括：

- `OrthogonalDraft` 集中准备障碍、端口、边序、走廊、OVG 和 channel facts。
- PortAssignmentSolver 统一 pre-route port side 决策。
- `ResourceGraph` 聚合 obstacles / OVG / channel / corridor。
- `PathAssignmentSolver` 以固定 3 轮执行冲突检测与 rip-up/reroute。
- lane 与 semantic bundle 已拆成 solve/materialize 两段。
- 冲突集合使用 `BTreeMap` 或显式排序，局部改进使用固定轮次。

证据：

- `routing/edge_routing_orthogonal/draft.rs:29-235`
- `routing/edge_routing_orthogonal/port_solver.rs:66-145`
- `routing/edge_routing_orthogonal/resource_graph.rs:22-144`
- `routing/edge_routing_orthogonal/path_solver.rs:153-338`
- `routing/edge_routing_orthogonal/lane_assignment.rs`
- `routing/edge_routing_orthogonal/semantic_trunk_merge.rs`

### 2.5 一部分旧层已删除

已确认：

- `route_after_node_moves` 不再是 trait 能力。
- `route_preserve` 不再是 trait 能力。
- `supports_refine` / `needs_obstacle_index` 调度布尔已从动态 trait 删除。
- post-route hook 文件已删除，PRS 改为显式调用。
- `stub_fix.rs` 已删除，反向端口选择前移到 PortSolver。
- route 后 SpaceBudget guard 的实际推点逻辑已改为空操作。

这些删除符合项目“不保留兼容转发层”的原则。

### 2.6 原子 canvas transform 基本达成

最终 canvas transform 会同步平移：

- nodes
- groups
- edge geometry
- labels / leader
- route annotations
- circular / sequence hints

证据：

- `crates/plotgram-core/src/layout/snap/canvas_finalize.rs:134-175`

---

## 3. 尚未达成或只部分达成的关键目标

### 3.1 FrozenNodeProduct 目前是运行时守卫，不是正式输入

方案要求 router 只消费 `FrozenNodeProduct + RoutingContract`。

当前实际接口仍是：

```rust
fn route(&self, diagram: &Diagram, result: LayoutResult) -> LayoutResult;
```

`route_with_frozen` 的默认实现只是调用旧 `route` 后比较节点 fingerprint。

证据：

- `crates/plotgram-core/src/layout/traits.rs:85-107`
- `crates/plotgram-core/src/layout/routing/coordinator.rs:95-132`

影响：

- Router 在类型上仍持有整个可变 `LayoutResult`。
- 节点/组写权仍靠 assert 发现，而不是 API 不允许。
- `Diagram` 解析和业务判断仍可散入任意 Recipe/solver。

结论：**未达成。**

### 3.2 PreparedRoutingInput 与富 RoutingContract 尚未成为主数据流

runner 会构造 `PreparedRoutingInput`，但结果只用于 signature 和 perf log，随后 Coordinator 仍把原始 `Diagram + LayoutResult` 交给 router。

证据：

- `pipeline/runner.rs:140-169`
- `routing/model/prepared.rs:133-152`

富 `RoutingContract` 目前只实际编译：

- SelfLoop
- ParallelGroup
- Forward

以下字段仍为空或占位：

- feedback / same-layer / cross-scope / monitor / business / pendant roles
- port intents
- transit intents
- corridors
- side gutters
- merge intents
- circle membership

证据：

- `routing/model/contract.rs:7-17`
- `routing/model/contract.rs:207-220`

结论：**R1 的类型和 signature 已落地，但“router 消费稳定输入与完整 contract”未达成。**

### 3.3 OrthogonalRecipe 仍是旧内核包装器

当前 `OrthogonalRecipe::compile`：

- 借用 `Diagram`
- 克隆整个 `LayoutResult`

当前 `solve`：

- 调用 `route_orthogonal_inner`
- 得到已经物化好的 `EdgeLayout`
- 再用 `GeometryMaterializer::lift_geometry` 逆向构造 `RouteSolution`

证据：

- `routing/recipe/orthogonal.rs:46-90`
- `routing/recipe/orthogonal.rs:92-124`

这意味着 Orthogonal Recipe 尚未实现：

```text
PreparedRoutingInput
  → OrthogonalDraft / RoutingProblem
  → topology solution
  → GeometryMaterializer
```

而是：

```text
mutable LayoutResult
  → legacy orthogonal pipeline
  → final EdgeLayout
  → lift back to RouteSolution
  → materialize again
```

结论：**显式 Recipe 已达成，Recipe/Kernel/Solver 的真实控制反转未达成。**

### 3.4 GeometryMaterializer 不是全局唯一几何写者

当前 typestate 只能约束 `MaterializedRouteGeometry` 自身，不能约束 `LayoutResult.edges`。

正式路径仍存在多类直接写者：

- `phase_route_edges`
- slot replan / port correction
- PathAssignmentSolver
- lane materialize
- semantic bundle materialize
- S4 escape / trunk reroute
- sanitize
- snap/repulse
- exact stub occupancy
- through/group/trunk repair
- crossing reduction

代表性证据：

- `routing/edge_routing_orthogonal/phases/build.rs:216-222`
- `routing/edge_routing_orthogonal/path_solver.rs:291-313`
- `routing/edge_routing_orthogonal/run.rs:327-401`
- `routing/edge_routing_orthogonal/finalize.rs:86-217`
- `snap/grid_snap.rs:425`

此外，自环通过 `RecipeSolution.overrides: Vec<Option<EdgeLayout>>` 绕开 materializer。

证据：

- `routing/recipe/mod.rs:52-68`
- `routing/recipe/mod.rs:208-227`

typestate 还有一个实际旁路：RecipeRouter 遇到全部为 `TooFewPoints` 的 audit failure 时，
会直接调用 `AuditedRouteGeometry::from_audited(materialized)` 推进状态，而不是让 Auditor
根据 `EmptyRouteReason` 判定 suppressed / unresolved 是否合法。

证据：

- `routing/recipe/mod.rs:184-203`
- `routing/model/materialize.rs:150-171`

结论：**局部 typestate 已建立，全局唯一写权未达成。**

#### 【Slice D+E 后事实更新（2026-07，E6）】剩余直接写者清单

仅记录事实，不改 Slice 定义。Slice D+E 后：

- 已删除的写者/入口：`RecipeSolution.overrides`（D1）、RecipeRouter `from_audited`
  旁路（D2）、runner 独立 snap（D4）、`OrthogonalFinalizer`/`NoopFinalizer`/
  `route_finalizer_for_style`/整个 D 段原地 repair（E4/E5）、`PolylineFreeze`（E5）、
  runner R8 shadow audit（E5）、`recheck_lint_pierce_post_freeze`（E5，由
  `RouteAuditor::audit_extended` + E3 loop 取代）。
- `EdgeLayout::set_polyline_points` / `polyline_points_mut` 已降为 `pub(crate)`（E5）。
- 剩余生产直接写者（全部位于 Coordinator 唯一真冻结点之前，逐调用点已注释归属）：
  - C 段 solver 产出→EdgeLayout 桥：`run.rs`（自环边 + S4 落盘）、
    `lane_assignment.rs`（lane 分槽 / reverse-pair gap / dock 分离）、
    `semantic_trunk_merge.rs`（语义 trunk 合并）、`phases/*`（build/refine/trunk）、
    `path_solver.rs`（rip-up/reroute）。
  - Coordinator solver finalize 步（freeze 前）：`snap/grid_snap.rs`、
    `stub_occupancy.rs`（exact 共柱，E4 收编）。
  - materializer 内核边界：`sanitize.rs`（canonicalize，D3 收编）。
  - 非正交/辅助：`common/self_loop.rs`、`group/post_route.rs`（组边框推离，
    freeze 前）、`refine/spline_fallback.rs`（E3 loop local re-solve）。
  - 测试内构造：`quality/metrics/collinear.rs` 及各 solver `mod tests`。
- freeze 后无任何写者：label solve 统一在 Coordinator freeze 之后执行，只写
  label/annotation；残余 obstacle-model 违规由 E3 repair loop 显式 degraded 报告。

#### 【Slice F 后事实更新（2026-07，F2c）】label 唯一写者与增量入口

仅记录事实，不改 Slice 定义。Slice F1+F2 后：

- **label 唯一写者**：label solve 全 family 统一在 Coordinator 唯一 freeze 之后
  执行（`routing/coordinator.rs`），Recipe 内提前 `LabelSolver::solve` 已删除
  （`recipe/mod.rs` 仅保留 plan-based `LabelSolver::place` 初始放置）；
  circular 的 `RadialPlacer` 第二写者已删除——径向候选并入
  `common/label_candidate.rs`（`LabelPlacementConfig.radial_center`），
  `common/label_placement.rs` 整个模块与 legacy `route_edges_circular` 已删除。
- **LabelAssignment 真实字段**：`conflicts_remaining`（固定序真实统计残余
  label-label 重叠）、`degraded: Vec<(edge_idx, reason)>`、`signature: u64`
  （量化 0.01px 中心 + 文案的确定性 FNV hash，同输入稳定，单测钉死）。
- **已删旧入口/旧写者（crates 内 grep=0）**：`RadialPlacer` / `LabelPlacer` /
  `label_placement`；`edge_routing_orthogonal` 的旧节点位移增量入口
  （reroute_edges_touching_nodes / reroute_edges_preserve）；`space_budget_guard`
  的空转增量路径（diff_moved_nodes / reroute_and_repulse，`resolve_budget_violations`
  仅保留 budget hint 设置）；`post_route::NODE_MOVE_REROUTE_EPS`。
- **增量依赖记录**：`StableEdge` 增 `identity`（from/to/parallel_ordinal），
  `StableEdgeStore::match_identities` 产出确定性 `EdgeIdentityDiff`；
  `routing/model/frozen_solution.rs` 的 `FrozenRoutingSolution::capture` 在
  freeze + label solve 后构建（逐边依赖记录：geometry/ports/端点/路径段/
  邻近障碍/group gates/bundle/conflict partners/label bbox/声明文案 +
  节点组几何指纹 + route_annotations），经 `LayoutResult.hints.frozen_routing`
  （Arc）暴露，无全局会话态；`dirty_set` 由指纹变化/边增删出发沿
  conflicts+bundle 做固定序连通分量闭包；E3 repair loop stalled 轮次按
  依赖图做 1 跳固定序扩张。
- **跨渲染增量入口**：`compute_layout_incremental(diagram, prev)`
  （`pipeline/entry.rs`）→ coordinator `execute(..., prev)`：identity match +
  `dirty_set` → preserved 边复用前逐边重过 `RouteAuditor::audit_extended`
  hard check（fail → 固定序扩张 ≤2 跳，仍 fail 回退全图）；zero-diff
  快路径原样写回冻结 edges/annotations（集成测试钉死字节一致）；
  orthogonal 走 `route_preserving` 逐边 preserve，非正交 family 全量重解；
  preserve 比例 < `MIN_PRESERVE_RATIO` 回退全图路由。

### 3.5 geometry freeze 不是全管线的真实冻结点

RecipeRouter 在初始 route 内已经 freeze 并运行 LabelSolver，但随后 pipeline 还会：

- route feedback 移动节点并直接重新 route
- repulse edge geometry
- group frame / PRS 后重新 route
- snap/repulse
- orthogonal finalizer 原地修改 geometry
- 再运行一次正交 LabelSolver

证据：

- `pipeline/runner.rs:175-205`
- `pipeline/runner.rs:207-323`
- `pipeline/runner.rs:325-356`
- `routing/edge_routing_orthogonal/finalize.rs:86-243`

所以当前存在两个不同含义的 freeze：

1. Recipe 内的 `FrozenRouteGeometry`：只约束 RecipeRouter 局部。
2. finalizer 内的 `PolylineFreeze`：所有 geometry repair 后才捕获，而且变化只打 warning。

结论：**标签时序局部正确，全局“freeze 后只改 label”未达成。**

### 3.6 repair intent 尚未回到 solver

`RouteRepairIntent` 已有结构，但文件注释和实现都明确说明：

- through / group / trunk / crossing repair 仍原地写 geometry
- intent 只是伴随记账
- forbidden resources / required clearance 大多为空
- overshoot / crossing / trunk 的 affected edges 甚至为空

证据：

- `routing/model/repair.rs:1-16`
- `routing/model/repair.rs:45-80`
- `routing/edge_routing_orthogonal/finalize.rs:67-113`
- `routing/edge_routing_orthogonal/finalize.rs:150-217`

Coordinator 的 repair loop 也没有实现：

- `coordinator.rs:128-131` 只读取后丢弃 `max_repair_rounds`
- finalizer 产生的 `RouteAuditReport` 只在 runner 中打印，不交给 Coordinator

结论：**R8 的报告结构达成，R10 的闭环未达成。**

### 3.7 Lane / Bundle 是一等“局部结构”，尚不是全局 RouteSolution 的一等解

当前改造已经把 lane 和 bundle 拆成 solve/materialize，这是明显进步。

但执行顺序仍是：

```text
已物化 EdgeLayout
  → solve_lanes
  → materialize_lanes 原地改 points
  → solve_bundles
  → materialize_bundles 原地改 suffix
```

而不是方案要求的：

```text
Path topology
  → LaneSolution / BundleSolution
  → 单次 GeometryMaterializer
```

Semantic merge 仍发生在普通路径与 lane 之后，严格说仍属于 post-hoc 路径改写，只是决策与写入已分层。

结论：**R7 部分达成。**

### 3.8 LabelSolver 已统一入口，但问题模型与时序还未完全收口

优点：

- 所有 RecipeRouter 都调用统一 `LabelSolver::solve`。
- 正交 D 段也通过同一入口。
- merge dedupe 消费 annotation。

残余：

- RecipeRouter 初始 freeze 后先解一次 label，正交后续 geometry 被改后再解一次。
- straight / bezier / spline / circular / organic 在 RecipeRouter 内解完 label 后，pipeline
  仍会 repulse / snap geometry，但非正交 finalizer 是 no-op，不会再次运行 LabelSolver；
  因此这些 family 仍可能出现 label 基于旧几何定位的问题。
- Circular 在共享 LabelSolver 后又执行 `RadialPlacer`，仍有 family-specific 第二写者。
- `LabelAssignment.conflicts_remaining` 固定返回 0，不能证明求解质量。
- LabelProblem 仍直接持有 `&mut [EdgeLayout]`，不是候选/assignment IR。

证据：

- `routing/recipe/mod.rs:205-240`
- `routing/recipe/circular.rs:370-380`
- `routing/recipe/label.rs:61-115`
- `routing/edge_routing_orthogonal/finalize.rs:224-246`

结论：**R9 的统一入口达成，独立离散求解模型和唯一最终写者部分达成。**

### 3.9 节点生命周期仍有 route-after-move

已经达成：

- refine 的 node push 被 `skip_push = true` 停用。
- SpaceBudget guard 不再实际推节点。

仍存在：

- route feedback 在 Coordinator 完成初始路由后重新求解坐标，并调用 `router.route`。
- group frame 恢复、architecture PRS、质心重申、pendant 对齐仍可能在初始 route 后移动节点。
- runner 中还有多处直接 `router.route(self.diagram, result)`，绕过 Coordinator。

证据：

- `refine/mod.rs:95-108`
- `demand/space_budget_guard.rs:40-77`
- `pipeline/runner.rs:175-205`
- `pipeline/runner.rs:214-323`
- `pipeline/runner.rs:418-485`

结论：**“node freeze 后正式 router 只执行一次”未达成。**

### 3.10 RoutingPlan 与正式配置不完整

仍会改变正式算法行为的环境变量包括：

- `PLOTGRAM_EDGE_ORDER_SCORE`
- `PLOTGRAM_CHANNEL_PLANNER`
- `PLOTGRAM_OVG_ENABLED`
- `PLOTGRAM_PORT_SOLVER_V2`
- `PLOTGRAM_PORT_PRESSURE_SLOT`
- `PLOTGRAM_CORRIDOR_SOFT`
- `PLOTGRAM_AWAY_PENALTY`
- `PLOTGRAM_STUB_EXIT_PENALTY`
- `PLOTGRAM_SKIP_REFINE`
- `PLOTGRAM_EDGE_PRESSURE_BUDGET`

Debug/trace 开关可以保留，但上述开关中有多项会改变候选、代价、求解器和路径输出，不属于纯诊断。

代表性证据：

- `routing/edge_routing_orthogonal/draft.rs:22-27`
- `routing/edge_routing_orthogonal/channel_planner.rs:296-302`
- `routing/edge_routing_orthogonal/visibility_graph.rs:690-696`
- `routing/edge_routing_orthogonal/port_solver.rs:492-500`
- `routing/edge_routing_orthogonal/scoring.rs:152-190`

其中 channel planner 的注释还存在默认极性冲突：Draft 字段注释写“默认关闭”，
`channel_planner_enabled()` 的实现却是默认开启。这进一步说明正式 config 必须收口到
typed plan，不能继续依赖散落 env + 注释解释。

结论：**最终定义第 14 项未达成。**

### 3.11 StableEdgeId 尚不支持跨版本增量路由

当前 `StableEdgeId` 等于 `diagram.relations` 的数组下标，只能保证一次 run 内稳定。

证据：

- `routing/model/stable_edge.rs:1-20`
- `routing/model/stable_edge.rs:68-85`

尚未实现：

- declaration identity + parallel ordinal
- previous frozen solution
- edge → resource / obstacle / bundle / conflict dependency
- dirty component 局部扩张
- preserved route 合法性复核

结论：**R10 增量依赖图和最终定义第 13 项未达成。**

### 3.12 Self-loop、degraded 与 piecewise cubic 仍未完成

Self-loop 当前主要是共享 `route_self_loop` helper，不是独立 `SelfLoopSolver`：

- bezier / spline / circular 的普通自环通过 `overrides` 绕开 materializer 与 LabelSolver。
- circular 自环退化到 Spline 时没有写 `DegradedReason`。
- organic 没有独立 self-loop 分支。

Typed degraded 也尚未闭环：

- bezier / circular 普通弧 / organic 非 mindmap 会写 `diagnostics.degraded`。
- straight 的 family 改变、circular 自环 detour 等分支未完整记录。
- Coordinator、Auditor 和 benchmark 不消费 `RouteSolution.diagnostics.degraded`。

Spline 的产品形态仍不是方案 §10.2 建议的 piecewise cubic：

- `RoutePath::Spline` 保存密采样 `Vec<Point>`。
- GeometryMaterializer 将其物化为 `PathGeometry::Polyline`。
- `PathGeometry` 目前只支持单段 Bezier，没有多段 cubic 控制点产品。

证据：

- `routing/common/self_loop.rs:35-74`
- `routing/recipe/mod.rs:57-63`
- `routing/recipe/circular.rs:259-300`
- `routing/model/solution.rs:123-127`
- `routing/model/materialize.rs:70-75`
- `layout/types.rs:64-82`

结论：**共享 helper 和局部 typed fallback 已有，但方案中的统一 SelfLoopSolver、全链路 degraded 与 piecewise cubic 未达成。**

---

## 4. 对原方案最终完成定义的逐项判定

### 已达成

1. **每个 geometry family 有显式 RoutingRecipe。**
2. **最终 canvas transform 能同步变换 geometry / label / annotation。**
3. **确定性卫生线目前有较强证据：38/38 基准 `det=true`，核心新增迭代多数有显式排序或固定轮次。**

### 部分达成

1. Router 有 FrozenNodeProduct 守卫，但仍消费 `Diagram + LayoutResult`。
2. Recipe 已分 compile/solve，但正交 Recipe 仍包装旧内核，简单 Recipe 仍直接读取 Diagram/图类型。
3. port/path/lane/bundle/label 已有局部 solver/model，但未统一进入一个窄 `RoutingProblem → RouteSolution` 生命周期。
4. Orthogonal 有 ResourceGraph 和 bounded reroute，但 ResourceGraph 主要是旧资源聚合视图，initial path 仍在 solver 外，接受准则也不是完整全局 lexicographic score。
5. semantic merge 有 `BundleProblem`，但仍在普通路径物化后重写 suffix。
6. annotation 已有单一生成 helper 和 merge solution 来源，但每次几何变化后仍需 refresh。
7. refine 与 SpaceBudget 的 route 后推点已停用，但 route feedback / group frame / PRS 仍会在初始 route 后移动节点。
8. hard correctness 有审计和显式 degraded 结构，但最新基准仍存在穿组/穿节点残余。

### 未达成

1. GeometryMaterializer 是全局 edge geometry 唯一写者。
2. topology-changing repair 回到 solver model。
3. geometry freeze 后只允许 label，不再改 points。
4. 正式 layout-route feedback 只在 node freeze 前通过 SpacingDemand 有界重解。
5. 增量路由基于跨版本 stable identity 与依赖图。
6. 所有正式配置进入 RoutingPlan，环境变量只用于诊断。

---

## 5. 建议的后续收敛计划

后续不建议继续按“再搬一个 helper / 再包一层 finalizer”推进。下一阶段应切换到**写权切换**：每个 Slice 必须删除一类旧写者或旧入口，不能只增加并行模型。

### Slice A：建立唯一的最终路由边界

目标：

> 所有会改变 node/group 几何的工作都在最终 route 之前完成；最终 route 只执行一次。

任务：

1. 把当前 `route feedback re-solve` 改成真正的 `RoutingDemandProbe`：
   - 输入冻结前的 node/group product。
   - 只估计 port、corridor、self-loop、label band 容量。
   - 输出 `SpacingDemand`，不生成正式 edge geometry。
2. 将 group frame、PRS、architecture centroid/pendant 重申收敛到 layout finalize。
3. 在这些 pass 全部完成后捕获唯一 `FrozenNodeProduct`。
4. 删除 runner 中 Coordinator 之后的所有 `router.route(...)`。
5. `FrozenNodeProduct::assert_unchanged` 从局部 route 断言提升为 final route 到 canvas transform 之间的全段断言。

退出判据：

- `rg "router\\.route" pipeline/runner.rs` 为 0。
- 正式路由入口只有 `RoutingCoordinator::execute`。
- final route 后 node/group fingerprint 不变；唯一允许的坐标变化是最终刚体 canvas transform。
- group frame 不再在 route 后恢复。

依赖：

- 不依赖新的正交 solver，可先做。
- PRS 若必须读 route，可保留“preview probe”，但 preview 产物不得进入最终 edge product。

### Slice B：让 PreparedRoutingInput / RoutingContract / RoutingPlan 成为真实 API

目标：

> Router 在类型上无法修改 nodes/groups，也不再解析完整 Diagram/LayoutResult。

任务：

1. 定义最终动态入口，例如：

```text
route(
  PreparedRoutingInput,
  RoutingPlan,
) -> Result<RouteSolution, RoutingError>
```

2. `PreparedRoutingInput` 直接借用 `FrozenNodeProduct`，不再另建两套冻结 store。
3. 扩充 `RoutingContract` compiler：
   - feedback / same-layer
   - cross-scope / transit gates
   - monitor / business
   - pendant
   - port intents
   - corridor / gutter resources
   - merge intents
   - circle membership
   - label policy
4. Recipe compile 只消费 Prepared input + typed plan。
5. 将所有正式环境变量迁入 `RoutingConfig`；只保留 `*_DEBUG` / trace 类 env。
6. problem signature 覆盖完整 config、contract 和 fallback policy。

退出判据：

- `RoutingRecipeDyn::route(&Diagram, LayoutResult)` 被删除。
- routing Recipe / solver 中不再借用整个 `LayoutResult`。
- 正式算法路径不读取 `std::env`。
- 同一 Prepared input + plan 的 signature 可复现完整运行。

### Slice C：正交 topology solution 切换

目标：

> 正交 solver 不再产出或修改 `EdgeLayout`，而是产出结构化 topology solution。

建议分三步：

#### C1：PortAssignment 完整化

- 把 side、slot、anchor、capacity、protected stub 合并成一个 `EndpointAssignment`。
- 删除 `phase_port_correction`、slot replan、dock/exact-stub 中的端口/anchor 重选。
- 把 fixed side、feedback、fanin、reverse-pair、bundle sharing 表达成 hard/soft intent。

#### C2：PathAssignment 输出 RoutePath

- 将 initial path 移入 PathAssignmentSolver。
- ResourceGraph 从“旧资源只读聚合器”升级为真正的 vertex/arc/resource IR。
- 每条候选使用 hard tuple + quality tuple，不把 hard violation 混入加权和。
- 保存 best hard-feasible snapshot。
- reroute 接受准则比较完整全局 score，而不是仅“新路径 clean”。
- degraded 原因统一使用 `routing/model/solution.rs::DegradedReason`，删除正交本地同名枚举。

#### C3：Lane / Bundle 进入 RouteSolution

- 扩展 neutral model 以支持 per-segment lane，不强行保留当前 per-edge `LaneAssignment`。
- BundleSolver 在普通成员路径 finalize 前建立 trunk/junction。
- lane/bundle solver 只改 topology/resource assignment。
- 删除 `materialize_lanes` / `materialize_bundles` 对 `EdgeLayout` 的直接写入。

退出判据：

- 正交 `solve` 不出现 `EdgeLayout`、`PathGeometry`、`set_polyline_points`。
- `OrthogonalRecipe` 不再 clone `LayoutResult`，不再调用 `route_orthogonal_inner`。
- `RouteSolution` 直接携带 port/path/lane/bundle/annotation source。

### Slice D：切换全局唯一 Materializer 与真 geometry freeze

目标：

> 所有 family（含 self-loop）只在一个地方生成最终 PathGeometry。

任务：

1. 把 self-loop 改成 `SelfLoopSolution`，删除 `RecipeSolution.overrides`。
2. 让 RouteAuditor 显式理解 `EmptyRouteReason`，删除 RecipeRouter 直接构造
   `AuditedRouteGeometry` 的旁路。
3. GeometryMaterializer 同时消费：
   - RoutePath
   - EndpointAssignment
   - LaneSolution
   - BundleSolution
   - annotation source
4. topology-preserving canonicalize 进入 materializer。
5. snap 改为 materialize 的 grid-aware 候选选择或 freeze 前的 solver finalize。
6. 删除/封闭 `EdgeLayout::set_polyline_points`、`polyline_points_mut` 的公开写入口。
7. 捕获一次 `FrozenRouteGeometry`，之后 API 只暴露只读 geometry + 可写 label product。

退出判据：

- 除 sequence 自产边的明确例外外，生产代码中 `PathGeometry` 的构造只在 GeometryMaterializer。
- `RecipeSolution.overrides` 删除。
- geometry freeze 后没有 `set_polyline_points` / `geometry =` / `polyline_points_mut`。
- `PolylineFreeze::warn_if_changed` 可删除，由 typestate 取代。

### Slice E：Coordinator repair 闭环与删除 OrthogonalFinalizer

目标：

> Auditor 只报告，repair 只修改 solver model，Coordinator 管理固定轮次与 best solution。

任务：

1. 完整 RouteAuditor：
   - continuity / finite
   - endpoint boundary
   - port/stub/approach
   - orthogonality / curve continuity
   - node/group interior
   - corridor gate
   - merge annotation consistency
2. 每个 `RouteRepairIntent` 填真实：
   - affected edges
   - violated constraint ids
   - forbidden resources
   - required clearance
3. Coordinator 实现固定轮次：

```text
solve
  → materialize
  → audit
  → compile repair intents
  → local re-solve
  → retain best hard-feasible
  → freeze
```

4. 把以下原地 pass 改成 solver intent 或删除：
   - overshoot merge
   - reverse pair gap/dock
   - exact stub occupancy
   - through repair
   - group interior repair
   - trunk separation
   - crossing reduction
5. 删除 `OrthogonalFinalizer` 和 style-keyed finalizer 分派。

退出判据：

- Auditor/Finalizer 不持有 `&mut LayoutResult`。
- repair intent 不是伴随记账，而是 Coordinator 的真实输入。
- 不存在 hard-feasible 解时返回显式 infeasible/degraded report。
- 最新基准中穿组、穿非端点节点、非正交段、端点/approach 违规均为 0，或有明确 contract allow-list。

### Slice F：LabelSolver 终态与增量路由

#### F1：LabelSolver 终态

- 删除 Recipe 内的提前 label solve。
- 在唯一 `FrozenRouteGeometry` 后构造 LabelProblem。
- 让 `LabelAssignment` 返回真实残余冲突、chosen candidate 和 degraded reason。
- Circular 的径向候选进入 candidate builder，删除 solve 后的 `RadialPlacer` 第二写者。
- merge dedupe 保持 hard contract。

#### F2：增量依赖图

- StableEdgeId 改为 declaration identity + parallel ordinal。
- FrozenRoutingSolution 记录：
  - endpoint nodes
  - traversed resources
  - nearby obstacles
  - group/corridor gates
  - bundle
  - conflicts
  - label segments
- 输入 diff 生成 dirty set。
- dirty component 固定顺序局部重解，失败时按固定跳数扩张，最终回退全图。

退出判据：

- label solve 后禁止修改 geometry。
- 同一输入的 label assignment signature 稳定。
- 单节点移动不会无关地重路由全图。
- preserved route 在复用前会重新过 hard audit。

#### 【Slice F 后事实更新（2026-07）】

Slice F1+F2 已全部落地，退出判据逐条钉死为测试：

- label solve 是最后写者：Coordinator freeze 后仅 label solve +
  `FrozenRoutingSolution::capture` 执行，二者均不改 edge geometry。
- signature 稳定：`recipe/label.rs` 单测（同输入两次 solve signature 一致）。
- 单节点移动 dirty_set 只含依赖分量内边：`frozen_solution.rs` 单测
  （含平行边 ordinal / 头部插入新 relation 后 identity 保持匹配）。
- preserved route 复用前重过 hard audit：`coordinator.rs` 单测（audit fail
  触发扩张 / 全 fail 回退 None / 改文案只脏对应边）。
- 同一 diagram 两次渲染（zero diff）→ 全 preserve 且边/注解输出字节一致：
  `coordinator.rs` 集成测试（经 `compute_layout_incremental` 入口）。
- `StableEdgeId` 保留为 solve 内 positional handle；一切跨版本持久 key
  统一用 `StableEdgeIdentity`（模块 doc 已说明定位）。
- circular 标签输出字节有意变化（radial 候选替代 RadialPlacer 后处理），
  按 AGENTS §8 豁免期重采基线（tag=sliceF）。

---

## 6. 推荐实施顺序与提交边界

建议顺序：

```text
Slice A  唯一最终路由边界
  → Slice B 真实 Prepared API + typed Contract/Plan
  → Slice C1 Port 完整化
  → Slice C2 Path topology solution
  → Slice C3 Lane/Bundle 进入 RouteSolution
  → Slice D 唯一 Materializer + 真 freeze
  → Slice E Coordinator repair 闭环 + 删除 finalizer
  → Slice F1 Label 终态
  → Slice F2 增量依赖图
```

每个提交必须满足：

1. 明确删除至少一个旧入口、旧写者或正式环境开关；禁止只增加影子类型。
2. 保持固定排序、固定轮次和稳定 tie-break。
3. 不引入图名/节点名特判。
4. 不在 WASM 路径使用裸 `std::time::{Instant, SystemTime}`。
5. 使用 `cargo run -p plotgram-cli` 验证实际源码产物，不执行陈旧 release binary。
6. 运行 `cargo test -p plotgram-core --lib`。
7. 当前全局优化豁免期间质量轨可以波动，但以下仍为硬门槛：
   - `det=true`
   - `edge_crosses_group_interior = 0` 或明确 contract allow
   - 非端点 `edge_through_node = 0`
8. 每个 Slice 更新本文件中的“剩余直接写者”和退出判据，不以注释中的 `Rxx 完成`替代实际 grep/调用链证据。

---

## 7. 下一刀的具体建议

下一步优先实施 **Slice A**，不要直接继续扩充 PathSolver。

原因：

1. 当前最大架构矛盾不是“缺一个更强的候选算法”，而是最终 route 之后仍会移动节点、重新 route、snap、repair。
2. 在最终路由边界不唯一时，任何 Materializer freeze、LabelSolver、annotation 同源或增量依赖都会被后续 pass 推翻。
3. Slice A 基本不要求改变正交寻路算法，风险低，却能为后续所有写权收口提供稳定时序。
4. Slice A 完成后，Slice B 才能安全地把 `PreparedRoutingInput` 提升为正式 API；否则 Prepared input 会在后续节点移动后立即过期。

建议 Slice A 的最小完成定义：

```text
layout solve
  → group/space/route demand finalize
  → FrozenNodeProduct
  → RoutingCoordinator（唯一正式 route）
  → route geometry lifecycle
  → LabelSolver
  → atomic canvas transform
```

在这个边界稳定前，不建议宣布 doc16 已完成，也不建议删除原方案文档中的“待实施”状态。
