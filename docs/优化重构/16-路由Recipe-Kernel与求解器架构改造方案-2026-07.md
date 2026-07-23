# 16 - 路由 Recipe / Kernel 与求解器架构改造方案

> 日期：2026-07-24  
> 状态：待实施  
> 范围：`straight`、`bezier`、`spline`、`circular`、`orthogonal`、`organic`，以及 self-loop、parallel/reverse edge、bundle、label  
> 目标：把当前“每个 router 直接改写 LayoutResult + 公共 pipeline 多轮补救”的模型，改造成“稳定输入 + 路由配方 + 共享内核 + 专用求解器 + 统一生命周期”。  
> 前置阅读：  
> - [`15-布局模型落地审计与后续提高方案-2026-07.md`](./15-布局模型落地审计与后续提高方案-2026-07.md)  
> - [`12-多图类型布局共享内核与独立配方架构-2026-07.md`](./12-多图类型布局共享内核与独立配方架构-2026-07.md)  
> - [`布局与路由核心手册-2026-07.md`](../总结经验/布局与路由核心手册-2026-07.md)

---

## 0. 一句话方案

路由不适合照搬坐标求解器的“一次凸优化”，但适合复用同一架构原则：

> **Recipe 编译路由语义和目标，Kernel 提供稳定算法能力，多个专用 Solver 分阶段求解，RoutingCoordinator 管理有界反馈与冻结。**

目标主链：

```text
FrozenNodeProduct + RoutingContract + RoutingPlan
  → PreparedRoutingInput
  → RoutingRecipe.compile
      → RoutingProblem
      → PortProblem
      → PathProblem
      → Lane/BundleProblem
      → LabelProblem（几何冻结后才实例化）
  → RoutingSolver
      → port assignment
      → path skeleton solve
      → bounded rip-up/reroute
      → lane/bundle solve
      → geometry materialize
  → RouteAuditor
  → geometry freeze
  → LabelSolver
  → RoutedLayout
  → atomic canvas transform
```

最重要的规则：

1. Router 只消费冻结节点和组，不拥有节点写权。
2. “为什么这样走”属于 Recipe/Contract；“如何寻找最优路径”属于 Kernel/Solver。
3. hard constraint、soft objective、degraded fallback 必须显式建模。
4. sanitize 不再承担纠正上游语义错误的职责。
5. label 是几何冻结后的独立求解问题。
6. annotation 与几何同源产生，不从最终折点反猜语义。
7. 正交路由的全局问题采用确定性的“逐边最短路 + 有界 rip-up/reroute”，不伪装成可一次精确求解的凸问题。

---

## 1. 当前路由架构审计

### 1.1 对外接口仍是旧式可变黑盒

当前 `EdgeRoutingStrategy` 的核心签名是：

```text
route(&Diagram, LayoutResult) → LayoutResult
```

附加接口：

- `route_after_node_moves`
- `route_preserve`
- `supports_refine`
- `needs_obstacle_index`
- `edge_snap_config`

问题：

1. Router 可以修改 nodes、groups、edges、hints 的任何字段。
2. 输入不是冻结路由事实，而是整个可变产品。
3. 增量路由通过“保留哪些 edge index”表达，缺少依赖关系和版本。
4. route family、diagram 语义 profile、后处理能力混在一个 trait。
5. pipeline 必须通过布尔能力猜测要执行哪些后续阶段。

### 1.2 正交 router 已经变成第二条巨型 pipeline

`routing/edge_routing_orthogonal/run.rs` 当前负责：

1. group routing context。
2. obstacle 和 shape polygon。
3. feedback side。
4. corridor plan / pressure。
5. difficulty score / edge order。
6. port + slot。
7. channel plan / OVG。
8. initial route。
9. two-round congested reroute。
10. deferred OVG reroute。
11. port correction。
12. conflict reroute。
13. lane assignment。
14. semantic trunk merge。
15. monitor feedback reroute。
16. annotation freeze。
17. sanitize。
18. monitor escape repair。
19. local trunk merge。
20. contract diagnostics。

随后 `pipeline/runner.rs` 继续执行：

1. edge repulse。
2. group frame 恢复与节点重路由。
3. algorithm-specific post-route hook。
4. SpaceBudget guard。
5. architecture 专属节点重申。
6. 再次重路由。
7. snap + repulse。
8. sanitize_ext。
9. reverse-pair gap / dock。
10. architecture exact stub occupancy。
11. through repair。
12. group interior repair。
13. trunk overlap separation。
14. crossing reduction。
15. lint recheck。
16. polyline freeze。
17. label resolve / merge dedupe。

这说明当前“phase”虽已拆文件，但仍缺少：

- 稳定 IR；
- 统一状态机；
- solver 的全局接受准则；
- 单一几何最终写者；
- 可复用的路由内核边界。

### 1.3 简单 router 之间仍有重复和行为分叉

straight / bezier / spline 已复用 `routing_skeleton` 的部分能力，但 circular / organic 仍维护自己的：

- endpoint 规则；
- parallel lane；
- obstacle 构建；
- curve fallback；
- label finalization；
- self-loop 接入。

当前还存在行为不一致：

- 一些 router 内部提前做 label avoidance。
- orthogonal 把 label 推迟到 pipeline 最后。
- circular 使用 RadialPlacer。
- mindmap 直接清空 label。
- bezier/circular/organic 穿障时可能把 geometry family 改成 Polyline。
- `supports_refine` 以输出是否可能变成 Polyline 来间接控制生命周期。

### 1.4 路由配置依赖大量环境变量

当前 orthogonal 相关环境开关包含：

- edge order score
- channel planner
- OVG
- port solver
- port pressure
- corridor soft
- away penalty
- stub exit penalty
- 多种 debug 开关

问题不是 debug 开关本身，而是正式算法能力也通过环境变量启停，导致：

- `RoutingPlan` 不能完整描述一次运行。
- problem signature 不完整。
- benchmark 结果难复现。
- WASM / playground 与 CLI 行为可能不一致。
- Recipe 无法审计“为什么选择了该路径”。

建议正式能力全部进入 typed config；环境变量只保留日志/trace 开关。

### 1.5 写权仍然分散

