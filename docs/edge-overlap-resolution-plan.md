# 边线重合问题解决方案（面向 AI Agent 执行）

> 本文档是三阶段执行方案，每阶段可独立验证、独立合入。AI Agent 按阶段顺序执行，每阶段完成后运行验证用例并报告性能数据。

## 背景与现状

### 当前系统的三个重合处理机制

| 机制 | 文件 | 状态 | 局限 |
|------|------|------|------|
| X-1 Reroute | `edge_routing_orthogonal/mod.rs::reroute_conflicting_edges` | 启用 | 逐边绕行，高密度下失败率高，无全局通道规划 |
| Nudge | `edge_routing_orthogonal/nudge.rs` | **已禁用** | Z 字弯破坏正交性，失败率曾达 93% |
| Bundling | `edge_bundling/` | **默认关闭** | 用"端到端方向"判兼容(≤60°)，局部重合但整体方向不同的边无法合并 |
| X-3 Lane Assignment | 无 | **未实现** | 仅作为注释中的计划存在 |

### 当前 SVG 中的重合案例（showcase/architecture/c.layout-stress-nested.dfy）

| 重合位置 | 涉及边 | 端到端夹角 | bundling 能否合并 | 原因 |
|----------|--------|-----------|-------------------|------|
| x=518 长竖线 | E4(auth_svc→redis) & E8(biz_svc→mq) | 66° | **不能** | 超过 60° 阈值，但局部段完全平行 |
| x=102 长竖线 | E5(biz_svc→db_master) & E10(async_worker→db_master) | 48° | **能** | 同宿 db_master，方向兼容 |
| y=374 横线 | E2(lb→auth_svc) & E5(biz_svc→db_master) | 70° | **不能** | 超过 60° 阈值 |

**核心矛盾**：bundling 用端到端方向判兼容，但重合发生在局部段。

### 关键常量

- `EDGE_PARALLEL_GAP` = 8.0px（`constants.rs:44`）—— 平行边最小间距
- `STUB_GUARD_LENGTH` = 24.0px（`mod.rs:205`）—— stub 豁免阈值
- `MAX_COMPAT_ANGLE_DEG` = 60.0°（`compatibility.rs:34`）—— bundling 方向兼容阈值
- `GRID_STEP` = 8.0px（`trunk.rs:29`）—— 网格量化步长
- `PORT_CLEARANCE` = 16.0px —— 端口 stub 长度

---

## 阶段一：段级兼容性 + Partial Bundling

### 目标

让 bundling 能合并"局部段平行但端到端方向不同"的边。解决 x=518（E4/E8）这类重合。

### 改动文件

主要改动：
- `crates/drawify-core/src/layout/edge/edge_bundling/compatibility.rs` —— 新增段级重叠检测
- `crates/drawify-core/src/layout/edge/edge_bundling/mod.rs` —— 启用 bundling（测试用例配置）

次要改动（视需要）：
- `crates/drawify-core/src/layout/edge/edge_bundling/trunk.rs` —— trunk 坐标取重叠段而非全路径中位数
- `crates/drawify-core/src/layout/edge/edge_bundling/path_rewrite.rs` —— partial bundling fork 点处理

### 执行步骤

#### Step 1.1：实现段级平行重叠检测函数

在 `compatibility.rs` 新增函数：

```rust
/// 检测两条边的路径是否存在显著的平行段重叠。
///
/// 返回重叠段信息（轴、层坐标、重叠投影范围），用于段级兼容判定。
/// 如果存在重叠，说明两条边在局部共通道，可以 partial bundle。
fn find_parallel_segment_overlap(
    e1: &EdgeFeatures,
    e2: &EdgeFeatures,
) -> Option<ParallelOverlap>
```

**判定逻辑**：
1. 用 `decompose_path` 分解两条边的路径为段
2. 找出同轴（同 Horizontal 或同 Vertical）、同方向（Positive/Negative）的段对
3. 检查层坐标是否接近（`|layer1 - layer2| ≤ LAYER_TOLERANCE`，建议 16px = 2×GRID_STEP）
4. 检查投影是否有重叠（重叠长度 ≥ `MIN_OVERLAP_LENGTH`，建议 48px = 3×fork_distance）
5. 返回重叠最长的那个段对

