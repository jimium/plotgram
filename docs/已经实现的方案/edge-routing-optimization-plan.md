# 边路由与标签布局优化方案

> 版本：0.1.0-draft | 状态：方案设计
>
> 相关文档：[Graphviz 算法研究](./graphviz-algorithms-research.md) | [Cytoscape.js 研究](./cytoscape-js-research.md) | [边路由预重构方案](./edge-routing-prerefactor-plan.md)

---

## 0. 背景与目标

在完成 [边路由预重构（P0–P2）](./edge-routing-prerefactor-plan.md) 后，路由栈已具备清晰的 `PathGeometry` 模型与共享骨架。本方案聚焦下一阶段：**提升路由路径的视觉呈现效果与标签布局质量**。

用户提出的四个优化方向：

1. 智能避障，减少路径穿越
2. 入口箭头合并 / 出口点合并
3. 标签重叠与碰撞
4. 标签整体美观度

经现状调研，这四个方向对应三类**已确认的系统性缺口**（详见 §2）。本方案给出按优先级排序的优化路线，每项均包含问题诊断、方案设计、实现思路与验收标准。

**设计原则**：

- **结合现状**：所有方案基于现有 `PathGeometry` / `EdgeRoutingStrategy` / `LabelPlacer` 架构，不推翻重写
- **渐进增强**：先补齐硬缺口（标签压边、bezier 不避障），再做视觉增强（入口合并）
- **参考而非照搬**：Graphviz 的 concentrate / Spline-o-Matic、Cytoscape 的 fcose 约束模型仅作灵感，实现贴合 Drawify 的 Rust 单体架构

---

## 1. 现状速览

### 1.1 路由器避障能力矩阵

| 路由器 | 障碍避让 | 机制 | 硬/软约束 | refine 兜底 |
|--------|---------|------|----------|------------|
| orthogonal | 有 | 评分惩罚 + 通道绕行候选 | 软（评分） | 是 |
| spline | 有 | 可见性图 + Dijkstra | 硬（路径规划） | 是（兜底采样误差） |
| bezier | **无** | 纯几何控制点 | — | **否** |
| circular | **无** | 圆弧几何 | — | **否** |
| straight | 无 | 两点直线 | — | 否 |

关键代码位置：
- orthogonal 评分：[scoring.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs) `obstacle_penalty` (L44-71)
- orthogonal 通道绕行：[path.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs) `build_channel_detours` (L98-223)
- spline 可见性图：[visibility.rs](../../crates/drawify-core/src/layout/edge/visibility.rs) `ObstacleIndex` (L113-150)
- refine 循环：[refine.rs](../../crates/drawify-core/src/layout/refine.rs) `run_refine` (L166-186)

### 1.2 标签处理现状

| 能力 | 现状 | 位置 |
|------|------|------|
| 标签-标签碰撞 | 检测 + 轴对齐推开 | [label_avoidance.rs](../../crates/drawify-core/src/layout/edge/common/label_avoidance.rs) L37-68 |
| 标签-节点碰撞 | 检测 + 轴对齐推开 | 同上 L71-81 |
| 标签-分组碰撞 | 检测 + 轴对齐推开 | 同上 L84-94 |
| **标签-边路径碰撞** | **不检测** | — |
| 标签尺寸估算 | 三套不一致实现 | label_avoidance.rs / edge_routing_circular.rs:428 / sequence.rs:328 |
| 迭代上限 | 固定 5 次，无震荡检测 | `DEFAULT_MAX_LABEL_ITERATIONS` |

### 1.3 入口/出口点现状

- **无任何入口/出口合并逻辑**（全代码库搜索 `merge|concentrate|dock` 在 edge 模块零命中）
- orthogonal slot 分配是**分散策略**：同侧多边按目标坐标排序后均匀分布在 40px 间距 slot 上（[slot.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs) `slot_fraction` L71-80）
- 平行边通过 `parallel_edges.rs` 的法向偏移分开，端点天然不同

---

## 2. 核心问题诊断

### 2.1 问题 P-L1：标签压在边路径上（高优先级）

