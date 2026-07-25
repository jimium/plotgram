# 22 - 路由 Recipe / Kernel（Slice A–F）实施代码审查报告

> 日期：2026-07-25
> 审查对象：
> - [`16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md`](./16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md)（原始方案，R0–R11 + §25 最终完成定义）
> - [`20-路由Recipe-Kernel实施审查与后续收敛计划-2026-07.md`](./20-路由Recipe-Kernel实施审查与后续收敛计划-2026-07.md)（上一轮审查 + Slice A–F 收敛计划）
>
> 代码基线：当前工作区 `developv2-bigchange`（含未提交改动）
> 审查口径：以**实际调用链与最终写权**为准。文件名、注释里的 `Slice X 完成`、类型是否存在，均不作为完成证明；每条结论附 `file:line` 或可复现命令。
> 验证手段：`cargo test -p plotgram-core --lib`（**986 passed / 0 failed**）、`cargo run -p plotgram-cli -- render`（实测 30 个 product 图）、`benchmarks/baselines/latest.json`（tag=`sliceF`，38 样例）、`cargo check -p plotgram-core` 警告清单、跨全部写权点的 grep。

---

## 0. 执行结论

Slice A–F **完成了 doc20 计划中份量最大的两件事**：管线时序被真正收束（唯一最终路由边界 + 唯一几何冻结点 + 标签唯一最后写者），以及大批旧入口被物理删除而非包一层。这两点用 grep 与运行时日志都能证实，不是注释宣称。

但 doc20 排在最前面的 **Slice C（正交 topology solution 切换）实质未完成**，而 C 是 B/D/E 能否真正闭环的前置：

- `OrthogonalRecipe::compile` 仍克隆整个 `LayoutResult`、`solve` 仍调 `route_orthogonal_inner`（C 的两条退出判据原文即为「不再 clone / 不再调用」）。
- `RouteSolution` 在正交内核**末尾**由已写好的 `result.edges` 逐边 `lift_geometry` 反向构造，不是求解器状态的产物。
- 因此 `PreparedRoutingInput` 虽已成为 trait 签名，但 `contract` / `edges` / `config` / `direction` 四个字段**零消费者**——Recipe 只取 `frozen` + `hints` 重建一个临时 `LayoutResult`，然后走旧算法。

一句话定位当前里程碑：

> **时序层（A / D / E / F）已收敛到方案终态；数据流层（B / C）仍是「新签名包裹旧可变管线」。写权从「散落全管线」收敛为「散落在唯一 freeze 之前」，但尚未收敛到「唯一 materializer」。**

三个必须点名的风险：

1. **正式配置能力净损失**：9 个 `PLOTGRAM_*` 正式开关下线并改成 typed `RoutingConfig` 是对的，但没有任何调用方填它——runner 传 `Default::default()`（注释「待后续从 pipeline 传入」），`OrthogonalRecipe::from_options` 也写死 `Default::default()`。env 删掉了，可配置性也一起消失了，且 `problem_signature` 里的 config 项恒为常量。
2. **增量路由在生产路径不可达**：`compute_layout_incremental` 唯一调用者是 `coordinator.rs` 的测试；CLI 与 WASM 都走 `compute_layout_with_plan`。同时 `FrozenRoutingSolution::capture` 每次渲染无条件执行——为不可达的功能付固定成本。
3. **扩展审计的残余违规面远大于 lint**：30 个 product 图中 **7 个**以 `residual_hard_edges > 0` 收尾（共 11 条边），而 lint 只看到 1 个穿组。这是审计变强的好现象，但意味着 doc16 §21.1 的第 4/5/6 项目前都不为 0。

---

## 1. 逐 Slice 判定

### Slice A：唯一的最终路由边界 —— **达成**

| 退出判据 | 结果 | 证据 |
|---|---|---|
| `rg "router\.route" pipeline/runner.rs` 为 0 | **有条件达成** | 仅剩 1 处：`runner.rs:203` 的 architecture PRS preview。doc20 §Slice A「依赖」一节显式允许保留 preview probe，且产物确实被丢弃——`preview_result` 是克隆体，只有 `groups` 被写回（`runner.rs:205-219`） |
| 正式路由入口只有 `RoutingCoordinator::execute` | 达成 | `runner.rs:289`；全仓 `RoutingCoordinator` 引用仅 3 处非注释命中 |
| final route 后 node/group fingerprint 不变 | 达成 | `runner.rs:306` `frozen.assert_unchanged(&result)`；`FrozenNodeProduct` 在所有构建模式下校验（`coordinator.rs:52-58`），非 `debug_assert` |
| group frame 不再在 route 后恢复 | 达成 | `restore_after_node_moves` / `recompute_group_bounds` 均在 `runner.rs:169-185`，即 frozen 捕获（`runner.rs:262`）之前 |

