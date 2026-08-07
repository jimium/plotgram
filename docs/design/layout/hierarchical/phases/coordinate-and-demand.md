# Hierarchical · Demand 与度量相

> 父页：[architecture](../architecture.md) §3.2–§3.3、§8–§9  
> 上游：[composition](composition.md) · [ports-and-channel](ports-and-channel.md)

Metric 是节点框、组框、Partition band、port point 与 track 像素坐标的唯一写者。

## 1. DemandBoard

### 1.1 Typed key

```text
DemandKey =
  NodeMinSize(NodeId)
  LayerGap { before, after }
  NodeGap { left, right }
  GroupMinSize(GroupId)
  PartitionBandMinSize { axis, id }
  GateSpan(GateKey)
```

每个值携带单位：

- `Size/LengthPx`：MetricBudget；
- `LayerSpan`：ComposeBudget；
- 禁止同 key 混用逻辑层数与 px。

### 1.2 合并

Board 只合并“同一 key 的多个下界”：

```text
board[key] = max(board[key], producer_lower_bound)
```

生产者内部先完成自身聚合：

- k 个独立端口：`2×padding + (k-1)×pitch`；
- 同一 label band 的多个堆叠标签：先 sum，再写一个下界；
- Channel 最大并发 track：先区间计数，再乘 pitch；
- Bundle 共享 track 只计一个容量单位。

把两个必须同时占空间的 band 直接 `max` 是错误；它们应先由生产者求和或使用不同 key。

### 1.3 Freeze epoch

| Epoch | Freeze 前必须已知 | 消费者 |
|-------|-------------------|--------|
| ComposeBudget | label dummy、强端口绕行、自环逻辑跨度 | ranking / properify |
| MetricBudget | PortPlan、ChannelPath/track count、组标题、节点/边 label 尺寸 | Metric |

消费者开始后，对应 epoch 变为只读。迟到需求是 phase-order invariant error。

## 2. Canonical 坐标

核心使用：

```text
main  = 沿流向
cross = 正交于流向
```

物理映射：

| Orientation | main | cross | main 正向 |
|-------------|------|-------|-----------|
| TB | y | x | top→bottom |
| BT | y | x | bottom→top |
| LR | x | y | left→right |
| RL | x | y | right→left |

内部先在 canonical TB 求正向 main，OrientationOut 再反射/交换并统一 normalize。所有 Rect、Point、Side、label slot、RouteScene 使用同一双射。

## 3. PartitionGrid

PartitionGrid 保持物理轴语义：

- `columns`：x 轴 left→right；
- `rows`：y 轴 top→bottom；
- axis id 与 node cell 不随 orientation 改名。

映射：

| Orientation | cross-axis band | main-axis band |
|-------------|-----------------|----------------|
| TB / BT | columns | rows |
| LR / RL | rows | columns |

反向流（BT/RL）只反转 main 的 rank 映射，不反转作者声明的物理 axis 序。Metric 最终仍保证 column 物理 left→right、row 物理 top→bottom。

约束：

- cross-axis cell → 层内连续块 + band 边；
- main-axis cell → 全局 rank 区间；
- 未分 cell 节点由 typed policy 决定自由区/错误；
- 空 band 保留标题与最小尺寸 Demand；
- 节点可同时属于 group 与 cell，两套包含约束共同进入求解。

## 4. 主轴

对每层求最大节点/虚节点厚度与 gap 下界：

```text
main_start[0] = 0
main_start[i+1] ≥ main_end[i] + resolved_layer_gap[i]
```

`resolved_layer_gap` 至少满足：

- 参数 `layer_gap`；
- Channel track demand；
- label band；
- self-loop / port stub；
- main-axis Partition band 边界。

反向边仍按 working DAG 分层，最终 path/箭头按 original 语义。

## 5. 次轴：BK 理想值 + VPSC 约束

BK 生成每个 node/dummy 的理想 cross coordinate；VPSC 统一求解硬分隔与软对齐。

**主轴共线 ∩ 扇出对称**不由 BK 焊点兼任：目标由显式 **SymmetryAxisWriter** 产出对称轴、刚体列成员表与 **FanPack** 扇叶槽位，约束/desired 只消费这些表（扇叶 desired 覆盖 BK ideal）。见 [symmetry-axis.md](symmetry-axis.md)；视觉裁定见 [expectations §6.1](../expectations.md)。

### 5.1 变量

- node center；
- group left/right boundary；
- cross-axis Partition band start/end；
- 必要的 gate/track guide。

### 5.2 硬约束

- 相邻 order 的 node separation；
- group 包含成员 + padding/title；
- sibling group 非重叠；
- partition band 声明序与 min size；
- cell node 落在对应 band；
- fixed alignment/rank 若被声明为 hard。

### 5.3 软目标

- 接近 BK 理想值；
- port 对齐；
- 长 dummy chain 竖直；
- group/architecture alignment set；
- 紧致化。

硬约束不可行时报告冲突链，不用事后挪节点修复。

## 6. 组框

组框边界是 VPSC/主轴变量，不是 `union(children)+padding` 的后验真源。

- 子节点/子组提供包含约束；
- title/padding 提供 GroupMinSize；
- nested group 自底向上提供约束，但父子在同一次可行系统中求终值；
- 空组若产品允许，必须有显式 min size；否则输入错误；
- Metric 输出全部 `GroupPlacement`，Facade 不重算。

## 7. Port point

Metric 根据 `PortPlan` 与最终 node frame 展开：

- Ordered(slot)：按同侧独立 port group 数、padding、pitch 均匀布局；
- Ratio(r)：沿 side 可用长度乘 r；
- LocalOffset(px)：经 Orientation 变换后映射到画布；
- port group 成员得到同一 point。

输出点必须落在 node frame 边界；端口数过多时应由 NodeMinSize Demand 预留，不得在这里临时扩大节点。

## 8. Track 坐标与 nudge

对每个 substrate segment：

1. 读取 Plan 的 TrackOrder；
2. 以通道可用区间建 VPSC separation；
3. 目标偏向 port point、直线延续与均匀间隔；
4. 固定 order，只求像素坐标；
5. 若可用区间不足，说明 MetricBudget 或 Plan 容量错误，硬失败。

## 9. MetricVerifier

最低检查：

- 所有数值 finite，宽高非负；
- node frame 不重叠；
- group 包含全部直接/间接成员；
- sibling group 满足分隔；
- partition band 有序且 cell node 落带；
- port point 在边界并符合 along_spec；
- track 像素次序与 Plan 一致；
- 所有 DemandKey 下界满足；
- Orientation round-trip 后 side/cell/id 不漂移。