当前 edge geometry 的写者包括：

- 初始 router。
- orthogonal port correction / reroute / lane / trunk / sanitize。
- post-route snap / repulse。
- through/group/trunk repair。
- crossing reduction。
- reverse pair enforcement。

annotation 也需要在每次几何变化后手工 refresh。label 在一些 router 内写，随后又可能被 sanitize 或 pipeline 重建。

根因：

> 当前阶段在直接改产品，而不是在修改一个可审计的路由解模型。

---

## 2. 设计原则

### 2.1 路由 Recipe 的两个输入维度

路由不能只按 diagram type，也不能只按 geometry family 建模。它有两个正交维度：

1. **Geometry family**
   - Straight
   - Bezier
   - Spline
   - Circular
   - Orthogonal
   - Organic

2. **Semantic contract**
   - flow direction
   - feedback edge
   - same-layer edge
   - monitor/business role
   - group/scope transit
   - corridor chain
   - merge/bundle group
   - mindmap depth/parent side
   - circular cluster
   - label policy

因此正确设计不是：

```text
if diagram_type == Architecture in OrthogonalKernel
```

而是：

```text
LayoutRecipe / semantic compiler
  → RoutingContract（typed roles/intents）

RoutingPlan
  → geometry family + numeric config

RoutingRecipe
  → 消费 Contract + Plan，编译 RoutingProblem
```

### 2.2 Contract 表达“为什么”，Profile 只表达“数值”

`RoutingContract` 应包含：

- edge role
- scope path
- hard/soft group transit
- port constraints/preferences
- corridor/gutter resource
- bundle/merge semantics
- topology metadata

`RoutingProfile` 只包含：

- clearance
- bend cost
- lane pitch
- crossing cost
- max repair rounds
- curve tension
- sampling tolerance

如果一个 bool 决定是否执行 architecture 业务链，它不应放在通用 profile；应编译成 typed intent。

### 2.3 功能核心，命令外壳

Kernel：

```text
input model → output model + diagnostics
```

不得：

- 读取 DiagramType。
- 读取图名。
- 修改 LayoutResult。
- 启动 layout re-solve。
- 依赖环境变量决定正式算法。

Coordinator：

- 准备路由输入。
- 选择 Recipe。
- 管理固定轮次的 solve/repair。
- 建立 geometry freeze。
- 调 label solver。
- 物化最终 DTO。

### 2.4 hard constraint 与 soft objective 分离

正交路由硬约束优先级：

```text
H0 端点存在、路径连续
H1 端点落在正确节点边界，stub/approach 合法
H2 不穿非端点节点
H3 按 RoutingContract 不穿无关 group interior
H4 保持 geometry family 的几何合法性（orthogonal 必须轴对齐）
H5 显式 corridor/port/pin 契约
```

软目标按词典序：

```text
Q1 degraded 数量 / hard repair 数量
Q2 crossing / exact overlap / illegal tight spacing
Q3 congestion / channel overflow / port crowding
Q4 bend / away segment / route length
Q5 symmetry / stability / visual consistency
```

不要把 H2/H3 仅表示成一个很大的 crossing penalty。

### 2.5 有界启发式，不用时间停机

多边正交路由是 multi-commodity path problem，精确全局最优通常不可行。推荐：

- 固定 edge order。
- 固定 candidate order。
- 固定 rip-up rounds。
- 固定 local improvement rounds。
- 固定 tie-break。
- 保存 best hard-feasible solution。

禁止：

- “跑 20ms 后停止”。
- 随机打乱边顺序。
- HashMap 迭代驱动。

---

## 3. 目标总体架构

```text
                    ┌──────────────────────────┐
FrozenNodeProduct ─▶│ PreparedRoutingInput     │
RoutingContract  ──▶│ stable edges/obstacles   │
RoutingPlan      ──▶│ scopes/resources/hints   │
                    └─────────────┬────────────┘
                                  │
                    ┌─────────────▼────────────┐
                    │ RoutingRecipe.compile     │
                    │ family-specific semantics │
                    └─────────────┬────────────┘
                                  │
                    ┌─────────────▼────────────┐
                    │ RoutingProblem            │
                    │ ports / paths / resources │
                    │ bundles / objectives      │
                    └─────────────┬────────────┘
                                  │
       ┌──────────────────────────┼───────────────────────────┐
       │                          │                           │
┌──────▼───────┐          ┌──────▼────────┐          ┌───────▼──────┐
│ PortSolver   │          │ PathSolver     │          │ Lane/Bundle  │
│ matching/DP  │          │ shortest path  │          │ coloring/DP  │
└──────┬───────┘          │ + rip-up       │          └───────┬──────┘
       │                  └──────┬────────┘                  │
       └─────────────────────────┼────────────────────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │ GeometryMaterializer     │
                    │ path + annotation        │
                    └────────────┬────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │ RouteAuditor / Repair    │
                    │ bounded, edge-only       │
                    └────────────┬────────────┘
                                 │
                         FrozenRouteGeometry
                                 │
                    ┌────────────▼────────────┐
                    │ LabelSolver              │
                    └────────────┬────────────┘
                                 │
                           RoutedLayout
```

---

## 4. 核心数据模型

### 4.1 PreparedRoutingInput

```rust
pub struct PreparedRoutingInput<'a> {
    pub nodes: &'a FrozenNodeStore,
    pub groups: &'a FrozenGroupStore,
    pub edges: &'a StableEdgeStore,
    pub contract: &'a RoutingContract,
    pub direction: LayoutDirection,
    pub canvas: RoutingCanvas,
    pub previous: Option<&'a FrozenRoutingSolution>,
}
```

要求：

1. nodes/groups 无 mutable API。
2. edges 按稳定 `EdgeId` 连续存储。
3. 不直接借用整个 Diagram。
4. AST 属性在 prepare/contract compiler 阶段解析完。
5. previous 只用于增量稳定目标，不改变语义。

### 4.2 RoutingContract

