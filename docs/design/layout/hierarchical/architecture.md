# Hierarchical · 目标架构设计

> 状态：**现行目标架构 v1**（驱动重建；非当前能力声明）  
> 日期：2026-08-01  
> 引擎注册名：`hierarchical`  
> 代码落点：`crates/plotgram-layout/src/layout/hierarchical/`  
> 约束入口：[写权纪律](../write-authority.md) · [AGENTS.md](../../../../AGENTS.md) §1  
> 证据与启发：[from-yfiles-reference](nodes/from-yfiles-reference.md) · [`docs/reference/yfiles/`](../../../reference/yfiles/00-索引与阅读指南.md) · v1 Atlas（功能真源，非目录真源）

本文钉死 Hier 的**目标形态与跨相契约**：管线、IR、算法选型、通道路由、组/分区、参数、Stage、验真与落地顺序。  
相级细节见 [`phases/`](phases/README.md)；实现进度不进本文，当前实现能力以代码与里程碑验收为准。

姊妹页：[README](README.md) · [scope](scope.md) · [shared/partition](../shared/partition.md) · [debug-profile](debug-profile.md)（hier Trace 扩展；壳见 [../debug-inspector.md](../debug-inspector.md)）

---

## 0. 一句话目标

```text
Sugiyama（分层有向流）
  + 组合相一次写完拓扑决策（含端口 / gate / channel）
  + 度量相一次写出坐标与 track
  + Ink 只展开、零新决策
  + 通道路由（非 OVG）作正交主路径
  + 参数化 profile / preset（引擎无图种分支）
```

**不做**：TSM 当流程图主路径；独立 `ArchitectureLayout`；Ink 发明端口/狗腿；三套平行组合宇宙。

---

## 1. 硬约束（不可协商）

| # | 约束 | 含义 |
|---|------|------|
| H1 | **单写者** | 每个几何自由度有且仅有一个写者 |
| H2 | **落笔零新决策** | Ink / 事后修只展开 Plan+Metric |
| H3 | **下游不推翻上游** | 唯一反向通路 = **DemandBoard**（上游执行前 `max` 合并） |
| H4 | **确定性** | 禁止依赖 `HashMap` 迭代序；平局显式 tie-break |
| H5 | **无图种分支** | 引擎只认算法名 + typed params（ADR-001） |
| H6 | **一个组合相** | Weak / StrongMacro 是收缩**参数**，产出类型一致 |

判断句：修法若是「在 Ink 再加特判」→ 先问该自由度的写者是谁。

### 1.1 文档状态词

| 词 | 含义 |
|----|------|
| **目标** | 本文要求最终必须满足；不代表代码已经实现 |
| **已落地** | 有代码入口且行为被消费；只有 bind/parse 成功不等于算法能力已落地 |
| **过渡** | 可运行的 stub；不得作为目标契约继续叠特判 |
| **后置** | 目标允许但不阻塞主路径；未实现时必须显式 `Unsupported`，不得静默忽略 |

对外能力以「已通过对应里程碑门禁」为准。能绑定参数但参数未被消费，视为**未支持**。

---

## 2. 总体架构

### 2.1 三层运行时 + Stage 外壳

```text
                    ┌─ OrientationStage（核心只实现 TB）
                    ├─ SelfLoop / ParallelEdges / Components …
Domain / DSL ──────►│
  profile expand    │   HierarchicalCore
  LayoutContract ──►│     Compose → Metric → Ink
  (无 profile 名)   │
                    └─ Normalize / 诊断
                         │
                         ▼
                   LayoutOutput → optional EdgeRouter → finalize（只读拼装）
```

| 层 | 输入 | 输出 | 性质 |
|----|------|------|------|
| **Compose（组合）** | Graph + typed params/LayoutData + sizes 预算 | **`Plan`** | 离散决策；可哈希；稳定序 |
| **Metric（度量）** | Plan + NodeSizes + DemandBoard | **`Metric`** | 一次求解；无搜索分支 |
| **Ink（落笔）** | Plan + Metric | **边折线 / 装饰** | 纯函数展开 |

外部 **`edge_routing: Some(...)`** → `EdgeGeometryMode::DeferToRouter`：Compose 仍写端口；Ink 不写路径；独立 `EdgeRouter` 只读节点、组框、端口与路由边界约束。