`spacing_demand_probe`（`runner.rs:154`）已把 route feedback 的坐标重解前移到路由之前，`refine` 的 node push 与 SpaceBudget guard 推点均已停用。

**残余问题（P1-3）**：PRS preview 是**一次完整的正式 router 运行**，不是 Slice A 任务 1 要求的轻量 `RoutingDemandProbe`（只估 port / corridor / self-loop / label band 容量）。实测 `product.cdn-cache.pgm` 的 `[perf] r6_path_solver` 日志出现两次，即 architecture 图的正交内核跑两遍。这是 architecture 图路由耗时的主要来源。

### Slice B：Prepared / Contract / Plan 成为真实 API —— **部分达成（约 40%）**

**已达成的部分是硬收益，不可低估：**

- 动态入口已换签名：`fn route(&self, input: &PreparedRoutingInput<'_>) -> RoutingProduct`（`traits.rs:105`）。旧的 `route(&Diagram, LayoutResult) -> LayoutResult` 已删除。
- `RoutingProduct` 只含 `edges` + hints delta，**类型上不可能修改 nodes/groups**（`traits.rs:84-92`）。这是比 assert 更强的约束，达成了 Slice B 的核心意图。
- `PreparedRoutingInput` 直接借用 `FrozenNodeProduct`（`prepared.rs:33`），没有另建第二套冻结 store。
- 9 个正式 env 开关全部下线为 typed `RoutingConfig`（`routing/config.rs`）。剩余 `std::env` 命中仅 6 处，5 处是 `*_DEBUG` trace。

**未达成的部分是本轮最大的结构缺口：**

`PreparedRoutingInput` 的字段消费情况（grep 全仓 `input.<field>`）：

| 字段 | 消费者 |
|---|---|
| `frozen` | `recipe/mod.rs:153-154`（重建 temp LayoutResult） |
| `hints` | `recipe/mod.rs:158` |
| `contract` | **无** |
| `edges` | **无** |
| `config` | **无** |
| `direction` | **无** |

即 Prepared input 是一个外壳：`RecipeRouter::temp_result` 从 `frozen` + `hints` 重新拼出一个 `LayoutResult`（`recipe/mod.rs:151-160`），Recipe 的 `compile` 签名仍是 `compile(&Diagram, &LayoutResult)`（`recipe/mod.rs:101`），下游算法照旧。

`RoutingContract::compile` 仍只填 3 个 role（`contract.rs:192-222`）：`SelfLoop` / `ParallelGroup` / `Forward`。`port_intents` / `transit_intents` / `corridors` / `side_gutters` / `merge_intents` 全部 `Vec::new()`，`circle_membership` 全 `None`。`EdgeRole::{Feedback, SameLayer, CrossScope, Monitor, Business, Pendant}` 六个变体在 crate 内**零读取点**（`rg "roles_of|edge_roles|EdgeRole::"` 排除定义文件后为空）。doc20 Slice B 任务 3 列的 9 项扩充，一项未做。

**连带后果（P0-3）**：`RoutingConfig` 无接线路径。

```296:296:crates/plotgram-core/src/layout/pipeline/runner.rs
            Default::default(), // RoutingConfig：待后续从 pipeline 传入
```

```40:40:crates/plotgram-core/src/layout/routing/recipe/orthogonal.rs
                routing: Default::default(),
```

Coordinator 收到 `routing_config` 后只把它塞进 `PreparedRoutingInput`（`coordinator.rs:120`）→ 只进 `problem_signature`（`prepared.rs:103`）。而正交内核读的是 `OrthogonalRecipe.config.routing`，那是构造时写死的 default。所以：`edge_order_score` 等 9 个正式参数既不能由 DSL 配、也不能由 pipeline 传，且 signature 中的 config 分量恒为常量——`problem_signature` 不能真正描述一次运行（doc16 §25 第 14 项、Slice B 第 4 条退出判据均未满足）。

### Slice C：正交 topology solution 切换 —— **未达成**

三条退出判据逐条判定：

| 退出判据 | 结果 | 证据 |
|---|---|---|
| 正交 `solve` 不出现 `EdgeLayout` / `PathGeometry` / `set_polyline_points` | **未达成** | `run.rs:185-203` 桥接 `RoutePath → EdgeLayout`（含 `PathGeometry::Polyline` 构造 + `set_polyline_points`）；此后 lane / bundle / S4 / sanitize / escape 全部原地写点 |
| `OrthogonalRecipe` 不再 clone `LayoutResult`、不再调 `route_orthogonal_inner` | **未达成** | `recipe/orthogonal.rs:79` `result: result.clone()`；`recipe/orthogonal.rs:85` `route_orthogonal_inner(...)` |
| `RouteSolution` 直接携带 port/path/lane/bundle/annotation source | **部分** | 结构上携带（`run.rs:488-496`），但 `paths` 由 `result.edges.iter().map(lift_geometry)` 反向构造（`run.rs:476-480`），不是求解器 `paths` 状态 |