**现象**：标签 bbox 与边路径线段相交，视觉上标签压在自己或别的边的线上。

**根因**：

1. 初始放置时法向偏移只有 `DEFAULT_LABEL_PERP_OFFSET = 8.0`（[constants.rs](../../crates/drawify-core/src/layout/constants.rs)），而标签高度 `DEFAULT_LABEL_FONT_SIZE + 2*DEFAULT_LABEL_PADDING = 19.0`，**偏移量不足以让 bbox 脱离边路径**
2. `resolve_label_overlaps`（[label_avoidance.rs:11-100](../../crates/drawify-core/src/layout/edge/common/label_avoidance.rs)）只检测三类碰撞，**完全不检测标签与边路径的碰撞**
3. 避让推开后不重新对齐到路径，标签可能漂移到其他边上

**影响范围**：所有带标签的边，尤其是 orthogonal 长折线边和 bezier 弧形边。

### 2.2 问题 P-R1：bezier / circular 不避障（高优先级）

**现象**：bezier 边的曲线控制点不检测是否穿过节点，circular 弧形也不检测。当中间有节点时，边会直接穿过节点。

**根因**：

1. `BezierRouting` 没有覆写 `supports_refine`（默认 `false`，[mod.rs:486-488](../../crates/drawify-core/src/layout/edge/mod.rs)），不参与 refine 循环
2. `compute_bezier_controls`（[edge_geometry.rs:78-149](../../crates/drawify-core/src/layout/edge/common/edge_geometry.rs)）只考虑端口方向和起止点距离，不查询障碍物
3. refine.rs 的 `analyze_edge_node_crossings`（[refine.rs:74](../../crates/drawify-core/src/layout/refine.rs)）只检测 `PathGeometry::Polyline`，bezier/circular 路径不检测

**影响范围**：架构图（默认 force-directed + bezier 路由）、状态图（circular 路由）中节点密集的场景。

### 2.3 问题 P-E1：入口/出口点分散（中优先级）

**现象**：多条边汇入同一节点同一侧时，箭头分散在 40px 间距的多个 slot 上，视觉上不集中，与用户期望的"汇流"效果不符。

**根因**：

1. orthogonal 的 `slot_fraction`（[slot.rs:71-80](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs)）设计目标是"均匀分布、不重叠"，而非"集中"
2. straight/bezier/spline 通过 `parallel_edges.rs` 的法向偏移分开，端点天然不同
3. 没有任何"汇流"或"concentrate"逻辑

**权衡**：完全合并会导致边重叠不可区分；完全分散则视觉混乱。需要折中方案。

### 2.4 问题 P-L2：标签避让算法鲁棒性不足（中优先级）

**现象**：密集图场景下标签仍然重叠，或被推到更差位置。

**根因**：

1. 固定 5 次迭代（`DEFAULT_MAX_LABEL_ITERATIONS`），对密集图不足
2. 无震荡检测：标签可能在两个障碍间反复弹跳
3. `push_label_from_obstacle`（[label_avoidance.rs:103-131](../../crates/drawify-core/src/layout/edge/common/label_avoidance.rs)）推开后**不回检**已处理障碍，可能引入新重叠
4. 轴对齐推开对斜线/弧形边不友好
5. 三套标签尺寸估算不一致（label_avoidance.rs / edge_routing_circular.rs:428 / sequence.rs:328）

### 2.5 问题 P-R2：refine 循环局限（低优先级）

**现象**：复杂图一次推开可能引入新的穿障，且开销大。

**根因**：

1. 默认 `max_passes: 1`（[refine.rs:30-39](../../crates/drawify-core/src/layout/refine.rs)）
2. 每轮 refine 后全量重路由（`router.route`），开销大
3. 无回退机制：推开后穿障反而增加时不会回退
4. 推开可能引入节点重叠（依赖后续 `OverlapResolver`，但不在路由阶段）

---

## 3. 优化方案设计

### 3.1 优先级排序

