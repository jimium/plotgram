# Hierarchical · D₁ Channel 分阶段契约

> 父页：[architecture](../architecture.md) §6 · [roadmap](../roadmap.md) §5 D₁  
> 相级总览：[ports-and-channel](ports-and-channel.md) · [coordinate-and-demand](coordinate-and-demand.md) · [ink-and-verification](ink-and-verification.md)  
> 写权尺子：[write-authority](../../write-authority.md)  
> Atlas 形状参考（非 schema 真源）：[atlas-reference/channel-substrate](../atlas-reference/channel-substrate.md) · [channel-search](../atlas-reference/channel-search.md)  
> 状态：可执行设计契约；不记录实现进度

本文把 roadmap **D₁ · Channel 正交**切成三个可串行交付的子里程碑，钉死写权、IR、管线序、Bus 边界与验收。发生冲突时以 [architecture.md](../architecture.md) 为准。

```text
今日: Compose(ports, BusPrefix?) → Metric(frames, bus_y?) → Ink(append_bend mid_y | bus path)
目标: Compose(... + RouteTopology + TrackOrder) → Metric(DemandBoard → track px) → Ink(只展开)
```

痛点真源（D1.0 触发）：`ink/route.rs` 的 `append_bend` 用 `(from.y+to.y)/2`，同层跨边共 `mid_y`，关 `auto_edge_grouping` 后呈「假 bus」。根治归 **L3 TrackOrder**，禁止在 Ink 抖 mid_y。

---

## 1. 写权（L2–L5）

| 层 | 自由度 | 写者 | 禁止 |
|----|--------|------|------|
| L1 | 端口 side + `along_spec` | Compose PortWriter（I.7，已落地） | Channel / Ink 改端口 |
| L2 | 路径拓扑（走哪些段 / 转弯） | Compose RouteTopoWriter / Channel search | Ink / nudging 改走向 |
| L3 | 走廊内 track 次序 | TrackOrderWriter | VPSC nudge 交换次序；Ink 发明轨号 |
| L4 | track / 节点 / 层缝精确像素 | Metric CoordWriter | 穿到另一侧（应上提） |
| L5 | 圆角 / 箭头 / 共线去噪 | Ink | 猜中点、加 dogleg、选另一条 channel |

**DemandBoard** 是唯一合法上游反馈（H3）：Channel 在 Metric 求解前写 `LayerGap` 等下界；禁止运行后回调改 Plan。

**失败**：无合法 path / track 偏序硬环 → 硬失败。禁止 Ink 穿障单肘线兜底。

---

## 2. IR（收敛 architecture §3.1）

Plan 稳定字段（不照搬 Atlas `channels` / `lane_indices` 平行命名）：

```text
RouteTopology =
  Orthogonal(ChannelPath)
  | Polyline(DirectOrVia)
  | Deferred

ChannelPath {
  segments: [SegmentRef]     // 有序；含起止 stub 宿主
  // D1.1+ 可附 gates: [GateKey]；D1.0 为空
}

TrackOrder {
  // SegmentKey → 该段上各边的 track_index（稠密、稳定）
  // track_index 非像素；像素由 Metric 发布
}

BundlePlan {                 // D1.2 升格；D1.0 过渡用 BusPrefix
  id
  member_edges
  shared_segments
  merge_gate? / split_gate?
}

CorridorId = RankGap(r)      // D1.0 走廊键；D1.1+ 升格为 Substrate SegmentKey
```

- **Plan 不嵌入 Substrate**：Substrate 由 blueprint 重建；Plan 可存 sketch（rank/order 计数）供 debug。
- **Lane** = `track.coord + lane_index × pitch`，导出量，非搜索变量。
- Polyline / Deferred 不经 Channel L2/L3；orthogonal 进入 Ink 前必须有完整 topology + track order + track coordinate（见 [ink-and-verification](ink-and-verification.md) §1）。

---

## 3. 与 BusPrefix / auto_edge_grouping 边界

| 模式 | 语义 | 写者 | 几何 |
|------|------|------|------|
| `auto_edge_grouping: false` | 独立边；水平段**分轨** | TrackOrderWriter | 外层/内层不同 Y（对齐 yFiles） |
| `auto_edge_grouping: true` | 同 `(node, N\|S)` FREE 簇合流 | Compose `BusPrefix`（D1.0）→ D1.2 `BundlePlan` | **故意共线**干线；Verifier 唯一豁免 |

二者不得互相冒充：

- 关 grouping 后共线 = **缺陷**（缺 L3），不是「免费 bus」。
- 开 grouping 后分轨 = **破坏合流事实**，除非显式降级并写 `relaxations`。

D1.0：`BusPrefix` 成员共享同一 `track_index`（或等价 Bundle 豁免），**不进入**分轨区间着色。

---

## 4. 管线插入点

