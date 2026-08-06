# Hierarchical · D1.3 Corridor Allocator 执行方案

> 父页：[roadmap](../roadmap.md) §5 D₁ · [phases/channel-d1.md](channel-d1.md)  
> 写权尺子：[write-authority](../../write-authority.md) · [architecture](../architecture.md)  
> 状态：**可执行计划**（未实现进度日记）  
> 日期：2026-08-06

目标：把 Channel 从「空闲边界轨上的词典序 Dijkstra」升格为 **端点诱导的层间走廊分配器**，朝 yFiles Hierarchical 的层间资源池语义靠拢。  
验收盯 **flat 全量 + stress 系统性指标**，不盯单图特判；`product.refund-process` 等冲顶样例只作连带观测。

```text
痛点（showcase 证据）：
  · Cross line 0 / 末线被当作廉价水平廊 → 多 rank 回边冲顶
  · Main og=0 垄断 → 长边贴左外廊、画布拉宽
  · 声明序 commit → 密图拥塞级联
  · 回边侧别与走廊代价分裂 → 同列短回边 N/S 互穿 / 扁宽图

目标：
  Compose(... + Ports + RouteOrder) → Channel(span 亲和 + 内层优先 + rip-up)
    → DemandBoard(Corridor) → Metric(track px) → Ink(只展开)
```

---

## 0. 原则与反模式

### 0.1 必须遵守

1. **单写者** — 路径拓扑 = Channel；端口侧别 = Compose PortWriter；track 像素 = Metric；Ink 零新决策。  
2. **唯一反向 = DemandBoard** — 走廊预算在 Metric 前 freeze；禁止路由后挪节点 / Ink 钳边。  
3. **一般代价，禁止图种分支** — 代价项用 rank/order/span/拥塞表达，不 `if diagram` / 不点名 fixture。  
4. **有界返工先于真 MCF** — RouteOrder + multi-round rip-up；不上全局整数流挡闭环。  
5. **确定性** — 代价与邻接遍历保持稳定序（BTree / 显式排序）；禁止墙钟影响路径。

### 0.2 明确禁止（伪增强）

| 禁止 | 原因 |
|------|------|
| Ink / Metric 钳 `y≥0`、加 pad 掩盖冲顶 | 下游掩盖 Channel 选错轨 |
| 仅软罚 `Cross line == 0` | 特判；换层数换坑 |
| 为单 fixture 改端口或边序 | 违反 ADR-001 / 单写者 |
| 在 Ink 加 dogleg「绕开」外廊 | 发明 L2 |
| 真 MCF / 加深全局优化挡本阶段闭环 | ROI 倒置 |

### 0.3 与 D1.0–D1.2 的关系

| 已有 | D1.3 增量 |
|------|-----------|
| Substrate + 词典序 Dijkstra（折点>长度>拥塞） | 加入 **span 亲和**、**内层走廊** 代价维 |
| `prefer_outer_main` 软罚（仅 Main） | **收束 / 重定义**：外侧 = 容量溢出，非偏好 |
| 声明序 + 有界 rip-up | **RouteOrderWriter** 显式排序 + 多轮 rip-up 策略 |
| LayerGap Demand（track_count） | **Corridor Demand** 预订槽位下界 |
| 多 rank 回边 `free_side` 跨轴 | **统一侧别代价**（含短同列回边） |

---

## 1. 总览与依赖

```text
① SpanAffinity（Channel/search）
        │
        ▼
② InnerCorridorFirst（Channel/search；收束 prefer_outer_main）
        │
        ▼
③ RouteOrderWriter + MultiRoundRipUp（channel/route_all）
        │
        ▼
④ CorridorDemandBoard（Compose/Channel → Metric）
        │
        ▼
⑤ UnifiedBackEdgeSideCost（compose/ports；读 occupancy 快照或预估）
```

| 步 | 名称 | 主写者 | 依赖 | 预估粒度 |
|----|------|--------|------|----------|
| **①** | Rank-span / 几何亲和代价 | Channel `search` | D1.1+ | S |
| **②** | 内层走廊优先 | Channel `search` + hints | ① | S |
| **③** | RouteOrder + 多轮 rip-up | Channel `route_all` | ①② | M |
| **④** | Corridor DemandBoard | Demand + Metric `track` | ①–③ 可并行设计，落地宜在 ③ 后 | M |
| **⑤** | 回边侧别统一代价 | Compose `ports` | ④ 的 occupancy 摘要更佳；可先用静态启发 | M |