`route_orthogonal_inner` 的实际时序（写权标注）：

```text
 0  OrthogonalDraft::compile              solver-side（产 endpoint_assignments）
 1  phase_route_edges                     solver-side（产 Vec<RoutePath>）
 2  path_solver::solve_paths              solver-side（改 paths）
 3  Bridge RoutePath → EdgeLayout         ★ 写点（run.rs:185-203）
 4  phase_lane / materialize_lanes        ★ 写点
 5  apply_semantic_trunk_merge            ★ 写点（重写 suffix）
 6  S4 reroute_feedback_after_trunk       ★ 写点
 7  route_annotations_from_solution       只读
 8  phase_sanitize / canonicalize         ★ 写点
 9  S4 escape repair                      ★ 写点
10  monitor_local_trunk_merge             ★ 写点
11  组装 hints.route_solution             从 result.edges lift 回来
```

分项细节：

- **C1（Port 完整化）：基本达成。** `EndpointAssignment` 已是单一结构，统一携带 side / anchor / slot_index / side_capacity / protected_stub / docking（`model/solution.rs:74-96`），唯一写者是 `draft.rs::build_endpoint_assignments`。`phase_port_correction` 与 `replan_slots` 已无函数定义（仅剩过时注释）。
- **C2（Path topology）：未达成。** 初始路径仍在 solver 之外（`path_solver.rs:7` 模块注释自陈「由 `phase_route_edges` 逐边求得（solver 之前已执行）」）；`ResourceGraph` 自陈「**纯只读适配层**：只持有对既有资源的不可变引用」（`resource_graph.rs:1-13`），不是 vertex/arc IR；候选代价是 4 项 `f64` 加权和（`scoring.rs:52-88`），硬违规以软罚项混入同一标量，没有 hard-tuple / quality-tuple 分离；rip-up 接受准则是局部「新路径是否 clean」（`conflict_reroute.rs:132-149`），`global_score` 仅存在于 `#[cfg(test)]`。**唯一达成项**：`DegradedReason` 已统一到 `model/solution.rs`，正交本地重复枚举已删。
- **C3（Lane / Bundle 进 RouteSolution）：未达成。** `LaneAssignment` / `BundleSolution` 类型已有且 solve/materialize 已分层，但执行顺序仍是「桥接成 EdgeLayout → solve_lanes → materialize_lanes 原地改点 → solve_bundles → materialize_bundles 原地重写 suffix」。`semantic_trunk_merge.rs:260-277` 的 `materialize_bundles` 注释自陈是「edge geometry 与 merge_intervals 的唯一写者」——即仍直接写 `EdgeLayout`。

### Slice D：唯一 Materializer 与真 geometry freeze —— **时序达成，写权部分达成**

| 退出判据 | 结果 | 证据 |
|---|---|---|
| `RecipeSolution.overrides` 删除 | 达成 | `recipe/mod.rs:60-62` 记录删除；全仓无该字段 |
| geometry freeze 后无 `set_polyline_points` / `geometry =` / `polyline_points_mut` | **达成** | 唯一真冻结点 `coordinator.rs:433-464`；此后仅 label solve（`coordinator.rs:470-496`）+ `FrozenRoutingSolution::capture`，二者都不改折点 |
| `PolylineFreeze::warn_if_changed` 可删除 | 达成 | grep=0 |
| 生产代码 `PathGeometry` 构造只在 GeometryMaterializer | **未达成** | 22 个非测试文件仍构造 `PathGeometry`；`set_polyline_points` / `polyline_points_mut` / `geometry =` 共 33 处命中，其中约 20 处是生产写点 |

Auditor 旁路已封堵：`RecipeRouter` 遇审计失败时不再 `from_audited` 推进，而是把违规边降级为声明性空边后**重新过 auditor**（`recipe/mod.rs:177-200`），`expect` 保证必经审计。这是一个真实的旁路删除。

**残余问题（P2-2）**：写口封闭只做了一半。`EdgeLayout::set_polyline_points` / `polyline_points_mut` 已降为 `pub(crate)`（`types.rs:371, 389`），但 `EdgeLayout.geometry` 字段仍是 `pub`（`types.rs:262`）且 `PathGeometry::polyline_points_mut()` 仍是完全 `pub`（`types.rs:135`）——crate 外仍可经 `edge.geometry.polyline_points_mut()` 或直接赋值改点。

