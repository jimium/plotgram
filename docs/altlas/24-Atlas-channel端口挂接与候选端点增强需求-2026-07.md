# 24 - Atlas Channel：端口挂接与候选端点增强需求

> 日期：2026-07-26  
> 状态：**可执行需求**（实现落点：`crates/plotgram-core/src/layout/atlas/channel/`）  
> 上游：[`22-Atlas 总纲`](22-Atlas下一代布局与路由架构-总纲-2026-07.md) §5.1 I.5 / I.6；[`channel/README`](../../crates/plotgram-core/src/layout/atlas/channel/README.md)  
> 读者：在 `channel` 内做增强的实现者

---

## 0. 一句话

在 `atlas/channel` 里补齐 **PortSlot 的语义身份与候选端点搜索**，使上层（I.5 / Legacy Adapter）能把「已定侧」或「候选侧」交给通道图，而 **不在本模块内实现完整端口侧决策策略**。

---

## 1. 范围边界（必读）

### 1.1 要做（In Scope）

| # | 能力 | 说明 |
|---|------|------|
| **R1** | PortSlot 携带 `(node, side[, slot])` | 今天只有 `id + track + capacity`，缺语义身份，无法对接上游 |
| **R2** | 按节点×侧挂接端口到宿主轨道 | 给定节点槽位与侧，解析应挂到哪条 track，并注册 PortSlot |
| **R3** | 候选端点选路 | `route` 除「单对 PortSlot」外，支持「源候选集 × 汇候选集」，按 LexCost 取最优可行对 |
| **R4** | 容量语义不变且可测 | 侧容量硬过滤；满容 → Infeasible；commit/release 可逆 |
| **R5** | derive 可选自动挂四侧端口 | `derive_substrate` 之后可为每个节点挂 0～4 个侧端口（可开关） |

### 1.2 明确不做（Out of Scope）

| 不做 | 理由 |
|------|------|
| 全局端口侧求解（`phase_port_slot` / `port_solver` 整套） | 属相 **I.5**，落点应是 `atlas/port` 或 Plan 装配层，不是 channel |
| 读 `Diagram` / `DiagramType` / 图名分支 | 违反 A5；channel 只吃无坐标 IR |
| 像素坐标、端点在节点边上的几何位置 | 相 III / 度量相职责 |
| 平行边 slot 的几何错开量 | 度量相；本层最多保留离散 `slot_index: u32` |
| 反馈边启发式、反向 stub 惩罚 | I.5 策略 |

**验收口令**：本增强完成后，channel 仍是「给定端点（或候选端点）→ track 序列」；**谁决定选哪一侧** 仍由调用方负责。

---

## 2. 背景与缺口

### 2.1 现状

```rust
// substrate.rs 现状（简化）
pub struct PortSlot {
    pub id: PortSlotId,
    pub track: TrackId,
    pub capacity: u32,
}
```

- `search::route(graph, from, to, occupancy)` 要求调用方事先知道两个 `PortSlotId`。
- `derive_substrate` 产出 track/gate，**不自动挂节点端口**；测试里手工 `attach_port`。
- 总纲 I.6 顶点规格是 `PortSlot(node, side, slot)`，现状缺 `node` / `side`。

### 2.2 与生产路径的对照（仅作语义锚，不复制实现）

| 生产概念 | channel 对应 |
|----------|----------------|
| `Port::{Top,Bottom,Left,Right}` | 本需求的 `PortSide`（可用别名或映射，**不要**依赖路由模块） |
| `PORT_SIDE_CAPACITY`（默认 4） | `PortSlot.capacity`；默认值建议复用 `kernel/route/capacity.rs` 常量或 channel 内同名常量 |
| `endpoint_map` / `from_side[i]` | 调用方选定后传入单个 `PortSlotId`，或传入候选集 |

---

## 3. 概念与类型需求

### 3.1 新增：`PortSide`

在 `substrate`（或 `channel` 根）定义**无几何**的侧枚举，与基底轴对齐：

```text
PortSide:
  MainLow   — 主轴低端（≈ 流向起点侧；TB 布局≈ Top）
  MainHigh  — 主轴高端（≈ Bottom）
  CrossLow  — 交叉轴低端（≈ Left）
  CrossHigh — 交叉轴高端（≈ Right）
```

要求：

- 与现有 `GateSide` **同构**（可 `From`/`Into`，避免两套枚举漂移）。
- **禁止**在本层引入屏幕方向依赖；与 `direction` 的映射留给 Dialect / Adapter。
- 提供与 `crate::layout::types::Port` 的**可选**双向映射函数（放在 channel 外或 `#[cfg]` 旁路均可），但 channel 核心类型不强制依赖「屏幕 Port」。

