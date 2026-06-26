# 正交边路由（orthogonal）重点投入方案

> 版本：2.3 | 状态：阶段 1+2+3 全部完成
>
> 目标读者：布局/边路由开发者、产品评审
>
> v2.0 变更：基于逐行代码审计修正诊断，补充 8 项原方案遗漏的问题；重构优化策略，将 friendliness 改为可插拔；重排优先级。
>
> v2.1 变更：新增 G8（端口选择缺乏同节点多边协调）与 P0-3（两阶段端口选择 + 同侧偏好），修复节点附近"本可避免"的边交叉。
>
> v2.2 变更：阶段 1+2 全部落地（P0-1/P0-2/P0-3/P1-1/P1-2/P1-3/§6），全量 715 测试通过。阶段 3 剩余 P2-1 空间索引、P1-2 Tasks 3&4、S1/S3 共享避障统一。
>
> v2.3 变更：阶段 3 全部落地——P2-1 性能与可观测性、P1-2 Task 3 (refine edge-overlap 感知)、P1-2 Task 4 (前置单边重路由)、S3 退化一致性测试（3 个测试，735 全量测试通过）。S1 统一 ObstacleIndex 构建经评估后标记为"可选未来工作"——当前 `needs_obstacle_index` trait 方法存在但调度层从未调用，各路由器内部各自构建索引；full unification 需要显著 trait 变更，收益有限，暂不实施。

---

## 实施进度总览

| 任务 | 状态 | 说明 |
|------|------|------|
| P0-1 候选生成器重构 + 硬约束 | ✅ 完成 | 候选生成器抽象 + 硬过滤 + 混合端口绕行 + 多档 channel_margin + Dijkstra 确定性 |
| P0-2 边-边交叉检测 | ✅ 完成 | `segments_conflict` 垂直交叉检测；通道预约机制（可选）未实现 |
| P0-3 端口选择全局协调 | ✅ 完成 | 两阶段端口选择 + 同侧偏好 + pair_group 一致性 |
| P1-1 汇流分叉路径 | ✅ 完成 | trunk+fork 路径形态 + overlap 豁免 + DSL 配置 |
| P1-2 refine 修复 + momentum | ✅ 完成 | Tasks 1-4 全部完成：锚点脱节修复 + momentum + edge-overlap 感知（综合评分回退）+ 前置单边重路由 |
| P1-3 分组通道感知 | ✅ 完成 | 分组边框障碍 + 端点分组豁免 + `node_to_groups` 映射 |
| §6 friendliness 解耦 | ✅ 完成 | `FriendlinessMode` 枚举 + DSL 解析 + V1/V2 条件执行（trait 化与 `RoutingContext.friendliness` 可选消费未实现，当前 DSL 控制已满足解耦目标） |
| P2-1 性能与可观测性 | ✅ 完成 | simplify stub 保护 + RefineDebugStats + OrthoDebugStats（degraded_count/hard_filter_reject_rate/avg_candidates_per_edge）+ edge_overlap bbox 预筛选 + bench 工具消费 debug 指标 |
| S1/S3 共享避障统一 | ✅ S3 完成 / S1 暂缓 | S2 Dijkstra 确定性已作为 P0-1 前置依赖完成；S3 退化一致性测试已落地（3 个测试，覆盖单边穿障/硬过滤清洁/多边穿障）；S1 统一 ObstacleIndex 构建经评估标记为可选未来工作（trait 变更成本高，收益有限） |

---

## 1. 背景与决策

Drawify 提供六种边路由算法：`straight`、`bezier`、`spline`、`circular`、`orthogonal`、`organic`。其中 **orthogonal 是产品默认路径**：

| 图表类型 | 默认边路由 |
|----------|-----------|
| Flowchart（流程图） | orthogonal |
| Architecture（架构图） | orthogonal |
| Custom（自定义图，继承 flowchart） | orthogonal |
| Mindmap | organic |
| State | circular |
| ER | straight |
| Sequence | 无（布局内置边几何） |

三种内置主场景默认走 orthogonal，且 flowchart / architecture 正是边最密、分组最多、用户对「不穿障、不交叉、入口整齐」要求最高的图种。

**结论：边路由算法的重点投入应放在 orthogonal。** 其余算法要么场景窄（straight、circular），要么主要作为曲线场景的退化兜底（bezier / organic → spline），要么质量已相对够用（spline 硬避障）。

本方案给出 orthogonal 的改进路线、验收标准与投入比例建议。文档自成体系，可直接作为实施 backlog 使用。

---

## 2. 现状诊断（基于代码审计）

### 2.1 架构概览

orthogonal 实现位于 `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/`，子模块分工：

| 模块 | 职责 |
|------|------|
| `slot.rs` | 磁吸点（slot）分配、同侧多边汇流策略（`DockingStrategy`） |
| `path.rs` | 候选路径生成、侧通道绕行（`build_channel_detours`） |
| `scoring.rs` | 路径评分：长度 + 折弯 + 障碍惩罚 + 边段重叠惩罚 |
| `simplify.rs` | 折线简化、共线合并 |
| `context.rs` | 路由上下文、端点对 |
| `mod.rs` | 主流程：slot 分配 → 按端点度数排序逐边路由 → 标签避让 |

路由完成后，若 `supports_refine() == true`（orthogonal 返回 true），`refine` 模块会检测 Polyline 穿障、推开问题节点并增量重路由。

### 2.2 已具备的能力（经代码验证）

- **端口选择**：`choose_pair_sides` 按节点几何关系确定性选边（上/下/左/右），避免组合爆炸；方向性（source/target）通过 `endpoint_bundling_key` 区分，出/入边分离有测试覆盖。
- **软障碍避让**：`NODE_CROSSING_PENALTY = 10_000.0`（`mod.rs:149`）对穿过非端点节点的路径段施加**有限值**惩罚；`build_channel_detours` 在共轴端口场景开侧通道绕行。
- **边段重叠惩罚**：已路由边段记入 `RoutedSegment`（扁平 `Vec`），后续边评分时惩罚与之**平行重叠**的新段。
- **汇流锚点**：`choose_docking_strategy`（`slot.rs:21-27`）按同侧边数分流——0-1 Single、2-3 Compact（pitch=min(slot_pitch, 16px)）、4+ Concentrate（共享 `base_frac` 锚点）。
- **refine 兜底**：`run_refine` 循环 `max_passes=3`，`push_distance=40px`，整图回退机制。
- **标签避让**：路由后调用 `resolve_label_overlaps`（纯后处理，不参与路径评分）。
- **stub 段**：`PORT_CLEARANCE = 16.0`（`mod.rs:143`），节点出口短直线，`simplify_path_preserving_stubs` 保护 stub 折点。

