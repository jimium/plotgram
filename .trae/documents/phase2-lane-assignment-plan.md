# Phase 2 + Phase 3 实现计划：X-3 Lane Assignment & 通道负载感知 Reroute

## Summary

实现两个阶段：
- **Phase 2 (X-3 Lane Assignment)**：对 bundling 无法合并的残余平行段，通过平移整段 cross-axis 坐标分配车道偏移，分离 anti-parallel 重合段。不插入 Z 字弯，保持正交性。
- **Phase 3 (通道负载感知 Reroute)**：在 X-1 reroute 的 scorer 中加入通道负载惩罚，使重路由优先选择低负载通道，从源头减少拥堵。

两阶段互补：Phase 2 治已存在的重合，Phase 3 防新重合产生。

---

## Phase 2: X-3 Lane Assignment

### 插入点

`mod.rs` 第 726-732 行（step 4f，原 disabled nudge 位置）。在 X-1 reroute 和 X-2 flip_stub 之后，X-0 统计之前。

### 核心设计：为什么 Lane Shift 不破坏正交性

正交路径中相邻段交替 H/V。平移段 si 的 cross-axis 坐标（H 段移 y，V 段移 x）时：
1. 被移段两端点 cross-axis 同移 → 仍为 H/V，长度不变
2. 前邻段 si-1（正交）：共享端点 main-axis 不变 → 仍为 V/H，仅长度变化
3. 后邻段 si+1（正交）：同理

**与 nudge 的区别**：nudge 修改 main-axis 坐标需 Z 字弯补偿；lane shift 修改 cross-axis 坐标，相邻段自动适配。

### 改动文件

#### 1. 新建 `lane_assignment.rs`

**路径**：`crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/lane_assignment.rs`

**公开 API**：
```rust
#[derive(Default, Debug, Clone, Copy)]
pub struct LaneAssignmentStats {
    pub lane_groups: usize,
    pub segments_shifted: usize,
    pub shifts_failed: usize,
}

pub fn assign_lanes(
    edges: &mut [EdgeLayout],
    grid: &mut SegmentGrid,
    nodes: &HashMap<String, NodeLayout>,
    sorted_node_ids: &[String],
    min_gap: f64,
) -> LaneAssignmentStats
```

**内部数据结构**：
```rust
struct SegmentInfo {
    ei: usize, si: usize,
    is_horizontal: bool,
    layer: f64,        // H段=y, V段=x
    is_positive: bool, // 方向
    length: f64,
    n_segs: usize,
    p1: Point, p2: Point,
}
```

**算法步骤**：

**Step 1: 收集可偏移段**
- 遍历所有边的所有段
- 跳过 stub 段：`si == 0 || si == n_segs - 1`
- 跳过不可偏移段：`si < 1 || si > n_segs - 2`（端点为锚点的段）
- 跳过退化段：`length < EPS`

**Step 2: 检测冲突对 + Union-Find 分组**
- 对每对同轴段检查 `|layer_i - layer_j| < min_gap` 且投影重叠
- 调用 `segments_violate_spacing` 确认冲突
- Union-Find 合并冲突段
- 复杂度 O(N²)，N 通常 < 100

**Step 3: 分配偏移**
- 每个组按 `(is_positive, ei, si)` 排序（Negative 在前）
- 偏移公式：`offset[i] = (i - (n-1)/2) * min_gap`
- n=2 [Neg,Pos]: -4,+4 → gap=8 ✓
- n=3 [Neg,Pos,Pos]: -8,0,+8 → gaps=8,8 ✓

**Step 4: 应用偏移 + 验证**
- H 段：`points[si].y += offset; points[si+1].y += offset;`
- V 段：`points[si].x += offset; points[si+1].x += offset;`
- 验证（任一失败则跳过）：
  - a. 邻段不反转：si-1/si+1 方向不反号
  - b. 邻段不退化：长度 ≥ `MIN_ADJACENT_LEN`（4px）
  - c. 无节点穿透：`Rect::expanded(NODE_OBSTACLE_PAD).segment_crosses_interior` 检查 si-1/si/si+1
