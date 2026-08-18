# `layout/demand` — 路由压力模型

> 一句话：在**真正画边之前**，用便宜的计算估一估「哪里会挤、哪条边会难走」，把结果给布局加缝、路由打分、诊断 dump 共用。

本模块**只读、不算最终折线**。改节点位置 / 改边路径仍走 `SpaceBudget`、正交 router、pipeline；这里只提供**数字与评分**。

设计背景见：

- [`docs/architecture/方案计划/路由压力模型收缩与复用方案-2026-07.md`](../../../../../docs/方案计划/路由压力模型收缩与复用方案-2026-07.md)
- [`docs/architecture/方案计划/压力模型算法吸收计划-P1-P4-2026-07.md`](../../../../../docs/方案计划/压力模型算法吸收计划-P1-P4-2026-07.md)

---

## 1. 为什么需要它

布局管线的惯用顺序是：**先排节点，再路由边**。  
边路由（正交绕障、走廊、车道）很贵；如果布局阶段完全不管「边以后好不好走」，就会出现：层缝太窄、组间廊过载、中间挡着一排盒子——路由只能硬绕或 degraded。

`demand` 的目标是：

1. **统一说法**：层缝需求、廊容量、端口挤、障碍挡、画布热点，用同一套类型说话，避免各文件各抄一套公式。  
2. **便宜预估**：毫秒级，可在路由前反复调用。  
3. **多种消费**：dump 观测、congestion 指标、EdgeDifficulty 打分、（后续）指导加缝；路由侧也可读廊 OVER 做 soft 惩罚。

---

## 2. 目录结构

```text
demand/
  mod.rs          对外再导出
  types.rs        共享类型：Band / Corridor / Port / EdgeFeatures …
  corridor.rs     组间走廊：load / capacity / 链
  pierce.rs       L 骨架障碍穿透（线段×节点）
  grid.rs         粗网格流量（L 骨架栅格化）
  port.rs         端口侧压力（几何偏好出边侧）
  features.rs     采集 EdgeFeatures + 归一化 score
  dump.rs         环境变量 dump / PressureSnapshot
```

邻层边带的**权威公式**仍在同级的 [`edge_band_demand.rs`](../edge_band_demand.rs)；本模块再导出，诊断侧委托它，避免双轨。

```text
          ┌─────────────────────────────────────┐
          │           layout/demand             │
          │  band · corridor · port · pierce    │
          │  grid · EdgeFeatures · score        │
          └─────────────────┬───────────────────┘
                            │ 只读数字
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
   SpaceBudget 加缝    正交 scoring soft     dump / congestion
   （D4 已接廊/层缝）   （廊 OVER 可关）      / calibrate 导出
```

---

## 3. 大白话：各块算法在干什么

### 3.1 层缝需求（band）—「两层之间要过几条边」

上下两层节点之间要过很多边、还有标签时，竖直空隙不够就会挤成一团。

- **算什么**：跨邻层边数、扇入扇出、标签带宽 → 一个 `demand`（像素量级）。  
- **和谁比**：层与层之间的**面距**（上一层底边到下一层顶边）。  
- **产出**：`BandDemand`（含 `deficit = max(0, demand - gap)`）。  
- **能力**：告诉布局「这条缝至少要留多宽」；诊断里看哪些缝已经欠账。

### 3.2 组间走廊（corridor）—「组与组之间的巷子挤不挤」

架构图有 group 时，跨 leaf 的边往往要走组间「走廊」。廊有多宽、能并排几条车道，就是 capacity。

- **算什么**：跨 leaf 边走最短廊链；每条廊上经过的边数 = `load`；`capacity ≈ floor(法向间隙 / 18)`。  
- **产出**：`CorridorModel` / `CorridorDemand`；边是否无链、是否超容（`CorridorRisk`）。  
- **能力**：  
  - B2 诊断对照穿组；  
  - 布局侧可对 OVER 廊抬缝 / 请求 corridor_boost；  
  - 路由打分可对「走在 OVER 廊上」的路径 soft 罚（`TAUTCORE_CORRIDOR_SOFT=0` 可关）。

车道间距常量 **`CORRIDOR_LANE_PITCH = 18`** 以本模块为单源，正交路由 / EGB 应引用它，不要再写第三份 `18.0`。

### 3.3 端口压力（port）—「这个节点某一侧要接几根线」

还没真路由时，用端点相对位置猜「大概从哪一侧进出」，按 `(节点, 侧)` 计数。

- **产出**：`PortPressure`；边级取两端压力较大者。  
- **能力**：反映「同侧插太多针」的拥挤；进 EdgeDifficulty 的 δ 项。  
- **不是**：真实 slot 分配或 stub 去冲突（那些仍在 orthogonal 里）。

### 3.4 障碍穿透（pierce）—「直线式 L 折线会不会撞盒子」

真路由会绕障；这里只问：源中心到宿中心，用最简单的 **L 形两段正交折线**（先横后竖 / 先竖后横），会穿过几个**非端点**节点？

- **怎么测**：每段调用与 lint 同源的 `segment_intersects_node`（线段×矩形），**不做**整框 AABB 冒充穿透。  
- **定死**：`obstacle_hits = max(两种 L 的命中数)`。  
- **能力**：补「中间挡了一排盒子」这种 band/廊看不到的信号；进 score 的 ε 项。

### 3.5 粗网格（grid）—「画布上哪一片车流量大」

把每条边的一条优选 L 骨架画到粗格子上（默认 pitch 40），格子上叠的边越多越堵。