### 2.3 核心缺口（按用户感知 × 修复成本排序）

> 以下每条缺口均附代码证据。原方案 v1.0 的诊断已修正并补充。

#### G1：候选生成不自适应 + 软约束 → 密集图穿障

**代码证据**：

- `NODE_CROSSING_PENALTY = 10_000.0`（`mod.rs:149`）是**有限值**，多段穿多节点时惩罚线性累加，仍是软约束。
- `select_best_path_with_scorer`（`path.rs:30-52`）候选生成阶段**无任何硬过滤**，对所有候选逐一打分取最小。
- `build_candidate_paths`（`path.rs:54-91`）生成固定模板：直线 1 个、L 形 ≤3 个、Z 形固定 8 个 ratio（`path.rs:337`）、U 形 1 个。单边候选 ≤10，**不根据障碍位置自适应**。
- orthogonal **完全不使用 `ObstacleIndex`**（全模块 grep 确认），`needs_obstacle_index()` 未覆写返回 false。障碍检测靠 `scoring.rs:59` 直接遍历 `ctx.nodes` HashMap。

**原方案遗漏的关键点**：

- `build_channel_detours`（`path.rs:114-118`）**仅在 `from_vertical == to_vertical` 时生成侧通道**。混合端口（一垂直一水平，即 L 形端口组合）穿障时**无任何绕行候选**——这是 L 形场景软约束失效的根因。
- 候选模板固定，无法在"障碍缝隙间穿行"。当 8 个 Z-ratio 和 2 个侧通道都穿障时，没有任何逃生路径，最终依赖 refine 推节点补救。

**用户感知**：复杂流程图、带分组的架构图中，边穿过中间节点，refine 推节点导致布局抖动、收敛不稳定。

#### G2：边-边交叉检测缺失（仅检测平行重叠）

**代码证据**：

- `edge_overlap_penalty`（`scoring.rs:77-94`）对路径每段与所有已路由段调用 `segments_conflict`。
- `segments_conflict`（`scoring.rs:96-131`）逻辑：
  - 同为水平段：y 间距 `gap <= EDGE_PARALLEL_GAP`（=8.0）且 x 区间重叠 → true。
  - 同为垂直段：同理。
  - **其他情况（包括垂直十字交叉）直接返回 false**（`scoring.rs:130`）。
- `RoutedSegment` 是扁平 `Vec`（`path.rs:8-15`），`edge_overlap_penalty` 双重循环线性扫描，无 bbox 剔除，无空间索引，总体 O(E²S²)。

**原方案诊断偏差**：v1.0 描述为"边-边交叉缺乏全局协调"，实际是**根本不检测垂直交叉**，比"缺乏协调"更严重。

**用户感知**：20+ 边的图上出现明显的边-边十字交叉，且路由器完全不感知。

#### G3：汇流与 overlap 惩罚拮抗

**代码证据**：

- `Concentrate` 模式（`mod.rs:313-337`）：同组边共享 `base_frac` → 同一 `slot_anchor` 坐标 + 同一 `from_side`。
- 共享锚点的边其 stub 段（segment 0）几何完全相同。
- 第 2 条及之后的边会被 `edge_overlap_penalty` 判定重叠（gap=0 < 8）而累加 `EDGE_OVERLAP_PENALTY=1200`（`mod.rs:152`）。
- 虽然该惩罚对所有候选是常数（不改变相对排序），但中段若趋同会被惩罚推开，**主动破坏汇流视觉**。
- 无 `fork_distance` / `trunk` 概念，汇流仅靠锚点几何重合实现。

**用户感知**：fan-in / fan-out 场景视觉上仍不够整洁，汇流后立即散开。

#### G4：refine 锚点脱节 bug + 无 momentum（震荡风险）

**代码证据（原方案完全遗漏）**：

1. **锚点脱节**：`reroute_subset`（`refine.rs:234-256`）每轮做**全量重路由**（`router.route(diagram, result.clone())`，`refine.rs:244`），全量重路由会基于推开后的节点位置**重新计算所有 slot 锚点**（`mod.rs:277-340`）。但随后只把 `affected_edges` 的边替换回原 result（`refine.rs:247-251`），**未受影响的边保留旧锚点旧路径**。被推开的节点上，未受影响边的锚点仍指向旧位置，与节点新位置脱节——视觉上边会"悬空"或"插入节点内部"。

2. **无 momentum**：`push_problem_nodes`（`refine.rs:152-169`）无历史位移记录，每轮独立计算推力。对比 `FriendlinessAdjuster` 有 `MomentumHistory`（`adjuster.rs:58`）和方向反转衰减（`adjuster.rs:228-237`），refine **没有任何 momentum 机制**。在 `max_passes=3` 内可能 A→B→A 振荡。

3. **只修 node-crossing，不修 edge-overlap**：`analyze_edge_node_crossings`（`refine.rs:67-110`）只检测边穿节点。推节点后可能引入新的边-边重叠，refine 完全不感知，回退机制也只看 node-crossings 数（`refine.rs:215`）。

**用户感知**：refine 后边与节点脱节；同一输入多次渲染节点位置反复抖动。

#### G5：friendliness 强耦合到调度主流程

**代码证据**：

- `compute_layout_with_plan`（`layout/mod.rs:1030-1089`）硬编码三阶段：V1 评估（总是执行）→ V2 调整（`FriendlinessAdjuster`）→ 路由后择优。
- V2 调整器 `FriendlinessAdjuster::with_default()`（`adjuster.rs:44-52`）硬编码 `enabled: true`，唯一禁用途径是环境变量 `DRAWIFY_NO_V2_ADJUST=1`（`mod.rs:1044`），**非 DSL 配置**。
- V2 强制增加 1 次路由开销：V2 改变布局时需额外路由 baseline（`mod.rs:1081`）对比择优。
- `congestion_score` 与 orthogonal 路由质量相关性弱（`mod.rs:169` `w_congestion: 0.06`，Pearson r<0.06）。
- `ChannelCongestion` 的 bbox 是**全图宽/高的条带**（`congestion.rs:140-153`），粒度过粗，不适合作为路径评分的查询单元。

**用户感知**：friendliness 不可关闭，大图额外路由开销；orthogonal 路由质量与 friendliness 分数弱相关，消费收益低。

#### G6：分组 / 架构图通道未建模

Architecture 图有 group 嵌套与组间通道需求。orthogonal 的 `build_channel_detours` 主要面向节点列障碍，对 group 边框、组间预留通道的感知有限（`channel_outside` 仅在分组边框附近调整通道位置，`path.rs:236-265`）。

#### G7：确定性隐患（次要）