```rust
pub struct RoutingContract {
    pub edge_roles: Vec<EdgeRoleSet>,
    pub port_intents: Vec<PortIntent>,
    pub transit_intents: Vec<TransitIntent>,
    pub corridors: Vec<CorridorResource>,
    pub side_gutters: Vec<SideGutterResource>,
    pub merge_intents: Vec<MergeIntent>,
    pub circle_membership: Vec<Option<CircleId>>,
    pub topology: RoutingTopologyMetadata,
    pub label_policy: LabelPolicy,
}
```

示例 role：

```rust
pub enum EdgeRole {
    Forward,
    Feedback,
    SameLayer,
    CrossScope,
    Monitor,
    Business,
    Pendant,
    SelfLoop,
    ParallelGroup(ParallelGroupId),
}
```

`EdgeRole` 必须来自声明语义、layout topology 或 Recipe compiler，不得从路径点数反猜。

### 4.3 RoutingPlan

```rust
pub struct RoutingPlan {
    pub family: RoutingFamily,
    pub config: RoutingConfig,
    pub incremental: IncrementalPolicy,
    pub trace: TracePolicy,
}
```

用户显式 `edge_routing` 只覆盖 family/config；已有 RoutingContract 语义仍保留。若 family 不支持某个 hard contract，compile 阶段返回明确错误或选择有理由的 degraded capability。

### 4.4 RoutingProblem

```rust
pub struct RoutingProblem {
    pub edges: Vec<EdgeDemand>,
    pub endpoint_candidates: Vec<EndpointCandidateSet>,
    pub obstacles: ObstacleModel,
    pub resources: RoutingResourceGraph,
    pub hard: Vec<RouteConstraint>,
    pub objectives: RouteObjectiveSet,
    pub bundles: Vec<BundleProblem>,
    pub stable_order: Vec<EdgeId>,
    pub config: RoutingSolverConfig,
}
```

`RoutingProblem` 是可序列化/可诊断的 IR，不包含 `&mut EdgeLayout`。

### 4.5 RouteSolution

```rust
pub struct RouteSolution {
    pub ports: Vec<EndpointAssignment>,
    pub paths: Vec<RoutePath>,
    pub lanes: Vec<LaneAssignment>,
    pub bundles: Vec<BundleSolution>,
    pub annotations: RouteAnnotationSet,
    pub diagnostics: RoutingDiagnostics,
    pub score: RouteScore,
}
```

路径内部使用 family-neutral skeleton：

```rust
pub enum RoutePath {
    Straight(StraightPath),
    Orthogonal(OrthogonalPath),
    Cubic(CubicPath),
    Spline(SplinePath),
    Radial(RadialPath),
    Empty(EmptyRouteReason),
}
```

不要用“Bezier 穿障后偷偷变 Polyline”而不记录原因。geometry family 改变时必须产生：

```text
DegradedReason::ObstacleFallback {
    requested: Bezier,
    actual: OrthogonalSkeleton/Spline
}
```

### 4.6 typestate

```text
PreparedRoutingInput
  → RoutingDraft
  → SolvedRouteTopology
  → MaterializedRouteGeometry
  → AuditedRouteGeometry
  → FrozenRouteGeometry
  → LabeledRouteProduct
```

写权：

- topology solve 可改 port/path/resource assignment。
- materializer 唯一写 geometry。
- auditor 只读。
- repair 只能回到 solver model，不能直接改最终 points。
- geometry freeze 后只允许 label/annotation metadata 补全。

---

## 5. Recipe、Kernel、Solver、Coordinator 的职责

### 5.1 RoutingRecipe

```rust
pub trait RoutingRecipe {
    type Draft;

    fn name(&self) -> &'static str;

    fn compile(
        &self,
        input: &PreparedRoutingInput,
        kernels: &RoutingKernels,
    ) -> Result<Self::Draft, RoutingError>;

    fn solve(
        &self,
        draft: &mut Self::Draft,
        kernels: &RoutingKernels,
    ) -> Result<RouteSolution, RoutingError>;

    fn materialize(
        &self,
        solution: &RouteSolution,
        kernels: &RoutingKernels,
    ) -> Result<MaterializedRouteGeometry, RoutingError>;
}
```

不建议增加 `before_x/after_y` hook。family-specific 顺序直接写在具体 Recipe 中。

### 5.2 RoutingKernels

建议共享能力：

```text
IdentityKernel
EndpointKernel
ObstacleKernel
VisibilityKernel
CorridorKernel
CandidateKernel
ShortestPathKernel
ConflictKernel
LaneKernel
BundleKernel
CurveKernel
GeometryKernel
LabelKernel
AuditKernel
IncrementalKernel
```

实际 Rust 可使用模块函数，不要求构造大对象。

### 5.3 RoutingSolver

“RoutingSolver”不是一个万能算法，而是一组稳定后端：

```text
PortAssignmentSolver
PathAssignmentSolver
ConflictRepairSolver
LaneAssignmentSolver
BundleSolver
CurveControlSolver
LabelPlacementSolver
```

各 solver 消费窄 IR，输出结构化解与 diagnostics。

### 5.4 RoutingCoordinator

统一生命周期：

```text
prepare
  → recipe.compile
  → recipe.solve
  → materialize
  → route audit
  → 如失败：编译 repair intents，固定轮次重解
  → geometry freeze
  → label candidates
  → label solve
  → final audit
  → product
```

Coordinator 不知道：

- architecture monitor。
- flowchart feedback hub。
- mindmap root。
- circular cluster。

它只知道：

- hard audit 是否通过。
- repair 是否还有轮次。
- geometry 是否已冻结。
- label solver 只能消费冻结几何。

---

## 6. PortAssignmentSolver

### 6.1 为什么端口应独立求解

当前正交端口经历：

```text
choose side/slot
  → path build
  → replan
  → port correction
  → stub fix
  → dock separation
```

后续阶段会推翻前面决策，说明端口不是局部几何函数，而是受全节点 incident edges 共同影响的分配问题。

### 6.2 问题模型

对每个 endpoint 建候选：

```text
Candidate {
  node
  side
  slot
  anchor
  capacity_resource
  compatibility
  cost_terms
}
```

硬约束：