### 3.2 扩展：`PortSlot`

```text
PortSlot {
  id: PortSlotId
  track: TrackId          // 宿主轨道（唯一出入口）
  capacity: u32           // 0 = 不限；默认建议 4
  // —— 新增 ——
  node: NodeKey           // 稳定节点标识（见下）
  side: PortSide
  slot_index: u32         // 同侧离散槽；无几何错开。默认 0 即可
}
```

**`NodeKey` 选型（二选一，实现时固定一种并写进 README）：**

| 方案 | 优点 | 缺点 |
|------|------|------|
| **A. `String`（节点名）** | 与 `ChannelBlueprint.nodes` 键一致，探针好接 | 分配 id 时略重 |
| **B. `NodeId(u32)` + Blueprint 侧映射表** | 更轻、确定性好 | derive/探针多一层映射 |

**推荐 A**（与现有 `ChannelBlueprint` 一致），除非你已有稳定 `NodeId` 体系。

### 3.3 不变量（构建期拒绝）

| ID | 规则 |
|----|------|
| P-inv-1 | `attach_port` 时 `track` 必须存在 |
| P-inv-2 | 同一 `(node, side, slot_index)` 不得重复注册（返回明确错误） |
| P-inv-3 | 同一 `PortSlotId` 不得重复（已有） |
| P-inv-4 | 可选加强：宿主 `track.orient` 必须与 `side` 相容——`MainLow/High` 挂 **Cross** 轨道（层间缝进出），`CrossLow/High` 挂 **Main** 轨道（走廊进出）。若与当前 derive 网格约定冲突，以 **derive 文档写死的约定为准**，并在单测钉死 |
| P-inv-5 | 容量语义：`capacity == 0` → 不限；否则 `port_load < capacity` 才 `port_open`（已有，保持） |

---

## 4. API 需求

### 4.1 Substrate：挂接

保留现有：

```text
attach_port(id, track, capacity)   // 可标 deprecated，或扩展为带默认 node/side 的兼容层
```

新增（名称可微调，语义必须覆盖）：

```text
attach_node_port(
  id: PortSlotId,
  node: NodeKey,
  side: PortSide,
  slot_index: u32,       // 常用 0
  track: TrackId,
  capacity: u32,
) -> Result<(), SubstrateError>
```

查询：

```text
port(id) -> Option<&PortSlot>
ports_of_node(node) -> impl Iterator<Item = &PortSlot>   // 确定性顺序：side、slot_index
ports_of_node_side(node, side) -> impl Iterator<...>
find_port(node, side, slot_index) -> Option<PortSlotId>
```

### 4.2 宿主轨道解析（derive / 辅助）

提供**纯函数**（可放 `derive.rs`）：

```text
resolve_host_track(
  index: &BlueprintIndex,  // 或等价区域轨道视图
  node: &str,
  side: PortSide,
) -> Option<TrackId>
```

约定（必须在 README 写死，单测覆盖）：

| side | 宿主轨道含义（相对节点 slot） |
|------|------------------------------|
| MainLow | 节点 `rank` 对应的 **低侧** Cross 缝（如 gap `rank` 或 `rank-1`，二选一写死） |
| MainHigh | 节点 `rank` 对应的 **高侧** Cross 缝 |
| CrossLow | 节点 `order` 对应的 **低侧** Main 走廊 |
| CrossHigh | 节点 `order` 对应的 **高侧** Main 走廊 |

边界节点（无某侧缝）→ 返回 `None`（调用方不得静默挂到错误 track）。

### 4.3 derive：可选批量挂端口

```text
DerivePortsOptions {
  enabled: bool,                 // 默认 true 或 false：推荐默认 true，便于探针
  default_capacity: u32,         // 默认 4（对齐 PORT_SIDE_CAPACITY）
  sides: &[PortSide],            // 默认四侧全挂；可只挂部分
}
```

`derive_substrate(blueprint, …)` 成功后：

- 对每个 `blueprint.nodes` 中的节点，对 `sides` 中每一侧：
  - `resolve_host_track` 成功 → `attach_node_port(...)`
  - 失败 → **跳过该侧**（不整图失败），并在 `BlueprintIndex` 中可查询「该节点实际挂了哪些侧」

返回值建议扩展：

```text
BlueprintIndex {
  ...
  node_ports: BTreeMap<NodeKey, Vec<PortSlotId>>,  // 确定性排序
}
```

### 4.4 search：候选端点选路

保留：

```text
route(graph, from: PortSlotId, to: PortSlotId, occupancy) -> Result<RouteOutcome, EndpointError>
```

新增：

