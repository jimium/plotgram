# Channel 搜索：词典序 Dijkstra + ScopeMask

> 父页：[atlas-reference/README.md](README.md)  
> 对应新架构：[architecture.md §6.1 L2/L3 写权](../architecture.md)  
> Atlas 源：`crates/v1/plotgram-core/src/layout/atlas/channel/{search,graph,verify}.rs`

## 这是什么

在 Substrate 上为每条边选 track 序列 + gate 序列。Atlas 用**词典序 Dijkstra**（Q3 折点严格优先 > Q4 长度 > Q5 拥塞软偏好）+ **ScopeMask 硬过滤**（L8 唯一归零机制）+ **有界 rip-up**（峰值 track 上的边 release → 重路）。这是 Atlas 最干净的一层，写权清晰。

## 核心数据结构

```rust
/// L8 作用域掩码：搜索期硬过滤，非计价。
/// 允许集 = {None} ∪ chain(u_scope) ∪ chain(v_scope)
pub struct ScopeMask { allowed: BTreeSet<Option<GroupId>> }

pub enum Via { Link, Gate(GateId) }

pub struct Transition { pub to: TrackId, pub via: Via }

/// 邻接表：顶点是 track，转移是 link / gate。
/// gate 按 crossings 逐对展开（一一配对非笛卡尔积），共享同一容量池。
pub struct ChannelGraph<'s> {
    substrate: &'s Substrate,
    adjacency: BTreeMap<TrackId, Vec<Transition>>,
}

/// track 是 Demand 输出（无上限）；gate/port 是约束（满容从图上消失）。
pub struct Occupancy {
    track_usage: BTreeMap<TrackId, u32>,   // lane_demand，无上限
    gate_usage: BTreeMap<GateId, u32>,     // 共享容量池
    port_usage: BTreeMap<PortSlotId, u32>,
}

pub struct RouteOutcome {
    pub tracks: Vec<TrackId>,              // 含起止宿主轨道
    pub gates: Vec<GateId>,                // 按穿越顺序
    pub from_port: Option<PortSlotId>,
    pub to_port: Option<PortSlotId>,
    pub cost: LexCost,                     // [v1-coupled] 来自 kernel::cost
    pub status: SolverStatus,              // [v1-coupled]
}
```

`LexCost` `[v1-coupled]` 是词典序五元组：`q3_bends > q4_length > q5_alignment`（Atlas 把拥塞软偏好复用在 q5 字段，名义是 alignment——新实现应给拥塞独立键）。

## 关键不变量

| 不变量 | 含义 |
|--------|------|
| L8 ScopeMask 硬过滤 | 「无关组的段不得进入」——A2 归零的唯一机制，非计价 |
| L7-T5 自环不建模 | 两端口同节点 → 显式 `Infeasible`，不返回单轨道平凡解 |
| I.5 端口侧容量硬约束 | 任一端 `!port_open` → `Infeasible` |
| 闸口满容 → 转移消失 | gate `Fixed(c)` 时 `gate_load >= c` → 邻接项不存在 |
| 轨道占用无上限（B1） | track_usage 无上限，lane_demand 是 Demand 输出 |
| 无几何兜底 | 无路可走 → 显式 `Infeasible`，注释原话「本模型无坐标可兜」 |
| gate 按边去重 | 同一边多次穿同一 gate（嵌套组往返）只占 1 单位容量 |
| commit/release 严格互逆 | 释放后重路由与首次完全一致 |

## 算法：route_inner（词典序 Dijkstra）

```
1. 解析 from/to port；同节点 → Infeasible（L7-T5）
2. 端口侧容量检查（任一满容 → Infeasible）
3. start_cost.q4 = start.span_weight
   若 congestion_bias：q5 = lane_demand(start)
4. start == goal：返回单轨道解
5. BinaryHeap + BTreeMap<TrackId, (cost, parent, via)>：
   - 邻居迭代中硬过滤：
     · 满容 gate 跳过
     · 目标 scope 不在 mask 内跳过（L8）
   - step_cost:
     · link 必 +1 折点（异向构造保证）
     · gate 同向 +0、异向 +1（防御兜底，实际不命中）
     · q3_bends 累加
     · q4_length += to.span_weight
     · congestion_bias 时 q5 += lane_demand(to)
6. reconstruct: 沿 parent 回溯，收集 via=Gate 的 gate 序列（正序）
```

**确定性**：`State` 的 `Ord` 反转使最小 LexCost 优先、平局按 `TrackId`；邻居按 `(to, via 序)` 排序，BinaryHeap 平局时展开序仍确定。

## 算法：route_candidates

```
1. froms = Substrate::ports_of_node_side(u, side)  // 按 (slot_index, id) 升序
   tos   = 同上 for v
2. 升序去重
3. 笛卡尔积遍历，严格 `<` 替换当前最优
4. 任一端空 → Err(EmptyCandidates)
5. 全不可行 → Ok(infeasible())，不修改 occupancy
```

平局首个最小代价对获胜（`(from_id, to_id)` 升序遍历）。