- **产出**：`GridDemand`（cell → load）；边级 `grid_overflow`（相对 soft_cap 多出的量）。  
- **能力**：画布热点视图；进 score 的 ζ 项；`TAUTCORE_DUMP_GRID_DEMAND=1` 可 dump。  
- **注意**：骨架不绕障，和 pierce 互补，不能单独当「外廊预测器」。

### 3.6 边难度分（EdgeDifficulty）—「把上面几样收成一个分数」

对每条边采集 `EdgeFeatures`，再加权求和。各维量纲不同（像素 / 边数 / 计数），先 **归一化到 0～1** 再加，避免「像素和个数直接比大小」。

```text
score ≈
  α·跨层跨度
+ β·跨组廊风险
+ γ·通道欠账（层缝 deficit 与廊 overflow 归一化后取大）
+ δ·端口压力
+ ε·L 骨架撞节点
+ ζ·网格超容
```

默认权重见 `DifficultyProfile`（可校准，**不**自动改代码里的默认值）。

---

## 4. 能提供的能力（按用法）

| 能力 | 怎么用 | 改几何吗 |
|------|--------|----------|
| 路由前压力快照 | `PressureSnapshot::compute` / `TAUTCORE_DUMP_PRE_ROUTE_PRESSURE` | 否 |
| 边难度 dump | `TAUTCORE_DUMP_EDGE_DIFFICULTY` | 否 |
| 网格 dump | `TAUTCORE_DUMP_GRID_DEMAND` | 否 |
| 拥堵样本指标 | `compute_congestion_sample_metrics`（含 score / hits / grid） | 否 |
| 校准导出 | `congestion-baseline --calibrate`（默认关 `PRESSURE_BUDGET`） | 否 |
| 布局加缝 | `SpaceBudget::enrich_from_pressure`（廊/层缝 + 边级 hits/grid；D4 P0 + P1.2/P3.2） | **是**（仅既有 budget 路径） |
| 路由边序 | `layer_order` 读难度分（同 rank 高分略提前；`TAUTCORE_EDGE_ORDER_SCORE=0` 可关） | 否（只改路由顺序） |
| 端口侧 | 默认：超载侧拒合并 + **Compact 组内 pitch 温和加大**（保留 Concentrate；`TAUTCORE_PORT_PRESSURE_SLOT=0` 可关） | 否 |
| 路由 soft | `OrthoRoutingContext::with_corridor_demands` + scoring | 否（只影响选哪条候选） |

开关：`TAUTCORE_PRESSURE_BUDGET=0` 关全部加缝；`TAUTCORE_EDGE_PRESSURE_BUDGET=0` 仅关边级 hits/grid；`TAUTCORE_EDGE_ORDER_SCORE=0` 关难度分边序；`TAUTCORE_PORT_PRESSURE_SIDE=0` 关端口超载拒合并；`TAUTCORE_PORT_PRESSURE_SLOT=0` 关同侧 slot 加大错开；`TAUTCORE_PORT_PRESSURE_PREFER=1` / `TAUTCORE_PORT_PRESSURE_RELIEVE=1` 开启更激进的换轴/分流（默认关）。

**本模块故意不做的事**：完整正交路由、可见性图、在布局循环里嵌 A\*、图名特判。

---

## 5. 观感下一刀（未做 · 只记档）

现有消费对 showcase **视觉增益有限**（缝已够 / 边序被 refine 抹平）。若以后要抠观感，优先：

1. **路径打分吃 hits / grid / score**（不只边序）——改选哪条折线；落点正交 `scoring`，soft、可关。  
2. **布局更早吃压力**——在排点 / sibling corridor 定宽时加宽组间距，而不是只靠 pre-route enrich。  

细则与约束见 [`压力模型算法吸收计划-P1-P4-2026-07.md`](../../../../../docs/方案计划/压力模型算法吸收计划-P1-P4-2026-07.md) §14。**当前不实施。**

---

## 6. 关键类型速查

| 类型 | 含义 |
|------|------|
| `BandDemand` | 一条邻层缝的 demand / gap / deficit |
| `CorridorDemand` | 一条组间廊的 load / capacity / gap |
| `CorridorModel` | 全图廊表 + 边→廊链 + 无链边列表 |
| `PortPressure` | `(node, side) → count` |
| `EdgeFeatures` | 单条边的多维特征（含 hits / grid_overflow） |
| `GridDemand` | 粗网格 cell 负载 |
| `DifficultyProfile` | α…ζ 权重 |
| `PressureSnapshot` | bands + corridor + features + scores 打包 |

---

## 7. 和周边模块的关系

| 模块 | 关系 |
|------|------|
| `edge_band_demand` | 层缝 demand **公式单源** |
| `space_budget` | 消费廊/层压力做加缝（写权出口） |
| `channel_occupancy`（B2） | 薄包装 `compute_corridor_model` + lint 对照 |
| `edge_routing_orthogonal` | 引用 `CORRIDOR_LANE_PITCH`；打分可读廊模型 |
| `refine::segment_intersects_node` | pierce 委托的障碍语义 |

---

## 8. 扩展时注意

1. 新「估难度」算法：优先产出 `EdgeFeatures` 字段或新 `*Demand` 类型，再挂进 `score` / dump。  
2. 障碍检测：委托已有线段×节点/组 primitive，不要再抄一套 AABB。  
3. 廊车道间距：只用 `CORRIDOR_LANE_PITCH`。  
4. 计时：`crate::layout::perf::Instant`（WASM 禁裸 `std::time::Instant`）。  
5. 行为改节点：必须走 `SpaceBudget` / pipeline 写权；本目录保持只读优先。
