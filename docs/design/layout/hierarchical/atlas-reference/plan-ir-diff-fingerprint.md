# Plan IR + diff + 稳定指纹

> 父页：[atlas-reference/README.md](README.md)  
> 对应新架构：[architecture.md §3.1 Plan IR](../architecture.md)  
> Atlas 源：`crates/v1/plotgram-core/src/layout/atlas/plan/{mod,diff,fingerprint}.rs`

## 这是什么

Plan 是整图离散决策的唯一中间表示：节点槽位、组作用域、端口、闸门、通道、lane 索引、bundle。Atlas 的 Plan 设计是**最值得新实现直接借鉴**的部分之一——稳定指纹、Change 三态 diff、决策口径三者共用，直接支撑新架构 §3.1 Plan IR 与 §12 增量重算。

## 核心数据结构

```rust
pub type GroupKey = String;
pub type ProvenanceMap = BTreeMap<EdgeId, Provenance>;

pub enum Provenance { ChannelRoute, LegacyAdapter, Manual }

pub struct SubstrateSketch { pub rank_count: usize, pub order_count: usize }

pub struct Slot { pub rank: usize, pub order: usize }

pub struct GroupScopeSpec {
    pub parent: Option<GroupKey>,
    pub ranks: (usize, usize),   // 闭区间
    pub orders: (usize, usize),
}

pub struct PortRef {
    pub node: NodeKey,
    pub side: PortSide,
    pub slot_index: u32,
    pub slot_id: Option<PortSlotId>,   // 不参与指纹 / semantic_eq
    pub side_order: u32,                // M1：同 (node, side) 侧内序；参与指纹
    pub along_offset: f64,              // M1：沿侧切向偏移；参与指纹
}

pub struct EdgePorts { pub from: PortRef, pub to: PortRef }

pub struct Plan {
    pub substrate: SubstrateSketch,
    pub node_slots: BTreeMap<NodeKey, Slot>,
    pub group_scopes: BTreeMap<GroupKey, GroupScopeSpec>,
    pub ports: BTreeMap<EdgeId, EdgePorts>,
    pub gates: BTreeMap<EdgeId, Vec<GateId>>,
    pub channels: BTreeMap<EdgeId, Vec<TrackId>>,
    pub lane_indices: BTreeMap<EdgeId, Vec<u32>>,
    pub bundles: Vec<Bundle>,
    pub provenance: ProvenanceMap,
}
```

## 关键不变量

- **Plan 不嵌入 Substrate**：Substrate 是运行态重产物，由 blueprint 重建；Plan 只存 `SubstrateSketch` + `GroupScopeSpec`。
- **`record_route` 严格性**：`status != Converged` 时返回 `RouteNotConverged` 且 Plan **逐字段不变**（不伪造记录）。
- **`record_route` 清空 bundles**：channels 变更后旧合流失效，须重跑 `detect_and_set_bundles`。
- **`validate()` 入口门槛**：channels 有边必有 gates/provenance、组 parent 不悬空且区间不倒置、端口只引用 `node_slots` 内节点。返回首个违例（键升序）。

## 三套相等口径（关键）

Atlas 定义了三套相等口径，**共用同一「决策口径」**：

| 口径 | 用途 | 忽略 | 包含 |
|------|------|------|------|
| `==`（结构相等） | 严格比较 | 无 | 全部字段（含 provenance / slot_id） |
| `fingerprint` / `semantic_eq` | 决策口径 / 一致性 / 增量缓存键 | provenance / slot_id / bundles 顺序 | side_order / along_offset |
| `diff` | 变更归档 | provenance | side_order / along_offset |

**「决策口径」**含义：provenance 是元数据（不是决策本身），slot_id 是运行态 id（不参与决策），bundles 顺序是派生序（不参与决策）；而 side_order / along_offset 是 M1 侧内决策，**参与**。

## Plan::fingerprint（FNV-1a 稳定指纹）

```rust
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME:   u64 = 0x0000_0100_0000_01b3;
struct Fnv1a(u64);
```

**设计要点**（全部值得保留）：

1. **不用 `DefaultHasher`**——跨 Rust 版本无稳定性承诺。FNV 常量硬编码。
2. **section 标签 0x01–0x07** 区隔字段防串位。
3. **长度前缀字符串**——防 "ab"+"c" 与 "a"+"bc" 同码。
4. **`BTreeMap` 迭代序**——保证同内容同编码。
5. **`side_code` 显式 match**（MainLow=0, MainHigh=1, CrossLow=2, CrossHigh=3）——不依赖编译器判别式布局。
6. **`along_offset.to_bits()`** 进指纹——浮点按 bit 比较。
7. **决策口径**——provenance / slot_id / bundles 顺序不参与。
8. **非密码学**——仅供一致性判定与增量缓存键。