## 算法：bounded_phase_i_ripup

```
常量：MAX_RIPUP_ROUNDS=2, MAX_RIPUP_EDGES=8   // 新实现应作为 profile 参数

for round in 0..MAX_RIPUP_ROUNDS:
    peak_track = argmax lane_demand
    candidates = 峰值 track 上的边，按 LexCost 降序取前 MAX_RIPUP_EDGES
    for edge in candidates:
        release(edge)
        new_route = route_candidates_congested(edge)  // 启用 q5
        accept if:
            · peak_after < peak_before
            · OR (peak 相等 && sum 更小)
            · OR (路径变 && path_load 更小)
        不接受则 release 新路径 + 恢复旧路径
```

## 独立证明器：verify_route_scope

`verify.rs` 的 `verify_route_scope(track_ids, gate_ids, substrate) -> Vec<RouteScopeViolation>`：

- **显式不复用 ScopeMask 代码路径**——允许集由 groups 的 parent 链在此重新推导
- 输入取 `&[TrackId]` + `&[GateId]` 而非 `RouteOutcome`，可复验 `Plan.channels` / `Plan.gates`
- violation 类型：`UnknownTrack` / `ForeignScope` / `MissingGate` / `GateMismatch` / `UnexpectedGate`
- 空路径（`Infeasible`）自然通过（gates 也须为空）

这是「构造保证 + 独立证明」双层范式的好例子——构造机制（ScopeMask）和证明义务（verify_route_scope）分离，互为冗余。

## 典型测试场景

| 测试名 | 验证什么 |
|--------|---------|
| `l8_scope_mask_blocks_borrowed_passage_through_group` | 两端在组外，掩码={None} 拦截借道直穿；放行组后经两侧 gate 直穿成功 |
| `exhausted_gate_yields_explicit_infeasible` | 唯一 gate 链容量 1：第二条边 `Infeasible`，tracks 空，不伪造路径 |
| `full_gate_forces_detour_through_alternative` | g1 满容后改走 g2 绕行（折点更多但 Converged） |
| `fewer_bends_beat_shorter_length` | Q3 > Q4：选 2 折点总长 12 而非 4 折点总长 5 |
| `equal_bends_prefer_shorter_length` | Q3 平局时 Q4 生效 |
| `occupancy_release_restores_feasibility` | commit/release 严格可逆 |
| `track_lane_demand_grows_without_limit` | 5 条边挤同一轨道全部成功，lane_demand=5 |
| `cross_group_route_passes_gates_innermost_first` | 嵌套组穿出 gate 序=[g2,g1]，三段同向 → q3_bends=0 |
| `boundary_gate_is_shared_capacity_pool` | 一个 gate 多配对共享容量池，容量 2 → 第三条 Infeasible |
| `port_side_capacity_limits_concurrent_edges` | 端口侧容量 1：第二条共用边 Infeasible |
| `verifier_passes_genuine_nested_group_route` | 独立验证器对真实 route 产出零违规 |

## 不该照搬

1. **`q5_alignment` 字段名复用做拥塞软偏好**——新实现应给拥塞独立键或显式区分 alignment vs congestion。
2. **`route_candidates` 是单边搜索**，不近似 Q2 交叉 / Q5 对齐 / Q6 对称——注释承认「属于上层全局协调」。新实现应明确这条边界。
3. **`LexCost` / `SolverStatus` 来自 `crate::layout::kernel::cost`** `[v1-coupled]`——新实现需独立 cost 词汇。
4. **gate 异向分支 `u32::from(differs)` 注释明说「实际不命中；保留作防御性兜底」**——新实现应删掉，不要保留防御性死分支。
5. **`Occupancy` 把 track/gate/port 三种占用混在一个结构**——track 是 Demand（输出）、gate/port 是约束（输入），语义不同。新实现可拆为 `LaneDemand`（输出）+ `CapacityLedger`（约束）。
6. **`EndpointError::From` for `SubstrateError`** 是反向耦合（搜索层错误转 substrate 错误）——新实现不让错误类型互转。
7. **`Occupancy::lane_demand` 在搜索层暴露给 step_cost 当软偏好**——这是合理的「相 I 内部 rip-up」，但**绝不应当**让度量相或下游再读 `lane_demand` 修端口选路（写权归属相 I）。

## 新实现建议

- 保留：词典序 Dijkstra + ScopeMask 硬过滤 + commit/release 互逆 + 有界 rip-up。
- 替换 `LexCost` 为独立 cost 词汇，给拥塞独立键。
- 拆 `Occupancy` 为 `LaneDemand`（Demand 输出，无上限）+ `CapacityLedger`（约束，满容消失）。
- 删除 gate 异向防御兜底分支。
- rip-up 常量（`MAX_RIPUP_ROUNDS` / `MAX_RIPUP_EDGES`）作为 profile 参数，不硬编码。
- rip-up 接受条件明确目标函数（降峰 vs 均摊），不要混。
- 保留 `verify_route_scope` 独立证明器，并扩展到「折点账目」「port 侧容量」「bundle 一致性」等其他不变量。
