# 抽象通道图（`layout/atlas/channel`）

> 状态：**生产接线中**（Hierarchical Ink 路径经 `channel_metric` → Plan → Ink；
> Stage 7 后为默认）。设计出处：[22 号文 §5.1 I.6](../../../../../../docs/新架构/22-Atlas下一代布局与路由架构-总纲-2026-07.md)；
> 推进：[23 号文](../../../../../../docs/新架构/23-Atlas分阶段推进方案-2026-07.md)。
>
> Tree / Sequence / Circular 仍委托 `LayoutPipeline`（Wave3 记债：非 Hier Ink 内化）。

## 一句话

**在没有坐标的世界里为边选路。** 路径不是一串像素点，而是一个 **track 序列**——坐标由度量相（相 II）事后一次性求出，因此路径永远不会因坐标变动而失效，repair loop 失去存在理由。

## 与 `kernel/route`（几何可见图）的关系

| | `kernel/route::ResourceGraph` | 本 mod `ChannelGraph` |
|---|---|---|
| 顶点 | 坐标点（障碍角点投影，带 `Point`） | **轨道**（`TrackId`，无坐标） |
| 边 | 轴向可见段（基于临时坐标计算） | link（轨道交口）/ gate（组闸口） |
| 规模 | O(V²)，需 `ovg_enabled ≤ 50` 规模开关 | O(层数 × 通道数)，小约两个数量级 |
| 坐标变动 | 路径失效 → repair | 不受影响（无坐标可失效） |
| 复用 | — | `LexCost` / `SolverStatus` 复用自中立模块 `kernel::cost` |

## 核心类型

```text
ChannelBlueprint  无坐标输入契约：节点 (rank,order) + 组树 + 边列表
               ↓ derive_substrate（P0 推导 adapter：每线切割成段 + 相交建 link + 逐线配对 gate）
Substrate      基底：Track（段模型：line + ext 奇偶坐标延展）/ GroupScope / Gate（边界线 crossings 配对 + GateCapacity）/ PortSlot（端口语义身份 + 端口侧容量）
               ↓ ChannelGraph::from_substrate
ChannelGraph   可搜索转移图（确定性邻接表）+ Occupancy 占用账本
               ↓ search::route / route_candidates（词典序 Dijkstra + L8 ScopeMask）
RouteOutcome   track 序列 + gate 序列 + LexCost —— 即 Plan.channels 的一项
               ↓ detect_bundles（共享 track 后缀）
Bundle         合流束（≥2 边共享后缀）——拓扑事实，替代几何共线检测
```

## 设计不变量（每条都有对应单测钉死）

1. **穿组不可表达（H2，[27 号文](../../../../../../docs/新架构/27-Atlas-channel模块审查与改造需求-2026-07.md) L1/L8/L6 三道防线）**
   - **构建期（L1）**：跨 scope 的轨道 `link` 拒绝（`CrossScopeConnection`）；不相交的段对
     拒绝（`NonIntersectingLink`，「相交才 link」——段延展必须覆盖对方所在线，B8 组内段
     无法旁路边界缝）；跨组转移只能经 `Gate`。
   - **搜索期（L8）**：`ScopeMask`（`{None} ∪ chain(u) ∪ chain(v)`）在松弛处硬过滤——
     两端在组外的边借道穿组（含同线直穿）不可表达，无路则显式 `Infeasible`。
   - **检查器（L6）**：`verify_no_group_penetration()` 静态断言无段与无关组内部相交，
     返回违规清单；探针每图先跑（A1）。

2. **正交性由构造保证（H3）**
   `link` 只允许 Main × Cross 异向轨道（同向拒绝，`ParallelLink`——平行轨道无交点）。
   推论：**折点 = link 转移**，折点计数是拓扑事实而非几何测量；gate 两侧同向时直穿无折点。

3. **lane 数与 gate 穿越数都是输出，不是输入（B1 核心机制 + 27 号文 L4）**
   轨道占用（`Occupancy::lane_demand`）**无上限**：N 条边挤同一轨道全部成功，
   lane 需求 N 作为 Demand 输出给度量相，由相 II 把两侧撑开。这就是删除
   SpaceBudget 单向预告回环的机制。
   gate 同理（L4）：生产恒 `GateCapacity::Unbounded`——crossing_demand
   （`gate_load`）作为相 I 输出交度量相换空间；`Fixed(c)` 仅供探针诊断扫描
   （旧「容量 = max(下限, 边界度数)」预算回环已删，`boundary_degree` 同删）。
   唯一保留的硬容量：**端口侧容量**（I.5 `PORT_SIDE_CAPACITY`），同一挂接点
   可容纳的边数上限，满容后该端点此刻不可布（`capacity = 0` 视为不限）。