### Slice E：Coordinator repair 闭环与删除 Finalizer —— **达成度最高的一个 Slice**

**旧层删除彻底（全部 grep=0）**：`OrthogonalFinalizer`、`NoopFinalizer`、`route_finalizer_for_style`、`PolylineFreeze`、`recheck_lint_pierce_post_freeze`、`repair_through_edges_post_route`、`repair_group_interior_edges_post_route`、`separate_trunk_overlaps_post_route`、`stub_fix.rs`。`edge_routing_orthogonal/finalize.rs`（整个 D 段）已从磁盘消失。

**E1 auditor 覆盖 10 类违规**（`audit.rs:39-60`）：`NonFinite` / `TooFewPoints` / `DegenerateEndpoints` / `TooManyPoints` / `EndpointBoundary` / `StubDirection` / `NonOrthogonalSegment` / `ThroughNode` / `GroupInterior` / `MergeInconsistent`。对照 doc20 Slice E 任务 1 的 7 项清单，**缺 corridor gate 与 curve continuity**。

**E2 intent 有真实内容**：`compile_from_violations`（`repair.rs:130-154`）按 `BTreeMap` 升序分组，填真实 `affected_edges`（升序去重）、`forbidden_resources`（穿越的节点/组 id）、`required_clearance`、`priority`。已不是 doc20 §3.6 批评的「空壳记账」。

**E3 repair loop 真实工作**——这是本轮最有说服力的运行时证据：

```
[perf]     r6_path_solver: 3 degraded edges
[perf]     e3_repair round=0 hard_edges=3
[perf]     e3_repair round=1 hard_edges=1
[warn] e3_repair degraded: residual_hard_edges=1
```
（`cargo run -p plotgram-cli -- render showcase/architecture/product.cdn-cache.pgm`）

固定轮次（`max_repair_rounds`，`coordinator.rs:333`）、best hard-feasible 快照保留与末轮回滚（`coordinator.rs:340-394`）、无 hard-feasible 时显式 `mark_degraded()` + 写 `annotation.degraded`（`coordinator.rs:399-428`）、stalled 轮次沿依赖图 1 跳固定序扩张（`coordinator.rs:376-383`）全部落地。「不静默冒充成功」这条红线有实测支持。

**残余问题（P1-1，方案层面而非实现层面）**：`forbidden_resources` 与 `required_clearance` 编译出来后**零读取点**（grep 仅命中定义与测试）。E3 的 local re-solve 入口是：

```317:333:crates/plotgram-core/src/layout/refine/mod.rs
pub(crate) fn reroute_edges_for_repair(
    result: &mut LayoutResult,
    diagram: &Diagram,
    edge_indices: &HashSet<usize>,
    aggressive: bool,
) {
    if edge_indices.is_empty() {
        return;
    }
    spline_fallback::reroute_edges_with_spline_ex(
        result,
        diagram,
        edge_indices,
        &RefineConfig::default(),
        aggressive,
    );
}
```

即 repair 只消费 `affected_edges` + 一个 `aggressive` 布尔，走原地 dogleg / 裙边改写，**不是**「带禁用资源与 clearance 回到 PathAssignmentSolver 重解」。doc16 §12.2「topology-changing optimization 必须回到 solver/repair model，在完整 obstacle/contract 下重新评分」未达成。这是 C2 未完成的直接连带——没有可复解的 solver 模型，intent 无处可交。

**残余问题（P1-2）**：Coordinator 出现了 family / algo 分支：`router.name() == "orthogonal"`（`coordinator.rs:223`）、`router.name() == "circular"`（`coordinator.rs:476`）、`algo == "architecture"`（`coordinator.rs:262`）。doc16 §5.4 明确要求「Coordinator 不知道 architecture monitor / circular cluster」。这**不违反** AGENTS §5 的「禁止图名特判」红线（分支键是 family / 算法名，不是图名），但违背 doc16 的职责划分，且把 architecture exact-stub 收口固化在协调器里。

### Slice F1：LabelSolver 终态 —— **达成**