- `visibility.rs:309-313` 的 `HeapNode::Ord` 在等距时返回 `Equal`，Dijkstra 等距路径选择可能不稳定，违反 AGENTS.md §2。当前 orthogonal 不用 ObstacleIndex，但 P0-1 若复用需先修复。
- `obstacle_penalty` 中 `for (node_id, nl) in nodes`（`scoring.rs:59`）遍历 HashMap 求和，浮点加法顺序受迭代序影响（实际影响可忽略，但严格违反 AGENTS.md §2 精神）。
- `build_channel_detours` 用 `simplify_path`（非 `preserving_stubs` 版本，`path.rs:165, 216`），侧通道候选的 stub 可能被共线合并。

#### G8：端口选择缺乏同节点多边协调（不必要交叉的直接原因）

**代码证据**：

- `choose_pair_sides`（`slot.rs:53-86`）只看 A、B 两个节点的几何关系（中心点 `dx/dy` + AABB 重叠），**逐对独立**决定端口对 `(side_a, side_b)`。
- 调用点（`mod.rs:184-207`）：按**无向节点对**分组，每组调用一次 `choose_pair_sides`，组内边共享端口对。**不同节点对之间无协调**——同一节点 X 上的多条边（属于不同 pair_group）各自独立选端口。
- slot 分配（`mod.rs:223-340`）在端口选择**之后**执行，只能基于已确定的 `from_side`/`to_side` 在同侧内分配磁吸点，无法纠正端口选择的不合理。

**现象**：节点 X 有两条出边 X→Y、X→Z。`choose_pair_sides(X,Y)` 因 Y 在 X 右侧选 Right 出，`choose_pair_sides(X,Z)` 因 Z 在 X 下方选 Bottom 出。两条边从不同侧出发，路径在 X 附近交叉。若 X→Z 也从 Right 出（slot 分配让两条边在右侧各占一个磁吸点），路径更整洁、无交叉。

**根因**：端口选择是局部最优（单条边几何最优），但缺乏同节点多边的全局协调——没有"同侧偏好"让同向边汇拢到同一侧出发。

**用户感知**：节点附近出现明显的"本可避免"的边交叉，用户感知为"路由很蠢"，比穿障更显眼。

---

## 3. 设计原则

实施时遵守以下原则，复杂度可控是核心约束：

1. **确定性**：迭代顺序、候选排序、tie-break 均不依赖 `HashMap` 遍历序（见 `AGENTS.md`）。Dijkstra 等距 tiebreak 必须用 node_id 稳定化。
2. **渐进增强**：硬过滤 → 通道扩展 → ObstacleIndex 退化，逐级降级，避免单点失败导致无边可画。
3. **friendliness 可插拔**：friendliness 是**可选诊断信号**，不是 orthogonal 的必需依赖。orthogonal 在 `None` 时退化为当前行为。切换路由算法的决策放在调度层，不在 router 内部。
4. **复杂度预算**：单边候选数 ≤ 20，路由主循环不引入全局迭代优化（SA/LP），refine `max_passes` 保持 ≤ 5。性能目标：100 边 / 50 节点图路由 < 50ms。
5. **不推翻两阶段架构**：节点布局与边路由分离；refine 是后处理，不是布局主循环。
6. **测试驱动**：每个 P 项至少 2 个单元测试 + 1 个 showcase 级集成场景。

---

## 4. 投入比例建议

```
65%  orthogonal 核心质量（本文 §5，含 P0-3 端口协调）
20%  refine 修复与稳定性（本文 §5.4）
10%  friendliness 解耦（本文 §6）
5%   共享避障基础设施（本文 §7）
```

friendliness 解耦不替代 orthogonal 主战场，但能降低全图种的质量下限、并消除 V2 强制多路由的开销。

---

## 5. orthogonal 核心任务（按优先级）

### P0-1：候选生成器重构 + 硬约束（穿障零容忍）

**问题**：仅靠 `NODE_CROSSING_PENALTY` 软约束无法保证不穿障；候选模板固定且过少；混合端口无绕行候选（G1）。

**方案（比 v1.0 更彻底）**：

1. **候选生成器抽象**：引入 `CandidateGenerator` trait，将 `build_candidate_paths` + `build_channel_detours` 统一为可扩展的候选源。默认实现保持现有模板，新增障碍感知候选源。

2. **障碍感知候选生成**：在 `build_channel_detours` 中：
   - **修复混合端口盲区**：L 形端口组合（一垂直一水平）穿障时，生成"先沿源端口方向绕行再转向目标端口"的候选。
   - **多档 channel_margin**：从固定 18px 改为 `[18, 28, 40]` 三档，逐档尝试。
   - **障碍缝隙穿行**：检测障碍列/行之间的缝隙，生成穿过缝隙的候选（替代固定 Z-ratio）。

3. **硬过滤**：在 `select_best_path_with_scorer`（`path.rs:30-52`）候选打分前，增加硬过滤——任何与非端点节点 AABB（含 `NODE_OBSTACLE_PAD` 膨胀）相交的候选直接丢弃。过滤用确定性顺序（按 node_id 排序）。

4. **二级退化**：若所有候选均被过滤，调用 `ObstacleIndex::shortest_path`（复用 `visibility.rs`）生成绕行折线，再正交化。**前提**：先修复 Dijkstra 确定性（见 P2-2）。

5. **记录退化原因**：导出 debug 指标 `orthogonal_degraded_count`、`hard_filter_reject_rate`。

**涉及文件**：`path.rs`（候选生成重构）、`scoring.rs`（硬过滤接口）、`mod.rs`（调度层预建 `ObstacleIndex`）、`visibility.rs`（确定性修复）

**验收标准**：

- 新增单元测试：三节点纵列，A→C 边不得穿过 B（零穿障，不依赖 refine）。
- 新增单元测试：L 形端口组合穿障时，生成绕行候选（修复 G1 混合端口盲区）。
- 新增单元测试：无障碍时路径与当前行为一致（无退化）。
- showcase 流程图 / 架构图：`edge_node_crossings` 指标相对当前下降 ≥ 50%（以 `edge_routing_bench` 为基准）。

---

### P0-2：边-边交叉检测 + 通道预约

**问题**：`segments_conflict` 不检测垂直交叉；逐边贪心 + 局部 `edge_overlap_penalty` 不足以避免通道争抢（G2）。

**方案**：

1. **修复交叉检测**：扩展 `segments_conflict`（`scoring.rs:96-131`），增加水平段与垂直段的交叉检测（线段相交几何）。交叉惩罚权重高于长度，低于 node-crossing。

2. **空间索引加速**：`RoutedSegment` 用网格或 R-tree 索引，候选线段检测 O(log N) 而非 O(N)。邻近边预筛选：仅比较 bbox 扩张 30px 内的边对。

