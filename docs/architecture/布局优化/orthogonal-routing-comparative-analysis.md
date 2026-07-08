# 正交路由算法对比分析

> 基于 flowml 当前正交路由实现，对比 ELK、yWorks/yFiles、Graphviz、draw.io/mxGraph、Dagre 等同类产品的算法，分析利弊。

## 1. 当前实现的核心算法特征

当前实现是一个**启发式候选枚举 + 多阶段后处理**的架构，而非经典的 A* on grid 或 visibility graph：

- **端口选择**：几何比例规则（`|dy| >= |dx| * 0.4`）+ 分组关系判定树，确定性选择而非打分
- **路径搜索**：候选路径枚举（非 A*、非 visibility graph）+ 4 项加权打分（长度+弯折+障碍+边重叠）
- **后处理**：5 层串行修复（replan_slots → straighten → X-1 reroute → X-2 flip_stub → X-3 lane）

### 1.1 模块组成

| 模块 | 职责 |
|---|---|
| `slot.rs` | 端口选择（`choose_pair_sides_with_group`）+ 磁吸点/Slot 分配 + 汇流策略（`DockingStrategy`）|
| `path.rs` | 候选路径生成（`select_best_path_with_scorer_stats`）+ 折线构建 + 走廊坐标偏好 |
| `scoring.rs` | 候选路径评分（长度+弯折+障碍+边重叠），穿障检测（`path_is_clean`）|
| `context.rs` | 路由上下文 `RoutingContext` + 障碍物索引 `PreparedObstacles` + 段空间索引 `SegmentGrid` + `EndpointPair` |
| `corridor_route.rs` | 跨组走廊三段式路由（源→组边框→走廊→组边框→目标），BFS 最短链 |
| `feedback_side.rs` | 回环边（Greedy FAS 反转边）左右通道分配 |
| `layer_order.rs` | 边路由顺序确定（有 rank 时分层批量，否则按连接度贪心）|
| `lane_assignment.rs` | X-3 车道分配：平行冲突段 cross-axis 平移分离 |
| `channel_load.rs` | 通道负载图，reroute 时偏好低负载通道 |
| `simplify.rs` | 路径简化（去重+去共线点，保留 stub 段）|

### 1.2 执行流水线

```
前置准备
  ├── 构建 GroupRoutingContext + 预排序障碍物 PreparedObstacles
  ├── 回环边侧向分配 (feedback_side)
  └── 走廊路由规划 (corridor_route::plan_corridor_routes)

Step 1: 端口选择
  ├── 按无向节点对分组，选端口 (choose_pair_sides_with_group)
  ├── 端口全局协调 (coordinate_port_sides) — 同侧偏好
  └── 回环边端口覆盖 (apply_feedback_side_overrides)

Step 2: Slot 分配
  ├── 按 slot 分组键分组端点 (node_id|side|is_from|arrow|style)
  ├── 子组内按目标节点切线方向排序
  ├── 子组间按 (arrow, style, min_edge_index) 排序
  ├── 选择 DockingStrategy (Single/Compact/Concentrate)
  ├── 计算锚点坐标 (slot_anchor)
  └── 平行边切线偏移 (reverse_pairs)

Step 3: 边路由顺序
  └── compute_edge_order — 有 rank 分层批量，否则连接度排序

Step 4: 逐边构建路径
  ├── 4a: 优先尝试走廊路径 (validated_corridor_path)
  ├── 4a: 回退到通用候选搜索 (select_best_path_with_scorer_stats)
  └── 插入 SegmentGrid

Step 4b: replan_slots (全局 Slot 重规划)
  ├── 按实际出口方向(effective_dir)重新排序锚点
  ├── 锚点块（Concentrate 端点）不可拆分
  ├── 一次性全局排序 + 轻量重路由
  └── 替代了旧版 fix_slot_inversions

Step 4c: 直连偏好对齐 (straighten_preferred_alignments)
  ├── 正对端口对 (Bottom→Top, Left→Right)
  ├── Single 端点对齐到多边端
  ├── 两端 Single → 中心线对齐
  └── 对齐后重路由受影响边

Step 4d: X-1 多轮冲突消解 (reroute_conflicting_edges)
  ├── 最多 3 轮迭代
  ├── 检测边间距违规 (path_edge_spacing_violations)
  ├── 逐步增大 channel_margin (+10/+25/+40)
  ├── 通道负载感知 (ChannelLoadMap)
  └── 路径硬检查: path_is_clean + path_is_clean_from_edges

Step 4e: X-2 反向 stub 检测与端口翻转 (fix_reverse_stub_ports)
  ├── 检测反向 stub (has_reverse_stub) — 直接反向 / U型折返
  ├── 检测侧向接入 (detect_side_approach)
  ├── 尝试 Flip(对面端口) / Rotate(相邻端口)
  └── 笛卡尔积搜索最优端口组合

Step 4f: X-3 Lane Assignment (assign_lanes)
  ├── 检测平行冲突段
  ├── Union-Find 分组
  ├── cross-axis 平移分离
  └── Architecture 专用: corridor_planned_offsets + unrelated trunk 分离

Step 5: 标签避让
  └── resolve_label_overlaps
```