**串行理由**：①② 改搜索目标函数，不改 commit 序；③ 改谁先占轨；④ 把占用变成缝宽预算；⑤ 在走廊语义稳定后再动端口，避免端口与搜索互相追打。

---

## 2. 步 ① — Rank-span / 几何亲和代价

### 2.1 问题（第一性原理）

边的合法廉价水平廊应落在端点跨层区间邻近的层缝；栈顶/栈底 Cross 仅当端点本身贴外沿才自然。当前搜索无 span 距离项 → 空闲的 Cross k=0 与中间层缝 **等代价**，稳定选到冲顶。

### 2.2 写者与接口

- **写者**：`channel/search.rs` 的词典序代价（扩展一维，或并入既有 soft 维且保持字典序优先级）。  
- **输入**（每边已知）：`src_rank`, `tgt_rank`（working 方向）、`src_order`, `tgt_order`（可选，Main 亲和用）。  
- **禁止**：改端口；改 Metric 外沿公式当主修；Ink 裁 y。

### 2.3 算法要点

对候选 track `t`：

**Cross（水平廊）**

```text
lo = min(src_rank, tgt_rank)
hi = max(src_rank, tgt_rank)
# Cross line k 服务的层缝语义：k=0 在 L0 之上；k∈[1,R] 在 L(k-1)–L(k) 之间；k=R+1 在末层之下
# 边的「自然带」= 闭区间 [lo, hi] 对应的层缝集合 S = {lo, lo+1, …, hi+1}（含两端外缝仅当需要 stub）
span_dist(k) = 0           if k ∈ [lo+1, hi]     # 严格内层缝（两端点之间）
             = 1           if k ∈ {lo, hi+1}     # 端点邻接外缝（出入针常用）
             = 1 + (k 到 [lo, hi+1] 的距离)      # 远离跨层带
```

代价并入词典序：**折点 > 长度 > span_dist（或与 congestion 合并的加权，但 span 不得被拥塞完全淹没）**。  
具体 lex 维顺序开工时钉死并写进 `channel-d1` / 单测；推荐：

```text
bends ≻ length ≻ span_affinity ≻ congestion ≻ soft_penalties
```

**Main（竖直廊）**

```text
# order_gap og 相对端点 order 的距离
order_lo = min(src_order, tgt_order)
order_hi = max(src_order, tgt_order)
main_dist(og) = 0  if og ∈ [order_lo, order_hi+1]   # 端点列之间的内侧间隙（含两侧脸）
              = |og 到该区间的距离|
```

同列端点（`order_lo == order_hi`）时，`og ∈ {order_lo, order_lo+1}` 为内侧脸；`og==0` / `og==order_count` 仅当区间触边才零代价。

### 2.4 验收

| 门禁 | 标准 |
|------|------|
| 单测 | 表驱动：跨层边在空图上优先选内层缝 Cross，不选 k=0（除非端点在 L0） |
| Showcase | `product.refund-process` e8、`demo.flat-realtime-recommendation` 长回边：**track 序列不含 Cross k=0**（除非端点 rank 迫使） |
| 回归 | `hier_eval`：flat 冲顶类 fixture 的 `max_bends` / `sum_bends` 不升；`reversed_count` 不变 |
| 观测 | 全 flat 统计「path min_y≈0 且端点 mid-graph」边数下降 |

### 2.5 刻意不做

- 禁止列表硬编码「永远禁用 k=0」（端点在顶层时 k=0 合法）。  
- 不在本步改 `prefer_outer_main`（留给 ②）。

---

## 3. 步 ② — 内层走廊优先（收束 prefer_outer_main）

### 3.1 问题

`prefer_outer_main` 把 **外侧 Main 当偏好**，与 yFiles「层间通道优先、外侧为溢出」相反；且只作用于 Main，不约束 Cross。① 之后 Cross 冲顶缓解，但长边仍可能偏好 og=0。

### 3.2 写者与语义重定义

- **写者**：`channel/search.rs` soft / hints；`route_all.rs` 的 `RouteHints` 字段语义变更。  
- **新语义**：

```text
InnerCorridorFirst:
  · 默认代价：内侧 order-gap（端点列之间） < 外侧 og=0 / og=order_count
  · 外侧仅当内侧拥塞超过阈值（或 span 迫使）才可竞争
  · reversed / multi-rank 边：不再「偏好外侧」；改为「允许外侧作为溢出」
```

- **迁移**：删除或反转 `prefer_outer_main` 布尔；若保留字段，bind 层改为 `outer_main_as_overflow: true`（默认），禁止再表达「prefer outer」。  
- **与 Compose 关系**：多 rank 回边的 W/E 端口（`free_side`）**保留**——侧别仍由 PortWriter 写；Channel 只决定走哪条 Main 间隙。