3. **通道预约机制**（轻量级全局协调，替代 v1.0 的"两阶段路由"）：
   - 路由前扫描所有边的端点对，按 `edge_order` 为每条边预约"主通道方向"（水平/竖直走廊）。
   - 占据唯一通道的边优先路由（在现有 `max(deg(from), deg(to))` 降序基础上，增加"通道唯一性"作为次级排序键）。
   - 预约信息记入 `RoutingContext`，候选生成时避开已预约通道。

4. **可选全局模式**：`orthogonal { global: true }` 启用两阶段路由（Phase A 通道骨架 → Phase B 折线生成），适合 30+ 边大图。默认关闭。

**涉及文件**：`scoring.rs`（交叉检测 + 空间索引）、`path.rs`（通道预约）、`mod.rs`（`edge_order` 增强 + `RoutingContext` 扩展）

**验收标准**：

- 新增测试：两条边垂直十字交叉时，`segments_conflict` 返回 true（修复 G2 检测缺失）。
- 新增测试：5 条边汇聚同一通道时，交叉数 ≤ 1。
- `edge_routing_bench` 的 `edge_crossings` 在 flowchart showcase 上下降 ≥ 30%。
- 100 边图路由耗时仍 < 50ms（空间索引补偿检测开销）。

---

### P0-3：端口选择全局协调（同侧偏好）

**问题**：`choose_pair_sides` 逐对独立决定端口，同一节点的多条边分散在不同侧出发，导致节点附近不必要的交叉（G8）。

**方案（两阶段端口选择）**：

1. **Phase 1 — 候选端口计算（现有逻辑 + 次选标记）**：
   - 对每条边，`choose_pair_sides` 算出主选端口对 `(side_x, side_y)`。
   - 同时计算次选端口对（正交方向：主选水平则次选垂直，反之），并标记次选是否几何可接受。
   - 次选可接受性判定：复用 `choose_pair_sides` 的阈值逻辑（`slot.rs:69` `dy.abs() >= dx.abs() * 0.4`）。若次选方向的对端节点位移比例低于阈值，则次选代价过高、不可接受。

2. **Phase 2 — 同节点端口协调（新增）**：
   - 对每个节点 X，收集所有以 X 为 **from 端**的边及其主选 `from_side`。
   - 按 `is_from` 分组（出边/入边分开协调，不强制出入同侧——方向相反）。
   - 统计 X 各侧的边数，找出同 `is_from` 组的**多数派侧** `majority_side`。
   - 对每条主选 ≠ `majority_side` 的边：
     - 若次选侧 == `majority_side` **且**次选几何可接受 → 切换到次选（`from_side` 改为 `majority_side`，`to_side` 保持不变，路径形态由 `select_best_path` 重新决定）。
     - 否则保持主选。
   - 同理对 X 作为 **to 端**的边协调 `to_side`。

3. **pair_group 一致性**：同一无向节点对（`pair_groups`）内的边必须共享端口对（`mod.rs:194-206` 约束）。协调以 pair_group 为最小单元——组内所有边一起切换，不破坏一致性。

4. **确定性**：协调遍历节点按 `node_id` 排序，边按 `edge_index` 排序，多数派 tiebreak 用 `edge_index` 最小者。

**复杂度**：O(E) 候选计算 + O(V + E) 协调，无空间索引，无迭代。

**与 P1-1 协同**：端口同侧后，汇流（Concentrate/Compact）才能发挥作用——同侧多边是汇流的前提。P0-3 让边"汇拢到同一侧"，P1-1 让同侧边"共享主干"。

**涉及文件**：`slot.rs`（`choose_pair_sides` 增加 `choose_pair_sides_with_alternative`）、`mod.rs`（端口选择阶段插入 Phase 2 协调循环）

**验收标准**：

- 新增测试：节点 X 有两条出边（一条主选 Right、一条主选 Bottom），次选可接受时，两条边均从 Right 出（修复 G8）。
- 新增测试：次选几何不可接受时（对端节点在正下方），保持主选 Bottom，不强行切换。
- 新增测试：pair_group 一致性——同组边端口对始终相同。
- 新增测试：确定性——同一输入多次运行，端口选择一致。
- showcase：节点附近"本可避免"的交叉显著减少（视觉抽检 + `edge_crossings` 下降）。

---

### P1-1：汇流分叉路径 + 解除 overlap 拮抗

**问题**：Concentrate 仅合并锚点，路径形态未变；且 overlap 惩罚拮抗中段趋同（G3）。

**方案**：

1. **trunk + fork 路径形态**：当 `DockingStrategy::Concentrate` 且 `count >= concentrate_threshold`（默认 4，可配置）：
   - 所有边共享入口锚点 `P_entry`；
   - 在距节点边界 `FORK_DISTANCE`（默认 20px）处设分叉点 `P_fork`；
   - 路径形态：`P_entry → P_fork → 各自 stub → 目标`。

2. **解除 overlap 拮抗**：汇流主干段（`P_entry → P_fork`）豁免 `edge_overlap_penalty`。识别方式：同组 Concentrate 边的 segment 0 若几何相同，跳过 overlap 检测。

3. **Compact 保持现状**：2-3 边保持 `slot_fraction_around`，`pitch` 上限 16px。

4. **DSL 配置**：

```dfy
edge_routing: orthogonal {
    concentrate_threshold: 4
    fork_distance: 20
}
```

**涉及文件**：`slot.rs`（trunk/fork 几何）、`mod.rs`（路径生成 + 配置解析）、`scoring.rs`（overlap 豁免）

**验收标准**：

- 测试：同侧 5 条出边，前 20px 路径段重合，之后分叉。
- 测试：同侧 2 条出边，锚点间距 ≤ 16px。
- 测试：Concentrate 模式下，同组边不因 overlap 惩罚而散开（修复 G3 拮抗）。
- 视觉回归：架构图 fan-in 入口箭头明显集中。

---

### P1-2：refine 修复 + momentum（稳定性）

**问题**：refine 锚点脱节 bug、无 momentum 震荡、只修 node-crossing 不修 edge-overlap（G4）。

**方案**：

1. **修复锚点脱节**：`reroute_subset`（`refine.rs:234-256`）改为：
   - 全量重路由后，**同步替换所有受推开节点上的边**（不仅是 `affected_edges`），保证锚点与节点位置一致。
   - 或：增量重路由接口 `reroute_edges_around_nodes`，只重路由受推开节点关联的边，避免全量重路由的副作用。

2. **引入 momentum**：`push_problem_nodes`（`refine.rs:152-169`）增加 `MomentumHistory`（复用 `adjuster.rs:58` 的设计），检测方向反转并施加衰减，与 `FriendlinessAdjuster` 一致。