| 退出判据 | 结果 | 证据 |
|---|---|---|
| label solve 后禁止修改 geometry | 达成 | Coordinator 唯一 freeze 之后只有 label solve + capture |
| 同一输入 label assignment signature 稳定 | 达成 | `LabelAssignment.signature`（量化 0.01px + 文案 FNV）；`recipe/label.rs` 单测钉死 |
| 删除 Recipe 内提前 label solve | 达成 | `recipe/mod.rs:223` 只留 plan-based `LabelSolver::place`；冲突消解统一在 Coordinator |
| 删除 circular 的 `RadialPlacer` 第二写者 | 达成 | `RadialPlacer` / `LabelPlacer` / `label_placement` 全部 grep=0；径向候选并入 `label_candidate.rs` 的 `radial_center` |
| `LabelAssignment` 返回真实残余冲突与 degraded | 达成 | `label.rs:83-95` + `audit_residual_conflicts`；实测 `conflicts_remaining=0 degraded=[(2,"label_node_overlap")]` |

**残余**：`LabelProblem` 仍持有 `&'a mut [EdgeLayout]`（`label.rs:71`），不是 doc16 §14.2 的候选/assignment IR；`chosen candidate` 未返回（doc20 F1 任务 3 提到但未实现）。属结构性遗留，不影响时序正确性。

### Slice F2：增量依赖图 —— **代码达成，生产不可达**

| 退出判据 | 结果 | 证据 |
|---|---|---|
| StableEdgeId 改为 declaration identity + parallel ordinal | 达成 | `StableEdgeIdentity{from, to, parallel_ordinal}`（`stable_edge.rs:36-40`）+ `match_identities` 产确定性 `EdgeIdentityDiff`；`StableEdgeId` 保留为 solve 内 positional handle（模块 doc 已说明定位） |
| FrozenRoutingSolution 记录 7 类依赖 | 达成 | `model/frozen_solution.rs`；经 `hints.frozen_routing`（`Arc`）暴露，无全局会话态 |
| 单节点移动不无关重路由全图 | 达成 | `dirty_set` 沿 conflicts + bundle 做固定序连通分量闭包；单测钉死（含平行边 ordinal、头部插入新 relation 后 identity 保持匹配） |
| preserved route 复用前重过 hard audit | 达成 | `coordinator.rs:613-681`：逐边 `audit_extended` → fail 则固定序 ≤2 跳扩张 → 仍 fail 回退全图；3 个单测覆盖 |
| zero-diff 重渲染字节一致 | 达成 | `coordinator.rs:888-925` 集成测试，经 `compute_layout_incremental` 入口 |

**残余问题（P1-5）**：整条链路在生产上不可达。

```
compute_layout_incremental 调用者：
  crates/plotgram-core/src/layout/routing/coordinator.rs:913   ← 测试
生产入口实际调用：
  crates/plotgram-cli/src/main.rs:494        compute_layout_with_plan
  crates/plotgram-wasm/src/lib.rs:394       compute_layout_with_plan
```

同时 `FrozenRoutingSolution::capture` 在 Coordinator 里**无条件**执行（`coordinator.rs:500-504`），每次渲染都构建逐边依赖记录 + 指纹。这是为不可达功能付的固定成本，也是 sliceDE → sliceF 出现 +1.8% 耗时的一个来源。

---

## 2. doc16 §25「最终完成定义」15 条逐项判定

| # | 条目 | 判定 | 关键依据 |
|---|---|---|---|
| 1 | Router 只消费 `FrozenNodeProduct + RoutingContract` | 部分 | 类型上不可写 nodes/groups ✓；contract 零消费 ✗；内部重建 LayoutResult ✗ |
| 2 | 每个 geometry family 有显式 RoutingRecipe | **达成** | registry 6 个 `RecipeRouter::new(...)` |
| 3 | Recipe 编译语义，Kernel 不读 DiagramType | 部分 | `recipe/orthogonal.rs:149` 读 `DiagramType::Mindmap`；circular/organic recipe 读图类型；Coordinator 读 family/algo |
| 4 | port/path/lane/bundle/label 各有独立问题模型与 solver | 部分 | 模型与 solver 都在，但未统一进窄 `RoutingProblem → RouteSolution` 生命周期 |
| 5 | OrthogonalRecipe 用统一 resource graph + 有界 rip-up | 部分 | 有界 rip-up ✓（固定 3 轮）；ResourceGraph 是只读聚合 ✗；初始路径在 solver 外 ✗；接受准则非全局 score ✗ |
| 6 | semantic merge 求解前是一等 BundleProblem | **未达成** | 仍在普通路径物化后重写 suffix |
| 7 | GeometryMaterializer 是 edge geometry 唯一写者 | **未达成** | freeze 前约 20 处生产写点；freeze 后 0 处 ✓ |
| 8 | topology-changing repair 回到 solver | **未达成** | intent 的 forbidden_resources / clearance 零消费；repair 走原地 dogleg |
| 9 | freeze 后只解 label，不再改 points | **达成** | 唯一真冻结点 + 其后仅 label/annotation |
| 10 | annotation 与 geometry 同源 + 原子 transform | 部分 | 单一同源生成器 ✓、`canvas_finalize` 原子平移 ✓；但几何变化后仍需 `refresh_route_annotations_*`（`coordinator.rs:273-281`） |
| 11 | layout-route feedback 只在 node freeze 前有界重解 | **达成** | `spacing_demand_probe` 前置 + route 后 fingerprint 断言 |
| 12 | group frame / SpaceBudget / refine 不越权移动节点 | **达成** | 三者推点逻辑均已停用或前移 |
| 13 | 增量路由基于 stable identity 与依赖图 | 部分 | 代码达成 ✓；生产路径不可达 ✗ |
| 14 | 所有正式配置进 RoutingPlan，env 仅诊断 | 部分 | env 下线 ✓；config 未接线 ✗；`PLOTGRAM_SKIP_REFINE` 仍改正式行为 ✗ |
| 15 | 穿组 / 穿节点 / 端点 / 确定性硬底线 | 部分 | det 38/38 ✓；product 穿组残 1；扩展 audit 残 7 图 11 边 |

