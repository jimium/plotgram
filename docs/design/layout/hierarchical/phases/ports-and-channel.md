# Hierarchical · 端口、Gate 与 Channel

> 父页：[architecture](../architecture.md) §6–§7  
> 上游：[composition](composition.md) · 下游：[coordinate-and-demand](coordinate-and-demand.md)

## 1. 端口输入与决议

### 1.1 作者约束

```text
PortConstraint =
  Free                              // 边未写 from_side/to_side
  FixedSide { side }                // 边 DSL 仅此档
  FixedOrder { side, order_key }    // 仅 group_anchor（side + 可选 slot）
```

边级已移除：`*_slot` / `*_ratio` / FixedPos / Candidates。

### 1.2 Plan 决议

```text
PortPlan
  side
  along_spec: Ordered(slot) | LocalOffset(px)
  group?
```

Compose 写 `along_spec`；Metric 写导出的 `PortPoint`。  
`Ordered(slot)` 的 slot 只定义同侧相对序（算法分配，非边 DSL）。  
双胞胎 N/S 平行走廊的**绝对列位**由 Metric [PortLane](port-lanes.md) 在 frames 之后写：走廊两端共列，且触及脸全部端点改 `LocalOffset`（服从 Compose 序；不依赖全局 grid）。

## 2. FREE 分配

对每个 node + side 候选集：

1. 排除违反 fixed side、scope 出针或 group boundary 的 side；
2. 以 working 流向估计逆向出针与最小 bend；
3. **回边走廊角色**（`pick_reversed_side`）：`span≥2` → E/W；`has_twin ∧ span=1 ∧ Δorder≤1` → N/S（平行）；无 twin 短回边 → E/W；FixedSide 尊守。  
   **E/W 极性**（两端同写）：取两 real 端中 rank 更大者为 tip；`tip.order > peer.order` → East；`<` → West；同列 → East。禁止 per-end「朝 peer」。
4. 平局按固定 Side 顺序；
5. 同侧边按对侧 `(layer, order, declaration_index, EdgeId)` 排序；
6. edge_group / port_group 先折叠成一个端口单位，再分 slot；
7. 映射回 original source/target 后写 Plan。

反向边不得交换最终 source/target port；只允许 working 流向参与 side 评分。`has_twin` = 同无向端点对上存在 `!reversed` 边（与 `-->` 正交）。

## 3. Scope 与 Gate

### 3.1 Crossing 序列

给定 source scope path 与 target scope path：

```text
source leaf → ... → LCA → ... → target leaf
```

每离开或进入一个 group 生成一个 `GatePlan`。Gate 是边界 crossing 许可，不是自由端口。

```text
GatePlan
  key
  group_id
  edge_id
  kind: Leave | Enter
  boundary_side_candidates
  inside_segment / outside_segment
```

Nested crossing 的序列必须与 scope path 一一对应。组内边不得离开自身最小合法 scope；与该组无关的边不得穿越该组。

### 3.2 Gate 容量

容量需求由经过同一 gate region 的**独立 track 单位**决定，不等于跨界边条数：

- bundle/port group 可共享一部分容量；
- 不同方向或不可重合的段分别计数；
- 局部计数先聚合，再以 px 下界写 MetricBudget；
- Metric 根据最终边界长度验证容量。

## 4. Channel Substrate

> **D₁ 分阶段落地**见 [channel-d1.md](channel-d1.md)：D1.0 仅层间 TrackOrder（无全图 Substrate）；D1.1 起 root-scope-only Substrate；D1.2 接 Gate。本节描述目标全貌。

Channel 在离散骨架上搜索：

```text
SubstrateNode = rank gap × order gap × scope
SubstrateArc  = horizontal/vertical channel segment
```

组边界会切断 arc；只有 GatePlan 能连接内外 Segment。  
搜索状态至少含 `(substrate_node, incoming_direction, scope_state)`，否则 bend penalty 与 scope legality 不成立。

## 5. 搜索代价与稳定性