3. **refine 感知 edge-overlap**：`analyze_edge_node_crossings` 扩展为 `analyze_crossings`，同时检测边-边重叠（复用 P0-2 的 `segments_conflict`）。回退决策综合 node-crossings 和 edge-overlaps。

4. **refine 前置单边重路由**：refine 推开前，先对受影响边尝试 `reroute_with_obstacle_index`（仅改边不改节点）。仅当单边走线重路由无法消除穿障时，才推节点。

**涉及文件**：`refine.rs`（主循环重构）、`edge_routing_orthogonal/mod.rs`（增量重路由接口）

**验收标准**：

- 测试：refine 推开节点后，所有边的锚点与节点位置一致（修复 G4 锚点脱节）。
- 测试：构造震荡场景（节点被两条边交替推），momentum 抑制振荡。
- 同一输入多次渲染，节点坐标与边路径确定性一致（AGENTS.md §2）。
- refine 触发节点推动的次数相对当前下降 ≥ 40%。

---

### P1-3：分组 / 架构图通道感知

**问题**：组边框与组间通道未作为一等障碍（G6）。

**方案**：

1. 将 `GroupLayout` 边框（可内缩 `group_padding`）纳入障碍集合，与节点障碍统一索引。
2. Architecture 布局 hints 若标注组间通道（`group_gap` 模块），路由时将该区域标记为「优先通道」或「禁止穿越」。
3. 组内边与组外边分层路由：先路由组内边，再路由跨组边（减少跨组边被组内边抢占通道）。

**涉及文件**：`path.rs`（`build_channel_detours`）、`scoring.rs`、`layout/friendliness/group_gap.rs`

**验收标准**：

- 测试：边不得穿过 group 边框（仅允许端口出入）。
- Architecture showcase：跨组边不再横穿组内节点密集区。

---

### P2-1：性能与可观测性

**问题**：硬过滤 + 更多候选 + 交叉检测可能增加大图耗时。

**方案**：

1. 空间索引：障碍 AABB 用网格或 R-tree，候选线段检测 O(log N)。
2. 邻近边预筛选：标签-边、边-边检测仅比较 bbox 扩张 30px 内的边对。
3. 导出 debug 指标：`orthogonal_degraded_count`、`hard_filter_reject_rate`、`avg_candidates_per_edge`、`refine_push_count`、`refine_momentum_reversals`。
4. **Dijkstra 确定性修复**：`visibility.rs:309-313` 的 `HeapNode::Ord` 在等距时用 `node_id` 作为稳定 tiebreak（修复 G7）。
5. **simplify stub 保护**：`build_channel_detours` 改用 `simplify_path_preserving_stubs`（修复 G7）。

**涉及文件**：`path.rs`、`scoring.rs`、`visibility.rs`、`drawify-eval` bench 工具

**验收标准**：

- 100 边 / 50 节点图，路由耗时 < 50ms（release，M 系列 Mac 基准）。
- bench JSON 含上述 debug 字段。
- Dijkstra 同一输入多次运行路径一致。

---

## 6. friendliness 解耦方案（10% 投入）

> 目标：friendliness 作为**可插拔诊断信号**，不作为 orthogonal 的必需依赖。orthogonal 在 `None` 时退化为当前行为。

### 6.1 现状问题

- friendliness 硬编码在 `compute_layout_with_plan`（`layout/mod.rs:1030-1089`），V1 总是执行，V2 强制多路由一次。
- `FriendlinessAdjuster::with_default()` 硬编码启用，唯一禁用途径是环境变量。
- `congestion_score` 与 orthogonal 质量相关性 r<0.06，消费收益低。
- `ChannelCongestion` bbox 是全图条带粒度，不适合路径评分。

### 6.2 解耦策略

1. **friendliness 阶段抽象为 trait**：

```rust
pub trait LayoutPostProcessor {
    fn name(&self) -> &'static str;
    fn enabled(&self, plan: &LayoutPlan) -> bool;
    fn apply(&self, diagram: &Diagram, result: LayoutResult) -> LayoutResult;
}
```

   `FriendlinessAdjuster` 实现该 trait，通过 `LayoutPlan` 配置启用/禁用，而非环境变量。

2. **DSL 配置**：

```dfy
layout: flowchart {
    friendliness: off          // off | diagnose | adjust
    friendliness_adjuster: off // off | on（仅 friendliness: adjust 时生效）
}
```

   - `off`：跳过 V1 评估和 V2 调整，零开销。
   - `diagnose`：仅 V1 评估（写入 `hints.friendliness_report`），不调整布局。
   - `adjust`：V1 + V2，当前默认行为。

3. **orthogonal 可选消费**：`RoutingContext` 加 `friendliness: Option<&'a FriendlinessReport>`。orthogonal 评分时若 `Some` 则加权 friendliness 信号，若 `None` 则退化为当前行为。

4. **消费方式（若启用）**：用 `predicted_crossings` 的**边级热点索引**（`crossing_predict.rs:17` `edge_indices`）做软指导，**不用**复合 `score` 或 `congestion_score`（粒度过粗、相关性弱）。

5. **路由算法切换放调度层**：`congestion_score` 超阈值且 orthogonal 硬过滤失败率偏高时，由调度层（`layout/mod.rs:1060` 路由器选择点）建议切换 `spline`，**不在 orthogonal 内部**。orthogonal 侧只输出诊断。

### 6.3 验收标准

- 测试：`friendliness: off` 时，`compute_layout` 不调用 `RoutingFriendlinessEvaluator`，路由只执行一次。
- 测试：`friendliness: diagnose` 时，`hints.friendliness_report` 有值，但布局未改变。
- 测试：`friendliness: adjust` 时，行为与当前一致（回归保护）。
- 测试：orthogonal 在 `friendliness: None` 时路由结果与当前一致。
- 性能：大图（100 节点）`friendliness: off` 相比 `adjust` 路由耗时下降 ≥ 30%（消除 V2 多路由开销）。

---

## 7. 共享避障基础设施（5% 投入）

bezier、organic、circular、spline 穿障后已退化到 `ObstacleIndex::shortest_path`。orthogonal P0-1 若也复用同一套索引：

| 任务 | 说明 | 状态 |
|------|------|------|
| S1 统一 `ObstacleIndex` 构建 | 调度层按 `needs_obstacle_index()` 预建，膨胀间距统一取自 `DEFAULT_NODE_MARGIN`。orthogonal 覆写 `needs_obstacle_index() -> true`。 | ⏸️ 暂缓（可选未来工作） |
| S2 Dijkstra 确定性修复 | `HeapNode::Ord` 等距时用 `node_id` 稳定 tiebreak（P2-1 前置依赖）。 | ✅ 完成 |
| S3 退化一致性测试 | 同一穿障场景，bezier 退化与 orthogonal 硬退化路径均不穿障且确定性一致。 | ✅ 完成 |