4. **不可行显式化，无静默兜底**
   无路可走返回 `SolverStatus::Infeasible` + 空路径（本模型无坐标，也就无法伪造
   一条"直线兜底"）。这是相 I 可行率探针（S4 评估门「相 I 成功率 100%」）需要的诚实信号。

5. **词典序代价（B4）**
   单边搜索只计 **Q3 折点 > Q4 长度**（`span_weight` 之和）。
   Q2 交叉 / Q5 对齐 / Q6 对称是**跨边**量，属于上层全局协调（有界块坐标下降，22 号文 §5.1 I.7）
   的职责——本层不做近似，避免像素级 OVG 用「重叠长度冒充交叉」的失真。

6. **确定性（AGENTS.md §2）**
   `BTreeMap` 容器 + 邻居显式排序 + 堆平局按 `TrackId` 打破。严格平局下重复求解、
   重建基底，结果逐字段一致。

7. **账本可逆**
   `Occupancy::commit / release`（gate 按边去重）、`commit_ports / release_ports` 严格互逆
   ——这是未来「词典序有界回溯」（泛化 rip-up/reroute）的前提。

8. **合流是拓扑事实，不是几何巧合**
   `detect_bundles` 在反向后缀 trie 上找共享 track 序列后缀：≥2 条边共享后缀 = bundle。
   今天 `semantic_trunk_merge` 要在几何上找共线段，Atlas 里这就是合流的定义本身。

## 推导 adapter（P0）：从真实图到 Substrate

`derive_substrate(ChannelBlueprint)` 是把通道图接到真实图的接缝：上游只需提供
**无坐标**的节点 (rank, order)、组树、边列表，adapter 按 27 号文 L1–L3 生成段级拓扑：

- **段模型（L1）**：不再全区域铺网格。每条 Cross/Main 线被覆盖它的组切成段
  （B1：`r0 < k ≤ r1` / B2：`o0 < og ≤ o1`；边界线本身不切），段带 `line` +
  `ext`（奇偶坐标闭区间：`2j` = 垂直方向 gap j、`2j+1` = 第 j 列/层节点体），
  scope = 内部区间包含该段的最深覆盖组。全部切点一次切完（B4）；零长段自动
  消失（B5）；相邻组共享缝独立成段、scope 取共同父（B6）；退化轴不产组内段
  （B7）；边界缝归外段（B8）。
- **link（L1）**：仅同 scope、异向且「区间互含」（几何相交）的段对。
- **gate（L2）**：组每侧至多一个边界线 gate，`crossings` = 逐线 (组内段, 组外段)
  配对（外侧段取最近祖先线上与边界相邻的段）；三条 G-inv 构建期钉死（走向匹配 /
  同线隔边界相邻 / 同 `(group, side)` 唯一）；`crossings` 为空不注册（B7 推论）。
- **span_weight（L3）**：= 段 `ext` 内的 gap 数（几何跨度），空 cover 段取 1，恒 ≥ 1。

这让「通道图粒度是否够细」可在 product 集上**实测**（而非手搓样例），直接回答
22 号文风险台账第一条。

## 端口挂接与候选端点（24 号文 R1–R5）

本层在「给定端点 → track 序列」之上补齐了 **PortSlot 的语义身份**与**候选端点选路**，
使上层（I.5 / Legacy Adapter）能把「已定侧」或「候选侧」交给通道图。

### PortSlot 语义身份（R1）

```text
PortSlot {
  id: PortSlotId
  track: TrackId        // 宿主轨道（唯一出入口）
  capacity: u32         // 端口侧容量（0 = 不限；默认建议 4）
  node: NodeKey         // 稳定节点标识（方案 A = String，与 ChannelBlueprint.nodes 键一致）
  side: PortSide        // MainLow / MainHigh / CrossLow / CrossHigh
  slot_index: u32       // 同侧离散槽（平行边区分用，无几何错开；默认 0）
}
```

`PortSide` 与 `GateSide` **同构**（`From`/`Into` 互转），无几何、与基底轴对齐：
`MainLow≈Top`、`MainHigh≈Bottom`、`CrossLow≈Left`、`CrossHigh≈Right`（TB 布局类比）；
与屏幕方向/`Port` 的映射留给 Dialect / Adapter，**本层不引入屏幕方向依赖**。

**构建期不变量**（`attach_port` / `attach_node_port` 拒绝，均有单测钉死）：

