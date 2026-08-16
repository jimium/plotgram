# Circular · 目标架构设计

> 状态：**现行目标架构 v1**（驱动重建；非当前能力声明）
> 日期：2026-08-15
> 引擎注册名：`circular`
> 代码落点：`crates/plotgram-layout/src/layout/circular/`（与 Hier / Tree / Sequence 同 crate；见 [ADR-006](../../adr/006-engine-io-and-crates.md)）
> 约束入口：[写权纪律](../write-authority.md) · [AGENTS.md](../../../../AGENTS.md) §1
> 证据：[05 §4](../../../reference/yfiles/05-树与径向布局.md) · [14 BCC / Fiedler](../../../reference/yfiles/14-图论与优化工具箱.md) · [产品条](../../../reference/yFiles-layouts-and-routing.md)
> 上游：[Circular Layout](https://docs.yfiles.com/yfiles-html/dguide/circular_layout/) · [CircularLayout API](https://docs.yfiles.com/yfiles-html/api/CircularLayout.html)
> 对照：[vs-reference.md](vs-reference.md)

本文钉死 Circular 的**目标形态与跨相契约**。产品面是 **分区成环 + 块割树骨架**，不是「把节点均匀撒在一个圆上」的配方，也不是 Tree 的径向 placer。

姊妹页：[README](README.md) · [scope](scope.md) · [phases/](phases/README.md) · [后置](deferred.md)。

> **当前代码现实**：M3 已落地（`bcc-compact` / `bcc-isolated` / `single-cycle` / 节点 `circle` 覆盖）。`automatic` / disk / `from_sketch` 诚实 `Unsupported`。DSL 节点键用 `circle:`（`partition` 是 PartitionGrid 保留字）。v1 `recipes/circular` 只读参考，不得迁 HashMap 序、图种门面、或「节点布局 + 独立 circular router」双真源。

---

## 0. 一句话目标

```text
按 partitioning 把节点分成互不重叠的分区
  + 每区一条圈序（谱序首选）
  + 每区一个圆（点在圆周，CYCLE）
  + 分区之间的块割树用 balloon 几何放置（本核骨架，不调用 layout: tree）
  + 边几何由 routing_policy 决定（区内弦 / 外弧 / 区际段）；Ink 只展开
  + 径向树仍是 Tree placer，不是第二注册名
```

**不做**：按 `state` / `er` 图种分支；把 BCC 树画成组织图；在 Ink 选哪些边出环。

对齐 yFiles 三步（API *Concept*）：

1. 按连通与 `partitioningPolicy` 找分区；把每个分区看成超点后，图呈树状。
2. 每区按 `PartitionDescriptor.style` 摆成圆（首期只 CYCLE）。
3. 用 `backboneLayout`（yFiles = `RadialTreeLayout`）摆这些圆。

---

## 1. 硬约束

| # | 约束 | 含义 |
|---|------|------|
| C1 | **单写者** | 分区、圈序、半径、角、分区圆心、边骨架各唯一写者 |
| C2 | **落笔零新决策** | Ink 不得发明圈序、半径、外弧集合、骨架肘点 |
| C3 | **DemandBoard** | 节点外延、`node_gap`、边标签高 → 半径 / 外弧间距下界；freeze 后再 Metric |
| C4 | **确定性** | 禁止 `HashMap` 迭代驱动序；平局 `(decl_index, NodeId)`；谱迭代固定初值与步数 |
| C5 | **无图种分支** | 引擎只认 `layout: circular` + typed params（ADR-001） |
| C6 | **Builtin 为主** | 默认写出边 path；`DeferToRouter` **允许**（与 Tree 同、与 Sequence 反） |
| C7 | **核保持纯粹** | BCC / 圈序 / 骨架是 Compose+Metric；不把节点图交给 TreeLayout 再画一遍 |

### 1.1 状态词

与其它核相同：**目标 / 已落地 / 过渡 / 后置**。bind 成功但未消费 = 未支持；已知名未实现策略必须 `Unsupported`，不得静默画成单环。

### 1.2 边几何立场

| | Hierarchical | Sequence | Tree | Circular |
|--|--------------|----------|------|----------|
| 默认 | 内建 Channel 正交 | **仅** Builtin | **内建**（placer） | **内建**（弦 / 外弧 / 骨架段） |
| 独立 Router | 可后接 | **禁止** | 可后接 | **可后接** |
| 风格写者 | `routing_style` | 消息拓扑 | 每个局部根的 placer | `routing_policy`（非整图 Channel） |

C6 执行：`DeferToRouter` 时本核仍写节点框；边 path 留空给门面 router。**不得**在 Ink 里对外弧再发明半径。

---

## 2. 总体架构

### 2.1 三相，不是单次撒点

```text
                    ┌─ Component pack（弱连通分量，声明序）
Domain / DSL ──────►│
  profile expand    │   CircularCore
  LayoutContract ──►│     Compose → Metric（+ Demand freeze）→ Ink
  (无 profile 名)   │
                    └─ Normalize / Diagnostics
                         │
                         ▼
                   LayoutOutput
                     可选 EdgeRouter（仅 DeferToRouter）
```

| 层 | 输入 | 输出 | 性质 |
|----|------|------|------|
| **Compose** | Graph + `CircularParams` | **`CircPlan`** | 离散：分区、割点归属、圈序、块割树、边角色（区内/区际/自环/平行） |
| **Metric** | Plan + sizes + DemandBoard | **`CircMetric`** | 半径、角、节点框、分区圆心、`CircRoute` 骨架 |
| **Ink** | Plan + Metric | **边折线** | 纯展开 |

与 Tree 对照：Tree 的递归单位是**子树节点**；Circular 的递归单位是**分区圆**。骨架算法可以长得像 balloon，输入却是块割树，不是作者的组织树。

### 2.2 输出契约

```text
LayoutOutput
  nodes:       实体框（Metric）
  edges:       Builtin path；Defer 时树/区内边亦空
  groups:      弱 group：finalize 包络；本核不发明组几何
  decorations: 首期空（圆环不是 decoration；圆心只进 debug trace）
  diagnostics: 分区数、非区内边计数、params_hash
```

### 2.3 模块边界（目标）

```text
plotgram-layout/layout/circular/
  params.rs
  compose/           # 分区 · 圈序 · 块割树
  plan.rs            # CircPlan
  demand.rs
  metric/            # 半径 / 角 / 骨架放置
  ink/
  verify.rs

plotgram-algo/
  bcc.rs             # Tarjan 双连通分量 + 块割树（目标零件）
  fiedler.rs         # 拉普拉斯第二特征向量 → 圈序（目标零件）
```

骨架放置的几何可与 Tree `polar` / balloon **共享纯函数**（`polar_point`、`2·asin` 二分），但 Circular **不得** `use layout::tree` 当嵌套布局。禁止为 circular 再注册 `layout: radial`。

落点与其它核同 crate，注册经 `Registry::standard`。

---

## 3. 核心 IR

### 3.1 `CircPlan`（Compose 写，下游只读）

| 字段 | 含义 |
|------|------|
| `components` | 弱连通分量，声明序（每个分量独立跑下面的分区） |
| `partitions` | 分区 id → 成员节点（圈序已排好） |
| `partition_of` | 每个实体 → 分区 id（覆盖；割点只属一个分区） |
| `backbone` | 分区超点上的树：`parent_partition` / `children_partitions` |
| `cut_of` | 骨架边 → 对应割点（若有） |
| `edge_role` | 每条边：`Intra` / `Inter` / `Loop` / `Parallel` |
| `order_method` | 实际采用的圈序策略（诊断） |

空图合法（空 output）。无实体边的孤立点：单点分区，半径 0。

### 3.2 `CircMetric`

```text
frames:     node_id → Rect
circles:    partition_id → { center, radius }
angles:     node_id → θ     # 与 Tree 径向相同：THETA0 = -π/2，y-down 下 θ 增大 = 顺时针
routes:     edge_id → CircRoute
```

`CircRoute` 是 typed 骨架，不是 Ink 再猜的弧：

| 变体 | 谁写 | Ink |
|------|------|-----|
| `Chord { start, end }` | interior 区内 / 默认区际 | 两点 |
| `ExteriorArc { origin, radius, t0, t1, start, end }` | 同区外弧 | 采样折线（只展开角宽） |
| `Spoke { start, end }` | 区际沿骨架、指向对方圆心的近径向段 | 两点或极短折 |

### 3.3 DemandBoard（Circular 键）

| Key | 生产者 | 消费者 |
|-----|--------|--------|
| `NodeWidth` / `NodeHeight` | 内容尺寸 | 框下界；半径弧长 |
| `NodeGap` | params | 相邻弧长 |
| `Radius(partition)` | 弧长约束 + 可选 `min_radius` | Metric 半径 |
| `ExteriorSep` | 外弧条数 × 间距 | 外弧半径抬升 |

节点标签已在 `NodeSizes`（CONSIDER），不另开 margin 键。边标签：至少把高度 `max` 进 `Radius` 或 `ExteriorSep`；GENERIC 落位后置。

---

## 4. 分区（Compose）

证据：yFiles `partitioningPolicy`，默认 **`BCC_COMPACT`**。

| 原子 | yFiles | 行为 |
|------|--------|------|
| `bcc-compact` | `BCC_COMPACT` | **默认**。Tarjan BCC；跨块的割点**只派给一个**分区 |
| `bcc-isolated` | `BCC_ISOLATED` | 割点各自成区（常是单点），夹在相邻 BCC 圆之间 |
| `single-cycle` | `SINGLE_CYCLE` | 该弱连通分量全体一圈 |
| `custom` | `layoutData.partitions` | 节点 `circle` / `partition` 覆盖；未标的回退 `bcc-compact`。M3 |

割点归属（`bcc-compact`）平局：该点所属 BCC 中，**最小树边声明下标**的那一块。禁止按 HashMap 遍历 BCC。

块割树：BCC 为块节点，割点为割节点。`bcc-compact` 把割点并入其归属块后，超图必须是树（或林）。若实现后发现环 → `InternalInvariant`（BCC 定义下不应发生）。

自定义分区若使超图有环：区际边里多出来的当 `Inter` 弦处理，**不**强行再跑一遍 FAS；诊断 warning。不要为了树去删作者的边。

相级细节：[phases/partition-and-order.md](phases/partition-and-order.md)。

---

## 5. 圈序与半径（Compose 序 · Metric 坐标）

### 5.1 圈序

圆上交叉最小化 NP 难。主选与 yFiles/文献一致：

| 方法 | 何时 | 说明 |
|------|------|------|
| `spectral` | **默认**（M1） | 分区诱导子图的 Fiedler 向量排序，再绕成圈（[14 §5.1](../../../reference/yfiles/14-图论与优化工具箱.md)） |
| `bfs` | 谱失败 / 点数 < 3 | 从声明序最小的节点 BFS |
| `declaration` | 显式或单点 | 成员声明序 |
| `from-sketch` | 后置 | 按输入极角 |

`linear_arrange`（Sequence 次轴）是**直线** MinLA，不能直接当圆序；圆序写者在本核。谱零件进 `plotgram-algo`。

### 5.2 半径

CYCLE：所有成员在圆周上。

```text
chord(i, i+1) ≥ extent(i) + extent(i+1) + node_gap
⇒ r ≥ (extent_i + extent_{i+1} + node_gap) / (2 sin(Δθ_i / 2))
r = max(相邻约束, min_radius, Demand Radius)
```

`Δθ` 按圈序均分**或**按 `extent` 加权均分（变宽节点）；默认加权，避免大节点重叠。单节点 `r = 0`。两节点：间距 = 外延和 + gap，圆心在中点。

漏掉弧长约束必重叠——与 Tree radial 同一条纪律。

### 5.3 角约定

与 Tree 径向相同：`THETA0 = -π/2`（12 点），y-down 下 θ 增大为顺时针。可选 params `rotation`（弧度）加在整图上，**不是**四向 OrientationStage。Circular 没有 canonical TB 要转。

---

## 6. 骨架（分区圆怎么摆）

yFiles：`backboneLayout` = `RadialTreeLayout`，另有 `placeChildrenOnCommonRadius`、`maximumDeviationAngle`、`minimumEdgeLength`、`compactnessFactor`。

plotgram：Compose 已写出分区树；Metric **BackboneWriter** 用 balloon 约束排分区圆：

```text
每个子分区圆半径 R_i（含子节点外延）
Σ 2·asin(R_i / D) ≤ 2π - reserved
二分求圆心距 D
```

非根分区为连向父的骨架边留角度缺口（同 Tree balloon）。**不**把作者节点再跑一遍 `placer: balloon`。

`place_children_on_common_radius`（默认 true）：同一父下的子分区圆心共半径。false 后置（更紧、实现更绕）。

`max_deviation_angle`：骨架边尽量过子圆圆心；默认 90°。键未注册，见 [deferred.md](deferred.md) §5.2。

多个弱连通分量：各自骨架算完后，按声明序水平装箱（`component_gap`），不要假装成一个 BCC。

---

## 7. 边

### 7.1 `routing_policy`

| 原子 | yFiles | 里程碑 |
|------|--------|--------|
| `interior` | `INTERIOR` | **M0 默认**：一律弦 |
| `exterior` | 同区非邻接走外弧；邻接仍弦 | **M2** |
| `automatic` | 启发式挑外弧 | 后置 `Unsupported` |

外弧只允许 **同一分区、两端都在圆周上** 的边。区际边、骨架边、自环不是外弧。

### 7.2 自环 / 平行边

yFiles 用 Stage。本核：Compose 打标；自环 = 节点外侧短弧（Ink 展开）；平行边弦向法向微偏。不要为自环开 Channel。

### 7.3 捆绑

后置。对齐 yFiles：仅 `CYCLE` 且非 `bcc-isolated`。外弧永不捆绑。

---

## 8. 参数

```text
1. CircularParams::default()     // partitioning = bcc-compact, routing = interior
2. layout: circular { … }
3. 节点 LayoutData（circle / partition）
```

引擎看不见 `profile:`。

### 8.1 核级

| 键 | 默认 | 说明 |
|----|------|------|
| `partitioning` | `bcc-compact` | `bcc-compact` / `bcc-isolated` / `single-cycle` / `custom` |
| `partition_style` | `cycle` | 首期只消费 `cycle`；`disk` / `organic` / `compact-disk` → `Unsupported` |
| `order` | `spectral` | `spectral` / `bfs` / `declaration` |
| `routing_policy` | `interior` | 见 §7.1 |
| `node_gap` | 24 | 同圈相邻下界 |
| `component_gap` | 48 | 弱连通分量装箱 |
| `min_radius` | 0 | 每区半径下界；0 = 仅由弧长约束 |
| `rotation` | 0 | 整图绕原点加性转角 |
| `place_children_on_common_radius` | true | 骨架子圆共半径 |
| `from_sketch` | false | 后置；true 未实现 → `Unsupported` |

### 8.2 节点级

| 键 | 说明 |
|----|------|
| `circle` / `partition` | 自定义分区 id（atom）；仅 `partitioning` 缺省或 `custom` 时生效。DSL 作者键为 `circle:`（`partition` 是图级 PartitionGrid 保留字） |

---

## 9. 写权与确定性

| Writer | 自由度 |
|--------|--------|
| `ComponentWriter` | 弱连通分量 |
| `PartitionWriter` | `partition_of`、割点归属 |
| `CircleOrderWriter` | 每区成员序 |
| `BackboneTopoWriter` | 分区树 |
| `DemandWriter` | 半径 / 间距下界（freeze） |
| `ShapeWriter` | 框、圆心、θ、r |
| `RouteWriter` | `CircRoute` |
| `InkWriter` | path 点列 |

确定性：

- 节点 / 边 / BCC 遍历：声明序或 Plan 已排好的圈序；
- Tarjan 邻接表按对端 `decl_index` 排；
- Fiedler：幂迭代固定步数、固定规范（首个 `|v_i|` 最大且 `v_i>0` 的分量定向）；
- 分量装箱：声明序。

---

## 10. 验真

| Verifier | 最低断言 |
|----------|----------|
| Plan | 每个实体恰好一个分区；圈序覆盖成员；骨架无环；`edge_role` 盖全边 |
| Metric | 框 finite；同区相邻弦 ≥ Demand；兄弟分区圆不重叠（AABB 或圆盘） |
| Ink | path 首尾 = route terminals；外弧不改 θ 跨度；Defer 时 path 空 |
| Facade | 双跑 bit-identical；`disk` 等未实现策略不静默降级 |

视觉：`single-cycle` 的 product 必须「点在一圈、相邻不叠」；`bcc-compact` 的 product 必须「多圈 + 割点桥」而不是一个巨圆。

---

## 11. 里程碑

| 里程碑 | 交付 | 验收 | 前置 |
|--------|------|------|------|
| **M0** | 单环（分量内全体 CYCLE）· 声明序/BFS · 弦 · 四则有限坐标 · 未知策略硬失败 | 简单环可出图；showcase `circular/cycle` smoke | algo 可后补谱 |
| **M1（已落地）** | Tarjan BCC + `bcc-compact` · 谱序 · Demand 半径 · balloon 骨架 | 多 BCC 不挤成一圈；深桥不重叠 | M0 · `plotgram-algo` BCC/Fiedler |
| **M2（已落地）** | `bcc-isolated` · `exterior` 同区外弧 · 自环短弧 | 密圈交叉下降可观测；割点夹在两圆间 | M1 |
| **M3（已落地）** | 节点 `circle` 自定义分区 · `single-cycle` 显式与 BCC 切换 | 同一拓扑两政策几何可区分 | M1 |
| **后置** | 见 [deferred.md](deferred.md)：disk/organic/compact-disk · automatic 外弧 · bundling · from-sketch · star · 射线标签 · 共半径关闭 | 显式 `Unsupported` 直至落地 | — |

---

## 12. 反模式

1. 按 `profile == state` 在 layout crate 分支
2. Ink 根据「看起来交叉多」把弦改成外弧
3. 嵌套调用 `layout: tree` / `placer: radial` 当骨架
4. 为 Circular 再注册 `radial` layout 名
5. 用 Hier rank 把节点排成一圈
6. `HashMap` 收集 BCC / 圈序
7. bind 了 `partitioning: bcc-compact` 却按单环静默画
8. 把 Tree balloon 的**节点树**与本核**分区树**混成一份 Plan
9. 独立 `edge_routing: circular` 当本核主路径（v1 债；重建边在 Ink）
10. 谱迭代无固定步数 / 无定向，双跑抖动