计：**达成 5 / 部分 7 / 未达成 3**。（doc20 审查时为：达成 3 / 部分 8 / 未达成 6。）

---

## 3. 硬正确性与质量数据

### 3.1 确定性与测试

- `det = true`：**38/38**（`latest.json`）。确定性红线有证据支持。
- `cargo test -p plotgram-core --lib`：**986 passed / 0 failed**（doc20 审查时 972）。

### 3.2 lint 硬指标（latest.json，tag=sliceF）

| 样例 | role | edge_crosses_group_interior | edge_through_node |
|---|---|---|---|
| `showcase/er/product.saas-schema.pgm` | product | **1** | 0 |
| `showcase/architecture/stress.layout-stress-flat-mesh.pgm` | stress | 0 | **1** |

对比 doc20 审查基线（`r12c`）：`product.cdn-cache.pgm` 的穿组已消除（2 → 1）。按 doc20 §6 第 7 条门槛，`product` 角色的 `edge_crosses_group_interior = 0` **仍未满足**（残 1 条，无 contract allow-list 记录）。

### 3.3 扩展审计残余（比 lint 严，实测 30 个 product 图）

7 个图以 `residual_hard_edges > 0` 收尾，共 11 条边：

| 图 | 残余 hard 边 |
|---|---|
| `product.user-session.pgm` | 3 |
| `product.password-reset.pgm` | 2 |
| `product.delivery-tracking.pgm` | 2 |
| `product.cdn-cache.pgm` | 1 |
| `product.symmetric-fanout.pgm` | 1 |
| `product.order-lifecycle.pgm` | 1 |
| `product.saas-schema.pgm` | 1 |

注意 `cdn-cache` 的 lint 穿组/穿节点均为 0，却仍报 `degraded=true`——因为 E1 auditor 额外检查 `EndpointBoundary` / `StubDirection` / `NonOrthogonalSegment` / `MergeInconsistent`。**审计变严是正确方向**，但需要明确口径：这些残余目前既不在 benchmark 门禁里，也没有 allow-list，只体现为 CLI 的 `[warn]` 与 `annotation.degraded`。建议把 `residual_hard_edges` 提升为 benchmark 采集项，否则它会成为下一个「只有跑 CLI 才看得见」的暗债。

### 3.4 质量与性能趋势（r12c = doc20 审查点 → sliceF）

| 指标 | r12c | sliceF | 变化 |
|---|---|---|---|
| `edge_crosses_group_interior`（合计） | 2 | 1 | **−1（改善）** |
| `edge_through_node`（合计） | 1 | 1 | 持平 |
| `edge_crossing`（合计） | 95 | 99 | +4 |
| `label_node_overlap`（合计） | 18 | 21 | +3 |
| 38 样例 median 合计 | 165.8 ms | 192.5 ms | **+16.1%** |
| `det` | 38/38 | 38/38 | 持平 |

AGENTS §8 豁免期内质量波动可接受，但 +16% 耗时有可定位来源，建议登记为技术债：

1. **E3 repair loop 的全量重复审计**：每轮都对**全部边** `lift_geometry → materialize → audit_extended`（`coordinator.rs:314-324`），`max_repair_rounds = 2` 意味着最多 3 次全量审计，加上 freeze 前的第 4 次。而 repair 实际只动少数边。
2. **`FrozenRoutingSolution::capture` 无条件执行**（见 P1-5）。
3. **architecture 图正交内核跑两遍**（PRS preview，见 P1-3）。

