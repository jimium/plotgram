# Substrate / Segment / Gate 数据结构

> 父页：[atlas-reference/README.md](README.md)  
> 对应新架构：[architecture.md §6.2 空间零件](../architecture.md)  
> Atlas 源：`crates/v1/plotgram-core/src/layout/atlas/channel/{substrate,derive}.rs`

## 这是什么

Atlas 的 Channel 子系统的**离散骨架**：在 rank×order 网格上切出段（Track）、注册组边界闸门（Gate）、挂接端口（PortSlot），全部**零几何**（无 Point/Rect/像素坐标，只有 id 与拓扑）。新架构 §6.2 的 Substrate / Segment / Gate 三种零件直接对应。

## 核心数据结构

```rust
pub struct TrackId(pub u32);
pub struct GroupId(pub u32);
pub struct GateId(pub u32);
pub struct PortSlotId(pub u32);
pub type NodeKey = String;
pub type EdgeId = usize;

pub enum TrackOrient { Main, Cross }       // 沿流向 / 正交流向
pub enum GateSide { MainLow, MainHigh, CrossLow, CrossHigh }
pub enum PortSide { MainLow, MainHigh, CrossLow, CrossHigh }  // 与 GateSide 同构
pub enum GateCapacity { Unbounded, Fixed(u32) }

/// 一条 track = 一个 Segment 零件。
/// 奇偶坐标编码：line 是 gap 索引；ext = (lo, hi) 闭区间，
///   2j = gap 缝，2j+1 = 节点体。零长段（lo==hi）不注册。
pub struct Track {
    pub id: TrackId,
    pub orient: TrackOrient,
    pub scope: Option<GroupId>,    // None = 顶层（根走廊）
    pub span_weight: f64,          // 段覆盖 gap 数；L3 编码槽位跨度，恒 ≥ 1
    pub line: usize,               // 所在 gap 线索引
    pub ext: (usize, usize),       // 奇偶坐标闭区间
}

pub struct GroupScope {
    pub id: GroupId,
    pub parent: Option<GroupId>,
    pub ranks: (usize, usize),     // 闭区间
    pub orders: (usize, usize),
}

/// 组边界闸门。crossings 是 (组内段, 组外段) 一一配对，非笛卡尔积。
pub struct Gate {
    pub id: GateId,
    pub group: GroupId,
    pub side: GateSide,
    pub line: usize,                              // 被穿越的边界线
    pub crossings: Vec<(TrackId, TrackId)>,       // 一一配对
    pub capacity: GateCapacity,
}

pub struct PortSlot {
    pub id: PortSlotId,
    pub track: TrackId,           // 宿主轨道（唯一出入口）
    pub capacity: u32,            // 0 = 不限
    pub node: NodeKey,
    pub side: PortSide,
    pub slot_index: u32,          // 同侧离散槽（平行边区分）
}

pub struct Substrate {
    tracks: BTreeMap<TrackId, Track>,
    groups: BTreeMap<GroupId, GroupScope>,
    gates: BTreeMap<GateId, Gate>,
    ports: BTreeMap<PortSlotId, PortSlot>,
    port_keys: BTreeSet<(NodeKey, PortSide, u32)>,    // P-inv-2 唯一性
    gate_keys: BTreeSet<(GroupId, GateSide)>,         // G-inv-3 唯一性
    links: BTreeSet<(TrackId, TrackId)>,              // 归一化 (min, max)
}
```

## 奇偶坐标编码（核心创新）

`ext: (usize, usize)` 是闭区间，坐标含义：

- `2j` = gap 缝（第 j 条 gap 线）
- `2j+1` = 节点体（第 j 个节点的中心带）

这让 B1–B8 切割规则**全部由编码自然产出**：

| 规则 | 含义 | 由编码如何保证 |
|------|------|---------------|
| B1 | lane 是输出，track 无容量上限 | track 不带 capacity 字段 |
| B4 | 一次切完，不留碎段 | 切点排序去重后一次注册 |
| B5 | 零长段消失 | `ext.0 > ext.1` 拒绝注册 |
| B6 | 相邻组共享缝独立成根段 | 共享缝 = 偶坐标，独立成段 scope=共同父 |
| B7 | 退化组（单节点）不切线 | 无切点 → 无段 → 宿主边界缝根段 |
| B8 | 边界缝归外段 | 内段 ext 贴 `2*line+1`，外段 ext 贴 `2*line` |

## 构建期防线

`Substrate::link` / `add_gate` / `attach_port` 是唯一写入路径，全部不变量在构建期强制：

| 不变量 | 含义 | 违反时错误 |
|--------|------|-----------|
| H2 穿组不可表达 | 跨 scope 直连轨道拒绝 | `CrossScopeConnection` |
| H3 斜线不可表达 | 同 orient 轨道相交拒绝；转移只在 Main×Cross 交口 | `ParallelLink` |
| L1 相交才 link | `ta.covers_gap(tb.line) && tb.covers_gap(ta.line)` | `NonIntersectingLink` |
| B5 ext 合法 | `ext.0 <= ext.1` | `InvalidExtent` |
| P-inv-2 端口唯一 | `(node, side, slot_index)` 唯一 | `DuplicatePortSlot` |
| P-inv-4 端口侧匹配 | `track.orient == side.required_orient()` | `PortSideMismatch` |
| G-inv-1 gate 侧匹配 | `GateSide::crossing_orient()` 与配对段 orient 相符 | — |
| G-inv-2 gate 边界相邻 | 配对段同 line、隔边界相邻（B8 缝归外段） | `GatePairMismatch` |
| G-inv-3 gate 侧唯一 | 同 `(group, side)` 只一个 gate | `DuplicateGateSide` |
| gate scope 合法 | inner.scope == Some(group)，outer.scope 是祖先 | `InvalidGateScope` |

