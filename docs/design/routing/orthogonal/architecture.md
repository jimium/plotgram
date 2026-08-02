# EdgeRouter · 目标架构设计

> 状态：**现行目标架构 v1**（驱动重建；非当前能力声明）
> 日期：2026-08-02
> 引擎注册名：`orthogonal`
> 代码落点：`crates/plotgram-router/`（`core` + `orthogonal` + `verify` + `score`）
> 约束入口：[写权纪律](../../layout/write-authority.md) · [AGENTS.md](../../../../AGENTS.md) §1 · [ADR-006](../../adr/006-engine-io-and-crates.md)
> 证据：[03 正交边路由](../../../reference/yfiles/03-正交边路由.md) · libavoid / Wybrow et al.
> 姊妹页：[README](README.md) · [scope](scope.md)

本文钉死**独立正交 EdgeRouter**的契约、自由度分层、算法选型与可脱离布局的开发方式。
它**不是** Hier Builtin Channel，也**不是** Sequence BuiltinEdges。

---

## 0. 一句话目标

```text
节点与端口冻结
  + 障碍 / 组边界 / 穿越许可构成 RouteScene
  + 搜索拓扑走向（L2）→ 走廊定序（L3）→ 精确偏移（L4）
  + 只写 EdgePlacement.path（及回传已决议端口，不得改值）
  + 可手写场景独立开发与验真
```

**不做**：在 Router 里发明端口、挪节点、按图种特判、用均匀网格冒充产品路径。

---

## 1. 硬约束

| # | 约束 | 含义 |
|---|------|------|
| R1 | **单写者** | path 唯一写者为本 Router；node / group / port 只读 |
| R2 | **零新决策（相对上游）** | 不得为好画而改端子或障碍；缺决策 → 失败或打回 Layout |
| R3 | **确定性** | 边序、OVG 顶点序、A* 平局、track 平局均稳定；禁 `HashMap` 迭代序 |
| R4 | **无图种分支** | 只认 `RouteScene` + typed options（ADR-001） |
| R5 | **组场景诚实** | 有组障碍却无法表达穿越许可 → `UnsupportedRouteScene`；禁止 silent 穿组 |
| R6 | **可夹具化** | 不依赖任何 `LayoutAlgorithm` 即可构造输入并验收 |

### 1.1 状态词

**目标 / 已落地 / 过渡 / 后置**。M0 + M1 + M2（组穿越首期 + L3/L4 VPSC nudge）= **已落地**。M3 的 Hier `DeferToRouter` 投影集成已通、Tree 接入 / Facade 组框投影 / FacadeVerifier 未做。嵌套组 scope / 交叉感知 track 序 = **后置**（逐项状态见 §10）。

---

## 2. 在管线中的位置

```text
LayoutAlgorithm（任意核，DeferToRouter）
  → LayoutOutput
       nodes / groups 冻结
       edges: terminals（ports 已决议；path 可空）
       route_scene: 组边界 + BoundaryCrossing（目标）
  → EdgeRouter::route   ← 本文
       只替换 path
  → finalize（labels / canvas；不得重算组框 / 端口）
```

与内建 Ink 对照：

| | Hier Builtin Channel | 独立 EdgeRouter |
|--|----------------------|-----------------|
| 搜索图 | 层间/层内走廊（布局结构） | OVG / reduced interesting lines |
| 端口写者 | Compose | Compose（Router 只读） |
| 何时用 | `edge_routing: None` | `edge_routing: Some("orthogonal")` |
| 节点假设 | 分层规则缝 | **自由位置**亦可 |

Facade 细则与 `RouteScene` 字段真源亦见 Hier [contracts-and-ir](../../layout/hierarchical/phases/contracts-and-ir.md) §2；本文是 Router 侧消费契约。

---

## 3. 输入 / 输出契约

### 3.1 目标：`RouteScene`（Router 真输入）

```text
RouteScene
  node_obstacles:     Rect[]              # 通常 = 节点 frame（可先 inflate）
  group_boundaries:   GroupBoundary[]     # 可空；空 = 无组场景
  boundary_permissions: EdgeId → BoundaryCrossing[]
  terminals:          EdgeId → TerminalPair
  edge_order:         EdgeId[]            # 稳定处理序；缺省按 id 字典序
  options:            OrthogonalRouteParams

TerminalPair =
  { source: PortPoint, target: PortPoint }

PortPoint =
  { node_id, side, along: Absolute|Fraction|Slot, stub_len? }

BoundaryCrossing =
  (group_id, Enter|Leave, gate_region?)   # 按 source→target 语义序
```