历史对照：相对本轮全局优化起点（`2026-07-20-053914-tier-a-before`）的公共样例子集，耗时 237.1 ms → 119.1 ms（**−49.8%**）。整轮优化的性能收益是显著的，上述 +16% 是最后几个 Slice 的局部回吐。

---

## 4. 新发现的问题清单

按严重度排列。P0 = 阻断方案终态；P1 = 功能/成本实质缺口；P2 = 卫生与一致性。

| ID | 严重度 | 问题 | 位置 |
|---|---|---|---|
| P0-1 | 高 | Slice C 三条退出判据全部未满足：`OrthogonalRecipe` 仍 clone `LayoutResult` + 调 `route_orthogonal_inner`；`RouteSolution` 由已物化几何 lift 回构；lane/bundle 在桥接后原地写点 | `recipe/orthogonal.rs:79,85`；`run.rs:185-203, 471-497` |
| P0-2 | 高 | `PreparedRoutingInput` 的 `contract`/`edges`/`config`/`direction` 零消费者；`RoutingContract` 仍只填 3 个 role，9 项 intent 全空；6 个 `EdgeRole` 变体零读取 | `recipe/mod.rs:151-160`；`contract.rs:207-222` |
| P0-3 | 高 | `RoutingConfig` 无接线：runner 与 `OrthogonalRecipe` 都传 `Default::default()`。env 已删但可配置性一并消失，signature 的 config 分量恒为常量 | `runner.rs:296`；`recipe/orthogonal.rs:40` |
| P1-1 | 中 | repair intent 的 `forbidden_resources` / `required_clearance` 零消费；local re-solve 走原地 dogleg，非「回 solver 重评分」 | `refine/mod.rs:317-333`；`repair.rs:96-98` |
| P1-2 | 中 | Coordinator 出现 family/algo 分支（`"orthogonal"` / `"circular"` / `"architecture"`），违 doc16 §5.4 职责划分（未违 AGENTS 图名红线） | `coordinator.rs:223, 262, 476` |
| P1-3 | 中 | architecture PRS preview 是完整正式 router 运行，非轻量 `RoutingDemandProbe`；正交内核跑两遍 | `runner.rs:187-219` |
| P1-4 | 中 | 7/30 product 图以 `residual_hard_edges > 0` 收尾（11 条边），未纳入 benchmark 采集，仅 CLI warn 可见 | 见 §3.3 |
| P1-5 | 中 | `compute_layout_incremental` 无生产调用者（CLI/WASM 都走 `compute_layout_with_plan`）；`FrozenRoutingSolution::capture` 仍每次渲染无条件执行 | `cli/main.rs:494`；`wasm/lib.rs:394`；`coordinator.rs:500` |
| P2-1 | 低 | 违 AGENTS §1（不保留旧代码）：`StraightRouting`/`BezierRouting`/`SplineRouting`/`OrthogonalRouting` 结构体仍在，`mod.rs:174` 注释写「保留为历史参考，不再参与 pipeline 调度」 | `edge_routing.rs:21` 等 |
| P2-2 | 低 | 83 个编译警告，含 **33 个 never-used function** + 10 个 never-used constant。整模块级死代码：`straighten.rs`（5 fn）、`sanitize.rs`（4 fn）、`crossing_reduction.rs`（3 fn）、`phases/refine.rs::phase_straighten_align`（内含 anchor 修改 + 重路由的等价遗留逻辑）；`refine/mod.rs` 用 `let skip_push = true;` 包住约 110 行死分支 | `cargo check -p plotgram-core` |
| P2-3 | 低 | 几何公开写口未完全封闭：`EdgeLayout.geometry` 仍 `pub`，`PathGeometry::polyline_points_mut()` 仍 `pub` | `types.rs:135, 262` |
| P2-4 | 低 | `PLOTGRAM_SKIP_REFINE` 是唯一残留的、会改变正式行为的 env | `refine/mod.rs:77` |
| P2-5 | 低 | 文档/注释与实现脱节：`repair.rs:6-11` 仍写「本 Slice 仅结构就位 + 记账，真正回 solver 留 R10」（E2/E3 后过时）；`audit.rs:7-17` 仍声明 Slice 2 范围；`recipe/orthogonal.rs:8-9` 与 `run.rs:456-467` 仍指「D 段 finalizer / pipeline.rs」，而该层已删除 | 多处 |
| P2-6 | 低 | `PathGeometry` 仍无 piecewise cubic：`RoutePath::Spline` 密采样物化为 `Polyline`（doc16 §10.2 未达成，Slice A–F 未覆盖，属遗留） | `types.rs:69-82` |
| P2-7 | 低 | OVG `overlay_map.iter()` 参与 Dijkstra 松弛顺序，同代价时可能影响路径选择（当前 det 全绿，但属确定性隐患） | `visibility_graph.rs:426` |