| 优先级 | 问题 | 方案 | 预估工作量 | 收益 |
|--------|------|------|-----------|------|
| **P0** | P-L1 标签压边 | 标签-边碰撞检测与避让 | 1-2 周 | 所有带标签图视觉质量提升 |
| **P0** | P-R1 bezier/circular 不避障 | bezier 穿障检测 + 退化到 spline | 1 周 | 架构图/状态图质量提升 |
| **P1** | P-E1 入口分散 | 汇流模式（concentrate） | 2-3 周 | 视觉一致性 |
| **P1** | P-L2 标签避让鲁棒性 | 震荡检测 + 回检 + 尺寸统一 | 1 周 | 密集图标签质量 |
| **P2** | P-R2 refine 局限 | 增量 refine + 回退机制 | 2 周 | 复杂图鲁棒性 |

### 3.2 P0-A：标签-边碰撞检测与避让

#### 3.2.1 问题

当前标签 bbox 与边路径线段相交时无任何处理（[label_avoidance.rs:1-4](../../crates/drawify-core/src/layout/edge/common/label_avoidance.rs) 注释明确未包含标签-边）。

#### 3.2.2 方案

在 `resolve_label_overlaps` 中增加**标签-边路径碰撞检测**，作为第四类碰撞。

**检测算法**：标签 AABB vs 边路径线段相交

```
对每条带标签的边 Ei：
  对每条其他边 Ej（j != i）：
    对 Ej 路径的每条线段 S：
      若 label_bbox(Ei) 与 S 相交：
        沿 S 的法线方向推开 Ei.label_pos
        推开距离 = 标签半高 + MIN_SEPARATION
```

**推开方向策略**：

- 优先沿边线段的法线方向推开（保持标签贴近边但不压线）
- 若法线方向推开会撞到节点/其他障碍，退化为轴对齐推开
- 推开后**回检**：确认新位置不压其他边/节点

**关键实现**：新增 `segment_vs_aabb_intersect` 工具函数（Liang-Barsky 算法，比 Cohen-Sutherland 更高效）。

```rust
/// Liang-Barsky 线段裁剪：判断线段 (p1,p2) 是否与 AABB 相交
fn segment_vs_aabb_intersect(
    p1: (f64, f64),
    p2: (f64, f64),
    bbox: (f64, f64, f64, f64),  // (left, top, right, bottom)
) -> bool {
    let (x0, y0, x1, y1) = bbox;
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    let mut t_min = 0.0;
    let mut t_max = 1.0;

    // x 方向
    for (p, q) in [(-dx, p1.0 - x0), (dx, x1 - p1.0)] {
        if p.abs() < 1e-9 {
            if q < 0.0 { return false; }
        } else {
            let t = q / p;
            if p < 0.0 { t_min = t_min.max(t); } else { t_max = t_max.min(t); }
            if t_min > t_max { return false; }
        }
    }
    // y 方向
    for (p, q) in [(-dy, p1.1 - y0), (dy, y1 - p1.1)] {
        if p.abs() < 1e-9 {
            if q < 0.0 { return false; }
        } else {
            let t = q / p;
            if p < 0.0 { t_min = t_min.max(t); } else { t_max = t_max.min(t); }
            if t_min > t_max { return false; }
        }
    }
    true
}
```

**性能考量**：

- 朴素实现 O(E² × P)（E 边数，P 平均路径点数）
- 优化：只检测"邻近边"——标签 bbox 外扩 30px 后与边 bbox 求交，排除大部分远边
- 对 100 边的图，约 10000 次检测，可接受

#### 3.2.3 与现有架构的集成