section 顺序：`0x01 substrate → 0x02 node_slots → 0x03 group_scopes（含 parent 标志位）→ 0x04 ports（write_port = node + side + slot_index + side_order + along_offset bits）→ 0x05 gates → 0x06 channels → 0x07 bundles（canonical_bundles 排序后）`。

## Plan::diff（Change 三态）

```rust
pub enum Change<T> { Added(T), Removed(T), Changed(T, T) }

pub struct EdgeChange {
    pub edge: EdgeId,
    pub ports: Option<Change<EdgePorts>>,
    pub gates: Option<Change<Vec<GateId>>>,
    pub channels: Option<Change<Vec<TrackId>>>,
}

pub struct PlanDiff {
    pub substrate_changed: bool,
    pub node_slots: Vec<(NodeKey, Change<Slot>)>,
    pub group_scopes: Vec<(GroupKey, Change<GroupScopeSpec>)>,
    pub edges: Vec<EdgeChange>,
    pub bundles_added: Vec<Bundle>,
    pub bundles_removed: Vec<Bundle>,
}
```

**设计要点**：

- 输出全部按键升序（确定性）。
- `is_empty`：**provenance 不参与判定**。
- 边差异：先按字段（ports/gates/channels）各自 `diff_map`，再按 EdgeId 归并成 EdgeChange。
- bundles 用集合差（`Bundle: Eq`，束数小，O(n·m) 可接受）。
- `Display` 给出人读摘要。

## 关键 API

```rust
impl Plan {
    pub fn record_route(&mut self, edge: EdgeId, outcome: RouteOutcome) -> Result<(), PlanError>;
    pub fn record_ports_from_outcome(&mut self, edge: EdgeId, outcome: &RouteOutcome, substrate: &Substrate);
    pub fn assign_port_side_orders(&mut self);              // M1：侧内序
    pub fn assign_port_along_offsets(&mut self, …);         // M1：切向偏移
    pub fn assign_lane_indices(&mut self, substrate: &Substrate);  // M3：lane 下标
    pub fn detect_and_set_bundles(&mut self, min_suffix: usize);
    pub fn validate(&self) -> Result<(), PlanError>;
    pub fn fingerprint(&self) -> u64;
    pub fn semantic_eq(&self, other: &Self) -> bool;
    pub fn diff(prev: &Self, curr: &Self) -> PlanDiff;
}
```

## 典型测试场景

| 测试名 | 验证什么 |
|--------|---------|
| 指纹跨双跑一致 | 同内容同指纹 |
| `diff` 按键升序 | 确定性输出 |
| `semantic_eq` 忽略 provenance | 元数据非决策 |
| `record_route` 非 Converged 不改 Plan | 不伪造记录 |
| `record_route` 清空 bundles | channels 变更后旧合流失效 |

## 不该照搬

1. **`Provenance::LegacyAdapter`** `[v1-coupled]`——旧管线反向构造的占位，新实现不需要。
2. **`assign_port_along_offsets` 用像素 bbox 算切向偏移**：Plan 本应纯离散，`along_offset: f64` 字段把像素决策塞进了 Plan。新实现可考虑把 along_offset 移到 Metric（Plan 只存 side_order 离散序，along_offset 由 Metric 从 side_order + 节点 bbox 算）。
3. **`assign_port_side_orders` / `assign_lane_indices` 写权位置**：这两个在 channel_metric publish 入口被调，写权泄漏到度量相。新架构应让 L1（端口侧序）和 L4（lane 偏移）各有独立 Writer，度量相只读 Plan。
4. **`Plan::record_route` 把 `bundles` 清空**：依赖调用方记得重跑 `detect_and_set_bundles`。新实现可考虑把 bundle 检测作为 Plan 的派生视图（lazy compute），不存储。

## 新实现建议

- **保留**：Plan 字段结构、三套相等口径、FNV-1a + section 标签 + 长度前缀、Change 三态 diff、决策口径（provenance 不参与）。
- **调整**：`along_offset` 移到 Metric；Plan 只存离散 `side_order`。
- **拆写权**：`assign_port_side_orders` 归 L1 Writer，`assign_lane_indices` 归 L4 Writer，不在度量相调。
- **删除** `Provenance::LegacyAdapter`。
- **bundle 作派生视图**（可选）：不存储在 Plan，由 `channels` 派生，避免清空/重跑的脆弱性。
- **Plan 不嵌入 Substrate** 这条保留——Substrate 由 blueprint 重建，Plan 只存 sketch。
- **stable key**：新架构 §3.1 要求 `NodeKey/EdgeKey/GateKey/SegmentKey` 类型化，Atlas 用 `String`/`usize`——新实现换成 typed newtype。