**数据结构**：
```rust
struct ParallelOverlap {
    axis: Axis,
    layer: f64,           // 重叠段的层坐标（取两者平均）
    overlap_start: f64,   // 重叠投影起点
    overlap_end: f64,     // 重叠投影终点
    overlap_length: f64,  // 重叠长度
}
```

#### Step 1.2：修改 `compute_compatibility` 增加段级兼容路径

当前 `compute_compatibility`（compatibility.rs:199）的硬条件 3 是：
```rust
// 端到端方向夹角 ≤ 60°
let angle_deg = direction_angle_deg(e1.direction, e2.direction);
if angle_deg > MAX_COMPAT_ANGLE_DEG {
    return 0.0;
}
```

改为：
```rust
let angle_deg = direction_angle_deg(e1.direction, e2.direction);
let angle_ok = angle_deg <= MAX_COMPAT_ANGLE_DEG;
// 段级兼容：端到端方向不兼容，但局部有显著平行重叠段
let segment_overlap = find_parallel_segment_overlap(e1, e2);
let segment_ok = segment_overlap.is_some();

if !angle_ok && !segment_ok {
    return 0.0;
}
```

**加分项调整**：当 `segment_ok && !angle_ok` 时（段级兼容但端到端不兼容），distance_score 用重叠段的距离（接近 1.0）替代端到端距离评分，因为两条边在重叠段几乎重合。

#### Step 1.3：调整 trunk 坐标计算（如需要）

当前 `compute_trunk_coordinate`（trunk.rs:208）取每条边主轴方向最长段的层坐标中位数。对于 partial bundling，两条边的"最长段"可能不在重叠区域。

**调整**：在 `allocate_single_trunk` 中，如果 bundle 是通过段级兼容合并的（可在 `BundleCandidate` 中加标记），trunk 坐标优先取重叠段的层坐标。

#### Step 1.4：验证 partial bundling 的路径重写

当前 `rewrite_single_edge`（path_rewrite.rs）生成的路径结构是：
```
from_anchor → FromStub → MergeLeg → Trunk → ForkLeg → ToStub → to_anchor
```

对于 partial bundling（两条边只在部分段重叠），这个结构仍然适用：
- entry_i 和 exit_i 是重叠段的两端
- MergeLeg 从 from 端走到重叠段入口
- Trunk 是共享的重叠段
- ForkLeg 从重叠段出口走到 to 端

需要验证：当两条边的 from/to 在 trunk 的不同侧时，merge/fork leg 不会出现反向 stub。如果出现，用现有的 `min_ink_saving` 回退机制过滤掉。

#### Step 1.5：在测试用例中启用 bundling

修改 `showcase/architecture/c.layout-stress-nested.dfy`，在 orthogonal options 中添加 `bundling: 1.0`。

或者在 pipeline 层面提供一个全局开关用于测试。

### 验证标准

1. **编译通过**：`cargo build --release -p drawify-core` 无 error
2. **单元测试通过**：`cargo test --release -p drawify-core` 全部 pass
3. **SVG 验证**：生成 `c.layout-stress-nested.dfy` 的 SVG，检查：
   - x=518 处 E4/E8 是否合并为共享 trunk（视觉上一条线）
   - x=102 处 E5/E10 是否合并
   - 其他边无回归（不出现新的重合或斜线）
4. **性能数据**：报告 bundling 启用前后的路由时间、bundling 时间、总 layout 时间

### 风险与回退

- **风险**：partial bundling 的 fork 点可能产生新的重合（fork leg 与其他边）
- **回退**：`min_ink_saving` 阈值会过滤掉 ink 节省不足的 bundle；`find_collision_free_coord` 会过滤穿障的 trunk
- **安全网**：bundling 是后处理，失败不影响路由正确性，只是保持原路径

---

## 阶段二：X-3 Lane Assignment（车道分配）

### 目标

对 bundling 无法合并的平行段（语义不同或流向冲突），分配车道偏移使其分离。解决 y=374（E2/E5）这类重合。

### 前置条件

阶段一完成且验证通过。

### 改动文件