### 2.2 Facade / Router 边界（目标契约）

现有 `LayoutOutput { nodes, edges }` 只是过渡接口，装不下本文的 Metric 真源。目标接口至少承载：

```text
LayoutOutput
  nodes:        NodePlacement[]        # Metric 写
  groups:       GroupPlacement[]       # Metric 写；finalize 不重算
  edges:        EdgePlacement[]        # Compose 端口 + Builtin path 或空 path
  route_scene:  RouteScene?            # 组障碍、scope/gate 穿越许可
  diagnostics:  LayoutDiagnostics      # warnings / relaxations / params_hash
```

执行顺序：

```text
LayoutAlgorithm
  → LayoutOutput(nodes, groups, edge terminals, route_scene)
  → optional EdgeRouter(只写 path；不得改 node/group/port)
  → finalize(只拼 labels/canvas/序列化；不得重算组框真源)
```

`DeferToRouter` 若不能表达组障碍或合法 boundary crossing，必须报 `UnsupportedRouteScene`；禁止退化为「只避节点、穿组不管」。  
通用 API 细化见 [contracts-and-ir](phases/contracts-and-ir.md)。

### 2.3 与经典 Sugiyama / yFiles 的对应

| Sugiyama / yFiles | 本核相 | 本核写者 | 产物 |
|-------------------|----------|----------|------|
| P1 FAS | Compose | CycleWriter | 边反向位 |
| P2 Layering | Compose | LayerWriter | `layer(v)` |
| P3 Ordering + dummies | Compose | OrderWriter | 层内序；proper hierarchy |
| 端口 / 边组 | Compose | PortWriter | side + along_spec + port group |
| Gate / Channel / Bundle | Compose | RouteTopoWriter | 走廊拓扑（非像素） |
| P4 Coordinates | Metric | CoordWriter | 节点框、组框、track、缝宽 |
| P5 Drawing | Ink | InkWriter（纯展开） | `EdgePlacement.path` |

证据：[01](../../../reference/yfiles/01-sugiyama分层布局.md) · [03](../../../reference/yfiles/03-正交边路由.md) · [09](../../../reference/yfiles/09-yfiles类引擎架构.md)。

### 2.4 模块边界（代码）

```text
plotgram-layout/src/layout/hierarchical/
  params.rs          # HierarchicalParams / Preset / bind（入口一次）
  compose/           # contraction · ranking · ordering · ports · channel
  plan/              # Plan IR（稳定字段；禁止 HashMap 序）
  metric/            # coordinate · track publish · group frames
  ink/               # routing_style 展开 · verify
  demand.rs          # DemandBoard 汇总（或提至 engine 共享）
  stages/            # Orientation 等（可先与 facade 共用）
  diagnostics.rs     # warning / relaxation / invariant report

plotgram-algo/       # VPSC · FAS · crossing count · orientation 变换 · ortho normalize
```

`LayoutContract` **保持** `AlgorithmRef { name, options: AttrMap }`；Hier 入口 `HierarchicalParams::bind` 一次，之后各相只读 typed params（见已实现 `params.rs`）。

---

## 3. 核心 IR

### 3.1 Plan（组合相唯一产物）

必须可序列化、可 diff、字段遍历**稳定序**。Plan 不直接暴露临时索引；所有对象使用稳定 key：

```text
NodeKey    = Real(NodeId) | Virtual { owner, kind, ordinal }
EdgeKey    = EdgeId
GateKey    = { group_id, edge_id, crossing_ordinal }
SegmentKey = { edge_id, segment_ordinal }
```

`ordinal` 由原始声明序与规范化遍历产生，禁止来自地址、哈希序或并发完成序。最低字段集：

| 字段 | 含义 |
|------|------|
| `edges: EdgeId → EdgePlan` | 原始端点、工作图端点、`reversed`、语义方向 |
| `layer: NodeId → u32` | P2（含虚节点） |
| `order: NodeKey → u32` | P3；每层内唯一且稠密 |
| `ports: EdgeId → { source, target: PortPlan }` | side + **唯一** `along_spec` + port group |
| `gates: GateKey → GatePlan` | 穿组界面的合法闸门与 scope 转移 |
| `routes: EdgeId → RouteTopology` | `Orthogonal(ChannelPath)` / `Polyline(DirectOrVia)` / `Deferred` |
| `track_order` | 同走廊段的稳定偏序/track index（非像素） |
| `bundles` | ≥2 边共享后缀的合流事实 |
| `contraction_meta` | Weak/Strong 展开所需元数据（若有） |