1. 每个 endpoint 选择一个 candidate。
2. slot capacity 不超限。
3. fixed port / explicit side 必须满足。
4. endpoint anchor 位于真实 shape boundary。
5. self-loop 的两个 endpoint 满足 loop side 规则。
6. merge intent 允许的 shared anchor 由显式 bundle key 控制。

软目标：

- flow direction。
- source/target relative direction。
- stub outward。
- same-layer consistency。
- fan-in/fan-out order preservation。
- reverse pair separation。
- port pressure balance。
- previous assignment stability。

### 6.3 求解算法

首期不需要通用 ILP。

按 node 分解，每个节点求 incident endpoint 分配：

1. 按 edge role、对端投影位置、EdgeId 稳定排序。
2. 每侧生成离散 slot。
3. 使用 min-cost bipartite matching / min-cost flow。
4. 对要求保持顺序的 fan 使用单调动态规划，禁止交叉 assignment。
5. bundle endpoints 先压缩成共享需求，再展开。
6. 正反向 pair 可增加互斥或最小 slot distance。

tie-break：

```text
total cost
  → side ordinal
  → slot ordinal
  → EdgeId
```

输出不仅是 side/anchor，还包括：

- outward direction。
- protected stub interval。
- bundle key。
- assignment reason。

这样后续 solver 不再重新“修端口”。

---

## 7. PathAssignmentSolver

### 7.1 正交路径图

统一构建 `RoutingResourceGraph`：

节点：

- endpoint stub exits。
- obstacle corners / projections。
- corridor entry/exit。
- side gutter tracks。
- planned channel intersections。
- bundle junction candidates。

弧：

- 水平/垂直可见 segment。
- corridor segment。
- outer-ring segment。
- endpoint stub。
- group border crossing gate。

每条弧携带：

```text
length
bend state
obstacle clearance
group transit permission
channel resource id
current load
crossing candidates
protected trunk relation
```

### 7.2 单边目标

对固定 endpoint assignment，路径成本采用词典序 key：

```text
(
  hard_violation_count,
  group_pierce_count,
  node_pierce_count,
  degraded_class,
  channel_overflow,
  crossing_count,
  exact_overlap,
  tight_spacing,
  bends,
  away_segments,
  length,
  stable_candidate_id,
)
```

硬违规不应用大权重与美学项相加。候选存在 hard-feasible 时，hard-invalid 候选无资格胜出。

### 7.3 初始解

1. edge 按稳定优先级排序：
   - fixed corridor / bundle trunk
   - feedback/long-span
   - cross-scope
   - ordinary forward
   - self-loop 可独立求解
2. 同级按：
   - span
   - estimated difficulty
   - declaration EdgeId
3. 每条边在当前 resource load 下跑 lexicographic Dijkstra/A*。
4. 资源负载使用边际成本：

```text
delta_cost(load → load + demand)
```

而不是仅在最终路径上做 overlap penalty。

### 7.4 全局冲突修复

多边问题采用确定性 rip-up/reroute：

```text
initial route
  → build conflict graph
  → 按 connected component 修复
  → 每轮选择 conflict contribution 最大的边
  → 临时移除其资源占用
  → 重新最短路
  → 只在全局 lexicographic score 改善时接受
  → 固定 MAX_REPAIR_ROUNDS
```

接受准则必须比较完整全局 score，不再用“本边 candidate 非 degraded”作为唯一条件。

保存：

- best hard-feasible solution。
- 每轮 changed edges。
- rejected reason。
- unresolved conflict component。

### 7.5 corridor 与 outer ring

当前 corridor path 和 free-route 是两套分支。新模型中：

- corridor 是带 capacity/permission 的资源子图。
- outer ring 是 canvas 外围资源子图。
- cross-scope contract 决定允许通过哪些 group gates。
- “有 chain 但构造失败”变成明确 infeasible/degraded reason。
- 不再由 `strict_group_transit` bool 在不同函数中重复解释。

### 7.6 OVG 与 channel planner 的统一

当前 candidate builder、OVG、channel planner、corridor plan 有重叠职责。

建议：

1. VisibilityKernel 只生成合法可见弧。
2. CorridorKernel 生成语义资源和 gates。
3. ChannelKernel 可在 resource graph 上增加 preferred track。
4. PathSolver 统一选择。
5. 大图延迟构建通过“按 dirty/conflict component 局部扩图”实现，而不是另走第二套 reroute 代码。

---

## 8. LaneAssignmentSolver

### 8.1 问题定义

同一 channel/resource 上的多个 route interval 需要分 lane。当前 post-hoc offset 可能：

- 推入障碍；
- 改坏端口 stub；
- 引入新 crossing；
- 被 sanitize 合并回去。

新模型应在 path skeleton 固定后、geometry materialize 前求 lane。

### 8.2 算法

1. 对每个 channel 收集 interval：

```text
(start, end, edge_id, direction, bundle_id, required_gap)
```

2. 建 interval conflict graph。
3. 使用确定性 greedy coloring 得到最少或近似最少 lane。
4. lane 以 channel center 对称展开。
5. 对相邻 channel junction，使用小规模 DP/交换优化减少 crossing。
6. reverse pair、parallel group 可施加 lane side intent。
7. lane offset 后由 obstacle auditor 验证；失败回到 lane solver，不能直接移动折点。

### 8.3 capacity feedback

若 lane 数超过 channel capacity：

- 路由 solver 尝试换 channel。
- 若仍失败，产生 `RoutingDemand` 给布局 Coordinator。
- 该 demand 只允许在节点 freeze 之前触发 layout re-solve。
- 节点已冻结后的正式路由不得再推节点。

---

## 9. BundleSolver 与语义合流

### 9.1 为什么不能继续 post-hoc 重写路径

当前 semantic trunk merge 在普通路径完成后重写 suffix，随后还需：

- protected trunk。
- monitor reroute。
- annotation refresh。
- local trunk merge。
- sanitize guard。

这说明 bundle 应成为一等路由问题。

### 9.2 MergeIntent

由上游语义编译：

```rust
pub struct MergeIntent {
    pub id: MergeGroupId,
    pub members: Vec<EdgeId>,
    pub mode: MergeMode,
    pub shared_endpoint: SharedEndpoint,
    pub preserve_prefix: bool,
    pub style_key: EdgeStyleKey,
    pub label_policy: MergeLabelPolicy,
}
```