- 通过：`edges[ei].set_polyline_points(points)`
- 失败：increment `shifts_failed`

**Step 5: 重建 SegmentGrid**
```rust
grid.remove_by_edges(&shifted_edges);
for &ei in &shifted_edges {
    grid.insert_path(&edges[ei].path_points(), ei);
}
```

**节点穿透检测**（内部函数，参考 nudge.rs:30 模式）：
```rust
fn segment_hits_node(a: Point, b: Point, nodes: &HashMap<String, NodeLayout>, sorted_node_ids: &[String]) -> bool
```

#### 2. 修改 `mod.rs`

**a. 声明子模块**（~line 36）：
```rust
pub(super) mod lane_assignment;
```

**b. step 4f 替换**（~line 726-732）：
```rust
// ── 4f. X-3: Lane Assignment 车道分配 ──
let t_lane = crate::layout::perf::Instant::now();
let lane_stats = lane_assignment::assign_lanes(
    &mut edges, &mut grid, &result.nodes, &obstacles.sorted_node_ids, EDGE_PARALLEL_GAP,
);
ortho_stats.lane_groups = lane_stats.lane_groups;
ortho_stats.lane_segments_shifted = lane_stats.segments_shifted;
ortho_stats.lane_shifts_failed = lane_stats.shifts_failed;
crate::perf_log!("[perf]     x3_lane_assignment: {:.2}ms ({} groups, {} shifted, {} failed)", ...);
```

保留 nudge 字段为 0（nudge 仍禁用）。

#### 3. 扩展 `OrthoDebugStats`（mod.rs:562）

添加三个字段：
```rust
pub lane_groups: usize,
pub lane_segments_shifted: usize,
pub lane_shifts_failed: usize,
```

#### 4. 单元测试（lane_assignment.rs 内）

1. `test_two_anti_parallel_v_segments_separated` — 反向 V 段同 x → 分离
2. `test_two_same_direction_segments_separated` — 同向 V 段同 x → 分离
3. `test_three_segments_centered` — 三段，中间不动
4. `test_stub_segment_not_shifted` — 首尾段不偏移
5. `test_segment_adjacent_to_anchor_not_shifted` — 3 点路径跳过
6. `test_shift_rejected_for_node_penetration` — 穿节点 → 跳过
7. `test_adjacent_segment_reversal_rejected` — 邻段反转 → 跳过
8. `test_no_conflict_no_shift` — 无冲突不偏移
9. `test_grid_updated_after_shift` — grid 同步更新
10. `test_deterministic_output` — 确定性

---

## Phase 3: 通道负载感知 Reroute

### 目标

让 X-1 reroute 的 scorer 具备全局通道视野，优先选择低负载通道，减少高密度场景下的重路由失败率。

### 设计

在 `DefaultScorer::score` 中新增 `channel_load_penalty` 项。通道负载图在 reroute 开始时从当前所有边的路径构建，通过 `RoutingContext` 传入。

### 改动文件

#### 1. 新建 `channel_load.rs`

**路径**：`crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/channel_load.rs`

**数据结构**：
```rust
use crate::layout::geometry::Axis;
use std::collections::HashMap;

/// 通道负载图：key = (轴, 量化层坐标)，value = 该通道上的段数
#[derive(Debug, Clone, Default)]
pub struct ChannelLoadMap {
    loads: HashMap<(Axis, i64), usize>,
    step: f64,  // 量化步长 = GRID_STEP = 8.0
}

impl ChannelLoadMap {
    pub fn build(edges: &[EdgeLayout], step: f64) -> Self { ... }
    pub fn load(&self, axis: Axis, layer: f64) -> usize { ... }
}
```