`PortPlan.along_spec` 是 Compose 的唯一真源；Metric 只把它展开为节点局部/画布像素点。不得同时在 `PortRef` 与旁表各存一份可独立修改的 along。  
**禁止**：度量/Ink 向 Plan 回写拓扑；Ink `unwrap_or` 默认端口侧；用数组下标冒充稳定 ID。

#### FAS 方向不变量

- `EdgePlan.original = (source, target)` 永远对应 Graph 语义、箭头与 head/tail label。
- `EdgePlan.working` 仅供 FAS/rank/order；`reversed=true` 时为 original 的交换。
- Port、最终 path 的首尾始终按 **original source → original target** 存储。
- 端口偏好可根据 working 流向求出，但冻结 Plan 前必须映射回 original source/target。
- bundle 的 source-prefix / target-suffix 也按 original 语义命名；不得因 FAS 偷换箭头或标签。

#### 环入口规则（声明序优先，环 reroot）

**声明序靠前的节点，布局上优先靠上。** 作者的声明顺序即叙述顺序，是布局的一等确定性信号：

- 主算法仍是标准 Greedy-FAS（ELS），反转数上界（≤ E/2 − V/6）与质量特性不变。
- 得到反转集后做一次**环 reroot**：若 0 号节点（声明最前）还不是工作源点、且它恰有一条未反转的原始入边，则把切边旋转到这条入边上——同时取消一条现有反转，经无环性校验后接受（候选按最小边序确定性选取）。
- 旋转不改变反转数，只改变环在哪条边被切开：回边落在「回归段」而非「入口段」，声明最前的节点因此 rank 居顶。不满足条件（已居顶、入边不唯一、无合法旋转）时保持 ELS 结果。

反转数回涨由 hier_eval 基线的 `reversed_count` 门禁守住。作者想改环的入口，重排声明即可，无需新语法。

### 3.2 Metric

| 字段 | 写者 |
|------|------|
| `node_frames` | CoordWriter |
| `group_frames` | 组框变量进求解（**非**事后包围盒当真源） |
| `port_points` | `PortPlan.along_spec × node_frame` 的确定性展开 |
| `main_track_coords` / `cross_track_coords` | 度量 publish |
| `layer_gaps` / `node_gaps`（已解析像素） | 含 Demand 合并后的终值 |
| `partition_band_coords`（若有 grid） | 物理 column/row 区间 |

### 3.3 DemandBoard（唯一合法反馈）

```text
预算生产者先计算局部总量（可 sum/count）
        │
        ▼
DemandBoard 按同一 key 仅 max 合并下界
        │
        ▼
对应消费者执行前 freeze；消费后禁止再写该 epoch
```

DemandBoard 是显式数据流，不是运行完成后的回调。分两个 freeze epoch：

| Epoch | 需求来源 | key / 单位 | 消费时机 |
|-------|----------|------------|----------|
| **A · ComposeBudget** | 边标签、强端口绕行、自环结构 | edge/layer-span；逻辑层数或 Size | properify/ranking 相关消费者之前 |
| **B · MetricBudget** | 端口数量、组标题、Channel track、标签带 | node/layer-gap/group/band；px | Metric 求解之前 |

同一生产者内部可先做加和，例如 `track_count × pitch`；Board 只负责多个来源对同一**下界**的 `max`。禁止把两个独立占用带错误地 `max` 掉。  
详细 key、单位与 freeze 规则见 [coordinate-and-demand](phases/coordinate-and-demand.md)。

纪律：覆盖式赋值禁止；某 epoch freeze 后再产生该类需求 = **相序错误**，不是就地挪节点。

### 3.4 Diagnostics 与失败语义

| 类别 | 默认行为 |
|------|----------|
| `InvalidInput` | 硬失败：缺端点、非法 ratio/position、未知 partition axis |
| `Unsupported` | 硬失败：已接受但未实现的 style/scene；不得 no-op |
| `InfeasibleConstraint` | 硬失败并报告冲突约束；作者硬约束不静默放宽 |
| `BudgetExceeded` | 返回稳定的最佳已验证解或硬失败；策略由 typed param 明示 |
| `InternalInvariant` | 硬失败；附 phase、object key、最小复现上下文 |