```rust
// label_avoidance.rs 新增
fn resolve_label_edge_overlaps(
    edges: &mut [EdgeLayout],
    relations: &[Relation],
) {
    let label_indices: Vec<usize> = /* 同现有逻辑 */;

    for _ in 0..DEFAULT_MAX_LABEL_ITERATIONS {
        let mut moved = false;
        for &i in &label_indices {
            let text = relations[i].label.as_ref().unwrap();
            let mut bbox = label_bbox(&edges[i], text);

            for (j, edge_j) in edges.iter().enumerate() {
                if j == i || edge_j.path_len() < 2 { continue; }
                let path_j = edge_j.path_points();
                for window in path_j.windows(2) {
                    if segment_vs_aabb_intersect(window[0], window[1], bbox) {
                        // 沿线段法线推开
                        push_label_along_normal(&mut edges[i].label_pos, &mut bbox, window[0], window[1]);
                        moved = true;
                        break;  // 一条边只推一次
                    }
                }
            }
        }
        if !moved { break; }
    }
}
```

在 `resolve_label_overlaps` 主循环中调用：

```rust
pub fn resolve_label_overlaps(edges, relations, nodes, groups) {
    // ... 现有标签-标签、标签-节点、标签-分组 ...
    // 新增第四类
    resolve_label_edge_overlaps(edges, relations);
}
```

#### 3.2.4 验收标准

- 新增测试：标签压在另一条边上时，避让后标签 bbox 不与边路径相交
- 新增测试：标签压在自己边上时，沿法线推开
- 现有测试全部通过
- 视觉对比：带标签的密集图，标签不再压线

---

### 3.3 P0-B：bezier / circular 避障增强

#### 3.3.1 问题

bezier 路由（[edge_routing_bezier.rs](../../crates/drawify-core/src/layout/edge/edge_routing_bezier.rs)）的 `compute_bezier_controls` 不查询障碍物，曲线可能穿过中间节点。circular 同理。

#### 3.3.2 方案：穿障检测 + 退化到 spline

**核心思路**：不重写 bezier 的控制点计算，而是在路由完成后**检测曲线是否穿障**，若穿障则**退化到 spline 路由**（已有完整可见性图避障）。

**为什么这样设计**：

1. bezier 的优势是平滑美观，但无避障能力是本质限制
2. spline 已有完整可见性图避障，且输出可拟合为平滑曲线
3. 退化策略保留了无障碍场景的 bezier 美观性，只在必要时切换

**实现流程**：

```
route_edges_bezier:
  1. 正常计算 bezier 控制点
  2. 采样曲线为 16 个点
  3. 对每条采样线段检测是否穿障（复用 visibility.rs 的 segment_intersects_obstacle）
  4. 若穿障：
     a. 构建障碍索引（复用 spline 的 ObstacleIndex）
     b. 调用 spline 的 shortest_path 求绕行折线
     c. 将折线拟合为 PathGeometry::Polyline（或贝塞尔链）
  5. 若不穿障：保持原 bezier
```

**关键修改**：

```rust
// edge_routing_bezier.rs
impl EdgeRoutingStrategy for BezierRouting {
    fn route(&self, ctx: &RoutingContext, diagram: &Diagram, result: LayoutResult) -> LayoutResult {
        let mut edges = /* 现有 bezier 计算 */;

        // 新增：穿障检测与退化
        let obstacle_index = ObstacleIndex::build(&result.nodes, OBSTACLE_PADDING);
        for edge in &mut edges {
            if edge.is_bezier() && curve_intersects_obstacles(edge, &obstacle_index) {
                // 退化到 spline 绕行
                if let Some(detour) = compute_spline_detour(edge, &obstacle_index) {
                    edge.geometry = PathGeometry::Polyline { points: detour };
                }
            }
        }

        finalize_edges(result, edges, diagram)
    }

    fn supports_refine(&self) -> bool { true }  // 改为 true，兜底退化后的 Polyline
}
```

**curve_intersects_obstacles 实现**：

```rust
fn curve_intersects_obstacles(edge: &EdgeLayout, obstacles: &ObstacleIndex) -> bool {
    let sampled = edge.sampled_path(16);  // 16 点采样
    for window in sampled.windows(2) {
        if obstacles.segment_hits_any(window[0], window[1]) {
            return true;
        }
    }
    false
}
```

**circular 路由同理**：在 `route_edges_circular` 末尾增加穿障检测，穿障的弧形边退化到 spline。

#### 3.3.3 验收标准

