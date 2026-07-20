# Phase A 实施细节：自环、贴边、弯折的具体修复

> 日期：2026-07-20
> 状态：✅ 已实施完成，实际实施记录见 [06-PhaseA实施笔记-2026-07.md](./06-PhaseA实施笔记-2026-07.md)
> 前置：[02-正交路由与布局优化方案.md](./02-正交路由与布局优化方案.md) Phase A
> 定位：代码级实施指南（原始计划，实际实施可能略有差异）

---

## A1. 自环边路由重写

### 当前代码位置

- `crates/plotgram-core/src/layout/edge/common/self_loop.rs`（329 行）
- 调用点：`edge_routing_orthogonal/phases/build.rs` 中 `phase_route_edges`

### 当前问题详解

```rust
// 当前：固定公式，不感知环境
fn route_orthogonal_self_loop(rel, node, loop_index) -> EdgeLayout {
    let corner = corner_for_index(loop_index);  // 固定轮转：右上→左上→右下→左下
    let loop_r = (node.width.min(node.height) * 0.28).max(16.0) + loop_index * 6.0;
    // ... 固定 5 点矩形路径
}
```

**问题 1**：`corner_for_index` 固定轮转，不考虑该方向是否有空间
- 如果右上角有相邻节点，自环会伸入邻居内部
- 如果节点在组的右上角，自环会伸出组边框

**问题 2**：`loop_r` 纯公式化
- 大节点（200×80）的自环 = 22.4px，视觉上太小
- 小节点（60×30）的自环 = 16px，几乎看不清

**问题 3**：不纳入 SegmentGrid
- 后续边路由不感知自环存在，可能穿越自环区域

### 修复方案

#### Step 1：新增 SelfLoopContext

```rust
// self_loop.rs 新增

/// 自环路由上下文：包含周围环境信息
pub struct SelfLoopContext<'a> {
    /// 所有节点（用于碰撞检测）
    pub nodes: &'a HashMap<String, NodeLayout>,
    /// 已排序节点 ID（确定性）
    pub sorted_node_ids: &'a [String],
    /// 组边框（用于边界检测）
    pub groups: &'a HashMap<String, GroupLayout>,
    /// 节点所在组 ID
    pub node_group: Option<&'a str>,
    /// 已路由边的段网格（避免重叠）
    pub segment_grid: &'a SegmentGrid,
    /// 当前节点的其他自环已占方向
    pub occupied_corners: Vec<CornerDirection>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum CornerDirection { TopRight, TopLeft, BottomRight, BottomLeft, Top, Right, Bottom, Left }
```

#### Step 2：空间感知方向选择

```rust
/// 计算四个方向的可用净空（到最近障碍物的距离）
fn compute_direction_clearance(
    node: &NodeLayout,
    ctx: &SelfLoopContext,
) -> [(CornerDirection, f64); 8] {
    let mut clearances = [
        (CornerDirection::TopRight, f64::MAX),
        (CornerDirection::TopLeft, f64::MAX),
        (CornerDirection::BottomRight, f64::MAX),
        (CornerDirection::BottomLeft, f64::MAX),
        (CornerDirection::Top, f64::MAX),
        (CornerDirection::Right, f64::MAX),
        (CornerDirection::Bottom, f64::MAX),
        (CornerDirection::Left, f64::MAX),
    ];
    
    // 对每个方向，射线检测最近障碍物
    // 障碍物包括：其他节点 bbox、组边框、已路由边段
    for (dir, clearance) in clearances.iter_mut() {
        *clearance = ray_cast_clearance(node, *dir, ctx);
    }
    
    // 按净空降序排列
    clearances.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    clearances
}

/// 选择最优方向：净空最大且未被占用
fn select_best_corner(
    clearances: &[(CornerDirection, f64)],
    occupied: &[CornerDirection],
    min_clearance: f64,
) -> (CornerDirection, f64) {
    for &(dir, clearance) in clearances {
        if !occupied.contains(&dir) && clearance >= min_clearance {
            return (dir, clearance);
        }
    }
    // fallback：选净空最大的，即使被占用
    clearances[0]
}
```