软偏好可按固定优先级放宽，但每次 relaxation 必须进入 `LayoutDiagnostics`。  
搜索预算使用候选数、扩展数、轮数等确定性计数；禁止以墙钟超时改变布局结果。

---

## 4. 组合相子阶段（目标流水线）

```text
I.0   Bind + validate      typed params；拒绝未支持组合
I.1   Stage projection     orientation / self-loop / parallel / component facts
I.2   Contraction          group_policy → 收缩图 + meta（Weak | StrongMacro）
I.3   Cycle removal        Greedy-FAS + 环 reroot（§3.1）→ working direction / reversed
I.4   Ranking              Network Simplex（组连续层、partition 主轴区间）
I.5   Properify + strong-port projection
                            长边、强侧/FIXED_ORDER+ port dummy
I.6   Ordering             median + transpose + 边权 1/2/8 + best snapshot
                            组/分区连续块 = 边界 dummy，不改交叉核
I.7   Port finalize        固定约束校验；FREE 按对侧 order 分配
I.8   Gate derive          组界面闸门与 scope transition
I.9   Route topology       Channel 搜索 + track order + 有界 rip-up + bundle
                            polyline 写 DirectOrVia；deferred 写 terminals
I.10  Freeze + PlanVerifier
```

端口约束分成 **I.5 投影**与 **I.7 决议**，不是两个写者：前者只把作者强约束变成 ordering 输入，后者由唯一 `PortWriter` 冻结 `PortPlan`。  
完整组合契约见 [composition](phases/composition.md) 与 [ports-and-channel](phases/ports-and-channel.md)。

度量：

```text
II.0  Freeze MetricBudget（端口 / 标注 / 自环 / channel / group title）
II.1  主轴：layer → main；应用主轴 partition band / layer gap 下界
II.2  次轴：BK 生成理想位置 → VPSC 统一解组框、节点、cross-axis band
II.3  展开 PortPlan → port_points；publish track 坐标与缝宽
II.4  MetricVerifier；冻结 Metric
```

落笔：

```text
III.1 读取 RouteTopology 展开折线（不得按缺省值猜拓扑）
III.2 bundle 干线几何接合
III.3 规范化 / 裁剪 / 圆角 / 箭头（不改 topology / track order）
III.4 InkVerifier（硬 FAIL）
```

---

## 5. 算法选型（最终表）

### 5.1 主选与替代

| 相 | **主选** | 可接受替代 | 明确不做 / 后置 |
|----|----------|------------|-----------------|
| P1 去环 | **Greedy-FAS**（ELS 骨架 + 声明序环入口规则，见 §3.1） | DFS 反向（仅诊断对照） | 精确 MFAS（NP；非主路径） |
| P2 分层 | **Network Simplex**（最小加权跨度） | Longest-path（仅 stub/极小图） | 每泳道独立分层 |
| P3 定序 | **median sweep + transpose + best snapshot** | barycenter（质量差一档） | 纯 ILP |
| P3 计数 | Barth–Jünger–Mutzel 累加树 | 朴素 $O(E^2)$（窄层） | — |
| P3 边权 | **real-real=1, real-virt=2, virt-virt=8** | 全 1（质量差） | — |
| P4 坐标 | **Brandes–Köpf** 起步 | Priority（质量差） | 长期只靠打包 |
| P4 约束 | **VPSC**（组框/端口/标签/列带） | 临时 LP | 事后挪节点消重叠 |
| P5 正交 | **通道图 + A\*(含方向) + 区间着色 track + VPSC nudge** | 单肘 stub（仅过渡） | Hier 主路径上的 OVG |
| 拥塞 | **history cost + 有界拆线重布** | — | Ink dogleg；真全局 MCF 挡 MVP |
| 交叉组 | **边界 dummy 连续块**（组与 partition 列同构） | — | 三套排序器 |

### 5.2 为何这样选（摘要）

- **NS > longest-path**：虚节点更少、层宽更均衡（01）。  
- **median+权+snapshot**：工业配方；几十行级 ROI 最高项之一（13）。  
- **BK → VPSC**：无约束时 BK 快且直；组/端口/标签一多不要在 BK 上叠特判（01、笔记 v1 solver）。  
- **Channel > OVG（Hier）**：层间空隙即走廊，规模小两个数量级（03、Atlas 25）。  
- **有界 rip-up > 先上真 MCF**：与写权「有界返工优先」一致（18、22）。