**S1 暂缓理由**：经代码审计发现 `needs_obstacle_index` trait 方法存在但调度层从未调用，每个路由器（bezier/spline/circular/organic）各自内部构建 `ObstacleIndex`，orthogonal 使用自己的 HashMap-based 障碍检测。Full unification 需要显著 trait 变更（调度层预建 + 注入），且当前各路由器独立工作正常，收益有限，风险高于价值。若未来新增路由器或需要统一膨胀间距策略，可重新评估。

**S3 落地内容**（3 个测试，位于 `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs`）：
- `test_s3_orthogonal_degradation_deterministic`：三节点纵列穿障场景，多轮路由路径完全一致（AGENTS.md §2 确定性要求）。
- `test_s3_orthogonal_hard_filter_produces_clean_path`：硬过滤后路径不穿障（验证退化候选仍满足硬约束）。
- `test_s3_multi_edge_degradation_deterministic`：6 节点 4 边多边穿障场景，多轮路由完全一致。

**不建议**：v1.0 的"正交化后处理"（spline 折线拉直为水平/竖直段）——会破坏 stub（`simplify_path_preserving_stubs` 保护逻辑失效），且产生大量微段与 `BEND_PENALTY` 冲突。orthogonal 退化应直接用 `ObstacleIndex::shortest_path` + 简单正交化（仅对绕行段，保留 stub）。

---

## 8. 实施路线图

```
阶段 1（3–4 周）— 质量底线 + 稳定性修复
  ├─ P0-1 候选生成器重构 + 硬约束 + ObstacleIndex 退化
  │   ├─ 修复混合端口无绕行候选（G1 盲区）
  │   ├─ 多档 channel_margin + 障碍缝隙穿行
  │   └─ S2 Dijkstra 确定性修复（前置依赖）
  ├─ P0-2 边-边交叉检测修复 + 空间索引
  ├─ P0-3 端口选择全局协调（同侧偏好，修复 G8 不必要交叉）
  └─ P1-2 refine 锚点脱节修复 + momentum（G4 紧急修复）

阶段 2（3–4 周）— 视觉一致性 + 解耦
  ├─ P1-1 汇流分叉路径 + 解除 overlap 拮抗（依赖 P0-3 同侧汇拢）
  ├─ P1-3 分组通道感知
  ├─ §6 friendliness 解耦（trait 化 + DSL 配置）
  └─ P0-2 通道预约机制（可选全局模式）

阶段 3（2 周）— 性能与收敛 ✅ 完成
  ├─ P2-1 空间索引 + bench 指标 + simplify stub 修复 ✅
  ├─ P1-2 refine 感知 edge-overlap + 前置单边重路由 ✅
  └─ S1/S3 共享避障统一（S3 ✅ 完成 / S1 ⏸️ 暂缓为可选未来工作）
```

**阶段 1 调整说明**：v2.0 将 refine 修复（P1-2）从阶段 3 提前到阶段 1，因为锚点脱节是 bug 而非优化，应优先修复。v2.1 新增 P0-3 端口协调到阶段 1，因为节点附近的不必要交叉是用户最直观感知的问题，且修复成本可控（O(E) 协调，无迭代）。

每阶段结束前：

1. 跑 `cargo test -p drawify-core` 全量测试；
2. 跑 `cargo run -p drawify-eval --bin edge_routing_bench`，对比 JSON 指标；
3. 对 flowchart / architecture showcase 做视觉抽检；
4. 同一输入多次渲染，验证确定性（AGENTS.md §2）。

---

## 9. 不建议投入的方向

| 方向 | 原因 |
|------|------|
| straight 路由 | ER 场景简单，边际收益低 |
| circular 弧形微调 | 状态图专用，穿障退化已补齐 |
| bezier 控制点美学 | 非默认；穿障已交 spline |
| 全面重写 orthogonal | 现有 slot + 评分架构可渐进增强，推翻成本高 |
| 全局 MILP / ILP 求解 | 与 Rust 单体、确定性迭代原则不符；复杂度预算超标 |
| spline 正交化后处理（v1.0 S2） | 破坏 stub，产生微段冲突 |
| friendliness 作为 orthogonal 必需依赖 | 相关性弱（r<0.06），粒度过粗，违背可插拔原则 |

---

## 10. 成功指标

阶段 1 启动前，先用 `edge_routing_bench` 对 showcase 全量跑一轮，将 JSON 结果存档为 `docs/architecture/布局优化/baselines/orthogonal-baseline-YYYY-MM-DD.json`，作为后续对比基准。

| 指标 | 当前基线（待测量） | 目标 | 对应任务 |
|------|-------------------|------|----------|
| flowchart showcase `edge_node_crossings` | 实测 | −50% | P0-1 |
| flowchart showcase `edge_crossings` | 实测 | −30% | P0-2 + P0-3 |
| architecture showcase `edge_node_crossings` | 实测 | −50% | P0-1 |
| 节点附近"本可避免"交叉数 | 实测 | −50% | P0-3 |
| refine 触发节点推动次数 / 图 | 实测 | −40% | P1-2 |
| refine 后锚点-节点脱节数 | 实测（应为 0） | 0 | P1-2（bug 修复） |
| 100 边图路由耗时（release） | 实测 | < 50ms | P2-1 |
| 带标签密集图 `label_overlaps` | 实测 | −60% | 标签层（见 §11） |
| `friendliness: off` 大图路由耗时 | 实测 | −30% vs `adjust` | §6 解耦 |
| 同一输入多次渲染确定性 | — | 路径完全一致 | P2-1 + 全局 |

---

## 11. 标签层横切任务（随阶段 2 并行）

标签避让在 `layout/edge/common/label_avoidance.rs`，orthogonal 路由结束后统一调用。以下改进惠及所有带标签的图种：

| 任务 | 说明 | 验收 |
|------|------|------|
| L1 邻近边预筛选 | 标签-边碰撞检测前，用 bbox 外扩 30px 排除远边 | 大图标签避让耗时下降，结果不变 |
| L2 边碰撞推开后回检 | 对标 `push_label_from_obstacle_safe`，避免推入其他边 | 密集图标签不再压线 |
| L3 迭代上限可配置 | `DEFAULT_MAX_LABEL_ITERATIONS` 从固定 5 改为按标签数自适应（如 `min(5, 2 + n/10)`） | 密集图标签重叠率下降 |
| L4 视觉回归集 | 固定 10 张带标签 showcase 截图对比 | CI 或手动 checklist |