#### Step 3：自适应尺寸

```rust
/// 根据可用净空计算自环尺寸
fn adaptive_loop_size(
    node: &NodeLayout,
    clearance: f64,
    loop_index: usize,
) -> f64 {
    let base = node.width.min(node.height);
    let min_r = 14.0;
    let max_r = base * 0.45;
    // 净空的 60% 作为环尺寸，但不超过节点尺寸的 45%
    let from_clearance = clearance * 0.6;
    let from_node = base * 0.30 + loop_index as f64 * 8.0;
    from_clearance.min(from_node).clamp(min_r, max_r)
}
```

#### Step 4：纳入 SegmentGrid

```rust
// 在 phase_route_edges 中，自环路由后将路径插入 grid
if rel.from == rel.to {
    let edge = route_self_loop_aware(rel, node, loop_index, &ctx);
    // 插入 SegmentGrid，后续边可感知
    let points = edge.path_points();
    grid.insert_segments(edge_index, &points);
    edges[edge_index] = edge;
    continue;
}
```

---

## A2. 组边框留白保障

### 当前代码位置

- 走廊生成：`layout/group/corridor.rs` → `build_corridors_from_groups`
- 组近距惩罚：`edge_routing_orthogonal/scoring.rs` → `obstacle_penalty`
- 边框排斥：`post_route/border_repulse.rs` → `repulse_edges_only`
- 组边框壳层：`layout/group/border_shell.rs`

### 当前问题详解

```rust
// corridor.rs: 走廊坐标 = 组间隙中点
// 当两组间隙仅 30px 时，走廊坐标距两边框各 15px
// 减去 GROUP_BORDER_SHELL_PAD(12px)，视觉留白仅 3px
let mid = (a_max + b_min) / 2.0;  // 无最小宽度保障
```

```rust
// scoring.rs: 组近距惩罚
const GROUP_NEAR_MISS_PENALTY: f64 = 2_000.0;  // 远低于 NODE_CROSSING(10000)
const GROUP_NEAR_MISS_EXTRA: f64 = 8.0;        // 检测范围仅 8px
// 效果：scorer 对"距组边框 10px"的路径惩罚很轻
```

### 修复方案

#### Step 1：走廊最小宽度

```rust
// corridor.rs: build_corridors_from_groups 中

const MIN_CORRIDOR_GAP: f64 = 44.0;  // 组间隙 < 44px 不生成走廊

// 修改 try_build_corridor：
fn try_build_corridor(
    ga: &GroupLayout, gb: &GroupLayout,
    axis: CorridorAxis,
) -> Option<GroupCorridor> {
    let gap = compute_gap(ga, gb, axis);
    if gap < MIN_CORRIDOR_GAP {
        return None;  // 间隙太窄，不生成走廊
        // 边路由将走外环（绕过组对）
    }
    let mid = compute_midpoint(ga, gb, axis);
    Some(GroupCorridor { coord: mid, .. })
}
```

#### Step 2：增强组近距惩罚

```rust
// scoring.rs

// 提高惩罚和检测范围
const GROUP_NEAR_MISS_PENALTY: f64 = 4_500.0;  // 2000 → 4500
const GROUP_NEAR_MISS_EXTRA: f64 = 16.0;       // 8 → 16

// 新增：贴边平行段额外惩罚
fn border_hugging_penalty(
    path: &[Point],
    group_ctx: &GroupRoutingContext,
) -> f64 {
    let mut penalty = 0.0;
    for window in path.windows(2) {
        let (a, b) = (window[0], window[1]);
        for group in group_ctx.groups() {
            let dist = segment_to_rect_border_distance(a, b, &group.rect());
            let parallel_len = segment_length_along_border(a, b, &group.rect());
            if dist < 20.0 && parallel_len > 40.0 {
                // 贴边且平行长度 > 40px：按长度比例惩罚
                penalty += parallel_len * 15.0;
            }
        }
    }
    penalty
}
```