**最低可跑输入（M0 夹具）**：`node_obstacles` + `terminals` + `edge_order`；无组时可空 `group_boundaries`。

### 3.2 过渡：现有 `RouteInput`

```text
RouteInput<'a>   # plotgram-engine-api 现行
  graph, nodes, edges, options
```

现行 stub 从 `nodes[].frame` + `edges[].from_port/to_port` 推导端子。
**已落地**：`engine/src/run.rs::project_route_scene` 把 layout 输出（nodes + edges）**投影**为 `RouteScene`（见 §10 M3）；Router 不直接依赖 Graph 全貌。

投影规则（目标）：

1. `node_obstacles` ← `nodes[].frame`（稳定 id 序）。
2. `terminals` ← 每边已决议 port → 锚点；缺 port → 硬失败（Layout 未尽职责）。
3. `group_boundaries` ← Layout 提供的组框；Facade 事后包围盒不得冒充真源。
4. `boundary_permissions` ← Layout Compose；Router 不推断「两端 LCA 可穿」。

### 3.3 输出

```text
Vec<EdgePlacement>   # 与输入边集一一对应（稳定序）
  id / source / target 与输入相同
  from_port / to_port 与输入相同（回传；不得改）
  path.points: 正交折线顶点（含两端锚点）
```

禁止：

- 增删边；
- 改 node 坐标；
- 改 port side / along；
- 输出非正交段（轴对齐约束；ε 内共线可合并）。

### 3.4 失败语义

| 情况 | 结果 |
|------|------|
| 缺端口 / 缺端点节点 | 硬失败 |
| 有组边界但本实现不支持穿越模型 | `UnsupportedRouteScene` |
| 单边无合法路径（预算内） | 硬失败（该边）；禁止退化穿障单肘 |
| 参数未识别 | bind 失败或 warning（与引擎 params 纪律一致） |

---

## 4. 自由度分层（写权）

与 [03 正交边路由](../../../reference/yfiles/03-正交边路由.md) 对齐；**L1 不在本 Router**：

| 层 | 自由度 | 写者 | 本 Router |
|----|--------|------|-----------|
| L1 | 端口 side / along | Layout Compose（或夹具） | **只读** |
| L2 | 路径拓扑（经哪些走廊/格点） | Path search | **写** |
| L3 | 同走廊多边次序 | Track / channel order | **写** |
| L4 | 走廊内精确偏移 | Nudging | **写** |
| L5 | 圆角、箭头缩进 | Render / decorations | 不写 path 拓扑 |

纪律：L4 不得推翻 L3 序；L3 不得改 L2 走向。Nudging「想穿到另一侧」→ L2/L3 目标函数缺项，上提，禁止就地特判。

```text
port stub 出针（展开端子）
  → 建搜索图（OVG / reduced lines）
  → 逐边 A*（状态含方向）          # L2
  → 按走廊聚合段 → track 定序       # L3
  → nudging 求偏移                  # L4
  → 折线规范化（共线消点、最小段长）
```

---

## 5. 算法选型

| 阶段 | 主选 | 替代 | 后置 / 不做 |
|------|------|------|-------------|
| 搜索图 | **Reduced interesting lines → OVG**（障碍边 ± margin、端子坐标、组框） | 全量 OVG | 均匀网格（仅原型） |
| 路径搜索 | **A\***，状态 `(vertex, Dir4)`；代价 = 长 + λ·弯 | Dijkstra 批量距离场 | 带交叉惩罚的非马尔可夫主搜（默认关） |
| 边序 | **EdgeId 稳定序**；可选两轮（第二轮共享段惩罚） | — | HashMap 序 |
| Track 定序 | 区间着色 + **垂向偏好空间序**；平局用色 id | 单边场景跳过 | 事后贪心挪交叉 |
| Nudging | **VPSC 最小位移**（`nudge.rs` + `plotgram_algo::vpsc`）；L3 用 `interval_color` | 均匀 track 间距（已替换） | 发现重叠就「挪一点」 |
| 组穿越 | permission 列表驱动的可走 gate；**首期硬禁违约**（见 [group-crossing](phases/group-crossing.md)） | 无组场景跳过 | silent 穿组；`gate_region: None` 整边穿 |
| Bus | — | — | Steiner / BusRouter（后置） |