- 新增测试：bezier 边穿过中间节点时，自动退化为绕行折线
- 新增测试：无障碍时保持原 bezier 形状
- `supports_refine` 改为 true 后，refine 循环正常工作
- 现有测试全部通过

---

### 3.4 P1-A：入口/出口点合并（汇流模式）

#### 3.4.1 问题

同侧多边分散在 40px 间距 slot 上，箭头不集中。用户期望"汇流"效果。

#### 3.4.2 方案：分级汇流（Tiered Concentration）

**灵感来源**：Graphviz 的 `concentrate=true` 选项合并平行边；Cytoscape 的 compound node 边聚合。但 Drawify 采用**分级策略**而非完全合并。

**核心思路**：根据同侧边数动态选择分布策略：

| 同侧边数 | 策略 | 视觉效果 |
|---------|------|---------|
| 1 | 单点 | 自然 |
| 2-3 | **紧凑分布**（间距压缩到 16px） | 接近汇流，仍可区分 |
| 4+ | **汇流 + 分支**（共享主干，末端分散） | 视觉集中，末端可区分 |

**汇流 + 分支模式**（4+ 边时）：

```
传统分散：              汇流 + 分支：
  ┌──┐                  ┌──┐
  │  │←─── 边1           │  │
  │  │←─── 边2           │  │←─┐
  │  │←─── 边3    →      │  │  ├─ 边1
  │  │←─── 边4           │  │  ├─ 边2
  │  │←─── 边5           │  │  ├─ 边3
  └──┘                  └──┘  └─ 边4
```

**实现思路**：

1. **检测汇流组**：在 slot 分配前，按 `(node_id, side, direction)` 分组，direction = incoming/outgoing
2. **选择策略**：根据组内边数选择分布模式
3. **紧凑分布**（2-3 边）：修改 `slot_fraction` 的 `pitch` 参数从 40px 降到 16px
4. **汇流 + 分支**（4+ 边）：
   - 所有边共享一个入口点（slot 中心）
   - 在距离节点边界 20px 处分叉，每条边偏移 8px
   - 路径形态：`入口点 → 分叉点 → 各自目标`

**关键修改**：

```rust
// slot.rs 新增
pub enum DockingStrategy {
    Single,           // 1 边，单点
    Compact,          // 2-3 边，紧凑分布（pitch=16）
    Concentrate,      // 4+ 边，汇流 + 分支
}

pub fn choose_docking_strategy(count: usize) -> DockingStrategy {
    match count {
        0..=1 => DockingStrategy::Single,
        2..=3 => DockingStrategy::Compact,
        _ => DockingStrategy::Concentrate,
    }
}
```

```rust
// edge_routing_orthogonal/mod.rs 的 slot 分配逻辑
for (key, group) in grouped_endpoints {
    let strategy = choose_docking_strategy(group.len());
    match strategy {
        DockingStrategy::Single | DockingStrategy::Compact => {
            // 现有逻辑，Compact 时 pitch=16
            let pitch = match strategy {
                DockingStrategy::Compact => COMPACT_SLOT_PITCH,  // 16
                _ => SLOT_PITCH,  // 40
            };
            for (rank, ep) in group.iter().enumerate() {
                let frac = slot_fraction(rank, group.len(), edge_len, pitch);
                // ...
            }
        }
        DockingStrategy::Concentrate => {
            // 所有边共享中心 slot
            for ep in &group {
                let frac = 0.5;  // 中心点
                // 路径生成分叉段
            }
            // 生成汇流段：所有边先到 (center_x, node_edge - 20) 再分叉
        }
    }
}
```

**适用范围**：仅 orthogonal 路由（slot 系统所在）。straight/bezier/spline 的平行边偏移保持现状（法向分开），因为它们的场景多为弧形边，汇流效果不明显。

#### 3.4.3 风险与权衡

- **风险**：汇流段可能与其他边交叉，需要评分系统感知汇流段
- **权衡**：汇流模式牺牲了边的独立可追溯性（多条边共享一段路径），但视觉更整洁
- **配置化**：提供 `concentrate_threshold` 配置项，默认 4，用户可调整或关闭