#### Step 3：路由时组边框膨胀为硬障碍

```rust
// path.rs: 候选路径生成时

// 在生成候选折点时，排除落在组边框膨胀区域内的坐标
fn is_valid_bend_point(
    p: Point,
    group_ctx: &GroupRoutingContext,
    from_id: &str,
    to_id: &str,
) -> bool {
    for group in group_ctx.groups() {
        // 跳过源/目标所在组
        if group.contains_node(from_id) || group.contains_node(to_id) {
            continue;
        }
        let inflated = group.rect().inflate(GROUP_OBSTACLE_PAD + 8.0);
        if inflated.contains(p) {
            return false;  // 折点落在组边框膨胀区内，无效
        }
    }
    true
}
```

---

## A3. 弯折消除

### 当前代码位置

- 弯折惩罚：`edge_routing_orthogonal/mod.rs` → `BEND_PENALTY = 16.0`
- 直连对齐：`edge_routing_orthogonal/straighten.rs`
- 微折折叠：`edge_routing_orthogonal/sanitize.rs` → `MICRO_JOG_LEN = 24.0`
- 候选生成：`edge_routing_orthogonal/path.rs` → `select_best_path_with_scorer_stats`

### 修复方案

#### Step 1：提高弯折惩罚 + 首段加重

```rust
// mod.rs
const BEND_PENALTY: f64 = 28.0;  // 16 → 28

// scoring.rs: DefaultScorer::score 中
let bend_count = path.len().saturating_sub(2) as f64;
let mut bend_score = bend_count * BEND_PENALTY * w.bend;

// 首段弯折加重：出发后第一个折点距端点 < PORT_CLEARANCE*2 时 ×2
if path.len() >= 3 {
    let first_seg_len = distance(path[0], path[1]);
    if first_seg_len < PORT_CLEARANCE * 2.0 {
        bend_score += BEND_PENALTY * 1.5;  // 额外惩罚
    }
}
score += bend_score;
```

#### Step 2：扩展 straighten 到 L 形端口

```rust
// straighten.rs

/// 扩展：L 形端口对（Bottom→Left, Right→Top 等）也可对齐
fn is_near_straightenable(
    from: Port, to: Port,
    from_nl: &NodeLayout, to_nl: &NodeLayout,
) -> Option<f64> {
    // 正对端口：直接对齐（现有逻辑）
    if is_opposite_port_pair(from, to) {
        return Some(compute_alignment_coord(from, to, from_nl, to_nl));
    }
    
    // L 形端口：检查是否可以退化为单折
    // 条件：from 的出射方向与 to 的入射方向垂直，
    //       且两节点在两个轴上都有投影重叠
    if is_l_shaped_pair(from, to) {
        let elbow = compute_l_elbow(from, to, from_nl, to_nl);
        if elbow_is_valid(elbow, from_nl, to_nl) {
            return Some(elbow);
        }
    }
    None
}
```

#### Step 3：微折阈值 + 单调性检查

```rust
// sanitize.rs

const MICRO_JOG_LEN: f64 = 30.0;  // 24 → 30

// collapse_micro_jogs 增加单调性检查：
fn collapse_micro_jog(points: &mut Vec<Point>, i: usize) -> bool {
    let jog_len = /* 计算折段长度 */;
    if jog_len > MICRO_JOG_LEN { return false; }
    
    // 单调性检查：折叠后不得反向
    let before_dir = direction(points[i-1], points[i]);
    let after_dir = direction(points[i+1], points[i+2]);
    if is_reverse(before_dir, after_dir) {
        return false;  // 折叠会造成反向，保留
    }
    
    // 执行折叠
    points.remove(i);
    true
}
```

#### Step 4：候选路径扩展（准直连）