### 3.3 算法要点

```text
outer = (og == 0 || og == order_count)
inner = !outer && og ∈ [order_lo, order_hi+1]

soft:
  if inner           → 0
  if outer && 内侧仍有空闲容量 → 正惩罚（明显大于旧 0.25，需标定）
  if outer && 内侧已饱和     → 0 或小惩罚（溢出合法）
```

「内侧饱和」可读当前边搜索时的 congestion map（D1.2 已有）；阈值用整数占用，确定性。

### 3.4 验收

| 门禁 | 标准 |
|------|------|
| 单测 | 同列长边空图：选内侧 Main，不选 og=0 |
| Showcase | `main_og0` 长竖跨边计数在 flat 上下降（相对 ① 后基线） |
| 回归 | 回边仍可在内侧饱和时走到外侧（`smoke.multi-rank-backedge` 仍可行，不硬失败） |
| 观测 | `product.typical-microservice-architecture` / `demo.software-release` 图宽收缩 |

### 3.5 刻意不做

- 不在 Metric 把 og=0 的 X 强行内推。  
- 不改端口侧别（⑤）。

---

## 4. 步 ③ — RouteOrderWriter + 多轮 rip-up

### 4.1 问题

`route_all` 按声明序 commit → 先到边霸占内侧轨 → 后到边被迫外廊（e8→e9 级联）。这不是搜索目标函数能单独修的。

### 4.2 写者

- **新自由度**：边的路由提交顺序。  
- **写者**：`channel/route_all.rs` 内显式 **RouteOrderWriter**（函数或模块），产出稳定的 `Vec<EdgeId>`。  
- **禁止**：用声明序当默认真源；用墙钟 / HashMap 迭代序。

### 4.3 排序键（确定性）

推荐词典序（全稳定、可测）：

```text
1. critical 降序（已有 Edge.critical）
2. span = |src_rank - tgt_rank| 降序（长跨层先占内层缝）
3. reversed 优先（回边先于普通长边，减少回边被挤出）
4. 估计宽度 / dummy 链长度 降序
5. 声明序升序（最后 tie-break）
```

键的精确元组开工钉死；变更必须更新单测与 debug trace 字段。

### 4.4 多轮 rip-up

在 D1.2 有界 rip-up 上扩展策略（非无限重搜）：

```text
round 0: 按 RouteOrder 全量搜 + commit
round 1..R:
  选牺牲边：高拥塞轨上、且 span 较小或非 critical 的边
  释放其 track 占用 → 按 RouteOrder 重搜
  接受条件：全局 lex 指标不劣（bends/cross 代理：占用冲突数、或 InkVerifier 前的轨冲突计数）
预算：最大边×轮次整数上限（已有 expansion 风格）
```

失败：超预算 → 保留 round-0 可行解 + `LayoutDiagnostics.relaxations` 记录未消冲突（与阶段 C 通道对齐）。

### 4.5 验收

| 门禁 | 标准 |
|------|------|
| 单测 | 同构图不同声明序 → **相同** RouteOrder 与相同 track 分配 |
| Showcase | `stress.layout-stress-flat-mesh` / `stress.layout-stress-yfiles-pipeline` crossings **可观测下降**（不要求一次达标 yFiles） |
| 回归 | 简单链 / fan smoke 几何零回归或仅观测级 bbox 变 |
| 诊断 | rip-up 轮次与牺牲边进 diagnostics / debug trace |

### 4.6 刻意不做

- 真 MCF / 全局 ILP。  
- 为降 cross 在 Ink 拆线。

---

## 5. 步 ④ — Corridor DemandBoard

### 5.1 问题

即使拓扑选对，层缝像素可能仍按「最少 track」挤；外侧 track 数未预订 → Metric 事后才发现空间不够。写权要求：走廊需求在 Metric **前** freeze。

### 5.2 写者与 epoch

对齐 [coordinate-and-demand.md](coordinate-and-demand.md) / architecture §3.3：

```text
Producer（Channel commit 后、Metric 前）:
  对每个 Cross line k / Main og:
    demand = f(占用边数, edge_gap, 端点 stub)
  → DemandBoard.max 合并

Consumer（Metric track.rs / main_axis layer gap）:
  freeze 后读下界；扩大 LayerGap / 外侧裕度
  禁止 Metric 后再写 Demand
```