### 5.3 `routing_style`（内建 Ink）

| 值 | 语义 | 落地 |
|----|------|------|
| `orthogonal`（默认） | 通道拓扑 + 正交展开 | **主路径** |
| `polyline` | Compose 明写 `RouteTopology::Polyline(DirectOrVia)`；Ink 只展开 | 支持；质量次之 |
| `octilinear` / `curved` | 产品面保留 | 可后置；未实现须显式错误 |

与 diagram 级 `edge_routing:` **正交**：后者是独立 EdgeRouter；前者是 Builtin Ink 风格。
`routing_style` 可被 bind 不等于已支持；若对应 Plan 生成器未注册，I.0 直接 `Unsupported`。

---

## 6. 通道路由架构（正交主路径）

### 6.1 五层写权（不可乱层）

| 层 | 自由度 | 写者 | 禁止 |
|----|--------|------|------|
| L1 | 端口位置 | Compose Port | Ink 发明侧 |
| L2 | 路径拓扑（走哪些段） | Channel search | nudging 改走向 |
| L3 | 走廊内 track 次序 | TrackOrderWriter | nudging 改次序 |
| L4 | 精确偏移 | VPSC nudge | 穿到另一侧（应上提） |
| L5 | 圆角/箭头 | Ink decorate | 改 L2/L3 |

### 6.2 空间零件

| 零件 | 含义 |
|------|------|
| **Substrate** | rank × order 离散骨架 |
| **Segment** | track 被组边界切开的段（**不可省**——省则穿组门禁失效） |
| **Gate** | 组边界合法进出；配对内外段 |
| **Lane** | `track.coord + lane_index × pitch`（导出量，非自由搜变量） |
| **Bundle** | ≥2 边共享 track **后缀**（拓扑事实；Ink 只接合） |

### 6.3 穿组三道防线（从 Atlas 保留）

1. **构建期**：跨非法 scope 的 link 拒绝  
2. **搜索期**：ScopeMask 硬过滤（消灭「同线直穿」）  
3. **检查期**：`verify_no_group_penetration` / `ink_verify` 硬 FAIL  

Gate 容量：相 I 累计 demand；相 II 用段长约束；**禁止**「容量 = 跨界边数」（恒真）。

Channel 搜索只在离散 `Substrate` 上工作，不读取尚未产生的像素坐标。代价由层跨度、转弯、scope crossing、历史拥塞和稳定 tie-break 组成。  
有界 rip-up 的边选择顺序固定为 `(失败次数 desc, edge priority desc, EdgeId asc)`；预算按 expansion/reroute 轮数计，不按时间。  
找不到合法 scope path 时默认硬失败，禁止由 Ink 画穿组 dogleg。

### 6.4 双后端（产品）

| 场景 | 后端 |
|------|------|
| Hier Builtin 正交 | **Channel** |
| 节点冻结后的独立路由 / 自由位置重布 | **EdgeRouter**（可 OVG）；与 Hier 共享 L3–L5 零件为佳 |

---

## 7. 端口

### 7.1 五档（对标 ELK / yFiles）

| 档 | 最低 IR 载体 | 语义 |
|----|---------------|------|
| FREE | `PortConstraint::Free` / `None` | 算法写 side + order |
| FIXED_SIDE | `{ side }` | 作者固定 side；算法写 order |
| FIXED_ORDER | `{ side, order_key }` | 作者固定同侧相对序；像素位置由 Metric 展开 |
| FIXED_RATIO | `{ side, ratio ∈ [0,1] }` | 作者固定沿边比例 |
| FIXED_POS | `{ local_point }` | 作者固定节点局部坐标；必须在边界上 |

目标 `PortPlan`：

```text
PortPlan {
  side: Side,
  along_spec: Ordered(slot) | Ratio(f64) | LocalOffset(f64),
  group: PortGroupId?,
}
```

`slot` 只表达稳定次序，不是像素真源；Metric 根据最终节点尺寸把 `Ordered(slot)` 展开成唯一 `port_point`。  
候选端口是作者约束输入，冻结 Plan 时必须已选成一个 `PortPlan`。`port group` 是多边共享同一 `PortPlan` 的显式事实。