#### 3.4.4 验收标准

- 新增测试：同侧 5 条边时，生成汇流 + 分支路径
- 新增测试：同侧 2 条边时，紧凑分布（间距 16px）
- 新增测试：同侧 1 条边时，单点（不影响）
- 视觉对比：架构图入口处箭头集中

---

### 3.5 P1-B：标签避让鲁棒性增强

#### 3.5.1 问题

[label_avoidance.rs](../../crates/drawify-core/src/layout/edge/common/label_avoidance.rs) 的避让算法存在多个鲁棒性问题（§2.4）。

#### 3.5.2 方案

**3.5.2.1 震荡检测**：

记录每轮每个标签的移动方向，若连续两轮方向相反且位移相近，标记为震荡，对该标签停止迭代。

```rust
struct LabelState {
    last_move: (f64, f64),  // 上一轮位移
    oscillating: bool,
}

// 在迭代循环中
let delta = (label_pos.0 - prev_pos.0, label_pos.1 - prev_pos.1);
if state.last_move.0 * delta.0 < 0.0 && state.last_move.1 * delta.1 < 0.0 {
    // 方向反转，疑似震荡
    state.oscillating = true;
}
if state.oscillating { continue; }  // 跳过震荡标签
```

**3.5.2.2 推开后回检**：

`push_label_from_obstacle` 推开后，立即回检已处理的所有障碍，若引入新重叠则尝试另一个轴方向。

```rust
fn push_label_from_obstacle_with_recheck(
    label_pos: &mut (f64, f64),
    bbox: &mut (f64, f64, f64, f64),
    obstacle: (f64, f64, f64, f64),
    all_obstacles: &[(f64, f64, f64, f64)],
) -> bool {
    let original = *label_pos;
    if push_label_from_obstacle(label_pos, bbox, obstacle) {
        // 回检
        for other in all_obstacles {
            if aabb_overlap(bbox, other).is_some() {
                // 尝试另一轴
                *label_pos = original;
                if try_push_other_axis(label_pos, bbox, obstacle) {
                    // 再次回检
                    if all_obstacles.iter().all(|o| aabb_overlap(bbox, o).is_none()) {
                        return true;
                    }
                }
                *label_pos = original;
                return false;  // 放弃推开
            }
        }
        true
    } else {
        false
    }
}
```

**3.5.2.3 标签尺寸估算统一**：

将三套估算统一为一套，提取到 `label_metrics.rs`：

```rust
// label_metrics.rs（新建）
pub struct LabelMetrics {
    pub width: f64,
    pub height: f64,
}

impl LabelMetrics {
    pub fn estimate(text: &str) -> Self {
        let width: f64 = text.chars().map(|c| {
            if c.is_ascii() { DEFAULT_ASCII_CHAR_WIDTH } else { DEFAULT_CJK_CHAR_WIDTH }
        }).sum();
        Self {
            width: width + DEFAULT_LABEL_PADDING * 2.0,
            height: DEFAULT_LABEL_FONT_SIZE + DEFAULT_LABEL_PADDING * 2.0,
        }
    }
}
```

circular 和 sequence 的私有 `label_bbox` 改为调用统一实现。

#### 3.5.3 验收标准

- 新增测试：震荡场景下标签稳定收敛
- 新增测试：推开后引入新重叠时回退
- 三套标签尺寸估算统一为一套
- 现有测试全部通过

---

### 3.6 P2：refine 循环增强

#### 3.6.1 方案

**3.6.1.1 增量 refine**：

当前每轮 refine 后全量重路由（`router.route`），开销大。改为**只重路由受影响的边**：

```rust
fn run_refine_incremental(
    diagram: &Diagram,
    result: &mut LayoutResult,
    router: &dyn EdgeRoutingStrategy,
    config: &RefineConfig,
) {
    for _ in 0..config.max_passes {
        let metrics = analyze_edge_node_crossings(result, diagram, config);
        if metrics.edge_node_crossings == 0 { break; }

        // 记录受影响的边
        let affected_edges: HashSet<usize> = metrics.problem_nodes.values()
            .flat_map(|info| info.edge_indices.iter().copied())
            .collect();

        push_problem_nodes(result, metrics, config);

        // 只重路由受影响的边
        reroute_subset(result, diagram, router, &affected_edges);
    }
}
```