- **键**：至少 `LayerGap(rank_gap_index)`；可选 `MainOuterMargin(side)`。  
- **与 D1.0**：扩展既有 `track_count × edge_gap` → 区分 **内层缝 vs 外沿缝** 的不同下界策略（外沿缝需求高时优先加内侧缝宽，而非一味外推画布）。

### 5.3 算法要点

```text
对每条已 commit 的 ChannelPath:
  统计每条 track 的占用
聚合:
  LayerGap[k] >= edge_gap * max(1, track_count[k])   # 已有
  # 新增：若仅外沿 track 占用高而内层空，不额外抬外沿——由 ①② 保证少用外沿；
  # 若内层 track_count 高 → 抬对应 LayerGap
```

自环 / 标签 reserve **不在本步**（属 E / loop reserve）；本步只服务 Channel 已选轨。

### 5.4 验收

| 门禁 | 标准 |
|------|------|
| 单测 | 多边共一层缝 → LayerGap 下界随 track_count 单调升 |
| Showcase | 密扇出 fixture 节点不穿轨（InkVerifier）；图宽不再无故膨胀 |
| 契约 | freeze 后再写 Demand → debug assert / 测试失败 |
| 回归 | `hier_eval` bbox 观测；bends 门禁不升 |

### 5.5 刻意不做

- finalize 加 padding 冒充走廊预算。  
- 路由阶段回调改 node frame。

---

## 6. 步 ⑤ — 回边走廊角色（非一刀切 E/W）

### 6.1 问题

- 有 dummy 的多 rank 回边：应走侧廊 E/W。  
- 同列 req-resp **双胞胎**（存在 `!reversed` 对边）：应走脊 N/S 平行（`three-tier` / `user-auth`）。  
- 无 twin 的短反馈回边：应走侧廊，避免与主流程互穿。  
早期「同列默认 East」恒胜会毁掉平行美学；全 N/S 又会抬高审批流 crossings。

### 6.2 写者

- **写者**：`compose/ports.rs`（PortWriter）。  
- **只读输入**：层内 order、`reversed`、span、`has_twin`（同无向端点对上存在 `!reversed` 边）、脊面 FREE 占用（`ns_load`）——不得在 ports 里跑完整 Channel 搜索。  
- **禁止**：Channel 改 side；Ink 改 side；用 `-->` / 图名特判。

### 6.3 算法要点（走廊角色表）

对每条边两端独立决议（FIXED_* 尊守）：

```text
FixedSide                         → 尊守
span ≥ 2                          → cross_axis (E/W)
has_twin ∧ span=1 ∧ Δorder≤1      → rank_dir (N/S)   # 平行美学
¬twin ∧ span=1                    → cross_axis       # 短反馈默认侧廊
其余                              → 软代价 argmin；平局跟已上规则
```

`Arrow::Response` **不**进决策（与 FAS `reversed` 正交）。

### 6.4 验收

| 门禁 | 标准 |
|------|------|
| 单测 | 表驱动：twin 短→N/S；span≥2→E/W；无 twin 短→E/W；FixedSide 不被覆盖 |
| Showcase A | `product.three-tier` 响应边 N/S 平行，非默认右绕 |
| Showcase B | `smoke.multi-rank-backedge` 长回边仍 E/W |
| 回归 | `hier_eval`；`user-auth` twin 优先 N/S（crossings 允许相对全 EW 小幅回升） |
| 写权 | ports 不调用 Channel |

### 6.5 刻意不做

- 逐 fixture 特判侧别。  
- 在 Channel 失败后回写端口（破坏单写者）。  
- 再用常数把 EW 或 N/S 写成恒胜。

---

## 7. 横切：观测、基线、文档

### 7.1 新增 / 扩展观测（建议进 debug trace 或 hier_eval 旁路统计）

| 指标 | 含义 |
|------|------|
| `cross0_midgraph_edges` | 端点非顶层却使用 Cross k=0 的边数 |
| `main_og0_long_edges` | Main og=0 且竖跨 ≥2 层的边数 |
| `route_order_hash` | RouteOrder 稳定指纹（③） |
| `corridor_demand_frozen` | Demand freeze 摘要（④） |

不把观测指标当硬门禁，除非连续两步后稳定。

### 7.2 基线流程

每步结束：

```text
HIER_EVAL_WRITE_BASELINE=1 cargo test -p plotgram-compile --test hier_eval
```

仅当 bends / reversed_count **有意**改善或持平时更新；交叉 / bbox 作观测 delta。

### 7.3 文档同步