### 7.2 时机

| 时机 | 动作 |
|------|------|
| P2 后 | 强侧约束可插 port dummy |
| P3 | FIXED_ORDER+ 在端口 dummy 上排序；FREE 可先序后按对侧分配 |
| P4 | **端口对齐**进目标，非仅节点中心 |
| P5 / Ink | 端口只读 |
| MetricBudget | 同侧独立端口数 × pitch → 节点最小尺寸 |

FREE 默认算法：按对侧端点的 `(layer, order, EdgeId)` 稳定排序，在候选 side 上分配 order；不得读取尚未产生的像素 x/y。

现有 `PortConstraint { side, slot }` / `PortRef { side, slot }` 只能覆盖五档子集，属于待替换过渡模型，不得据此宣称五档已落地。  
详见 [ports-and-channel](phases/ports-and-channel.md)。

---

## 8. 组与 PartitionGrid

### 8.1 三分语义（不得混真源）

| 概念 | 相对流向 | 职责 |
|------|----------|------|
| **rank** | 沿流向 | 层号 |
| **group** | 树状包含 | 嵌套、组框、跨组 gate |
| **PartitionGrid** | 物理 x/y 网格 | 全局 column/row；由 Orientation 映射到 main/cross |

- columns 永远按物理 x 轴声明序，rows 永远按物理 y 轴声明序。  
- TB/BT：columns 是 cross-axis band，rows 是 main-axis band；LR/RL：二者交换。  
- 泳道只声明正交于流向的一轴：TB/BT 常用 columns，LR/RL 常用 rows。  
- **禁止**用 group Horizontal 堆叠冒充 PartitionGrid（ADR-008）。  
- 层是**全局的**：不能每泳道各自分层。

### 8.2 Group policy（参数，非第二布局器）

| `group_policy` | 直觉 | 典型 profile 取值 |
|----------------|------|-------------------|
| **Weak** | 组收缩为超节点再排；组间堆叠 | flowchart / state |
| **StrongMacro** | 组内分层 → macro rank → 回填；偏 equal-track / hub | architecture |

目标：`contract → rank/order → expand` 同一套 Plan 类型；差异只在收缩策略与后续约束松紧。

组树与跨 scope 边的最低不变量：

1. 每个 real node 恰有一条 root→leaf scope path；组不可重叠归属。  
2. 跨组边按两端 scope path 的 LCA 推导边界 crossing 序列。  
3. 每次 crossing 恰对应一个稳定 `GateKey`；嵌套组按内→外 / 外→内顺序配对。  
4. `StrongMacro` 可递归求局部 Plan，但展开后必须归一为同一全局 `Plan` schema；不得让 Ink 区分 strong/weak。

### 8.3 连续块机制（组与列同构）

P3：**边界 dummy** 夹住同组（或同列）节点 → 不改交叉最小化内核。  
P4：组框边、列带边 = **VPSC 变量**，禁止事后包围盒当真源。

---

## 9. 参数体系

### 9.1 覆盖层级

```text
1. HierarchicalParams::default()     # 算法默认
2. HierarchicalPreset.apply          # 仅改一组默认数值（compact/spacious）
3. layout { … } 显式字段             # 图级覆盖
4. HierarchicalLayoutData            # 元素级 typed 约束；作者事实最强
```

`profile:`（flowchart 等）在**解析层**展开为 `layout: hierarchical` + 若干字段/preset；**不是** `HierarchicalPreset` 本身。  
Preset = 对默认值的命名补丁（如只改 gaps）；**不**夹带 group_policy 语义包。

结构事实继续放 Graph（group tree、PartitionGrid、端口约束、edge_group）；Hier 专属的 node/edge/group 偏好进入 typed `HierarchicalLayoutData`，不得让算法从元素 `attrs` 临时猜键：

```text
HierarchicalLayoutData {
  node:  NodeId  → { rank?, rank_range?, order?, alignment_set? }
  edge:  EdgeId  → { min_span?, weight?, priority? }
  group: GroupId → { policy?, sizing?, align? }
  alignment_sets: AlignmentSetId → NodeId[]
}
```

作者硬约束与算法软偏好必须分类型；冲突优先级固定为：

```text
输入合法性 > fixed port/partition > group containment
  > 显式 rank/order > soft alignment > 交叉/边长优化
```