只有 arrow/style/方向/业务语义兼容的成员才进入同一 intent。

### 9.3 求解方式

FanIn：

1. 求目标侧公共 trunk endpoint。
2. 为 trunk 建虚拟共享 commodity。
3. 先路由 trunk。
4. 每个成员只路由源 stub/prefix 到 junction。
5. prefix 必须保留各自 obstacle/corridor 语义。

FanOut 对偶处理。

同 rank pendant 或 monitor local trunk 使用不同 `MergeMode`，但共享同一 solver 数据模型。

输出 annotation 直接来自 BundleSolution：

- merge interval。
- protected run。
- ownership。
- shared label group。
- degraded members。

禁止从 `points.len()` 或固定 index 反推。

---

## 10. CurveControlSolver

### 10.1 Bezier

BezierRecipe 编译：

- endpoint tangent intent。
- tension range。
- obstacle clearance。
- parallel normal offset。

首期可生成少量确定性控制点候选：

1. direct tangent。
2. stronger shoulder。
3. obstacle-side bow。
4. skeleton-guided cubic。

对候选采样审计，选择：

```text
hard clearance
  → curvature fairness
  → length
  → stability
```

如果必须降级为 skeleton/spline，记录 actual family 与 reason。

### 10.2 Spline

SplineRecipe 应明确分两层：

```text
PathSkeletonSolver
  → obstacle-safe polyline skeleton

CurveControlSolver
  → piecewise cubic controls
```

最终产品最好保存真正的 piecewise cubic，而不是把密采样点写成 `Polyline`。这样可避免：

- refine 把平滑曲线误当折线。
- 大量采样点放大存储和碰撞成本。
- 后处理误用正交 sanitize。

### 10.3 Organic

OrganicRecipe 消费 typed mindmap contract：

- depth。
- parent/child side。
- radial root tangent。
- sibling fan。
- label suppression policy。

Kernel 不直接读取 `DiagramType::Mindmap`。连接点均匀分布可复用 PortAssignmentSolver 的 side-local monotone assignment。

### 10.4 Circular

CircularRecipe 消费：

- `CircleId` membership。
- position index。
- circle center/radius。
- inter-circle relation。

同圆弧侧、parallel/reverse lane 使用 LaneSolver；跨圆路径可调用 Bezier/Spline kernel。不要在 CircularRecipe 内再维护一套独立 parallel grouping。

---

## 11. Self-loop 与 parallel/reverse edge

### 11.1 SelfLoopSolver

self-loop 是独立子问题，不应散落在每个 router 的主循环。

输入：

- node shape。
- occupied ports。
- 邻近 obstacles。
- 其他 loop index。
- requested family。
- flow direction。

候选：

- top/right/bottom/left loop。
- orthogonal box loop。
- curved loop。
- group-border constrained loop。

目标：

- 不穿邻居和 group。
- 避开已占用 port。
- 多 self-loop 分层展开。
- 箭头方向自然。
- label 有独立候选空间。

输出统一 `RoutePath + EndpointAssignment + annotation`。

### 11.2 ParallelGroup

parallel/reverse edge 分组只构建一次，生成：

- stable group id。
- canonical direction。
- style compatibility。
- endpoint sharing policy。
- lane side preference。
- label offset class。

各 geometry family 复用同一事实，不再分别实现 pair grouping。

---

## 12. GeometryMaterializer 与 sanitize

### 12.1 唯一几何写者

Solver 输出 topology/path resources，Materializer 一次生成：

- endpoint anchor。
- points/controls。
- port。
- annotation。
- geometry fingerprint。

后续不应再有十几个 pass 直接改 points。

### 12.2 将 sanitize 拆成两类

#### Topology-preserving canonicalization

可以在 materialize 内执行：

- 删除重复点。
- 合并严格共线点。
- 归一化零长度段。
- 规范浮点误差。
- 保持 protected run 和 merge interval。

#### Topology-changing optimization

例如：

- overshoot merge。
- Z 折拉直。
- lane shift。
- crossing reroute。
- outer escape。

必须回到 solver/repair model，在完整 obstacle/contract 下重新评分。不得由无障碍语义的 sanitize 猜测。

### 12.3 snap

正交 snap 应进入 route coordinate domain：

1. channel/lane 坐标优先使用 grid-compatible 候选。
2. final quantization 后完整 route audit。
3. snap 破约则回滚该候选或进行固定轮次局部 re-solve。
4. 不在 geometry freeze 后 repulse/sanitize。

### 12.4 annotation 同源

Materializer 根据 solution 中的语义对象直接生成 annotation。任何合法 rigid transform 必须通过：

```text
RoutedLayout::translate_all(dx, dy)
```

同步变换：

- nodes
- groups
- paths/controls
- labels
- stub/protected runs
- merge intervals
- occupancy/corridor geometry

---

## 13. RouteAuditor 与 repair

### 13.1 审计层次

#### Problem validation

- stable id 完整。
- edge count 与 relation identity 一致。
- candidate 非空或有 explicit empty reason。
- config finite。
- contract 引用合法。

#### Geometry hard audit

- path continuity。
- endpoint boundary。
- port/stub/approach。
- orthogonality/curve continuity。
- node interior。
- group interior。
- corridor gate。
- merge annotation 一致。
- geometry/annotation transform 一致。

#### Quality audit

- crossing。
- exact overlap。
- tight spacing。
- congestion。
- bends。
- away segments。
- total length。
- unnatural port。
- label overlap。

### 13.2 repair 模型

auditor 不直接修 points，只产生：

```rust
pub struct RouteRepairIntent {
    pub affected_edges: Vec<EdgeId>,
    pub violated_constraints: Vec<RouteConstraintId>,
    pub forbidden_resources: Vec<ResourceId>,
    pub required_clearance: Vec<ClearanceDemand>,
    pub priority: RepairPriority,
}
```

Coordinator 将 intent 交回 Recipe/Solver，最多固定轮次。

如果最终仍失败：

- 保留 best hard-feasible solution。
- 若不存在 hard-feasible，返回显式 degraded/infeasible report。
- 不静默保留穿组路径并冒充成功。