## 关键算法：derive_substrate

从 `ChannelBlueprint { nodes(rank,order), groups(members,parent), edges }` 推导 Substrate：

```
1. 组矩形 + GroupId 分配
   - collect_descendants 收集直系+子组成员
   - 包围盒 = min/max(rank, order)
   - 按深度升序注册（父先于子）

2. 组矩形交叠 / 纯净性检查
   - 兄弟组包围盒相交非嵌套 → OverlappingGroups
   - 矩形内不得有非后代节点 → ForeignNodeInGroupRect
   - 纯容器组（无直属节点）合法

3. 切割注册段（cut_line）
   - 每条 Cross 线 k：收集 r0 < k ≤ r1 的组为 cutter
   - 沿线内部奇偶区间 (2·o0+1, 2·o1+1)
   - 每组贡献两个切点：interior.0 与 interior.1+1
   - 切点排序去重 → 一次切完
   - 每段 scope = 包含它的最深切割组
   - span_weight = [a,b] 内偶坐标个数，空 cover 取 1

4. link：对每对异向段按「区间互含 + 同 scope」建（按线索引扫描避免全平方）

5. add_side_gate × 4 侧
   - 遍历被切线 lo..=hi，找贴边界的 (组内段, 组外段) 对
   - 内段判据：scope == Some(gid) 且贴边界
   - 外段判据：scope 是 gid 的祖先 且贴边界缝
   - 配对为空不注册（B7 推论）
```

## L6 自反证：verify_no_group_penetration

`Substrate::verify_no_group_penetration() -> Vec<PenetrationViolation>`：

- 对每段 × 每非祖先组，按 orient 选轴
- Cross 段看 `r0 < line ≤ r1` × `[2·o0+1, 2·o1+1]`
- Main 段对称
- `line_inside && ext_overlaps` → 违规
- 用奇坐标表达「内部」，故贴边界的组外段不误报

**注意**：这是事后自反证，正确性来源是 derive 的构建期防线；L6 仅作回归探针。新实现**不应**把 L6 当合法性来源。

## 典型测试场景

| 测试名 | 验证什么 |
|--------|---------|
| `link_across_scopes_is_rejected` | H2 穿组拒绝 |
| `parallel_tracks_cannot_intersect` | H3 斜线拒绝 |
| `non_intersecting_segments_cannot_link` | L1 不相交拒绝 |
| `inverted_extent_is_rejected` | B5 ext 合法 |
| `gate_pair_orientation_must_match_side` | G-inv-1 |
| `gate_pair_must_be_boundary_adjacent` | G-inv-2 |
| `b8_boundary_seam_belongs_to_outer_segment` | B8 缝归外段 |
| `l6_detects_injected_penetrating_segment` | L6 注入越界段报一条 |
| `a9_cut_rules_segment_structure_b1_b2_b4_b8` | 4×4 网格 + 单组，逐字段断言 B1/B2/B4/B8 |
| `b6_shared_seam_between_adjacent_groups_is_root_segment` | 相邻组共享列缝独立成根段 |
| `b7_degenerate_group_hosts_on_root_seam` | 单节点组不切线、不产 gate |
| `derive_container_group_without_direct_members` | 纯容器组合法 |
| `derive_rejects_overlapping_group_rects` | 兄弟组矩形相交非嵌套拒绝 |
| `derive_nested_groups_route_through_gate_chain` | 嵌套组穿出 ≥2 gate，L6 全过 |

## 不该照搬

1. **`PortSide` 与 `GateSide` 同构但保留两套枚举**——新实现可合并简化。
2. **`PortSlot.capacity` 与 v1 的 `PORT_SIDE_CAPACITY` 隐含耦合** `[v1-coupled]`——新架构应让 L1 端口写权归一处理。
3. **`Substrate` 同时承担「零件库 + 关系图」双职责**（持有 `links`）——新实现可拆 Substrate（零件）/ Segment-邻接（关系）。
4. **`derive_substrate` 内部用 `expect()` panic 兜底**构建期不变量——新实现应返回 `Result`（derive 是接缝，输入可能不合法）。
5. **`ChannelBlueprint::gate_capacity_override`** 让 substrate 构建期埋诊断分支——新实现不应在构建期埋诊断逃逸口。
6. **`super::probe::sanitize_overlapping_groups`** `[v1-coupled]` 在 derive 入口消解交叠组——新架构应让组矩形消解在上游 Plan 阶段，不在 derive 内部。
7. **`derive_node_ports` 默认四侧 × 4 slot** 与 `PORT_SIDE_CAPACITY` 耦合——新架构应让 L1 端口写者独立决定挂接策略，derive 只产 Substrate 骨架。

## 新实现建议

- 保留奇偶坐标编码 + B1–B8 切割模型 + 构建期防线（H2/H3/P-inv/G-inv）原样。
- 把 `Substrate` 拆为「零件注册器」+「邻接视图」两个类型，邻接视图借用 Substrate。
- derive 接口换成抽象蓝图（节点槽位 + 组树 + 边列表），不读 `Diagram`。
- `verify_no_group_penetration` 保留为回归探针，但合法性由构造保证。
- L1 端口挂接独立成一个 Writer，不在 derive 里批量默认挂四侧。