---

## 12. 关键代码索引

| 模块 | 路径 | 关键行 |
|------|------|--------|
| orthogonal 主流程 | `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs` | `route_edges_orthogonal:165`、`edge_order:344`、`Concentrate:313` |
| 候选路径与通道绕行 | `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs` | `select_best_path:30`、`build_channel_detours:98`（混合端口盲区:114）、`RoutedSegment:8` |
| 路径评分 | `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs` | `obstacle_penalty:46`、`segments_conflict:96`（垂直交叉不检测:130） |
| 磁吸点与汇流策略 | `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs` | `choose_docking_strategy:21` |
| 折线简化 | `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/simplify.rs` | `preserving_stubs:5` |
| 路由上下文 | `crates/drawify-core/src/layout/edge/edge_routing_orthogonal/context.rs` | `RoutingContext:19`（待加 friendliness: Option） |
| 可见性图避障 | `crates/drawify-core/src/layout/edge/visibility.rs` | `ObstacleIndex:102`、`shortest_path:271`（Dijkstra 确定性隐患:309） |
| refine 循环 | `crates/drawify-core/src/layout/refine.rs` | `run_refine:179`、`reroute_subset:234`（锚点脱节:244）、`push_problem_nodes:152`（无 momentum） |
| 布局调度主流程 | `crates/drawify-core/src/layout/mod.rs` | `compute_layout_with_plan:926`、`EdgeRoutingStrategy:808`、friendliness 三阶段:1030-1089 |
| friendliness 评估器 | `crates/drawify-core/src/layout/friendliness/mod.rs` | `FriendlinessReport:33`、`Hotspot:58` |
| friendliness V2 调整器 | `crates/drawify-core/src/layout/friendliness/adjuster.rs` | `MomentumHistory:58`、`apply:96` |
| friendliness 拥堵 | `crates/drawify-core/src/layout/friendliness/congestion.rs` | 条带 bbox:140-153 |
| 标签避让 | `crates/drawify-core/src/layout/edge/common/label_avoidance.rs` | — |
| 路由 benchmark | `crates/drawify-eval/src/bin/edge_routing_bench.rs` | `RouterResult:38` |
| 布局质量指标 | `crates/drawify-eval/src/metrics.rs` | `edge_node_crossings:454`、`edge_crossings:499` |
| 图表默认路由 | `crates/drawify-core/src/profile/mod.rs` | `profile_for:223` |
| showcase 数据集 | `showcase/{architecture,er,flowchart,mindmap,sequence,state}/` | ~77 个 `.dfy` 文件 |

---

## 13. v1.0 → v2.0 变更摘要

| 维度 | v1.0 | v2.0 | v2.1 |
|------|------|------|------|
| G1 诊断 | 软约束，惩罚不够 | + 候选模板固定过少、混合端口无绕行候选、不用 ObstacleIndex | — |
| G2 诊断 | 缺乏全局协调 | 修正：根本不检测垂直交叉，仅检测平行重叠 | — |
| G3 诊断 | 只做锚点合并 | + overlap 惩罚拮抗中段趋同 | — |
| G4（新） | — | refine 锚点脱节 bug + 无 momentum + 只修 node-crossing | — |
| G5（原 G4） | 未消费 friendliness | 重新定性：friendliness 强耦合，应解耦为可插拔 | — |
| G7（新） | — | Dijkstra 确定性隐患 + simplify 不保护 stub | — |
| G8（新） | — | — | 端口选择缺乏同节点多边协调，导致不必要交叉 |
| P0-1 | 硬过滤 + 退化 | + 候选生成器重构 + 混合端口修复 + 障碍缝隙穿行 | — |
| P0-2 | 交叉预测评分 | + 修复垂直交叉检测 + 通道预约 + 空间索引 | — |
| P0-3（新） | — | — | 端口选择全局协调（两阶段：候选 + 同侧偏好投票） |
| P1-2（新） | — | refine 锚点脱节修复 + momentum + edge-overlap 感知（提前到阶段 1） | — |
| friendliness | P1-2 消费信号 | §6 独立章节，trait 化 + DSL 配置 + 可插拔 | — |
| 路线图 | P2-1 refine 协同在阶段 3 | refine 修复提前到阶段 1（bug 优先） | P0-3 加入阶段 1，P1-1 标注依赖 P0-3 |
| 不建议 | — | + spline 正交化后处理、friendliness 必需依赖 | — |

---

## 11. 复盘：文档与代码不符项及后续计划

> v2.3 复盘审计发现以下文档承诺与代码实现存在差异。按"需决策"和"后续可做"分类，供未来迭代参考。

### 11.1 文档承诺但代码未实现（需决策）

| # | 项目 | 文档位置 | 现状 | 决策建议 |
|---|------|----------|------|----------|
| D1 | **CandidateGenerator trait** | P0-1 | 文档承诺"引入 CandidateGenerator trait，将 build_candidate_paths + build_channel_detours 统一为可扩展的候选源"，代码仅有 CandidateScorer trait（评分抽象），候选生成仍是自由函数 | **建议更新文档**：CandidateScorer trait 已满足"可扩展评分"需求，CandidateGenerator trait 属于过度抽象。当前候选生成逻辑稳定，无需 trait 化。 |
| D2 | **障碍缝隙穿行** | P0-1 | 文档承诺"检测障碍列/行之间的缝隙，生成穿过缝隙的候选（替代固定 Z-ratio）"，代码仍用固定 Z-ratio `[0.25, 0.18, 0.32, 0.12, 0.4, 0.5, 0.6, 0.75]` | **建议后续实现**：缝隙穿行可显著改善密集图路由质量。实现方案：扫描障碍节点 bbox，按行/列聚类，检测间隙宽度 ≥ CHANNEL_MARGIN 的缝隙，生成穿过缝隙中点的候选。优先级中等。 |
| D3 | **ObstacleIndex 二级退化** | P0-1 | 文档承诺"若所有候选均被过滤，调用 ObstacleIndex::shortest_path 生成绕行折线"，代码退化为取最低惩罚脏候选（`best_clean.or(best_dirty)`） | **建议保持现状**：当前退化策略更简单且不依赖 ObstacleIndex，配合 refine 循环已能有效消除穿障。ObstacleIndex 退化会引入正交化后处理复杂度（参见 §7"不建议"）。 |
| D4 | **BBOX_EXPAND = 10.0 ≠ 文档 30px** | P0-2/P2-1 | 文档说"bbox 扩张 30px"，代码实际 `BBOX_EXPAND = 10.0` | **建议更新文档**：10px 已含 EDGE_PARALLEL_GAP(8px) + 2px 余量，足够预筛选。30px 过于保守，会增加不必要的精确检测。 |
| D5 | **group_gap 模块集成** | P1-3 | 文档承诺"Architecture 布局 hints 标注组间通道，路由时标记为优先通道/禁止穿越"，orthogonal 未消费 `friendliness/group_gap.rs` | **建议后续实现**：group_gap 可改善架构图跨组边路由。实现方案：RoutingContext 新增 `group_channels: Option<&GroupGapReport>`，候选生成时优先沿 group_gap 标注的通道。优先级低（当前分组边框障碍已避免穿组）。 |
| D6 | **组内/组外边分层路由** | P1-3 | 文档承诺"先路由组内边，再路由跨组边"，代码无分层逻辑，所有边按 edge_order 统一路由 | **建议暂不实现**：分层路由会增加复杂度且收益有限。当前 edge_order 按复杂度排序已足够。 |
| D7 | **性能目标全部未验证** | §10 | 文档 §10 列出 9 项成功指标，仅 2 项有测试覆盖（确定性 + 锚点脱节=0），其余 7 项（< 50ms、−30%、−50% 等）无 bench 断言 | **建议后续补充**：在 edge_routing_bench 中添加 baseline 对比模式，记录优化前后的 edge_node_crossings / edge_crossings / 路由耗时。优先级中等。 |