---

## 14. LabelPlacementSolver

### 14.1 时序

```text
route solve
  → materialize
  → snap/canonicalize
  → route audit
  → geometry freeze
  → label candidates
  → label solve
  → merge dedupe
```

label solve 后禁止再改 route geometry。

### 14.2 LabelProblem

每个 label 生成候选：

- path parameter `t`。
- segment/radial anchor。
- tangent/normal offset。
- side。
- bbox。
- merge ownership。

硬约束：

- 不盖 node。
- 不越 forbidden group。
- 不盖 arrowhead/endpoint。
- 显式 merge dedupe。

软目标：

- label-label overlap。
- 距默认 t 的偏差。
- leader line 成本。
- 与对应 edge 的归属清晰度。
- parallel/reverse edge 分侧。

### 14.3 求解算法

Label placement 同样是离散冲突问题：

1. 构建 candidate conflict graph。
2. label 按候选少、优先级高、EdgeId 稳定排序。
3. deterministic greedy 初始化。
4. 固定轮次局部交换/重放。
5. 保存 best assignment。

对于小 conflict component 可使用 branch-and-bound；设固定节点数/搜索节点上限，不使用时间上限。

---

## 15. 各路由算法如何接入

### 15.1 StraightRecipe

调用：

- EndpointKernel。
- ParallelGroup/LaneKernel。
- StraightPath materializer。
- LabelSolver。

若 hard obstacle contract 无法满足，必须明确：

- 返回 infeasible；或
- 按 RoutingPlan 允许的 fallback 切换到 Spline/Orthogonal。

### 15.2 BezierRecipe

调用：

- PortAssignmentSolver。
- CurveControlSolver。
- Obstacle audit。
- 必要时 skeleton-guided fallback。
- LabelSolver。

### 15.3 SplineRecipe

调用：

- PortAssignmentSolver。
- Visibility/ShortestPathKernel。
- CurveControlSolver。
- Curve audit。
- LabelSolver。

### 15.4 CircularRecipe

调用：

- Circular contract compiler。
- Parallel/LaneSolver。
- Radial/Bezier curve kernel。
- inter-circle skeleton fallback。
- Radial label candidate builder + LabelSolver。

### 15.5 OrganicRecipe

调用：

- Mindmap semantic contract。
- side-local PortAssignmentSolver。
- Organic CurveControlSolver。
- sibling fan obstacle policy。
- label policy。

### 15.6 OrthogonalRecipe

显式编排：

```text
compile
  1. edge roles / scope transit
  2. port candidates
  3. obstacle/resource graph
  4. corridor/gutter resources
  5. merge/bundle problems
  6. objective profile

solve
  7. PortAssignmentSolver
  8. bundle trunk pre-plan
  9. initial PathAssignmentSolver
 10. conflict graph
 11. bounded rip-up/reroute
 12. LaneAssignmentSolver
 13. BundleSolver finalize branches
 14. global score/audit

materialize
 15. orthogonal points + annotations
 16. topology-preserving canonicalize
 17. grid-aware finalize
```

pipeline 不再含 orthogonal 专属 D 段。

---

## 16. 与布局模型的时序契约

### 16.1 完整生命周期

```text
LayoutRecipe.compile
  → LayoutRecipe.solve
  → NodeLayoutProduct
  → RoutingDemandProbe（只读、粗粒度）
  → 如需空间：SpacingDemand → LayoutRecipe re-solve（≤ 2）
  → Group/Node hard audit
  → FrozenNodeProduct
  → RoutingRecipe.compile
  → RoutingRecipe.solve/repair（只改边解）
  → FrozenRouteGeometry
  → LabelSolver
  → final canvas transform
```

### 16.2 DemandProbe 与正式 Router 分离

布局 feedback 阶段不应运行整套最终 router 再推节点。建议 `RoutingDemandProbe` 只估计：

- incident port capacity。
- self-loop outer band。
- adjacent rank edge band。
- cross-scope corridor capacity。
- label minimum band。

输出 `SpacingDemand`，不生成最终 geometry。

正式 Router 只在节点 freeze 后运行。

### 16.3 Group frame

group frame 必须在路由输入前稳定：

- architecture semantic group frame 是 NodeLayoutProduct 的一部分。
- decorative group 也必须在 freeze 前确定。
- Router 只读取 group obstacles/corridor gates。
- route 后不得通过恢复 group frame 移动节点。

### 16.4 refine 的新定义

删除当前“refine 可推节点”的语义。

新 refine 只允许：

- 生成 route repair intent。
- 局部重解 affected route component。
- 改 route topology/lanes，但必须在 geometry freeze 前。

节点空间不足时返回 `UnresolvedRoutingDemand`，由上层决定是否在下一次完整 layout run 中吸收，不能在 route 阶段越权推点。

### 16.5 canvas finalize

推荐在 route/label 后计算完整 bounds，再执行唯一刚体变换：

```text
FinalLayoutTransform::translate_all
```

如果节点 freeze 语义要求绝对坐标不可变，则把“刚体 transform”建模为产品坐标系变换，不开放单节点 mutation。

---

## 17. 增量路由

### 17.1 稳定 identity

不要再以 `usize` relation index 作为跨版本唯一身份。使用：

```text
StableEdgeId = declaration identity + parallel ordinal
```

运行期仍可映射到连续 EdgeId。

### 17.2 依赖图

记录：

```text
edge → endpoint nodes
edge → traversed resources
edge → nearby obstacles
edge → corridor/group gates
edge → bundle
edge → conflicting edges
label → route segments
```

输入变化后得到 dirty set：

- endpoint node moved/resized。
- obstacle changed。
- contract changed。
- resource occupancy affected。
- bundle member changed。

### 17.3 增量求解

1. 未 dirty 的 route 作为 fixed occupancy。
2. 先验证 preserved route 对新 obstacles 仍合法。
3. dirty edges + 直接 conflict neighbors 形成局部 component。
4. 固定顺序重解。
5. 若局部不可行，扩大一跳；达到固定上限后回退全图。
6. 比较 stability objective，避免无关边抖动。