```rust
// path.rs: 在候选生成阶段

/// 对切线投影重叠 > 50% 的非正对端口，生成 L 形候选
fn generate_l_shaped_candidates(
    from: Point, to: Point,
    from_port: Port, to_port: Port,
) -> Vec<Vec<Point>> {
    let mut candidates = Vec::new();
    
    // L 形：从 from 沿出射方向走，在 to 的入射方向延长线上转弯
    let from_dir = port_outward(from_port);
    let to_dir = port_outward(to_port);
    
    // 计算肘点：from 出射线与 to 入射线的交点
    if let Some(elbow) = ray_intersection(from, from_dir, to, to_dir) {
        candidates.push(vec![from, elbow, to]);
    }
    
    // 变体：肘点偏移（避免与其他边重叠）
    for offset in [-12.0, 12.0, -24.0, 24.0] {
        if let Some(elbow) = ray_intersection_offset(from, from_dir, to, to_dir, offset) {
            candidates.push(vec![from, elbow, to]);
        }
    }
    
    candidates
}
```

---

## A4. 回环边通道预留

### 当前代码位置

- `edge_routing_orthogonal/feedback_side.rs`（718 行）
- 调用点：`run.rs` 中 `assign_feedback_sides`

### 修复方案

```rust
// feedback_side.rs 新增

/// 回环边专用通道坐标（在图 bbox 外侧）
pub struct FeedbackChannelReservation {
    /// 通道方向（TB 布局 → Left/Right；LR 布局 → Top/Bottom）
    pub side: Port,
    /// 通道基准坐标（图 bbox 边缘 + margin）
    pub base_coord: f64,
    /// 车道间距
    pub lane_pitch: f64,
}

/// 在路由前为回环边预留外通道
pub fn reserve_feedback_channels(
    nodes: &HashMap<String, NodeLayout>,
    feedback_edges: &[usize],
    horizontal: bool,
) -> Vec<FeedbackChannelReservation> {
    if feedback_edges.is_empty() { return vec![]; }
    
    // 计算图 bbox
    let bbox = compute_graph_bbox(nodes);
    let margin = CHANNEL_MARGIN + 10.0;  // 28px
    
    let mut reservations = Vec::new();
    
    // 左侧通道（TB 布局）或上方通道（LR 布局）
    let left_count = feedback_edges.len() / 2 + feedback_edges.len() % 2;
    for i in 0..left_count {
        reservations.push(FeedbackChannelReservation {
            side: if horizontal { Port::Top } else { Port::Left },
            base_coord: if horizontal { bbox.y - margin - i as f64 * 20.0 }
                        else { bbox.x - margin - i as f64 * 20.0 },
            lane_pitch: 20.0,
        });
    }
    
    // 右侧/下方通道
    let right_count = feedback_edges.len() / 2;
    for i in 0..right_count {
        reservations.push(FeedbackChannelReservation {
            side: if horizontal { Port::Bottom } else { Port::Right },
            base_coord: if horizontal { bbox.y_max + margin + i as f64 * 20.0 }
                        else { bbox.x_max + margin + i as f64 * 20.0 },
            lane_pitch: 20.0,
        });
    }
    
    reservations
}
```

---

## 验证 Checklist

每项修复完成后，按以下顺序验证：

1. **编译通过**：`cargo build -p plotgram-core`
2. **单测通过**：`cargo test -p plotgram-core`
3. **目标图验证**：`cargo run -p plotgram-cli -- render showcase/flowchart/product.password-reset.pgm -o /tmp/test.svg`
4. **基线对比**：
   ```bash
   ./benchmarks/snapshot.sh --tag phase-a-{fix-name}
   ./benchmarks/compare.sh benchmarks/baselines/latest.json benchmarks/baselines/*phase-a*.json
   ```
5. **视觉检查**：打开 SVG 确认目标问题已修复、无新引入问题
6. **确定性**：同一输入渲染两次，输出完全一致