```text
Compose
  I.7  Port finalize（已有）
  I.8  Gate derive          ← D1.2（D1.0/1.1 跳过；跨组保持今日「允许」）
  I.9  Route topology       ← D1.0: 模板 + TrackOrder；D1.1+: Substrate 搜索 + TrackOrder
  I.10 Freeze + PlanVerifier

Metric
  II.0 Freeze MetricBudget  ← Channel track_count × edge_gap → LayerGap
  II.1–II.2 主轴 / 次轴（layer_gap 含 demand）
  II.3 publish track_coord[segment][track_index]；port_points
  II.4 MetricVerifier

Ink
  III.1 读 RouteTopology + track_coord 展开（禁止 mid_y 缺省）
  III.2 bundle / bus 干线接合
  III.3–III.4 装饰 + InkVerifier
```

代码插缝（实现时）：`hierarchical/mod.rs` Compose 末（ports 之后）写 topology/track；Metric 发布 track 像素后 Ink 替换 `append_bend` mid_y 路径。

---

## 5. 子里程碑

```text
D1.0 LayerGap TrackOrder  →  D1.1 Substrate Search  →  D1.2 Gate + RipUp + Params
```

### 5.1 D1.0 · 层间走廊 TrackOrder（最小闭环）

**拥有自由度**：L3 + 极简 L2（固定拓扑模板，**非** A*）。

**拓扑模板**（orthogonal；相邻层 / proper hop；无 grouping）：

```text
Port → VerticalToTrack → HorizontalOnTrack(corridor, track_index) → VerticalToPort
```

直竖边（两端 cross 相同）不占水平 track。

#### TrackOrder 算法

1. 按 `RankGap(r)` 收集需水平 jog 的边。
2. 每条边占用区间（cross 轴）= `[min(src_cross, tgt_cross), max(...)]`。  
   D1.0 实现：先用 base `layer_gap` 跑一轮次轴得像素帧，再在该帧上着色；随后 Demand 撑开层缝并重算主轴（cross 不变）。不得在 Ink 改 `track_index`。
3. `plotgram-algo::interval_color` 求最少 track 数。
4. 偏序 / 外内轨：源与汇端口 `Ordered.slot` + `(declaration_index, EdgeId)` 稳定决定「外轨优先于内轨」（对齐 yFiles 嵌套横杠）。
5. 硬端口序成环 → `Infeasible`；本阶段无软偏好环、无 rip-up。
6. `BusPrefix` 成员共享 `track_index`，不参与分轨着色。

#### DemandBoard（最小）

生产者写：

```text
LayerGap { before: r, after: r+1 } ≥
  layer_gap + max(0, track_count - 1) × edge_gap
```

Bundle / Bus 共享轨只计 **1** 个容量单位。恢复 `edge_gap` bind 并真正消费（pitch 真源）；键进入 `params_hash`。

#### Metric / Ink

- Metric：发布 `track_coord[RankGap(r)][track_index]`；grouping ON 与 `bus_y` **共用同一走廊坐标系**（此时 `track_count = 1`）。
- Ink：orthogonal 非 bus 路径只读 track 坐标展开；**删除**对该类边的 `mid_y` 发明。`polyline` / `curved` / `selfloop` 本阶段不动。

#### D1.0 明确不做

Substrate 全图、Gate、ScopeMask、rip-up、多 rank 回边外侧走廊、`min_first/last_segment`、`bus_routing`、octilinear、`BundlePlan` 升格。

#### 验收

| 项 | 标准 |
|----|------|
| grouping off | `flat/smoke.fan-out-four.pgm`（及同类扇出）水平轨主轴坐标两两可区分 |
| grouping on | 同簇共享干线；现有 bus fixture 几何语义不变 |
| 回归 | `hier_eval` 硬不变量全绿 |
| 纪律 | Ink 无 mid_y 缺省；缺 track → `InternalInvariant` |
| 参数 | `edge_gap` bind 恢复且被 LayerGap demand 消费 |

---

### 5.2 D1.1 · 顶层 Substrate + Channel 搜索

**实现状态（代码）**：`crates/plotgram-layout/.../hierarchical/channel/` — root-scope Substrate + 词典序 Dijkstra；Ink 展开 `ChannelPath`；debug `channels` 非 null。

- 从 `PlanGraph` ranks / orders 派生 **root-scope-only** Substrate。奇偶 `ext` 编码与切割规则参考 Atlas；**无组边界切割**，Segment 仍注册但 `scope = None`。
- L2：词典序 Dijkstra；代价键 **折点 > 长度 > 拥塞软偏好**；邻接按 TrackId 稳定序。
- 产出完整 `RouteTopology::Orthogonal(ChannelPath)`；相邻层单 Cross 轨是「直连层间 hop」特化。
- TrackOrder 泛化到任意 substrate track（仍区间着色）；Cross 轨 lane 数回写 RankGap Demand。
- **Gate / ScopeMask 仍不做**；跨组边保持今日「允许」语义，由 D1.2 / D₂ 接管。
- 非法 scope arc 在本阶段不适用；仍禁止墙钟超时决定返回哪条 path。