### 11.2 代码做了但文档未记录

| # | 项目 | 说明 |
|---|------|------|
| C1 | **CandidateScorer trait** | 代码引入了评分器 trait（允许注入自定义 scorer），文档 P0-1 只提到 CandidateGenerator trait，未记录 CandidateScorer |
| C2 | **NODE_NEAR_MISS_PENALTY 近距擦过惩罚** | 代码对水平段从节点正上方/下方近距离擦过施加 2500 惩罚，文档未记录此机制 |
| C3 | **side_acceptable 完整判定逻辑** | 代码实现了完整的 side_acceptable 函数（含方向检查、阈值检查、双重叠判定），文档 P0-3 只提到"复用 choose_pair_sides 的阈值逻辑" |
| C4 | **combined_crossing_score 权重 10:1** | 代码明确 node-crossings 权重 10、edge-overlaps 权重 1，文档 P1-2 只说"综合" |
| C5 | **端点并线三原则** | 代码实现了箭头类型/线型/出入方向三原则并线分组，文档 §12 关键代码索引未记录 |
| C6 | **sub_group_sort_key 不含 is_from** | 代码注释说明"同一 edge 在两端 is_from 相反，若用 is_from 排序会导致两端排名不一致"，文档未记录此设计决策 |

### 11.3 后续可做的优化计划

按优先级排序：

#### 优先级高

1. **性能 baseline 对比（D7）**
   - 在 `edge_routing_bench` 中添加 `--baseline` 模式，记录优化前后的 edge_node_crossings / edge_crossings / 路由耗时
   - 为 §10 的成功指标提供量化验证
   - 工作量：bench 工具扩展 + showcase 图表跑批

2. **障碍缝隙穿行（D2）**
   - 扫描障碍节点 bbox，按行/列聚类，检测间隙宽度 ≥ CHANNEL_MARGIN 的缝隙
   - 生成穿过缝隙中点的候选路径
   - 预期收益：密集图路由质量显著提升
   - 工作量：path.rs 新增 `build_gap_candidates` 函数 + 测试

#### 优先级中

3. **group_gap 集成（D5）**
   - RoutingContext 新增 `group_channels: Option<&GroupGapReport>`
   - 候选生成时优先沿 group_gap 标注的通道
   - 预期收益：架构图跨组边路由更优雅
   - 工作量：context.rs 扩展 + path.rs 通道偏好逻辑

4. **空间索引（R-tree/grid）**
   - `edge_overlap_penalty` 的 bbox 预筛选已将 O(E²) 降为近似 O(E·k)，但 `obstacle_penalty` 仍遍历所有节点
   - 对 100+ 节点图，可用 grid 索引加速障碍查询
   - 工作量：scoring.rs 新增 GridIndex + 障碍查询

5. **测试覆盖补全**
   - 空输入（0 边 / 0 节点）测试
   - 自环边（A→A）测试
   - 边引用不存在节点测试
   - refine max_passes=0 / enabled=false 测试
   - 工作量：8-10 个边界测试

#### 优先级低

6. **LayoutPostProcessor trait 化（§6.3）**
   - friendliness 阶段抽象为 trait，支持第三方后处理器
   - 当前 DSL 控制已满足解耦目标，trait 化属架构优化
   - 工作量：trait 定义 + 调度层集成

7. **RoutingContext.friendliness 可选消费（§6.4）**
   - orthogonal 路由可选消费 friendliness 报告的边级热点索引
   - 用 predicted_crossings 的 edge_indices 做软指导
   - 工作量：context.rs 扩展 + scoring.rs 软惩罚

8. **S1 统一 ObstacleIndex 构建**
   - 调度层按 `needs_obstacle_index()` 预建，orthogonal 覆写
   - 需要显著 trait 变更，收益有限
   - 工作量：trait 变更 + 调度层重构

### 11.4 代码质量收尾记录（v2.3 已完成）

以下代码质量问题已在 v2.3 复盘后修复：

| # | 问题 | 修复方式 |
|---|------|----------|
| Q1 | `EPS` 多处定义且值不一致 | refine.rs 局部 EPS 提取为模块级常量，加注释说明与 `label_avoidance::EPS`(1e-6) 语义不同 |
| Q2 | `SLOT_PITCH` 重复定义 | 提取到 `constants::ORTHO_SLOT_PITCH`，orthogonal 和 port_conflict 共享引用 |
| Q3 | `PARALLEL_GAP` 重复定义 | 提取到 `constants::ORTHO_PARALLEL_GAP`，orthogonal 和 refine 共享引用 |
| Q4 | `select_best_path` / `select_best_path_with_scorer` 仅测试使用 | 移除两个 dead code 函数，测试直接调用 `select_best_path_with_scorer_stats` |
| Q5 | refine.rs 公共函数可见性过宽 | `analyze_edge_node_crossings`/`analyze_edge_overlaps`/`analyze_crossings`/`push_problem_nodes`/`segment_intersects_aabb` 降级为 `pub(crate)`；`segment_intersects_node` 保持 `pub`（drawify-eval 跨 crate 引用） |
| Q6 | `BBOX_EXPAND` 隐藏在函数体内 | 提升为 scoring.rs 模块级常量 |
| Q7 | `best_metrics` 未使用赋值 | 移除 refine.rs 中 `best_metrics = new_metrics` 死赋值 |