### 5.1 障碍与出针

- 建图前对障碍做 **inflate(`spacing`)**，路径自然离边。
- 端口先沿 side 法向走出 `port_stub`，再进入搜索图，保证垂直出入。
- 起/终点所属障碍：**仅自身**免检，不得全局关碰撞。

### 5.2 代价（量级，标定进 params）

```text
cost = Σ segLen
     + bend_penalty * #bends
     + shared_penalty * sharedLen     # 仅第二轮
     + cluster_penalty * illegal_crossings
```

`bend_penalty` 典型为节点间距量级的 1–4 倍。交叉惩罚默认 0（进主搜会破坏可采纳启发）；交叉优先靠 L3 消。

### 5.3 与 `route/core`、`plotgram-algo`

| 零件 | 位置 |
|------|------|
| `port_anchor` / `orthogonal_elbow` / 矩形并集 | `route/core`（无策略） |
| 折线规范化、共线消点 | `route/core` 或 algo |
| VPSC nudging | `plotgram-algo` |
| OVG 构造 / A* / track | `route/orthogonal`（本核策略） |

Hier Builtin Ink 可调 `core`，**不得**调用本 Router 的 OVG 策略模块倒灌。

---

## 6. 参数（算法级）

与 `profile:` 区分：此处是 router options / typed `OrthogonalRouteParams`。

| 参数 | 含义 | M0 |
|------|------|----|
| `spacing` | 障碍 inflate / 边-边最小间距 | 要 |
| `port_stub` | 出针长度 | 要 |
| `bend_penalty` | 弯折代价 | 要 |
| `shared_penalty` | 第二轮共享段惩罚；0 = 单轮 | M1 |
| `min_segment` | 最小段长（圆角预算） | M1 |
| `max_search_nodes` | A* / OVG 规模门控 | M1 |
| `route_rounds` | 1 或 2 | M1 |

未实现的参数：bind 时失败或显式 unsupported，禁止静默 no-op。

---

## 7. 脱离布局的开发方式（推荐）

本 Router **可以、且应当**先独立于布局系统开发。

### 7.1 夹具形状

```text
fn fixture_two_boxes() -> RouteScene {
  // 手写两个 Rect、两条边的 PortPoint（side + along）
  // 无 group_boundaries
}

fn fixture_with_blocker() -> RouteScene {
  // 源/汇之间插入第三障碍，迫使绕行
}
```

单测只断言：

- `path` 正交、端点贴端子；
- 不与非自身障碍相交（inflate 后）；
- 同输入双跑 bit-identical；
- 弯折数 / 包围盒等可观测几何（可用 `insta` json snapshot）。

不断言内部 OVG 顶点数等计数器。

### 7.2 与 Facade 接线（后做）

```text
夹具验收绿灯
  → 实现 EdgeRouter::route 投影 RouteInput → RouteScene
  → registry 注册 "orthogonal"
  → Hier/Tree DeferToRouter 集成测（节点由 layout 出）
```

集成前不必等 Hierarchical Channel 完备；Layout 只要能冻节点 + 决议端口即可（甚至用最小 stub layout）。

### 7.3 stub 原语定位（M0 后）

M0 已落地：主路径为 reduced lines OVG + A* 搜索（避障、确定性、单轮）。
`core::orthogonal_elbow` 保留为**无策略肘线原语**：供 Hier Builtin Ink 与「无搜索」对照基线使用，不再是本 Router 的主路径。

---

## 8. 模块边界（目标）

```text
crates/plotgram-router/src/
  lib.rs          # crate 入口
  core/           # 无策略：anchor、elbow、rect、polyline normalize
  orthogonal/
    mod.rs        # EdgeRouter impl
    ovg.rs        # interesting lines / visibility
    search.rs     # A* (vertex, dir)
    track.rs      # L3 corridor detect + interval_color
    nudge.rs      # L4 VPSC track coordinates
  verify.rs       # 正交性、碰撞、端口附着、组穿越自检
  score.rs        # 质量度量（弯折 / 长度 / 交叉 / 共线）

crates/plotgram-router/tests/
  fixtures.rs     # 夹具场景 + 集成测试
```