硬约束冲突报 `InfeasibleConstraint`；不得静默丢弃。

### 9.2 参数域（按相组织；扁平 struct）

代码真源：`HierarchicalParams`（可增字段，保持扁平）。

| 组（文档用） | 字段 | 默认直觉 |
|--------------|------|----------|
| General | `orientation` | TB |
| Gaps | `node_gap` / `layer_gap` / `edge_gap` | 24 / 40 / 16 |
| Edge | `routing_style` | orthogonal |
| Group | `group_policy` / `group_sizing` / `group_align` | Weak / Fit / Center |
| Preset | `preset` | default → compact/spacious 只改 gaps |

`hub_client_align: bool` 没有 hub/client 的 typed 选择器，不能形成可验证语义；目标架构删除该布尔开关，以 `alignment_sets` + priority 表达。无向后兼容层。

后续按需扩展（仍进同一 struct，不按图种分表）：

| 候选字段 | 相 | 说明 |
|----------|-----|------|
| `layering` | P2 | NetworkSimplex / FromSketch… |
| `crossing` | P3 | median 配方开关 |
| `coord` | P4 | BK / VPSC 策略阈 |
| `edge_grouping` / `bus` | Compose | 自动边组 / bus |
| `port_policy` | Port | 默认 FREE 粒度 |
| `component_arrangement` | Stage | 分量拼合 |
| `failure_policy` | 全局 | BudgetExceeded 时 hard-fail / verified-best |

DSL：`layout: hierarchical { … }` 自由 map → `bind`；未知 key 警告；错类型、非有限数、负 gap、越界 ratio 与无效组合均错误。warning 必须进入 `LayoutDiagnostics`，不得在 layout 入口丢弃。

### 9.3 Orientation

- 核心算法**只实现 canonical TB**，内部使用 `(main, cross)`，不直接散落 x/y 分支。  
- `OrientationStage`：物理坐标/side/partition axis → canonical → 核心 → 物理坐标；变换节点、折点、组框、端口、标签槽与 RouteScene。  
- columns/rows 保持物理轴身份；Stage 只映射其在 canonical 中扮演 main 还是 cross，不改 axis id/cell 归属。  
- BT/RL 必须反转 main 轴；仅用 `is_vertical()` 换轴不算支持四方向。  
- **禁止**四方向四套代码。

Stage 的完整包裹顺序与局部/全局坐标写权见 [contracts-and-ir](phases/contracts-and-ir.md)。

---

## 10. Writer 与确定性

### 10.1 Writer（类型化写权，目标）

| Writer | 自由度 |
|--------|--------|
| `CycleWriter` | working direction / reversed |
| `LayerWriter` | layer |
| `OrderWriter` | 层内 position |
| `PortWriter` | side / along_spec / port group |
| `RouteTopoWriter` | segment / gate / route topology / bundle |
| `TrackOrderWriter` | 走廊内 track 次序 |
| `CoordWriter` | 次轴坐标、组框、track 像素 |
| `InkWriter` | 折点与装饰展开（只读上游） |

实现可先用「字段分模块 + `&mut` 视图」；最终目标是把「下游偷改」变成编译期/门禁错误。

Stage 只写自己拥有的变换自由度：核心写 component-local 坐标，ComponentStage 写 component transform，NormalizeStage 写全图统一平移；三者不是同一自由度的多写者。

### 10.2 确定性清单

- 邻接与候选边：显式排序  
- 平局：声明序优先，稳定 id 次之；每个算法必须在本 phase 文档写出完整 tuple  
- 容器：`BTreeMap` / `IndexMap`，禁止 `HashMap` 驱动布局迭代  
- BK 四候选合并、median 左右扫：固定规则  
- `params_hash` 进诊断（归因参数 vs 代码）
- 搜索预算按迭代/候选/expansion 计数；禁止时间预算影响结果

---

## 11. 验真与质量门禁