## 2. 与同类产品算法对比

| 维度 | 当前 flowml | ELK | yWorks/yFiles | Graphviz(ortho) | draw.io/mxGraph |
|---|---|---|---|---|---|
| **路径搜索** | 候选枚举+打分 | 网格 A* + channel | channel-based + pathfinding | 简化路径搜索 | waypoint + 曼哈顿 |
| **端口选择** | 几何规则+分组树 | side constraint | port constraint solver | 几何启发式 | 固定/浮动 port |
| **平行段分离** | Lane assignment(平移) + flowchart trunk+fork | 多层 nudging | 高级 nudging | 无 | 简单偏移 |
| **同源 fan-out** | slot 汇流 + trunk+fork 候选 | 无原生 | bus routing | 无 | 无 |
| **分组路由** | corridor 三段式 | 透传+nudging | group-aware | 绕行 | 绕行 |
| **增量路由** | 支持 preserve | 部分支持 | 支持 | 不支持 | 支持 |
| **确定性保证** | BTreeMap+显式排序 | 有 | 有 | 有 | 弱 |

## 3. 优势（利）

### 3.1 确定性渲染输出
当前实现严格遵守 `AGENTS.md §2`，所有分组排序用 BTreeMap + 显式 sort key（如 `sub_group_sort_key` 不含 `is_from` 以保证两端排名一致）。yWorks 和 ELK 虽也有确定性，但很多 JS 生态库（如早期 Dagre）在 HashMap 迭代上存在抖动问题。这一点对"同一输入多次渲染结果一致"至关重要，是工程质量的体现。

### 3.2 Slot 磁吸点系统设计精细
`DockingStrategy`（Single/Compact/Concentrate）根据同侧边数自适应：
- 1 条 → 居中
- 2-3 条 → 紧凑分布（16px 间距）
- 4+ 条 → 汇流共享入口

这种**入口合并**效果是 yWorks 的标志性特性，Graphviz/draw.io 都做不到。当前实现原生支持，视觉上接近商业产品。

### 3.3 多阶段后处理修复能力强
5 层串行修复（X-1 到 X-3 + replan_slots + flip_stub）解决的是启发式路由的遗留问题：
- `replan_slots`：全局排序修正 slot 与实际走向不一致（ELK 用 nudging 解决类似问题，但粒度不同）
- `fix_reverse_stub_ports`：反向 stub 端口翻转——这是**当前实现独创的痛点修复**，同类产品少见
- `lane_assignment`：cross-axis 平移分离，保持正交性（比 nudge 插 Z 弯更优雅）

### 3.4 走廊路由（corridor）处理跨组边
跨组边三段式路由（源→组边框→走廊→组边框→目标）+ BFS 最短链，这是 ELK 也有的特性，但 Graphviz/draw.io 缺乏。对 Architecture 图这类强分组场景效果好。

### 3.5 工程化的增量路由
`reroute_edges_touching_nodes` / `reroute_edges_preserve` 支持"节点移动后只重算相关边"，且 85% 阈值回退全图。这是**交互式编辑器的关键能力**，Graphviz 完全不支持，draw.io 实现较粗糙。

### 3.6 通道负载感知
`ChannelLoadMap` 在 reroute 时让 scorer 偏好低负载通道，从源头减少拥堵——这是较先进的思路，ELK 的 nudging 是事后分离，当前实现是事前规避。

## 4. 劣势（弊）

### 4.1 路径搜索非最优：候选枚举 vs A*
这是**最大的算法差距**。当前 `select_best_path_with_scorer_stats` 是枚举有限候选 + 打分，而非在障碍物空间中搜索全局最优路径。