新增：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/lane_assignment.rs` —— 新模块

修改：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs` —— 在 reroute 之后、grid_snap 之前调用 lane assignment

### 执行步骤

#### Step 2.1：实现平行段检测与车道分组

```rust
/// 检测所有边的平行段冲突，按通道分组返回。
/// 同一通道（同轴+同层坐标±tolerance）内的平行段为一个 LaneGroup。
fn detect_lane_groups(edges: &[EdgeLayout]) -> Vec<LaneGroup>
```

**LaneGroup** 包含：轴、层坐标、组内所有段（edge_index, segment_index, 方向）。

#### Step 2.2：车道偏移分配

```rust
/// 为 LaneGroup 内的每个段分配车道偏移。
/// 同方向的段按 edge_index 排序，分配 ±n × EDGE_PARALLEL_GAP 偏移。
fn assign_lane_offsets(group: &LaneGroup) -> Vec<(usize, usize, f64)>
```

**规则**：
- 同方向段：按 edge_index 排序，第 i 条分配偏移 `(i - n/2) × EDGE_PARALLEL_GAP`
- 反方向段：分配到通道另一侧（避免对向流重叠）
- 段长 < STUB_GUARD_LENGTH 的 stub 段不参与（保持豁免）

#### Step 2.3：应用车道偏移到路径

```rust
/// 将车道偏移应用到边的路径上，修改段坐标。
fn apply_lane_offsets(edges: &mut [EdgeLayout], offsets: &[(usize, usize, f64)])
```

**关键约束**（与 nudge 的区别）：
- 只修改段中点坐标，不插入 Z 字弯
- 拐点坐标由相邻两段共享，需同时满足两段正交性
- 修改后验证正交性，失败则跳过该段

#### Step 2.4：集成到路由流水线

在 `route_orthogonal` 中，X-1 reroute 之后、grid_snap 之前调用：
```rust
// X-3: Lane Assignment
let lane_stats = assign_lanes(&mut edges, ...);
stats.lane_assignment = lane_stats;
```

### 验证标准

1. y=374 处 E2/E5 分离到不同车道（间距 ≥ 8px）
2. 无新的正交性破坏（所有段仍为纯水平/垂直）
3. 无节点穿透

---

## 阶段三：通道负载感知的 Reroute 增强

### 目标

让 X-1 reroute 具备全局视野，主动避开拥堵通道，而非逐边被动绕行。

### 前置条件

阶段二完成且验证通过。

### 改动文件

修改：
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs::reroute_conflicting_edges`
- `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs` —— 加入通道负载评分

### 执行步骤

#### Step 3.1：构建通道负载图

```rust
/// 统计每个通道（axis, layer）的边数，用于 reroute 时的通道选择。
fn build_channel_load_map(edges: &[EdgeLayout]) -> ChannelLoadMap
```

#### Step 3.2：reroute 时优先选择低负载通道

在 `select_best_path_with_scorer_stats` 的评分中加入通道负载惩罚：
- 候选路径经过的每个通道，按负载量叠加惩罚分
- 负载 > 3 的通道施加高惩罚，迫使 reroute 选择替代通道

#### Step 3.3：渐进式通道扩容

当所有低负载通道都被占用时，扩大通道搜索范围（增加 channel_margin），而非放弃 reroute。

### 验证标准

1. 高密度场景下 reroute 成功率提升（失败边数减少）
2. 无新的重合引入

---

## 执行顺序总览

```
[阶段一] 段级兼容 + Partial Bundling
  └─ 修改 compatibility.rs → 测试 → 验证 SVG → 报告性能
       ↓ 验证通过
[阶段二] X-3 Lane Assignment
  └─ 新建 lane_assignment.rs → 集成 → 测试 → 验证 SVG
       ↓ 验证通过
[阶段三] 通道负载感知 Reroute
  └─ 修改 reroute + scoring → 测试 → 验证 SVG
```

## 最终流水线顺序（三阶段完成后）

```
replan_slots
  → bundling(段级兼容版)      ← 阶段一
  → X-3 lane_assignment       ← 阶段二
  → X-1 reroute(负载感知版)   ← 阶段三
  → grid_snap
```