| ID | 规则 |
|----|------|
| P-inv-1 | `track` 必须存在 |
| P-inv-2 | 同一 `(node, side, slot_index)` 不得重复（`DuplicatePortSlot`） |
| P-inv-3 | 同一 `PortSlotId` 不得重复（`DuplicatePort`） |
| P-inv-4 | 宿主 `track.orient` 须与 `side` 相容：`MainLow/High` 挂 **Cross** 轨道、`CrossLow/High` 挂 **Main** 轨道（`PortSideMismatch`） |
| P-inv-5 | `capacity == 0` 不限；否则 `port_load < capacity` 才 `port_open` |

查询 API（确定性顺序）：`port(id)`、`ports_of_node(node)`（按 id 升序）、
`ports_of_node_side(node, side)`（按 `(slot_index, id)` 升序）、`find_port(node, side, slot_index)`。

### 宿主轨道解析（R2）

`BlueprintIndex::resolve_host_track(bp, node, side) -> Option<TrackId>`（纯函数）。
节点槽位 `(rank, order)`，约定与 `edge_port_tracks` 一致：

| side | 宿主轨道（相对节点 slot） |
|------|------------------------------|
| `MainLow`  | `rank` 对应的低侧 Cross 缝（gap `rank`） |
| `MainHigh` | `rank` 对应的高侧 Cross 缝（gap `rank+1`） |
| `CrossLow` | `order` 对应的低侧 Main 走廊（gap `order`） |
| `CrossHigh`| `order` 对应的高侧 Main 走廊（gap `order+1`） |

轨道取自节点**所在区域**（组内节点取组区域）。越界 / 未知节点 → `None`，
调用方不得静默挂到错误 track。注：derive 的区域会在包围盒外补一圈缝
（root cross `0..=rank_count`、main `0..=order_count`），故**已声明节点四侧均能解析**；
`None` 是防御契约（未知节点 / 自定义索引越界），而非生产常见路径。

### derive 可选批量挂端口（R5）

`derive_node_ports(substrate, bp, index, options)`：对每个节点、`options.sides` 中每一侧，
`resolve_host_track` 成功 → 挂接 `slot_index = 0 .. default_capacity`（每槽 `capacity=1`）；失败 → **跳过该侧**（不整图失败）。
挂接结果写入 `BlueprintIndex.node_ports: BTreeMap<NodeKey, Vec<PortSlotId>>`（按挂接序，确定性）。

```text
DerivePortsOptions { enabled: bool, default_capacity: u32, sides: Vec<PortSide> }
// 默认：enabled=true、default_capacity=4、sides=四侧全挂
```

### 候选端点选路（R3）

`route_candidates(graph, from_candidates, to_candidates, occupancy)`：在 `from × to` 笛卡尔积上
取 **LexCost 最优可行对**。规格：空候选 → `Err(EmptyCandidates)`（不 panic）；跳过未知 id /
任一端 `!port_open` / `Infeasible`；平局按 `(from_id, to_id)` 升序（升序遍历 + 严格 `<` 替换，
首个最小代价对获胜，确定性）；**不修改** `occupancy`；全部不可行 → `Ok(infeasible())`，不伪造路径。
便利 API `route_node_sides(graph, from_node, from_sides, to_node, to_sides, occupancy)` 内部
查 `ports_of_node_side` → `route_candidates`。

### 与 I.5 的接缝（范围边界）

**本层仍是「给定端点（或候选端点）→ track 序列」；谁决定选哪一侧由调用方负责。**
全局端口侧求解（`phase_port_slot` / 反馈边启发 / 平行边全局求解）属相 **I.5**，落点是
`atlas/port` 或 Plan 装配层，**不在 channel**。channel 不读 `Diagram`、无图名分支、零几何。

### 与 24 号文的 API 差异

| 24 号文 | 本实现 | 说明 |
|---|---|---|
| 保留旧 `attach_port(id, track, capacity)` | `attach_port` 直接改为完整语义签名 `(id, node, side, slot_index, track, capacity)`，`attach_node_port` 为其别名 | AGENTS.md §1 无向后兼容，不留 deprecated 转发层 |
| `resolve_host_track(index, node, side)` | `BlueprintIndex::resolve_host_track(&self, bp, node, side)` | 需 `bp` 查节点 rank/order |
| `ports_of_node -> impl Iterator` | 返回 `Vec<&PortSlot>`（已排序） | 语义一致，确定性顺序不变 |
| `DerivePortsOptions.sides: &[PortSide]` | `Vec<PortSide>`（拥有所有权，便于 `Default`） | 语义一致 |

## 与 22 号文规格的分歧点（待探针数据裁决）