依赖纪律（ADR-006）：

```text
plotgram-router → engine-api, model, algo
plotgram-router ↛ engine 门面 / layout 实现
engine → plotgram-router（消费 core 原语 + 注册 OrthogonalEdgeRouter）
layout Builtin Ink → plotgram_router::core（可）；↛ orthogonal 策略
```

---

## 9. 验真与诊断

| 检查 | 时机 |
|------|------|
| 折线正交、端点 = 端子锚点 | 每边输出后 |
| 与障碍（除自身）分离 ≥ 0 | 每边；组 permission 外不得穿 |
| 端口回传与输入相等 | FacadeVerifier |
| 同 Scene 双跑 path bit-identical | 属性测 |
| 边 id 集守恒 | FacadeVerifier |

诊断（目标）：`RouteDiagnostics { bends, length, unsupported_reason?, gated: ... }`；可进 LayoutDiagnostics 子段，勿只打 log。

---

## 10. 里程碑（设计口径）

| 里程碑 | 状态 | 交付 | 验收 |
|--------|------|------|------|
| **M0** | **已落地** | `RouteScene` 类型 + 夹具 API；reduced lines + A\*；无组；单轮；无 L3/L4（单边走廊可共线） | 手写障碍绕行正确；确定性；不穿障 |
| **M1** | **已落地** | inflate/stub 标定；两轮 shared（`route_rounds=2` + `shared_penalty`）；走廊 track 分离（均匀偏移 `k×spacing`，`track.rs`）；规模门控（`max_search_nodes`）；min_segment（stub 拉长 + 驼峰消除）；替换 stub 主路径 | 多边不完全重合；门控可观测 |
| **M2** | **已落地** | 组穿越首期 + L3 `interval_color` + L4 VPSC nudging（[group-crossing](phases/group-crossing.md) · [track-and-nudge](phases/track-and-nudge.md)） | 组场景验收全绿；共享走廊 track 间距 ≥ `spacing` |
| **M3** | **部分** | Hier `DeferToRouter` 投影集成已落地（`run.rs::project_route_scene` + registry 注册 `orthogonal`，集成测通过）；Tree 接入 / FacadeVerifier 端口回传检查未做 | layout 冻节点后 path 可换（已达成）；ports 不变 |
| **后置** | 未做 | 增量路由、Bus、交叉进主搜 | — |

---

## 11. 反模式速查

| 反模式 | 应做 |
|--------|------|
| Router 改 port 消交叉 | 上提 Layout PortWriter / 夹具 |
| 穿障单肘当 fallback | 硬失败或扩搜索预算 |
| 均匀网格进产品 | OVG / interesting lines |
| HashMap 边序 | `BTreeMap` / 显式 `edge_order` |
| Nudging 改拓扑 | 回 L2/L3 |
| 为 sequence 特判水平消息 | Sequence 自有 Builtin；本 Router 不认图种 |
| 等 Hier 完备才开写 | **先夹具独立做 M0** |

---

## 12. 相关阅读

| 文档 | 用途 |
|------|------|
| [scope](scope.md) | 能力 / 非目标 |
| [group-crossing](phases/group-crossing.md) | M2-group 穿越契约 + 夹具字段 |
| [track-and-nudge](phases/track-and-nudge.md) | L3 区间着色 + L4 VPSC |
| [ADR-006](../../adr/006-engine-io-and-crates.md) | Trait / crate 边界 |
| [write-authority](../../layout/write-authority.md) | 单写者尺子 |
| Hier [contracts-and-ir](../../layout/hierarchical/phases/contracts-and-ir.md) | `RouteScene` 与 Facade |
| Hier [ports-and-channel](../../layout/hierarchical/phases/ports-and-channel.md) §9 | Builtin vs Defer |
| Sequence [message-routing](../../layout/sequence/phases/message-routing.md) | **对照**：非本路径 |
| [03 正交边路由](../../../reference/yfiles/03-正交边路由.md) | 算法证据 |

---

> **摘要**：独立 EdgeRouter = 冻节点 + 冻端口上的正交搜索与 nudging；主选 OVG/interesting lines；可手写 `RouteScene` 脱离布局开发；组场景要么合法穿越要么显式 unsupported。