这取代当前仅以 `moved_node_ids` 或 `preserve_edges: HashSet<usize>` 驱动的增量接口。

---

## 18. 确定性与数值约束

1. StableNodeId/EdgeId/ResourceId 连续编号。
2. HashMap 只查询，不驱动。
3. obstacle、candidate、channel、conflict component 显式排序。
4. Dijkstra heap tie-break 包含 stable vertex/path id。
5. 所有 local improvement 固定轮次。
6. 曲线采样数或误差阈值固定，不按耗时调整。
7. 浮点比较使用统一 epsilon 和 total ordering wrapper。
8. route score 固定累加顺序。
9. problem signature 包含正式 config 和 contract。
10. 禁止随机 edge order。
11. 禁止 `std::time::{Instant, SystemTime}` 作为 WASM 路径算法控制。

---

## 19. 建议目录结构

```text
layout/
  routing/
    model/
      prepared.rs
      contract.rs
      problem.rs
      solution.rs
      resource.rs
      diagnostics.rs
      typestate.rs

    coordinator/
      mod.rs
      repair.rs
      incremental.rs
      finalize.rs

    kernel/
      endpoint/
      obstacle/
      visibility/
      corridor/
      candidate/
      shortest_path/
      conflict/
      lane/
      bundle/
      curve/
      geometry/
      label/
      audit/

    solver/
      port.rs
      path.rs
      repair.rs
      lane.rs
      bundle.rs
      curve.rs
      label.rs

    recipes/
      straight.rs
      bezier.rs
      spline.rs
      circular.rs
      organic.rs
      orthogonal/
        mod.rs
        compile.rs
        solve.rs
        materialize.rs

    common/
      self_loop.rs
      parallel.rs
      annotation.rs
```

迁移初期不必先移动所有文件。先让现有代码通过窄 API 产出/消费新 IR，再移动目录。

---

## 20. 分阶段迁移计划

### R0：写权地图与基线

目标：不改行为，固定现状。

任务：

1. 枚举 nodes/groups/edge geometry/label/annotation 最终写者。
2. 为现有每个 post-route pass 标记未来归属：
   - compile intent
   - solver
   - materializer
   - auditor
   - delete
3. 记录 orthogonal 当前 phase 输入输出。
4. 建 route/annotation/node fingerprint。
5. 记录 hard correctness 与质量指标。

退出判据：

- 每个直接改 points 的函数都有唯一未来归属。

### R1：建立 PreparedRoutingInput 与 RoutingContract

目标：router 不再直接解析完整 Diagram/LayoutResult。

任务：

1. StableEdgeId。
2. FrozenNode/Group store adapter。
3. 抽取 edge roles、scope transit、corridor、merge intent。
4. 现有 router 通过 legacy adapter 消费新输入。
5. 正式配置从 env 移入 RoutingPlan。

退出判据：

- problem signature 可完整描述一次 route run。
- Kernel 不读取 diagram type。

### R2：建立 RouteSolution / Materializer / Auditor

目标：分离“解”与“最终几何”。

任务：

1. 路径、port、lane、annotation 进入 RouteSolution。
2. GeometryMaterializer 成为 points/controls 唯一写者。
3. 完整 hard auditor。
4. label 从 geometry materialization 中拆出。
5. 引入 geometry typestate。

退出判据：

- geometry freeze 后没有 pass 改 points。

### R3：迁移简单 Recipes

顺序：

1. Straight。
2. Bezier。
3. Spline。
4. Circular。
5. Organic。

目的：

- 用简单算法稳定 Recipe/Kernel 接口。
- 统一 endpoint、parallel、自环、obstacle、label。
- 删除各 router 的重复 skeleton/finalize。

退出判据：

- 这些 router 不再实现 `route(LayoutResult) → LayoutResult`。

### R4：OrthogonalDraft 与 legacy solver adapter

目标：先把巨型 run 编译成结构化 Draft，不立即改算法。

Draft 至少包含：

```text
edge roles
port candidates
endpoint map
obstacles
corridor/resources
edge order
merge intents
config
```

现有 phase 暂时作为 `LegacyOrthogonalSolver` 消费 Draft。

退出判据：

- compile 可独立检查。
- `run.rs` 不再自行准备所有事实。

### R5：PortAssignmentSolver

目标：删除：

- slot replan。
- port correction。
- reverse stub port fix 中的端口重选。
- D 段 dock 端口补救。

保留 geometry 相关 stub audit，但端口 assignment 单一写者。

### R6：统一 ResourceGraph + PathAssignmentSolver

目标：

1. candidate/OVG/channel/corridor 统一。
2. initial shortest path。
3. conflict graph。
4. bounded rip-up/reroute。
5. best global score。
6. degraded typed reason。

退出判据：

- two-round、deferred OVG、conflict reroute 不再是三套控制流。

### R7：Lane 与 Bundle 一等求解

目标：

1. lane 在 materialize 前求解。
2. semantic merge 编译为 BundleProblem。
3. monitor local trunk 使用 MergeMode。
4. annotation 由 solution 同源生成。
5. 删除 post-hoc trunk rewrite 与 annotation refresh 链。

### R8：收口 repair / sanitize / snap

目标：

1. through/group/trunk/crossing repair 变成 repair intent。
2. topology-changing sanitize 回到 solver。
3. finalizer 只做 topology-preserving canonicalize。
4. snap 后 hard audit。
5. 删除 pipeline orthogonal D 段。

### R9：LabelSolver

目标：

1. 所有 family 在 geometry freeze 后统一生成 label candidates。
2. geometry-specific candidate builder 可不同。
3. shared conflict solver。
4. merge dedupe 成为 hard label contract。

### R10：RoutingCoordinator 与冻结类型

目标：

1. 删除旧 `EdgeRoutingStrategy` mutable 接口。
2. Router 只消费 FrozenNodeProduct。
3. 固定 repair rounds。
4. 增量依赖图。
5. final atomic transform。
6. 删除 NodeFreeze/PolylineFreeze 的重复旁路守卫；typestate 成为主约束。

### R11：删除旧层

删除：