| 规格（I.6） | 本 mod 现状 | 处理 |
|---|---|---|
| 顶点 = PortSlot / **TrackSlot(track_id, lane_index)** / Gate | track 级选路，lane 数事后导出 | **刻意简化**：lane 分配本质是度量相的事。若探针显示度量相频繁不可行，再回 lane 级 |
| Q1 硬残差为软罚项（`lex_astar` 可出 Degraded 解） | 容量硬过滤 → `Infeasible` | 与 H0–H6 二值语义一致；接 `RelaxationLadder` 时再改 |

### 27 号文 L1–L8 落地状态与段维定案

| 需求 | 状态 | 实现锚点 |
|---|---|---|
| L1 段模型 + 相交才 link | ✅ | `Track{line, ext}`（奇偶坐标）、`link()` 几何校验、derive 切割算法 |
| L2 边界线 gate 配对 | ✅ | `Gate{line, crossings}`、G-inv-1/2/3 构建期拒绝 |
| L3 跨度加权 | ✅ | `span_weight` = ext 内 gap 数，空 cover 取 1 |
| L4 gate 容量→输出 | ✅ | `GateCapacity::{Unbounded, Fixed}`；`boundary_degree` 已删 |
| L5 探针三口径 | ✅ | `atlas_probe`：表达上界 / 端口生产 / gate 诊断（附录）+ 需求画像 |
| L6 穿透检查器 | ✅ | `verify_no_group_penetration()`，返回违规清单 |
| L7 技术债 | 部分 | T1（links → `BTreeSet`）/ T2（随 `boundary_degree` 删）/ T5（自环同节点显式 `Infeasible`，不伪造平凡解）/ T6（注释）已做；T3（slots_per_side）/ T4（多源 Dijkstra）留债 |
| L8 作用域掩码 | ✅ | `ScopeMask` + `route*` 掩码参数，松弛处硬过滤 |

段的统一表示（文档未给数据结构的部分，本实现定案）：`line` = 所在线 gap 索引；
`ext` = 可 link 的垂直延展**闭区间**，奇偶坐标编码（`2j` = gap j、`2j+1` = 第 j
列/层节点体），B8 开闭规则已折算进区间端点；空 cover（只经 gate/端口的组内段，
如单列组）用 `ext.0 > 偶坐标范围` 自然表达；端口宿主解析取包含节点体奇坐标的段
（贴组边节点因「边界线不被切」恒得根段，B7 退化组自动成立）。

探针期补充定案（重采中暴露，27 号文未预见）：

- **组矩形嵌套树前提显式化**：切割模型要求任两组矩形「分离或有祖先关系」且矩内
  无非后代节点，derive 构建期拒绝（`OverlappingGroups` / `ForeignNodeInGroupRect`）。
  LayeredKernel flat 网格不做组感知布局，两种病态都会出现：探针门面
  （`probe.rs`）确定性丢弃病态组（含被丢空容器组的级联清理），报告以
  「丢交叠组」标注；生产接线（Legacy Adapter）镜像真实分区布局，不经此路径。
- **探针端点口径 = 四侧候选**（R5 `derive_node_ports` + R3 `route_candidates`）：
  B7 退化轴的组内边正解是边界缝侧端口，钉死单一侧对会人为制造同 gate
  双穿（假 A3）；四侧候选下三集 A1/A2/A3 恒 0、A4=100%。

## 刻意不做的事

- **不做几何**：无 `Point` / `Rect` / 像素；折点展开是相 III（Ink）的职责
- **不做全局协调**：按难度序排边、有界回溯在上层（未来 `atlas::plan`）；bundle 检测已备于本层
- **不做 Demand 结构体**：`lane_demand` 先返回裸计数；`Demand{min,preferred,grow}` 等 Stage 0 的 `atlas/space.rs` 定型后再对接
- **不接生产**：接线时机是 Stage 4（4a 对拍 → 4b 切换 → 4c 删补救）

## 下一步（对齐 23 号文）

| 步骤 | 内容 |
|---|---|
| **端口挂接增强**（已完成） | [24 号文](../../../../../../docs/新架构/24-Atlas-channel端口挂接与候选端点增强需求-2026-07.md) R1–R5：PortSlot 带 `(node,side,slot_index)`、`resolve_host_track` / `derive_node_ports` 按侧挂接、`route_candidates` / `route_node_sides`（**不含**完整 I.5 侧决策） |
| 可行率探针 | 用 `derive_substrate` 接旧管线中间产物（Legacy Adapter，S1 交付 1.3 提供 rank/order/group），统计相 I 成功率与 track 占用分布 |
| 全局协调 | 边按难度序选路 + 满容时词典序有界回溯（复用 `Occupancy::release`） |
| Demand 对接 | `lane_demand` 裸计数 → `atlas/space.rs` 的 `Demand` 结构 |