| Verifier | 最低断言 |
|----------|----------|
| **PlanVerifier** | stable key 唯一；working graph 可分层；proper hierarchy；每层 order 稠密唯一；每边两端 PortPlan 完整；gate/scope 序列合法；route topology 连通 |
| **MetricVerifier** | 全数值 finite；节点无重叠；组包含成员且组框互不非法重叠；partition cell 落在 band；track 次序保持；所有 Demand 下界满足 |
| **InkVerifier** | path 首尾等于 Metric port point；orthogonal 轴对齐；不穿非端点节点；穿组只经 gate；非 bundle 边不完全重合；圆角最小段长；箭头/label 方向保留 original 语义 |
| **FacadeVerifier** | 输出 id 集与输入一致；DeferToRouter 不改 nodes/groups/ports；同输入双跑 bit-identical |

每相先「由构造保证」，再用线性或近线性 verifier 自反证；「由构造保证」无不变量说明则不算完成。  
失败报告包含 phase、object key、违反的不变量与相关约束，不返回半合法几何。详见 [ink-and-verification](phases/ink-and-verification.md)。

---

## 12. 落地顺序（设计里程碑，非日记）

按依赖与 ROI（对齐 reference 13，收敛为重建口径）：

| 里程碑 | 交付 | 验收要点 |
|--------|------|----------|
| **M0** | LayoutOutput/RouteInput/Diagnostics 目标契约 · typed Writer · Demand epoch · VPSC · Orientation Stage | warning 可观测；组框可由 Layout 输出；四向变换 round-trip；确定性 |
| **M1** | EdgePlan/FAS · NS · properify · median(+权+snapshot) · BK · Plan/Metric verifier | 无重叠；反向边语义保持；长边更直 |
| **M2** | 五档 Port IR · strong-port projection · FREE finalize · label/loop reserve | 端口序一致；无 Ink fallback |
| **M3** | Channel topology · track order · demand · 有界 rip-up · InkVerifier | 不穿节点；无非法重合 |
| **M4** | 组 scope/gate · 连续块统一壳 · 组框进 VPSC · Weak/Strong 同 Plan | 不穿组；一组 Plan schema；finalize 不重算组框 |
| **M5** | PartitionGrid 引擎消费 + Orientation 映射 | cell 落带；全局层跨泳道一致；四向轴语义正确 |
| **后置** | octilinear/curved · 真 MCF · from-sketch · 完整 integrated labeling | 不挡主路径闭环 |

**迁移原则**：v1 Atlas 是**功能与坑**真源；重建按相迁拓扑与门禁，**不**整包拷贝三路径 `solve` 壳。

---

## 13. 反模式速查（审查用）

1. Ink / stub `unwrap_or` 默认端口侧  
2. 路由发现挤再挪节点  
3. 事后组框包围盒当真源  
4. Hier 主路径捡回 OVG  
5. Weak/Strong 永久三宇宙、产出类型不一致  
6. 引擎 `if profile / DiagramType`  
7. 每泳道独立分层  
8. nudging 改 track 次序或走向  
9. Demand 用覆盖合并代替 `max`  
10. 拥塞靠单边特判而非 rip-up / history cost  
11. `HashMap` 迭代驱动布局  
12. group Horizontal 冒充 PartitionGrid  
13. 参数 bind 成功但执行 no-op  
14. BT/RL 只换轴不反向  
15. Router 在组框产生前运行  
16. `slot` 同时被当作次序与像素位置真源

---

## 14. 与现有文档 / 代码的关系

| 资产 | 角色 |
|------|------|
| **本文** | Hier **目标架构**真源 |
| [scope.md](scope.md) | 能力 / 非目标 / 典型域 |
| [phases/](phases/README.md) | 本文契约的相级展开；不得改变本文写权 |
| [from-yfiles-reference.md](nodes/from-yfiles-reference.md) | 阅读启发纪要（不替代本文） |
| [write-authority.md](../write-authority.md) | 全布局尺子 |
| `docs/reference/yfiles/*` | 算法证据 |
| `docs/archive/atlas/*` | 历史总纲与债（只读） |
| `hierarchical/params.rs` | 参数 / preset / bind（已落地入口） |
| stub `rank/place/ports/ink` | **过渡**；须按本文替换，禁止在其上叠特判 |

---

## 15. 收敛命题

> **一个 Hierarchical 核** = Sugiyama 主选算法表 + 单一组合 Plan + Channel 正交 + epoch 化 DemandBoard + Orientation Stage + 类型化 params/layout data；  
> 图种差异只在编排层展开进参数；组策略与分区是约束机制，不是平行布局器；  
> Layout 输出节点与组框真源，Router 不改端口，finalize 不重算几何；Ink 永不发明。