```text
cost =
  length_weight × logical_length
  + bend_penalty
  + congestion_cost
  + history_cost
  + boundary_crossing_cost
```

- 非法 scope arc 直接过滤，不用大罚分“尽量避免”；
- open set 平局 tuple 固定为 `(f, bends, logical_length, state_key)`；
- 邻接 arc 按 Direction 固定序 + SegmentKey；
- expansion/rip-up budget 是确定性整数；
- 禁止墙钟超时决定返回哪条 path。

## 6. TrackOrder

> D1.0 先在 `RankGap(r)` 水平走廊上落地本节子集（固定拓扑模板 + 区间着色）；完整「先搜索再着色」自 D1.1。算法细则与 Bus 边界见 [channel-d1.md](channel-d1.md) §3、§5.1。

搜索得到 L2 segment sequence 后，L3 写 track 次序：

1. 同 substrate segment 的共线区间分组；
2. 区间着色决定最少 track 数；
3. 端点相对序与 crossing constraint 决定 track 偏序；
4. 偏序环若来自硬端口序则不可行；若来自软路由偏好则按稳定政策拆线重搜；
5. Freeze 后 VPSC nudge 只能改变精确偏移，不能交换 track order。

Track identity 是 `(segment_key, track_index)`，像素坐标属于 Metric。

## 7. Bundle

Bundle 是 Plan 中的共享拓扑事实：

```text
BundlePlan
  id
  member_edges
  shared_segments
  merge_gate
  split_gate
```

- source-prefix / target-suffix 按 original edge 语义命名；
- 只有明确 edge_group 或启用自动 bundling 时生成；
- shared segment 允许几何重合，Verifier 以 BundlePlan 作为唯一豁免；
- Ink 只接合共享干线，不搜索公共前缀。

## 8. Rip-up

触发：搜索失败、track precedence 环、容量超过可扩张上界。

```text
选择受影响边集
  → 移除其 history-sensitive route
  → 增加拥塞 history
  → 按稳定优先级重搜
  → 运行局部 RouteTopology verifier
```

边选择顺序固定为：

```text
(failure_count desc, edge_priority desc, declaration_index, EdgeId)
```

达到预算后：

- 已有全部合法 path：可按 `verified-best` 返回并记录 BudgetExceeded warning；
- 任一边无合法 path：硬失败；
- 禁止退化到穿障碍单肘线。

## 9. Builtin 与 DeferToRouter

| 模式 | Compose 输出 | 后续 |
|------|--------------|------|
| Builtin orthogonal | 完整 ChannelPath + TrackOrder | Metric 坐标化；Ink 展开 |
| Builtin polyline | DirectOrVia | Ink 直线/折线展开 |
| DeferToRouter | terminals + BoundaryCrossing | 独立 Router 写 path |

Defer 模式可跳过 Hier Channel 搜索，但不能跳过端口、组框与 boundary permission。Router 不得修改端口以迁就自己的算法。

## 10. Shape port policy

内建表：`plotgram_model::policy_for(NodeShape)` → `ShapePortPolicy { allowed, preference, capacity_per_side }`。无 DSL。

| 规则 | 说明 |
|------|------|
| 写者 | 仍是 Compose I.7 `assign_ports`；表只提供默认侧策略 |
| FREE | 拓扑 `free_side` 得 primary → 在 `allowed` 内按 preference/容量选侧 |
| FixedSide | 作者胜出；不读 allowed、不计入 FREE 容量账 |
| 容量满 | **同脸软超容**：primary ∈ allowed 时不因容量换脸（Ordered 错开）；换脸仅当 primary ∉ allowed |
| `Node.shape == None` | 按 `NodeShape::DEFAULT`（`rounded_rect`）查表 |
| 非本阶段 | DSL 扩展、MetricBudget、轮廓锚点、Channel 硬容量 |

产品差异点：`Diamond` 每侧容量 1（容满仍钉拓扑 primary，扇出共底边）；`Person` 禁 North；其余多为开放四侧。