**构建逻辑**：
```rust
pub fn build(edges: &[EdgeLayout], step: f64) -> Self {
    let mut loads = HashMap::new();
    for ei in 0..edges.len() {
        let points = edges[ei].path_points();
        for w in points.windows(2) {
            let dx = (w[1].x - w[0].x).abs();
            let dy = (w[1].y - w[0].y).abs();
            if dx < EPS && dy > EPS {
                // V 段，layer = x
                let key = (Axis::Vertical, (w[0].x / step).round() as i64);
                *loads.entry(key).or_default() += 1;
            } else if dy < EPS && dx > EPS {
                // H 段，layer = y
                let key = (Axis::Horizontal, (w[0].y / step).round() as i64);
                *loads.entry(key).or_default() += 1;
            }
        }
    }
    Self { loads, step }
}
```

**查询逻辑**：量化 layer 后查 HashMap，未命中返回 0。

#### 2. 修改 `context.rs` — 扩展 RoutingContext

```rust
pub struct RoutingContext<'a> {
    pub nodes: &'a HashMap<String, NodeLayout>,
    pub group_ctx: &'a GroupRoutingContext,
    pub grid: &'a SegmentGrid,
    pub cfg: &'a OrthoConfig,
    pub obstacles: &'a PreparedObstacles,
    /// Phase 3: 通道负载图（reroute 时传入，初始路由为 None）
    pub channel_load: Option<&'a ChannelLoadMap>,
}
```

更新所有 6 处 `RoutingContext` 构造（mod.rs:547/654/913/1056/2103 + orthogonal_tests.rs:609）：
- 初始路由、replan_slots、flip_stub、tests：传 `None`
- reroute_conflicting_edges：传 `Some(&load_map)`

#### 3. 修改 `scoring.rs` — 新增 channel_load_penalty

在 `DefaultScorer::score` 中：
```rust
fn score(&self, path: &[Point], ctx: &RoutingContext, pair: &EndpointPair) -> f64 {
    let mut score = path_length(path);
    score += path.len().saturating_sub(2) as f64 * BEND_PENALTY;
    score += obstacle_penalty(path, pair.from_id(), pair.to_id(), ctx.nodes, ctx.group_ctx, &ctx.obstacles);
    score += edge_overlap_penalty(path, ctx.grid);
    // Phase 3: 通道负载惩罚
    if let Some(load_map) = ctx.channel_load {
        score += channel_load_penalty(path, load_map);
    }
    if !ctx.group_ctx.corridors.is_empty() {
        score += corridor_misalignment_penalty(path, &ctx.group_ctx.corridors, ctx.group_ctx.corridor_misalignment_penalty);
    }
    score
}
```

**新增函数**：
```rust
const CHANNEL_LOAD_THRESHOLD: usize = 3;   // 负载超过此值才惩罚
const CHANNEL_LOAD_PENALTY: f64 = 200.0;   // 每条多余边的惩罚

pub fn channel_load_penalty(path: &[Point], load_map: &ChannelLoadMap) -> f64 {
    let mut penalty = 0.0;
    for w in path.windows(2) {
        let dx = (w[1].x - w[0].x).abs();
        let dy = (w[1].y - w[0].y).abs();
        let (axis, layer) = if dx < EPS && dy > EPS {
            (Axis::Vertical, w[0].x)
        } else if dy < EPS && dx > EPS {
            (Axis::Horizontal, w[0].y)
        } else {
            continue; // 跳过退化段
        };
        let load = load_map.load(axis, layer);
        if load > CHANNEL_LOAD_THRESHOLD {
            penalty += (load - CHANNEL_LOAD_THRESHOLD) as f64 * CHANNEL_LOAD_PENALTY;
        }
    }
    penalty
}
```

**惩罚值设计**：
- `CHANNEL_LOAD_PENALTY` = 200.0 —— 介于 `BEND_PENALTY`(16) 和 `EDGE_OVERLAP_PENALTY`(1200) 之间
- 一个候选路径经过 load=5 的通道 → 多 2×200=400 惩罚 ≈ 25 个 bend penalty
- 足以让 scorer 偏好低负载通道，但不会完全压制更短路径

#### 4. 修改 `mod.rs::reroute_conflicting_edges`

