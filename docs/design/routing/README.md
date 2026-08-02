# 边路由（独立 EdgeRouter）

> 状态：现行目标
> 代码：`crates/plotgram-router/`（`core` 原语 + 各算法 router + `verify` + `score`）
> 约束：[写权纪律](../layout/write-authority.md) · [AGENTS.md](../../AGENTS.md) §1 · [ADR-006](../adr/006-engine-io-and-crates.md)

横切夹：描述**与各布局核解耦**的边几何契约与路由算法，避免把搜索逻辑写进某个核的 Ink 文档里。

---

## 核心概念

节点与端口冻结之后，求边路径：避障、少弯、确定性。路径风格由 **注册名** 区分（如 `"orthogonal"`），各算法实现同一 [`EdgeRouter`](../../crates/plotgram-engine-api/src/traits.rs) trait。

---

## 两种边几何模式

| 模式 | `EdgeGeometryMode` | 谁写边路径 |
|------|-------------------|------------|
| 内建 | `Builtin` | 布局核 Ink（Hier：Channel 拓扑 + 展开） |
| 独立路由 | `DeferToRouter` | 布局写节点与端口；**`EdgeRouter` 写 path** |

各核默认选哪种见该核 README；本夹只钉 Router 契约与原语。

---

## 算法目录

| 算法 | 注册名 | 状态 | 搜索图 / 策略 | 典型场景 | 文档 |
|------|--------|------|---------------|---------|------|
| **正交** | `orthogonal` | M0+M1 已落地 | reduced OVG + A* + track 分离 | 流程图、ER 图、网络拓扑 | [orthogonal/](orthogonal/) |
| 折线 | `polyline` | 未做 | — | Hier 内建 routing_style | — |
| 八方向 | `octilinear` | 未做 | — | 电路图、地铁图 | — |
| 曲线 | `curved` | 未做 | — | 有机图、美化输出 | — |
| 直线 | `straight` | 未做 | — | 最简连接、调试 | — |
| 总线 | `bus` | 后置 | — | 超图、EDA 总线聚合 | — |

> 未实现的算法：bind 时失败或显式 unsupported，禁止静默 no-op。

---

## 共享契约

### RouteScene（Router 真输入）

```text
RouteScene
  node_obstacles:       Rect[]              # 通常 = 节点 frame（可先 inflate）
  group_boundaries:     GroupBoundary[]     # 可空；空 = 无组场景
  boundary_permissions: EdgeId → BoundaryCrossing[]
  terminals:            EdgeId → TerminalPair
  edge_order:           EdgeId[]            # 稳定处理序；缺省按 id 字典序
  options:              <AlgorithmSpecificParams>
```

- 各算法有各自的 params 类型（如 `OrthogonalRouteParams`）；未识别参数 → bind 失败。
- **最低可跑输入**：`node_obstacles` + `terminals` + `edge_order`；无组时可空 `group_boundaries`。

### 输出

```text
Vec<EdgePlacement>   # 与输入边集一一对应（稳定序）
  id / source / target 与输入相同
  from_port / to_port 与输入相同（回传；不得改）
  path.points: 折线顶点（含两端锚点）
```

### 硬约束（所有算法共享）

| # | 约束 | 含义 |
|---|------|------|
| R1 | **单写者** | path 唯一写者为本 Router；node / group / port 只读 |
| R2 | **零新决策** | 不得为好画而改端子或障碍；缺决策 → 失败或打回 Layout |
| R3 | **确定性** | 边序、顶点序、平局 tie-break 全链稳定；禁 `HashMap` 迭代序 |
| R4 | **无图种分支** | 只认 `RouteScene` + typed options（ADR-001） |
| R5 | **组场景诚实** | 有组障碍却无法表达穿越许可 → `UnsupportedRouteScene` |
| R6 | **可夹具化** | 不依赖任何 `LayoutAlgorithm` 即可构造输入并验收 |

---

## 写权（短表）

| 自由度 | 写者 |
|--------|------|
| 节点框 / 组框 | Layout Metric（或测试夹具） |
| 端口 | Layout Compose（或测试夹具） |
| 路径拓扑 / track / 偏移 | **本 Router** |
| 圆角 / 箭头缩进 | Render |

Router **不得**改端口或节点。纪律：[write-authority](../layout/write-authority.md)。

---

## 与布局核的关系

| 核 | 默认边几何 | DeferToRouter 可选 |
|----|-----------|-------------------|
| Hierarchical | Builtin Channel | ✓ → `orthogonal` |
| Tree | Builtin | ✓ → `orthogonal` |
| Circular | Builtin | ✓ → `orthogonal` |
| Sequence | Builtin 消息路由 | ✗ 禁止独立 Router 作主路径 |

共享：`route/core` 无策略原语（锚点、肘线、矩形运算、折线规范化）。不共享：各算法的搜索图策略。

---

## 共享代码落点

| 模块 | 职责 |
|------|------|
| `plotgram-engine-api` | `EdgeRouter` trait / `RouteScene` / `EdgeGeometryMode` |
| `plotgram-router/src/core` | 无策略原语（锚点、肘线、矩形、折线规范化、段重叠） |
| `plotgram-router/src/{verify,score}` | 几何验收门 + 质量度量（所有算法共用） |
| `plotgram-algo` | 可复用零件（VPSC nudging、交叉计数等） |
| `plotgram-router/src/orthogonal` | 正交 Router 实现（ovg / search / track） |

---

## 文档索引

| 文档 | 内容 |
|------|------|
| [orthogonal/](orthogonal/) | 正交路由：契约、L2–L4、算法选型、夹具、里程碑 |
| [ADR-006](../adr/006-engine-io-and-crates.md) | `EdgeRouter` Trait / crate 边界 |
| [03 正交边路由](../../reference/yfiles/03-正交边路由.md) | 算法证据（yFiles 参考） |
| [写权纪律](../layout/write-authority.md) | 单写者尺子 |
