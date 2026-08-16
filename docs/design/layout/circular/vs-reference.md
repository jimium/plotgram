# Circular · 与 yFiles 对照

> 父页：[architecture.md](architecture.md)
> 上游：[Circular Layout](https://docs.yfiles.com/yfiles-html/dguide/circular_layout/) · [CircularLayout](https://docs.yfiles.com/yfiles-html/api/CircularLayout.html) · [PartitionDescriptor](https://docs.yfiles.com/yfiles-html/api/PartitionDescriptor.html)
> 算法证据：[05 §4](../../../reference/yfiles/05-树与径向布局.md) · [14](../../../reference/yfiles/14-图论与优化工具箱.md) · Six–Tollis 1999

学理念与能力边界，**不**复刻 Stage 栈与全量 descriptor。本表回答：yFiles 有什么、plotgram 取哪几个、为什么。

---

## 1. 产品结构对照

| yFiles | plotgram 目标 |
|--------|----------------|
| `CircularLayout` 核 | `layout: circular` |
| `partitioningPolicy` | `partitioning` |
| `partitionDescriptor.style` | `partition_style`（首期只 `cycle`） |
| `backboneLayout` = **`RadialTreeLayout`** | 本核 Metric 内建 balloon 骨架；**不**嵌套 Tree 注册名 |
| `edgeRoutingPolicy` | `routing_policy` |
| `fromSketchMode` | `from_sketch` 后置 Unsupported |
| `layoutData.partitions` | 节点 `circle` / `partition`（M3） |
| `ComponentLayout`（默认 packed circle） | 弱连通分量声明序装箱；packed-circle 后置 |
| `GroupHidingStage` | 实体布局；组框 finalize 包络 |
| `SelfLoopRouter` / `ParallelEdgeRouter` | Compose 打标；自环短弧；平行边弦法向微偏 |
| `PortPlacementStage` | 弦的端点取框边界朝向对方；沿边分布后置 |
| `edgeBundling` | 后置；限制同 yFiles（CYCLE 且非 isolated） |
| `GenericLabeling` | Demand 预留；GENERIC 后置 |
| 独立 `RadialLayout` / `RadialTreeLayout` 产品名 | 树状径向 = Tree `placer: radial` / `balloon`；**不**为本核再开注册名 |

yFiles 默认 `partitioningPolicy = BCC_COMPACT`、`edgeRoutingPolicy = INTERIOR`、`partition style = CYCLE`。plotgram 默认对齐这三项。

---

## 2. `partitioningPolicy`

| yFiles | 行为 | plotgram | 何时 |
|--------|------|----------|------|
| **BCC_COMPACT** | 每 BCC 一区；跨块割点只派一区 | `bcc-compact` | **默认 · M1** |
| **BCC_ISOLATED** | 割点各自成区 | `bcc-isolated` | **M2** |
| **SINGLE_CYCLE** | 全体一圈 | `single-cycle` | M0 即可跑；作显式政策 **M3** 与 BCC 对照 |
| `layoutData.partitions` | 作者分区，忽略 policy | 节点 `circle` | **M3** |

---

## 3. `PartitionDescriptor.style`

| yFiles | 行为 | plotgram | 何时 |
|--------|------|----------|------|
| **CYCLE** | 全体在圆周上 | `cycle` | **M0–M1** |
| **DISK** | 连向它区的点在边界，其余可在盘内 | `disk` | 后置 Unsupported |
| **ORGANIC** | 盘内有机；跨区点也可在内部 | `organic` | 后置 |
| **COMPACT_DISK** | 密堆积；宜配外弧 | `compact-disk` | 后置 |

首期不做盘内力导：那会把 Circular 做成「每个分区跑一遍 Organic」。要紧凑先调 `node_gap` / 骨架二分，不要假装 `compact-disk`。

`minimumNodeDistance` → `node_gap`。`automaticRadius` / 固定半径 → `min_radius`（0 = 全自动）。

---

## 4. 骨架（`backboneLayout`）

| yFiles RadialTreeLayout 旋钮 | plotgram | 状态 |
|------------------------------|----------|------|
| 分区圆当节点的径向树 | balloon 约束排分区圆心 | **M1** |
| `placeChildrenOnCommonRadius` | 同名，默认 true | M1 读；false 后置 |
| `maximumDeviationAngle`（默认 90°） | `max_deviation_angle` | M2 |
| `minimumEdgeLength` | 骨架圆心距下界（与二分 lo 合并） | M1 用 `node_gap` 量级，不单开一大组键 |
| `compactnessFactor` | 后置；先调 gap | |
| `allowOverlaps` | 禁止默认开；后置 | |

**不**把 `backboneLayout` 暴露成可换的第二 layout 名。

---

## 5. 边

| yFiles | plotgram | 状态 |
|--------|----------|------|
| `INTERIOR` 弦 | `routing_policy: interior` | **M0 默认** |
| 同区外弧 | `exterior` | **M2** |
| `AUTOMATIC` 选边 | `automatic` | 后置 |
| `exteriorEdgeDescriptor` 间距 | Demand `ExteriorSep` | 随 M2 |
| 区际弦 / 过圆心 | `Spoke` / `Chord` | M1 |
| bundling | — | 后置 |
| 后接 EdgeRouter | `DeferToRouter` | 允许 |

yFiles 原文：弦会穿节点与标签；需要时后接 router。plotgram 同样：**不**在 Circular 内核里做 OVG。

---

## 6. 圈序

| 来源 | plotgram |
|------|----------|
| yFiles 单环交叉启发式（Single Cycle） | M1 谱序（Fiedler）；小图 BFS 回退 |
| `fromSketchMode` 保圆周序 | 后置 |
| `nodeComparator` | 不做；圈序策略只有 params `order` |
| Sequence `linear_arrange` | **直线** MinLA，不当圆序真源 |

---

## 7. 明确不搬 / 后置

后置项的键、写者与失败类别见 **[deferred.md](deferred.md)**。

明确不搬：

- Stage 栈（PortPlacement / GroupHiding / Subgraph / Component packed-circle / GenericLabeling）当独立运行时配置面
- 独立 `RadialLayout` / `RadialTreeLayout` 产品名（树状径向归 Tree placer）
- `nodeComparator` 任意比较器
- `circleIdsResult` 作为产品 API（debug trace 可带 partition id）
- CompactDiskLayout、RadialGroupLayout（仙人掌）——**另一产品**，不是本核变体

---

## 8. 相对 v1 recipe

| v1 | 重建 |
|----|------|
| 单 BCC → 单环，多 BCC → 多环（启发式切） | 显式 `partitioning`；默认 BCC_COMPACT |
| 边交给 `edge_routing: circular` | 本核 Ink；独立 router 仅 Defer |
| `applicable_diagram_types` | 删除（ADR-001） |
| `HashMap` 节点表 | `BTreeMap` / 声明序 |
| 图种门面 `state` | profile 展开；引擎不看 |