**3.6.1.2 回退机制**：

记录每轮的穿障数，若推开后穿障数增加，回退到上一轮状态。

```rust
let mut best_result = result.clone();
let mut best_crossings = usize::MAX;

for _ in 0..config.max_passes {
    let metrics = analyze_edge_node_crossings(result, diagram, config);
    if metrics.edge_node_crossings == 0 { break; }
    if metrics.edge_node_crossings < best_crossings {
        best_result = result.clone();
        best_crossings = metrics.edge_node_crossings;
    }
    push_problem_nodes(result, metrics, config);
    result = router.route(diagram, result);
}

// 若最终比最优差，回退
if analyze_edge_node_crossings(result, diagram, config).edge_node_crossings > best_crossings {
    *result = best_result;
}
```

**3.6.1.3 提高 max_passes 默认值**：

从 1 提高到 3，配合回退机制避免恶化。

#### 3.6.2 验收标准

- 增量 refine：只重路由受影响边，性能提升
- 回退机制：穿障数不增加
- max_passes=3 时复杂图穿障数 ≤ max_passes=1 时

---

## 4. 实施路线图

```
Phase 1（P0，2-3 周）
  ├─ P0-A：标签-边碰撞检测与避让
  │   ├─ 新增 segment_vs_aabb_intersect 工具函数
  │   ├─ resolve_label_edge_overlaps 实现
  │   ├─ 集成到 resolve_label_overlaps
  │   └─ 测试与视觉验证
  └─ P0-B：bezier/circular 避障增强
      ├─ curve_intersects_obstacles 实现
      ├─ 退化到 spline 的逻辑
      ├─ supports_refine 改为 true
      └─ 测试与视觉验证

Phase 2（P1，3-4 周）
  ├─ P1-A：入口/出口点合并（汇流模式）
  │   ├─ DockingStrategy 枚举
  │   ├─ 紧凑分布模式（pitch=16）
  │   ├─ 汇流 + 分支模式
  │   └─ 测试与视觉验证
  └─ P1-B：标签避让鲁棒性增强
      ├─ 震荡检测
      ├─ 推开后回检
      ├─ 标签尺寸估算统一
      └─ 测试

Phase 3（P2，2 周）
  └─ refine 循环增强
      ├─ 增量 refine
      ├─ 回退机制
      ├─ max_passes 调整
      └─ 测试
```

---

## 5. 效果对比预期

### 5.1 标签压边（P0-A）

**优化前**：

```
  ┌───┐
  │ A │──────[label]──────┐
  └───┘                    │
       ╲                   │
        ╲  ┌───┐           │
         ╲─│ B │           │
           └───┘           │
              ╲            │
               ╲  ┌───┐    │
                ╲─│ C │    │
                  └───┘    │
                           │
              标签压在 A→B 边上
```

**优化后**：

```
  ┌───┐
  │ A │──────┐
  └───┘      │
       ╲     │ [label]  ← 标签沿法线推开
        ╲    │
         ╲  ┌───┐
          ─│ B │
            └───┘
```

### 5.2 bezier 避障（P0-B）

**优化前**：bezier 曲线穿过中间节点

**优化后**：检测到穿障，退化为 spline 绕行折线

### 5.3 入口合并（P1-A）

**优化前**（5 条边分散）：

```
  ┌──┐
  │  │←─── 边1
  │  │←─── 边2
  │  │←─── 边3
  │  │←─── 边4
  │  │←─── 边5
  └──┘
```

**优化后**（汇流 + 分支）：

```
  ┌──┐
  │  │
  │  │←─┐
  │  │  ├─ 边1
  │  │  ├─ 边2
  │  │  ├─ 边3
  │  │  ├─ 边4
  │  │  └─ 边5
  └──┘
```