在每个 reroute round 开始时构建 load map：
```rust
for round in 0..MAX_REROUTE_ROUNDS {
    // Phase 3: 构建当前轮次的通道负载图
    let load_map = ChannelLoadMap::build(&edges, GRID_STEP);

    // ... 冲突检测 ...

    for &(ei, _) in &conflicts {
        // ... 移除 ei from grid ...

        for &margin in &reroute_margins {
            let ctx = RoutingContext {
                nodes, group_ctx, grid,
                cfg: &r_cfg, obstacles,
                channel_load: Some(&load_map),  // NEW
            };
            // ... select_best_path ...
        }
    }
}
```

注意：load_map 在 round 开始时构建，包含所有边的当前路径。当 ei 被移除 grid 后，load_map 仍包含 ei 的旧路径——这是有意的，因为我们要避免 ei 的新路径走回原来的拥堵通道。

#### 5. 修改 `mod.rs` 声明子模块

```rust
pub(super) mod channel_load;
```

并在 mod.rs 顶部 re-export：
```rust
pub(super) use channel_load::ChannelLoadMap;
```

#### 6. 扩展 `OrthoDebugStats`

添加字段：
```rust
/// Phase 3: reroute 时构建的通道负载图的最大负载值
pub max_channel_load: usize,
```

在 `reroute_conflicting_edges` 中记录：
```rust
ortho_stats.max_channel_load = load_map.max_load();
```

在 `ChannelLoadMap` 上添加 `pub fn max_load(&self) -> usize`。

#### 7. 单元测试（channel_load.rs 内）

1. `test_build_load_map_counts_segments` — 正确统计通道段数
2. `test_load_returns_zero_for_empty_channel` — 空通道返回 0
3. `test_load_quantizes_layer` — 相近 layer 量化到同一通道
4. `test_channel_load_penalty_zero_for_low_load` — load ≤ 3 无惩罚
5. `test_channel_load_penalty_scales_with_load` — load=5 → 2×200=400
6. `test_scorer_prefers_low_load_channel` — 端到端验证 scorer 偏好低负载路径

---

## 实现顺序

```
1. Phase 2: lane_assignment.rs → 集成 mod.rs → 测试 → 验证 SVG
2. Phase 3: channel_load.rs → 扩展 RoutingContext → 修改 scorer → 集成 reroute → 测试
3. 全量验证：cargo test + SVG 检查 + 性能数据
```

## Verification Steps

### 编译
```bash
cargo build --release -p plotgram-core
```

### 单元测试
```bash
cargo test --release -p plotgram-core
```

### SVG 验证
生成 `showcase/architecture/c.layout-stress-nested.pgm` 的 SVG：
- E6/E9 的 V 段分离到不同 x，gap ≥ 8px（Phase 2）
- 无正交性破坏、无节点穿透
- reroute 后高负载通道边数减少（Phase 3）
- 其他边无回归

### 性能数据
- `x3_lane_assignment` 耗时（预期 < 1ms）
- `x1_reroute` 耗时变化（Phase 3 可能略增因 load map 构建，但 reroute 成功率应提升）
- 总 routing 时间

### 确定性
同一输入多次运行，输出路径完全一致（AGENTS.md §2）。

## Assumptions & Decisions

1. **Phase 2 偏移公式**：`(i - (n-1)/2) * min_gap`，对称分布，anti-parallel 段通过排序自然分离
2. **Phase 2 仅偏移 interior 段**（1 ≤ si ≤ n_segs-2）：保护端口锚点
3. **Phase 2 不检查偏移后新间距违规**：偏移量小（±8px），目标是分离，X-0 统计会报告残余冲突
4. **Phase 3 load map 每 round 构建**：不做增量更新，保证 round 内一致性
5. **Phase 3 惩罚值 200**：介于 BEND_PENALTY(16) 和 EDGE_OVERLAP_PENALTY(1200) 之间
6. **Phase 3 阈值 3**：load > 3 才惩罚，避免对低密度通道过度干预
7. **Phase 3 load map 包含被 reroute 的边**：有意为之，防止新路径走回原拥堵通道
8. **两阶段均使用 BTreeMap/显式排序**：保证确定性（AGENTS.md §2）