| 文档 | 动作 |
|------|------|
| 本文 | 真源执行方案 |
| [channel-d1.md](channel-d1.md) | 每步落地后补「D1.3.x 已交付」摘要（不写日记） |
| [roadmap.md](../roadmap.md) | D₁ 表增加 D1.3 一行 |
| [architecture.md](../architecture.md) | 代价维 / RouteOrder 写者写入写者地图（步 ③⑤ 后） |

---

## 8. 里程碑切片（建议 PR 边界）

| PR | 内容 | 可单独合并 |
|----|------|------------|
| **D1.3.1** | ① SpanAffinity + 单测 + showcase 观测 | 是 |
| **D1.3.2** | ② InnerCorridorFirst（含 `prefer_outer_main` 迁移） | 是（依赖 ①） |
| **D1.3.3** | ③ RouteOrderWriter + 多轮 rip-up + diagnostics | 是 |
| **D1.3.4** | ④ Corridor DemandBoard 扩展 | 是 |
| **D1.3.5** | ⑤ UnifiedBackEdgeSideCost | 是 |

每 PR：**算法能力陈述写在 PR 描述「为何不是补丁」**；禁止夹带 Ink 钳边。

---

## 9. 成功图像（相对 yFiles 的阶段性对齐）

完成本方案 ①–⑤ 后，Hier 应达到：

1. **走廊分配**：边优先消耗端点跨层带内的层间资源，而不是画布外框。  
2. **拥塞**：长/critical/回边先占内层；短边可被 rip-up；密图 crossings 系统性下降。  
3. **端口与走廊同模型**：短/长回边侧别由统一代价决议，Channel 只消费侧别。  
4. **写权闭合**：Demand 预订 → Metric 发布 → Ink 展开；无事后几何补丁。

**仍不宣称对齐**（留给 D₂ / E）：组框 VPSC 真源、PartitionGrid、StrongMacro、integrated labeling、真 MCF、octilinear。

---

## 10. 开工检查清单（步 ①）

- [x] 在 `channel/search.rs` 钉死 lex 维顺序与 `span_dist` 公式（含 orientation 无关的 rank/line 映射）  
- [x] 表驱动单测：空图跨层边不选 k=0  
- [x] 跑 flat 全量 debug 统计 `cross0_midgraph_edges` 前后对比（`product.refund-process` e8：`[10,0,9]` → `[10,2,9]`，path 不再触 y=0）  
- [x] 更新 `hier_eval` 基线（refund `max_bends`/`sum_bends` 下降）  
- [ ] PR 描述引用本文 §2，声明非补丁  

**D1.3.1 已交付**（2026-08-06）：`LexCost` 为 `bends ≻ length ≻ span_affinity ≻ congestion`；`RouteHints.span` 由 `route_all` 写入端点 rank/order；`cross_span_dist` / `main_span_dist` 为公开公式。

**D1.3.2 已交付**（2026-08-06）：`prefer_outer_main` → `outer_main_as_overflow`；外侧 Main 在内侧 order-gap 仍空闲时受软罚，内侧饱和后允许溢出。

**D1.3.3 已交付**（2026-08-06）：`compute_route_order`（critical↓ span↓ reversed↓ dummy↓ decl↑）；commit 按 RouteOrder；rip-up 优先牺牲短 span / 非 critical；`ChannelDebug.route_order`；stress crossings 下降（flat-mesh −2，yfiles-pipeline −6）。

**D1.3.4 已交付**（2026-08-06）：`demand::DemandBoard`（`DemandKey::LayerGap` max-merge + freeze）；`publish_channel_layer_gap_demand` 仅内层缝；外沿 Cross 不抬 LayerGap；freeze 后再 publish / 未 freeze 就 resolve → panic。

**D1.3.5 已交付**（2026-08-06；2026-08-06 修订为走廊角色表）：`pick_reversed_side` 按 twin/span/Δorder 分脊（N/S）与侧廊（E/W）；twin 短回边平行；长/无 twin 短回边走侧廊；FixedSide 尊守。

---

## 11. 参考

- Showcase 根因样本：`apps/showcase/hierarchical/flat/product.refund-process.pgm`（Cross k=0 + y=0）  
- 代价现状：`crates/plotgram-layout/src/layout/hierarchical/channel/search.rs`  
- 路由序现状：`.../channel/route_all.rs`  
- 外沿坐标：`.../metric/track.rs`（本方案主路径不改公式；④ 只扩 Demand）  
- yFiles 层间路由理念：`docs/reference/yFiles-layouts-and-routing.md` · `notes/from-yfiles-reference.md`