#### 验收（增量）

- 多 hop / 多拐 orthogonal 边有显式 `ChannelPath`，Ink 不再对 dummy 链猜肘点。
- 同 segment 并发边 track 隔离；Demand 撑开对应层缝。
- debug：`ChannelDebug` 可投影 substrate sketch + path + track_index（不再恒 `null`）。

---

### 5.3 D1.2 · Gate、rip-up、参数闭环

#### Gate 与 D₂ 边界

| 职责 | 归属 |
|------|------|
| GatePlan 派生、ScopeMask、路径级穿组 verifier | **D1.2 Channel** |
| 组框像素进 VPSC、finalize 不重算组框 | **D₂**（可并行消费同一 Gate IR） |

穿组三道防线（architecture §6.3）：构建期拒非法 link → 搜索期 ScopeMask 硬过滤 → 检查期 `verify_no_group_penetration` / `ink_verify` 硬 FAIL。Gate 容量 ≠ 跨界边条数。

#### Rip-up

触发：搜索失败、track precedence 环、容量超过可扩张上界。

```text
边选择序 = (failure_count desc, edge_priority desc, declaration_index, EdgeId)
预算 = 整数 expansion / reroute 轮数（非墙钟）
```

达到预算：全部有合法 path 可 `verified-best` + `diagnostics.relaxations`；任一边无 path → 硬失败。

#### 参数真消费（对照 [edge-parameters](../edge-parameters.md) §4）

| 参数 / 行为 | 写者 | 子里程碑 |
|-------------|------|----------|
| `edge_gap` | Track pitch / Demand | **D1.0** |
| 回边外侧走廊（默认，无布尔开关） | Channel L2 | D1.2（D1.1 可先占位代价） |
| `min_first_segment` / `min_last_segment` | Channel 搜索约束 | D1.2（此前 bind unsupported） |
| `bus_routing` | Channel track 干线 + Ink | D1.2 |
| critical → rip-up priority | Rip-up 排序 | D1.2 |
| `BusPrefix` → `BundlePlan` 后缀合流 | Compose + Ink 接合 | D1.2 |
| `octilinear` | Channel 45° 骨架 | D1.2 后或阶段 E |

---

## 6. 决议（消解文档张力）

| # | 张力 | 决议 |
|---|------|------|
| 1 | Atlas `channels`+`lane_indices` vs architecture `routes`+`track_order` | **采用 architecture**；Atlas 仅形状参考 |
| 2 | Segment「不可省」vs MVP 无组 | Segment **保留**；D1.0/1.1 用 root-scope-only，Gate **后置**到 D1.2 |
| 3 | D₁ 与 D₂ 二选一 vs Channel 要 Gate | D1.0/1.1 **不依赖** Gate；D1.2 消费 Gate IR，组框像素写权仍属 D₂ |
| 4 | DemandBoard 未立 vs track demand | D1.0 **同步立** MetricBudget 最小 Board（至少 `LayerGap`） |
| 5 | bus v1 单 `bus_y` vs 非 bundle 不重合 | grouping ON = Bundle/Bus 豁免；grouping OFF 必须分轨 |
| 6 | `along_offset` 写权 | 像素归 Metric；Plan 只持 `along_spec` / Ordered slot |

---

## 7. 非目标（整个 D₁）

- Hier 主路径 OVG / 真全局 MCF
- Ink 发明端口、狗腿、mid_y 特判
- 布尔 `backloop_routing` 开关
- 图种分支第二布局器（ADR-001）
- StrongMacro / PartitionGrid（阶段 E / 另册）
- 复制 Atlas 源码进重建 crate；只引用 `atlas-reference/`

---

## 8. 下游只读字段

| 消费者 | 只读 |
|--------|------|
| Metric | `RouteTopology`、`TrackOrder`、`BundlePlan`/`BusPrefix`、DemandBoard freeze 后的 LayerGap |
| Ink | 同上 + Metric `track_coord` / `port_point` / `bus` 级别 |
| Verifier | Plan + Metric；非 bundle 不完全重合；orthogonal 缺 track → FAIL |
| Debug | Substrate sketch、path、track_index、occupancy（实现后） |

---

## 9. 相关文件（实现时）

| 路径 | 角色 |
|------|------|
| `crates/plotgram-layout/.../hierarchical/mod.rs` | 管线插缝 |
| `compose/ports.rs` | Port + BusPrefix；D1.0 后接 TrackOrder |
| `metric/bus.rs` / `main_axis.rs` | track / bus 像素与 layer_gap |
| `ink/route.rs` | 删除 mid_y 发明；展开 ChannelPath |
| `params.rs` | `edge_gap` 恢复消费；D1.2 段长键 |
| `plotgram-algo::interval_color` | L3 着色 |
