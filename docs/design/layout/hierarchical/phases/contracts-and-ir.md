# Hierarchical · 边界契约、IR 与 Stage

> 父页：[architecture](../architecture.md) §2–§3、§9–§10  
> 相关：[composition](composition.md) · [coordinate-and-demand](coordinate-and-demand.md)

## 1. Engine 边界

### 1.1 输入

```text
HierarchicalInput
  graph: Graph
  node_sizes: NodeSizes
  params: HierarchicalParams
  data: HierarchicalLayoutData
  edge_geometry: Builtin | DeferToRouter
```

- `Graph` 只承载跨算法结构事实：节点、边、组树、PartitionGrid、端口约束、edge group。
- `HierarchicalParams` 是图级、已填满默认值的算法参数，无 `Option`。
- `HierarchicalLayoutData` 是元素级 typed 约束；不得从 `attrs` 即席取算法键。
- label/group-title 尺寸若要参与 Demand，必须在输入预算中有真实测量值；只有文本字符串不够。

### 1.2 输出

```text
LayoutOutput
  nodes: NodePlacement[]
  groups: GroupPlacement[]
  edges: EdgePlacement[]
  route_scene: RouteScene?
  diagnostics: LayoutDiagnostics
```

`groups` 是 Metric 真源。Facade 只能：

1. 调可选 EdgeRouter 替换 `EdgePlacement.path`；
2. 拼装不参与布局的 render metadata；
3. 由全部已定几何求 canvas union；
4. 序列化输出。

Facade 不得重新包围节点计算组框，也不得调整节点/组/端口。

## 2. RouteScene

独立 Router 的最低输入：

```text
RouteScene
  node_obstacles: Rect[]
  group_boundaries: GroupBoundary[]
  boundary_permissions: EdgeId → BoundaryCrossing[]
  terminals: EdgeId → { source: PortPoint, target: PortPoint }
```

`BoundaryCrossing` 按原始 source→target 顺序列出 `(group_id, enter|leave, gate_region)`。  
自由位置 OVG Router 可以不用 Hier 的 Channel topology，但必须遵守同一组边界许可。Router 若不支持该 scene，返回 `UnsupportedRouteScene`，不得忽略 groups。

## 3. 稳定 key

### 3.1 实体

```text
NodeKey =
  Real(NodeId)
  Virtual { owner: EdgeId|GroupId, kind: VirtualKind, ordinal: u32 }

VirtualKind =
  LongEdge | PortNorthSouth | GroupBoundary | PartitionBoundary | Label
```

- `ordinal` 由 owner 内稳定遍历产生。
- 临时 arena index 可用于性能，但不得进入 Plan 序列化、tie-break 或诊断身份。
- `GateKey`、`SegmentKey` 同样由语义 owner + ordinal 组成。

### 3.2 集合

- 需要声明序：`IndexMap` 或 `Vec + id→index`。
- 需要 key 序：`BTreeMap/BTreeSet`。
- `HashMap` 只可用于不影响顺序的查找缓存；任何遍历前必须投影到稳定序。

## 4. Plan / Metric 冻结

```text
MutableComposeState --freeze--> Plan --Metric--> Metric
```

- `Plan` freeze 后不暴露 `&mut`。
- `Metric` 只接受 `&Plan`，不能持有回写 Plan 的句柄。
- `Ink` 只接受 `&Plan + &Metric`。
- Debug 可序列化 Plan/Metric；schema 可直接演进，不提供旧版兼容层。

每次 freeze 先运行对应 verifier。失败不产出下游半成品。

## 5. EdgePlan 的双方向

```text
EdgePlan
  original_source / original_target
  working_source / working_target
  reversed
  source_port / target_port
  route_topology
```

FAS 只改 working 方向。以下字段永远按 original 语义：

- source/target port；
- path 点序；
- source-prefix / target-suffix bundle；
- head/tail label；
- 箭头方向。

因此 Ink 无需也不得根据 `reversed` 猜测最终箭头。

## 6. Stage 外壳

推荐包裹顺序：

```text
ValidateInput
  → OrientationIn
  → SelfLoopExtract
  → ParallelEdgeGroup
  → ConstraintCoupledComponents
  → HierarchicalCore
  → ComponentArrange
  → ParallelEdgeRestore
  → SelfLoopRestore
  → OrientationOut
  → NormalizeTranslation
  → FacadeVerifier
```

### 6.1 写权拆分

| Stage | 可写自由度 |
|-------|------------|
| Orientation | 物理↔canonical 的双射变换，不做布局决策 |
| SelfLoop | loop topology 与自身 Demand；不得挪宿主节点 |
| ParallelEdge | 边组内 port/track 次序；不得改节点 order |
| Component | component-local frame 的全局平移/装箱 |
| Normalize | 全图统一平移；不得改变相对位置 |

组、PartitionGrid 或 alignment 约束会把普通连通分量耦合起来；ComponentStage 必须在“约束耦合图”上分量化，不能只看原始 edge connectivity。

## 7. Diagnostics

```text
LayoutDiagnostics
  warnings[]
  relaxations[]
  unsupported[]
  phase_metrics[]
  params_hash
```

- unknown option warning 必须从 bind 贯通到输出；
- soft constraint 被放宽时记录对象、原优先级、原因；
- phase metrics 只能通过项目 perf 抽象计时；布局结果不得依赖时间；
- `params_hash` 覆盖 bind 后参数与影响行为的 LayoutData。

## 8. 失败类别

| 类别 | 例子 | 可否继续 |
|------|------|----------|
| InvalidInput | 未知端点、非法 cell、NaN gap | 否 |
| Unsupported | curved 未实现、Router 不认 group scene | 否 |
| InfeasibleConstraint | fixed order 环、band/组包含冲突 | 否；软约束可先按固定政策放宽 |
| BudgetExceeded | A* expansion / rip-up 轮数耗尽 | 仅在 `verified-best` 政策且已有合法解时可继续 |
| InternalInvariant | Plan order 重复、path 断裂 | 否 |