- route_after_node_moves。
- route_preserve 旧接口。
- supports_refine。
- needs_obstacle_index 调度 bool。
- post-route algorithm hooks。
- SpaceBudget guard 推节点。
- orthogonal pipeline 专属分支。
- 正式算法环境开关。
- 仅为旧时序存在的 refresh/repair wrapper。

---

## 21. 验收指标

### 21.1 正确性硬指标

1. `edge_crosses_group_interior = 0` 或仅有明确 contract 允许项。
2. 非端点 node interior crossing = 0。
3. orthogonal 非轴对齐段 = 0。
4. endpoint 不在边界 = 0。
5. approach/stub hard violation = 0。
6. annotation 与 geometry 不一致 = 0。
7. geometry freeze 后 points 修改 = 0。
8. node freeze 后节点修改 = 0。
9. 同输入 deterministic signature 完全一致。

### 21.2 路由质量指标

```text
degraded_edge_count
empty_route_count
crossing_count
exact_overlap_length
tight_spacing_count
channel_overflow
max_channel_load
total_bends
total_manhattan/euclidean_length
away_segment_count
unnatural_port_count
bundle_shared_ink
label_overlap_count
label_node_overlap_count
route_stability_delta
```

### 21.3 求解器指标

```text
port_problem_size
port_assignment_cost
resource_graph_vertices/arcs
candidate_count
dijkstra_expansions
repair_rounds
rerouted_edges
accepted/rejected_repairs
conflict_component_count
lane_count_by_resource
bundle_degraded_count
label_component_size
```

### 21.4 性能

目标复杂度：

- prepare/index：近似 O(V + E)。
- per-node port matching：按节点 incident degree 的小问题。
- initial path：O(E × shortest_path)。
- repair：只处理 conflict components，固定轮次。
- lane：每资源 O(k log k)。
- label：按 conflict component 分解。

性能测量才使用 release；日常实现继续使用 debug。

---

## 22. 风险与控制

### 22.1 过度抽象

风险：把所有 family 强塞进同一个 phase DSL。

控制：

- Recipe 用显式 Rust 编排。
- 共享窄 Kernel 和 IR。
- OrthogonalRecipe 与 OrganicRecipe 可以有不同 solve 顺序。

### 22.2 全局路由求解成本过高

控制：

- 不追求精确 multi-commodity optimum。
- 初始逐边最短路。
- conflict component 局部 repair。
- 固定轮次。
- 大图局部 resource graph。
- 增量依赖图。

### 22.3 hard contract 导致无路

控制：

- compile 时验证 corridor/gate 完整性。
- 明确 infeasible/degraded reason。
- best hard-feasible snapshot。
- 布局前 DemandProbe 预留资源。
- 不通过穿组软降级静默逃逸。

### 22.4 bundle 破坏成员独立语义

控制：

- MergeIntent 显式声明。
- style/direction compatibility。
- preserve_prefix。
- trunk 与 branch 分开求解。
- annotation 同源。

### 22.5 snap 重新制造违规

控制：

- grid-aware resource candidate。
- snap 后 hard audit。
- 失败回滚或局部重解。
- geometry freeze 前完成。

### 22.6 迁移期新旧双写

控制：

- 可双跑比较，不可双写产品。
- 每迁一个阶段立即删除旧最终写者。
- 临时 adapter 只存在于迁移提交，不形成长期 runtime toggle。
- 回滚依赖 git 阶段提交，不依赖永久旧管线。

### 22.7 曲线路由与折线 fallback 混淆

控制：

- RoutePath 保存 actual family。
- degraded reason 显式。
- piecewise cubic 不再密采样写回 Polyline。
- auditor 按 actual family 检查。

---

## 23. 不建议做的事

1. 不把当前 orthogonal phase 列表直接包装成 `Vec<Box<dyn Phase>>`。
2. 不建立可动态插 hook 的万能 Routing DSL。
3. 不用一个巨型加权和混合 hard correctness 与美学。
4. 不继续增加 post-route 直接改 points 的 pass。
5. 不在 route 后推节点。
6. 不在 label solve 后 sanitize geometry。
7. 不从路径点数猜 merge/sharp/round 语义。
8. 不让 Kernel 读取 DiagramType 或算法名。
9. 不用环境变量控制正式算法分支。
10. 不保留 mutable `LayoutResult` 作为 Router API。
11. 不用随机 edge order 或时间预算停机。
12. 不为单张图增加图名/节点名特判。

---

## 24. 推荐首批实施切片

为了降低一次性重写风险，建议第一批只做架构闭环，不立即替换所有正交算法：

```text
Slice 1
  PreparedRoutingInput
  + RoutingContract
  + StableEdgeId

Slice 2
  RouteSolution
  + GeometryMaterializer
  + RouteAuditor

Slice 3
  Straight/Bezier/Spline Recipes
  + unified LabelSolver skeleton

Slice 4
  OrthogonalDraft
  + LegacyOrthogonalSolver adapter

Slice 5
  PortAssignmentSolver

Slice 6
  ResourceGraph + PathAssignmentSolver
```

到 Slice 4 时，架构边界已经可验证；Slice 5–6 才开始替换核心算法。

---

## 25. 最终完成定义

本方案完成时，应满足：

1. Router 只消费 `FrozenNodeProduct + RoutingContract`。
2. 每个 geometry family 有显式 RoutingRecipe。
3. Recipe 编译语义，Kernel 不读取 DiagramType。
4. port、path、lane、bundle、label 都有独立问题模型和 solver。
5. OrthogonalRecipe 使用统一 resource graph 和有界 rip-up/reroute。
6. semantic merge 在求解前是一等 BundleProblem，不再 post-hoc 重写。
7. GeometryMaterializer 是 edge geometry 唯一写者。
8. topology-changing repair 必须回到 solver。
9. geometry freeze 后只解 label，不再改 points。
10. annotation 与 geometry 同源产生并支持原子 transform。
11. layout-route feedback 只在 node freeze 前通过 SpacingDemand 有界重解。
12. group frame、SpaceBudget guard、refine 不再越权移动节点。
13. 增量路由基于 stable identity 与依赖图。
14. 所有正式配置进入 RoutingPlan，环境变量仅用于诊断。
15. 穿组、穿节点、端点、确定性继续作为硬正确性底线。