---

## 5. 建议的下一步

### 5.1 先补两个「零成本收口」，再动算法

这两项不改任何算法，风险极低，但直接消掉 P0-3 与 P2 一片：

1. **接线 `RoutingConfig`**：`LayoutPlan` → `runner` → `Coordinator` → `PreparedRoutingInput.config` → Recipe → `OrthoConfig.routing`。删掉两处 `Default::default()` 占位。同时把 `PLOTGRAM_SKIP_REFINE` 变成 `RoutingConfig.skip_refine`。完成后 `problem_signature` 才真正可复现一次运行（doc16 §25 第 14 项）。
2. **按 AGENTS §1 删旧层**：删除 4 个 legacy router 结构体、`straighten.rs`、`sanitize.rs` 的 4 个死函数、`phase_straighten_align`、`refine/mod.rs` 的 `skip_push` 死分支。目标把 33 个 never-used function 压到个位数。顺手修正 P2-5 的过时模块注释——当前注释会主动误导下一个读者（例如仍指向已删除的「D 段 finalizer」）。

### 5.2 Slice C 仍是唯一的关键路径，且拆法要改

doc20 §7 判断「下一刀是 Slice A」已被本轮验证为正确。现在同样的逻辑指向 C：**P0-2（Prepared/Contract 未成主数据流）、P1-1（intent 无处可交）、§25 第 6/7/8 项都是 C2 未完成的连带**，绕开 C 去补 B 或 E 只会再造一层影子模型。

但建议调整 C 的拆分粒度——本轮 C 的失败模式很清楚：一次性要求「solve 不出现 EdgeLayout」跨度过大，实际落成了「保留桥接 + 在末尾 lift 回 RouteSolution」的折中，反而新增了一次反向转换。建议改为**从后往前逐段掐断桥接**：

- **C2-α**：把 `phase_route_edges` 移入 `PathAssignmentSolver`，让 initial path 与 rip-up 共享同一份 `paths` 状态；`hints.route_solution.paths` 改为直接来自 solver 的 `paths` 变量（删掉 `run.rs:476-480` 的 `lift_geometry` 回构）。此步不动 lane/bundle，桥接点仅向后平移。
- **C2-β**：候选代价拆成 `(hard_tuple, quality_tuple)` 双元组比较，rip-up 接受准则改为全局 lexicographic score + best hard-feasible 快照。把已存在但仅 `#[cfg(test)]` 的 `global_score` 转正。
- **C3-α**：`solve_lanes` / `solve_bundles` 改为只产 topology/resource assignment，`materialize_lanes` / `materialize_bundles` 的写点合并进 C 末的单次物化。此步之后桥接点就落到了 `run.rs` 末尾一处。
- **C2-γ**：`ResourceGraph` 从只读聚合升级为持有自有 vertex/arc/occupancy 的 IR。这一步最重，且只有在 α/β 完成后才有明确收益，建议最后做。

每步都必须删掉一个写点或一次转换，而不是新增一层。

### 5.3 建议纳入门禁的两个新指标

当前门禁看不见本轮最重要的两个事实，建议补进 `benchmarks/`：

- `residual_hard_edges`（来自 `RouteAuditReport`）：把 §3.3 的 7 图 11 边从「CLI warn」变成可回归的数字。这是比 lint 更严的口径，不纳入采集就会持续暗涨。
- `route_ms` 拆分 `probe / solve / repair_loop / freeze / label`：+16% 耗时的三个来源（重复全量审计、无条件 capture、architecture 双跑）现在只能靠读 perf 日志推断。

### 5.4 关于文档状态

doc16 与 doc20 的「待实施 / 未完成」状态**不建议现在撤销**。可达成的部分建议按本报告 §2 的表格在两份文档中标注逐项状态，但 §25 的第 6/7/8 项（BundleProblem 一等化、唯一 materializer、repair 回 solver）在 Slice C 收敛前都不成立，而这三项正是 doc16 的核心主张。

---

## 6. 一句话总结

Slice A–F 把「什么时候写」这件事做对了——唯一路由边界、唯一冻结点、标签唯一最后写者、固定轮次 repair 且不静默冒充成功，这些都有 grep 与运行时日志支撑，且旧层是真删而非包装。剩下的全部难度集中在「谁来写」：正交内核仍以 `EdgeLayout` 桥接为中心，导致 `PreparedRoutingInput` / `RoutingContract` / repair intent 三套新模型都还没有真正的消费者。下一阶段应当只做 Slice C，并且改用「逐段掐断桥接」的拆法。