**后果**：
- 复杂障碍场景下可能找不到可行路径（虽然有 degraded 降级）
- 候选集有限，路径质量上限受限于枚举策略
- ELK/yWorks 用 A* on grid 或 visibility graph，能在任意障碍配置中找到最短无碰撞路径

当前实现用 5 层后处理弥补这一缺陷，但这是"先产生问题再修复"的思路，而 A* 是"一开始就避开问题"。后处理失败率（nudge 失败率曾达 93%）说明这条路有天花板。

### 4.2 缺少全局优化视角
当前是**逐边贪心路由**（按 edge_order 顺序，先路由的边占通道，后路由的边避让）：
- `edge_order` 按 rank + 连接度排序，但这是启发式，非全局最优
- 先路由的边可能占据后续边的最优通道，导致不必要的绕行
- ELK 的 orthogonal routing 有**全局 channel 分配**阶段，yWorks 更是有 pool-based 全局优化

**后果**：高密度图中边交叉和绕行比商业产品多。

### 4.3 Nudging 实现偏保守
当前 `lane_assignment` 只做 cross-axis 平移，不做 nudge（middle segment 微调 + Z 弯补偿）。

- ELK 的 nudging 是**多轮迭代 + 分层分离**，能产生干净的平行段束
- yWorks 的 nudging 更精细，支持不同间距的束
- 当前实现的 lane_assignment 是"全有或全无"的平移，缺乏渐进式微调

**后果**：平行边束的视觉质量不如 ELK/yWorks，密集场景下可能仍有重合或间距不均。

### 4.4 边交叉最小化缺失
当前实现**没有显式的边交叉最小化阶段**。端口选择是几何规则，路径是贪心，后处理只解决间距和反向 stub，不优化交叉数。

- yWorks 有专门的 crossing minimization
- ELK 在 layer-based layout 中有交叉减少阶段
- 当前实现依赖 sugiyama_ranks 间接减少交叉，但路由阶段不主动优化

**后果**：复杂图中边交叉可能比商业产品多。

### 4.5 计算复杂度风险
5 层后处理中每层都可能触发重路由：
- `replan_slots`：全局排序 + 重路由受影响边
- `straighten`：重路由对齐边
- `X-1 reroute`：最多 3 轮，每轮全边扫描
- `X-2 flip_stub`：笛卡尔积端口组合搜索
- `X-3 lane`：Union-Find + 验证

每层重路由都调用 `select_best_path_with_scorer_stats`，最坏情况复杂度可能较高。A* 一次搜索到位的方案在大图上可能更快。不过当前有 `SegmentGrid` 空间索引和 `PreparedObstacles` 缓解，实际性能需测试。

### 4.6 缺少 hyperedge / bus routing
flowchart profile 的 trunk+fork 候选提供轻量同源 fan-out 视觉束，但不是真正的 hyperedge routing。
- yWorks 支持真正的 bus routing（多边共享总线）
- ELK 有 hyperedge 支持
- 当前 trunk+fork 仅覆盖同源 fan-out，跨语义边仍依赖 lane assignment 分离

### 4.7 自环边处理简单
`self_loop::route_self_loop` 独立处理，与主路由流程割裂。ELK/yWorks 的自环边与主路由统一处理，能更好利用通道。

## 5. 总结判断

**当前实现的定位**：介于 Graphviz（简单）和 ELK（专业）之间，接近 ELK 的功能覆盖，但算法内核不如 ELK/yWorks 精密。

**核心权衡**：
- 用**工程化的后处理层**弥补**算法内核（候选枚举 vs A*）的差距**
- 优势在于确定性、slot 系统、走廊路由、增量路由这些**工程化能力**
- 劣势在于**路径搜索的全局最优性**和**交叉/平行束的视觉质量**

## 6. 改进建议

如果要对标提升，优先级建议：

1. **P0**：路径搜索引入 A* on grid 或 visibility graph，替代候选枚举（解决根本问题）
2. **P1**：加入显式的边交叉最小化阶段
3. **P2**：恢复或增强 nudge 模块，做渐进式平行段分离
4. **P3**：全局 channel 分配替代逐边贪心

当前架构的**后处理层设计良好**，如果升级路径搜索内核，后处理层可以保留并简化（很多修复层是为了弥补候选枚举的不足）。