```text
route_candidates(
  graph,
  from_candidates: &[PortSlotId],
  to_candidates: &[PortSlotId],
  occupancy,
) -> Result<RouteOutcome, EndpointError>
```

行为规格：

1. **空候选** → 明确错误（如 `EmptyCandidates`），不得 panic。
2. 笛卡尔积 `from × to`（含 `from == to` 的同 track 平凡解）。
3. 跳过：未知 id、任一端 `!port_open`、`route` 返回 `Infeasible`。
4. 在所有 `Converged`（或你们现有成功 status）结果中，取 **LexCost 最小**；平局打破顺序必须确定性：  
   `(cost, from_id, to_id)` 升序，或文档写死等价规则。
5. **不修改** `occupancy`（与现 `route` 一致：调用方决定是否 `commit`）。
6. 全部不可行 → `Ok(RouteOutcome::infeasible())`（与单对 API 语义一致），或统一 `Infeasible`；**禁止**伪造路径。
7. 性能：候选规模预期很小（每端 ≤4）；可用朴素双重循环；不必上启发式剪枝。

可选便利 API（非必须）：

```text
route_node_sides(
  graph,
  from_node, from_sides: &[PortSide],
  to_node, to_sides: &[PortSide],
  occupancy,
)
```

内部：查 `ports_of_node_side` → `route_candidates`。

---

## 5. 与 I.5 的接缝（给调用方的契约）

```text
I.5（本需求不做）
  输入：边列表、节点邻接、可选反馈边标注
  输出：每条边的 (from_side, to_side) 或候选侧集合
        ↓
channel（本需求）
  attach / derive 挂 PortSlot
  route 或 route_candidates
  输出：RouteOutcome { tracks, gates, cost, status }
        ↓
上层 commit Occupancy，进入下一条边 / 全局协调
```

Legacy Adapter（Stage 1）最小用法：

1. 从旧 `from_side[i]` / `to_side[i]` 映射为 `PortSide`
2. `find_port(node, side, 0)` 得到 id
3. 调 `route`（单对即可，暂不必候选）

探针增强用法：对难边用 `route_candidates` 在四侧上试，统计「侧已定」vs「侧开放」的可行率差。

---

## 6. 单测验收清单（必须全部绿）

| # | 用例 | 期望 |
|---|------|------|
| T1 | `attach_node_port` 写入 `node/side/slot` | `port(id)` 字段完整 |
| T2 | 重复 `(node,side,slot)` | Err，不覆盖 |
| T3 | `ports_of_node` 顺序 | 同输入多次一致 |
| T4 | `resolve_host_track` 四侧 | 与文档表一致；越界 `None` |
| T5 | derive + 自动挂端口 | 每节点至少挂上存在的侧；`node_ports` 可查 |
| T6 | `route` 单对（回归） | 现有用例不坏 |
| T7 | `route_candidates` 优选 | 两对均可时选 LexCost 更优；平局确定性 |
| T8 | 候选中一端满容 | 该对跳过；另一对仍可成功 |
| T9 | 全部候选满容/无路 | Infeasible，空 tracks |
| T10 | `commit_ports` / `release_ports` | 与现有可逆语义一致（含新挂的 port） |
| T11 | 空候选切片 | 明确 Err |

现有 22 个测试应继续通过；新增覆盖 R1–R5。

---

## 7. 文档同步（实现时一并改）

1. `channel/README.md`：更新 `PortSlot` 字段图；标明 I.5 边界；写死 `resolve_host_track` 表。  
2. 本需求若有 API 命名偏差，在 README「与 24 号文差异」记一行。  
3. **不要**改 22 号文总纲算法叙述；若发现总纲 `PortSlot(node,side,slot)` 与实现不一致，只在 README/本文修订。

---

## 8. 非目标验收（防止范围膨胀）

实现 PR / 自测时自问：

- [ ] 是否新增了「读 Diagram 选侧」的逻辑？→ **应无**
- [ ] 是否在 channel 内写了反馈边 / 平行边全局求解？→ **应无**
- [ ] `route_candidates` 是否只做 LexCost 比较、不做额外美学启发？→ **应是**
- [ ] 是否仍零 `Point`/`Rect`？→ **应是**

---

## 9. 建议实现顺序

1. `PortSide` + 扩展 `PortSlot` + `attach_node_port` / 查询 API（R1）  
2. `resolve_host_track` + derive 批量挂端口（R2 / R5）  
3. `route_candidates`（R3）  
4. 补齐 T1–T11，更新 README  

预估：小步可测，不阻塞 Stage 1 Legacy Adapter；Adapter 可先只走「单对 route」。

---

## 10. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-07-26 | 初稿：R1–R5 范围、类型、API、单测与 I.5 边界 |