---

## 6. 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| 标签-边检测 O(E²) 性能 | 大图卡顿 | 邻近边预筛选（bbox 外扩 30px） |
| bezier 退化到 spline 改变视觉风格 | 用户感知不一致 | 配置项控制，默认开启 |
| 汇流模式牺牲边可追溯性 | 多条边共享路径难区分 | 末端分叉保留可区分性 |
| refine 回退机制增加内存 | result.clone() 开销 | 仅在穿障数增加时回退 |
| 标签尺寸估算统一改变现有行为 | circular/sequence 视觉变化 | 视觉回归测试 |

---

## 7. 与现有研究文档的关系

| 文档 | 借鉴点 | 本方案对应 |
|------|--------|-----------|
| [Graphviz 算法研究](./graphviz-algorithms-research.md) | Spline-o-Matic 可见性图 | P0-B 复用 spline 的可见性图 |
| [Graphviz 算法研究](./graphviz-algorithms-research.md) | concentrate 平行边合并 | P1-A 汇流模式灵感 |
| [Cytoscape.js 研究](./cytoscape-js-research.md) | fcose 约束模型 | P1-B 标签避让约束求解参考 |
| [边路由预重构方案](./edge-routing-prerefactor-plan.md) | PathGeometry 模型 | 本方案基于此模型构建 |

**关键区别**：

- Graphviz 的 concentrate 是布尔开关（全合并或全分散），本方案采用**分级策略**（按边数自适应）
- Graphviz 的 Spline-o-Matic 是独立边路由系统，本方案**复用现有 spline 路由器**作为 bezier 的退化路径
- Cytoscape 的 fcose 约束是全局约束求解，本方案的标签避让保持**迭代式局部优化**（更轻量）

---

## 8. 附录：关键代码位置索引

| 模块 | 文件 | 关键函数 |
|------|------|---------|
| 标签避让 | [label_avoidance.rs](../../crates/drawify-core/src/layout/edge/common/label_avoidance.rs) | `resolve_label_overlaps` L11 |
| 标签放置 | [label_placement.rs](../../crates/drawify-core/src/layout/edge/common/label_placement.rs) | `LabelPlacer` trait L36 |
| orthogonal 路由 | [edge_routing_orthogonal/mod.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs) | slot 分配 L249-284 |
| orthogonal 评分 | [scoring.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs) | `obstacle_penalty` L44 |
| orthogonal 通道绕行 | [path.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs) | `build_channel_detours` L98 |
| spline 可见性图 | [visibility.rs](../../crates/drawify-core/src/layout/edge/visibility.rs) | `ObstacleIndex` L113 |
| bezier 路由 | [edge_routing_bezier.rs](../../crates/drawify-core/src/layout/edge/edge_routing_bezier.rs) | `route_edges_bezier` L97 |
| circular 路由 | [edge_routing_circular.rs](../../crates/drawify-core/src/layout/edge/edge_routing_circular.rs) | `route_edges_circular` L41 |
| refine 循环 | [refine.rs](../../crates/drawify-core/src/layout/refine.rs) | `run_refine` L166 |
| 平行边 | [parallel_edges.rs](../../crates/drawify-core/src/layout/edge/common/parallel_edges.rs) | `group_parallel_edges` L21 |
| 路由骨架 | [routing_skeleton.rs](../../crates/drawify-core/src/layout/edge/common/routing_skeleton.rs) | `resolve_endpoints` L65 |
| 常量 | [constants.rs](../../crates/drawify-core/src/layout/constants.rs) | 标签/边常量 |

---

## 9. 下一步

本方案为设计文档，实际实施前需：

1. **视觉验证基准**：准备一组典型图（密集流程图、架构图、状态图），截图当前渲染结果作为对比基准
2. **P0-A 先行**：标签-边碰撞检测是最高 ROI 项，建议优先实施
3. **逐步推进**：每个 P 项完成后做视觉回归测试，确认无退化再进入下一项
4. **配置化**：所有优化项提供配置开关，默认行为可回退到当前逻辑
