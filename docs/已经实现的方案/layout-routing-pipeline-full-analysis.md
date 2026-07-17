# 布局与路由算法 Pipeline 全面分析

> 本文档基于对 `crates/plotgram-core/src/layout/` 全部源码的逐文件分析整理，记录当前算法 pipeline、布局算法、路由算法及各子模块的完整逻辑。

---

## 目录

1. [整体架构与数据流](#1-整体架构与数据流)
2. [主 Pipeline 编排](#2-主-pipeline-编排layoutpipeliners)
3. [核心数据结构与 Trait](#3-核心数据结构与-trait)
4. [LayoutPlan 与 options 注入](#4-layoutplan-与-options-注入)
5. [架构图布局 architecture_v2](#5-架构图布局-architecture_v2)
6. [流程图布局 sugiyama_v2 + flowchart 分治](#6-流程图布局-sugiyama_v2--flowchart-分治)
7. [边路由入口与分派](#7-边路由入口与分派)
8. [正交路由 pipeline](#8-正交路由-pipeline)
9. [边后处理：bundling / lane_assignment / X-1 / X-2](#9-边后处理bundling--lane_assignment--x-1--x-2)
10. [refine 精修与 spline 兜底](#10-refine-精修与-spline-兜底)
11. [grid_snap 节点对齐与像素量化](#11-grid_snap-节点对齐与像素量化)
12. [group 模块与 group_frame 三层模型](#12-group-模块与-group_frame-三层模型)
13. [friendliness 友好性评估](#13-friendliness-友好性评估)
14. [intent 意图系统](#14-intent-意图系统)
15. [lint 质量检查](#15-lint-质量检查)
16. [关键设计要点与确定性保障](#16-关键设计要点与确定性保障)

---

## 1. 整体架构与数据流

### 1.1 调用链

```
compute_layout                                    // mod.rs:1023
  → compute_layout_with_plan                      // mod.rs:1034
    → compute_layout_with_plan_and_overlay        // mod.rs:1052
      → validate_layout_config                    // mod.rs:1203
      → pipeline::LayoutPipeline::new(...).run() // pipeline.rs:39
```

### 1.2 完整阶段顺序

从 `PreparedDiagram`（含已解析的 `LayoutPlan`）到 `LayoutResult` 的完整数据流：

| 阶段 | 函数 | 文件:行 | 作用 |
|---|---|---|---|
| 0 | `validate_layout_config` | mod.rs:1203 | 校验算法名/路由名/方向/交叉兼容 |
| 1 | `build_layout_strategy` | pipeline.rs:42 | 构建 strategy + 读 `node_align_config` + 应用 `align` 覆盖 |
| 2 | `validate_topology_intents` | pipeline.rs:57 | 校验拓扑意图（有 overlay 时） |
| 3 | `strategy.compute_with_overlay` | pipeline.rs:60 | **主布局**：节点坐标 + 分组包围框 + LayoutHints |
| 4 | `evaluate_topology_satisfaction` | pipeline.rs:64 | 评估拓扑意图满足度 → RefinementReport |
| 5 | `apply_geometric_refinement` | pipeline.rs:68 | 几何意图精修 → PinSet（有 overlay 时） |
| 6 | `grid_snap::align_nodes` | pipeline.rs:178 | **L3 节点对齐**：rank 轴同层对齐 + layer 轴重叠消除 |
| 7 | `GroupFramePass::apply_after_node_snap` | pipeline.rs:181 | **L1 组框**：组间排列/尺寸/对齐/量化 |
| 8 | `update_canvas_bounds` | pipeline.rs:182 | 重算画布尺寸 |
| — | (sequence 短路) | pipeline.rs:75-83 | produces_edges=true 时直接 finalize 返回 |
| 9 | `feedback.apply_pre_route` | pipeline.rs:197 | **friendliness V1 诊断 + 可选 V2 微调** |
| 10 | `resolve_effective_edge_routing` | pipeline.rs:201 | 解析有效边路由算法（显式优先，否则 hints 推荐） |
| 11 | `router.edge_snap_config` + 自适应 | pipeline.rs:212-217 | 构建 EdgeSnapConfig + `snap:false` 覆盖 + `adaptive_grid_step` |
| 12 | `gf_pass.refresh_before_route` | pipeline.rs:220 | 有 group 时刷新组框 |
| 13 | `feedback.complete_routing` | pipeline.rs:225 | **router.route + run_refine** |
| 14 | `edge_postprocess::repulse_edges_only` | pipeline.rs:234 | 路由后几何排斥（不含量化） |
| 15 | `check_alignment_after_refine` | pipeline.rs:242 | 几何对齐意图复核（有 overlay+pinned 时） |
| 16 | `run_post_route_group_frame` | pipeline.rs:246 | 组框幂等恢复 + 增量重路由 + repulse_edges_only |
| 17 | (architecture PRS) | pipeline.rs:248-315 | 壳层扩展 + sibling overlap 解决 + 增量重路由 + 内容包络 + repulse |
| 18 | `edge_postprocess::snap_and_repulse_edges` | pipeline.rs:318 | **末尾像素量化 + 边框排斥（仅一次）** |
| 19 | `postprocess::finalize_canvas_bounds` | pipeline.rs:90 | 全局包围框 + 平移居中 + 设 total_width/height |
| 20 | 返回 `(LayoutResult, Option<RefinementReport>)` | pipeline.rs:97 | overlay 非空时带报告 |

### 1.3 三层 Frame 模型（贯穿整个 pipeline）

```
L1  Group Frame（组间）   — 顶层/同级 group 的 track 几何    group_frame + group
L2  Intra Frame（组内）   — 单 group 内节点的排列模式          group_layout_hint / group_divide
L3  Node Frame（节点）   — rank/layer 对齐 + 像素量化          grid_snap
```

L2 组内节点排列由布局算法内部完成；L3 节点对齐在路由前执行（结构修正，影响路由输入）；L1 组框在 L3 之后、路由之前后多次幂等应用；边像素量化在管道最末尾执行一次。

---

## 2. 主 Pipeline 编排（layout/pipeline.rs）

`LayoutPipeline::run`（`pipeline.rs:39-98`）是中枢，核心子流程如下。

### 2.1 `apply_node_frame`（pipeline.rs:161-184）

1. 若 `node_align_config.enabled` 且方向非 `radial`，调 `grid_snap::align_nodes(result, config, horizontal, pinned)`（L3 节点对齐）。
2. `GroupFramePass::resolve(...).apply_after_node_snap(...)`（L1 组框：组间排列/尺寸/对齐/量化）。
3. `grid_snap::update_canvas_bounds`。

`horizontal = effective_dir == Some("left-to-right")`。

### 2.2 `run_routing_pipeline`（pipeline.rs:186-327）

路由阶段总控，内部顺序：

1. **预路由反馈**：`LayoutRouteFeedback::apply_pre_route` 执行 friendliness V1 诊断 + 可选 V2 节点微调。
2. **解析有效边路由算法**：用户显式配置优先，否则按 `hints.edge_routing_style` 推荐回退。
3. **构建 router + edge_snap_config**：`router.edge_snap_config()`；`snap: false` 可关闭；`grid_step = adaptive_grid_step(node_count)`（<20→4px / 20-50→8px / >50→16px）。
4. **组框 refresh**：有 group 时 `gf_pass.refresh_before_route`。
5. **路由 + refine**：`feedback.complete_routing(router, result, &refine_config)`。
6. **几何排斥（不含量化）**：`edge_postprocess::repulse_edges_only`。
7. **几何对齐意图复核**：有 overlay 且有 pinned 对齐时调 `intent::geometric::check_alignment_after_refine`。
8. **组框修复 + 增量重路由**：`run_post_route_group_frame`。
9. **architecture 专属 PRS 壳层扩展**：`post_route_shell_expand` → 必要时 `resolve_all_sibling_overlaps` → 检测移动节点 → `router.route_after_node_moves` → `expand_groups_to_contain_contents` → 再次 `repulse_edges_only`。
10. **末尾像素量化**：`edge_postprocess::snap_and_repulse_edges(&mut result.edges, &result.groups, &edge_snap_config)`。

### 2.3 `run_post_route_group_frame`（pipeline.rs:329-405）

1. `gf_pass.restore_after_node_moves`（幂等恢复 L1 组框）。
2. 比较节点位移 `max_node_disp`，≥1.0 时收集 `moved_nodes` 调 `router.route_after_node_moves` 增量重路由。
3. 无论是否重路由都调 `edge_postprocess::repulse_edges_only`（仅几何排斥，量化推迟到末尾）。
4. `grid_snap::update_canvas_bounds`。

---

## 3. 核心数据结构与 Trait

### 3.1 核心数据结构（layout/mod.rs）

| 结构 | 位置 | 说明 |
|---|---|---|
| `NodeLayout` | mod.rs:103 | `x, y, width, height` |
| `GroupLayout` | mod.rs:123 | 同上四元组 |
| `Port` | mod.rs:143 | `Top/Bottom/Left/Right` |
| `PathGeometry` | mod.rs:155 | `Straight \| Bezier \| Polyline`，分离"几何表达"与"渲染采样" |
| `EdgeLabelLayout` | mod.rs:302 | `text, center, size, leader_to, rotation`，`center` 为标签包围框几何中心 |
| `EdgeLayout` | mod.rs:346 | `geometry + labels + from_port + to_port` |
| `EdgeRoutingStyle` | mod.rs:488 | 布局算法对边路由风格的推荐枚举 |
| `LayoutHints` | mod.rs:506 | 布局阶段产出、供路由/refine/诊断读取的提示包 |
| `LayoutResult` | mod.rs:657 | `nodes: HashMap + groups + edges: Vec<EdgeLayout> + total_width/height + hints` |

### 3.2 `LayoutStrategy` trait（mod.rs:809-891）

- `fn compute(&self, diagram) -> LayoutResult` — 基础布局。
- `fn compute_with_overlay(&self, diagram, valid_topology)` — 默认委托 `compute`；SugiyamaV2/Flowchart/Er/ArchitectureV2 覆写以原生消费拓扑意图。
- `fn produces_edge_geometry(&self) -> bool` — `true` 时跳过通用边路由（当前仅 `sequence`）。
- `fn node_align_config(&self) -> NodeAlignConfig` — 声明节点结构对齐配置，默认禁用。

### 3.3 `EdgeRoutingStrategy` trait（mod.rs:899-980）

- `fn route(&self, diagram, result) -> LayoutResult`。
- `fn supports_refine(&self) -> bool` — refine 只处理 Polyline。
- `fn needs_obstacle_index(&self) -> bool` — 仅 spline 需要。
- `fn route_after_node_moves(&self, diagram, result, moved_node_ids)` — 增量重路由，默认全图重路由。
- `fn route_preserve(&self, diagram, result, preserve_edges)` — refine 增量重路由。
- `fn edge_snap_config(&self) -> EdgeSnapConfig` — 声明边量化配置，默认禁用。

---

## 4. LayoutPlan 与 options 注入

### 4.1 `LayoutPlan`（plan.rs:115-123）

```rust
pub struct LayoutPlan {
    pub layout_algo: String,
    pub layout_options: ResolvedAlgoOptions,
    pub edge_routing: String,
    pub edge_options: ResolvedAlgoOptions,
    pub friendliness: FriendlinessMode,
}
```

### 4.2 解析流程（plan.rs:127-159）

1. `resolve_algo_name(diagram, LAYOUT, profile.default_layout)` — DSL 显式配置优先，否则 profile 默认。
2. `layout_option_specs(&layout_algo)` — 从 strategy trait 查 spec 列表。
3. `ResolvedAlgoOptions::resolve(diagram, LAYOUT, algo, specs, profile.default_layout_options)`。
4. 同样流程解析 `edge_routing`。
5. `resolve_friendliness_mode(diagram)`：`off / diagnose / adjust` 三档，默认 `adjust`。

### 4.3 `ResolvedAlgoOptions`（plan.rs:54-112）

`{ values: HashMap<String, f64> }`。`resolve` 流程：取 DSL 配置块 options → `OptionsReader::new` → 对每个 spec 调 `read_spec_or_default`（DSL 值非法时回退 spec 默认）→ 再用 `profile_defaults` 补充。

### 4.4 `OptionsReader`（algorithm_config.rs:320-403）

按 `OptionKind`（`NonNegativeNumber / PositiveNumber / Number{min,max,exclude_min}`）校验。`warn_unknown_keys` 对未知 option key 发警告。

### 4.5 各算法 config（algorithm_config.rs）

| 算法 | option key（默认值） |
|---|---|
| sugiyama / sugiyama-v2 / flowchart / er | `group_padding`(28) |
| mindmap | `padding`(48) / `level_gap`(200) / `branch_gap`(70) / `node_gap`(22) / `center_gap`(100) |
| sequence | `group_padding`(20) / `node_spacing`(80) / `message_spacing`(50) |
| force-directed | `group_padding`(20) / `padding`(48) / `component_gap`(120) |
| architecture | `group_padding`(28) / `padding`(40) |
| circular | `group_padding`(20) / `padding`(48) / `component_gap`(40) |
| bezier / spline | `tension`(0.5) |
| orthogonal | `slot_pitch`(40) / `channel_margin`(18) |

### 4.6 边路由算法名解析（plan.rs:162-179）

`resolve_effective_edge_routing`：用户显式配置优先；否则按 `hints.edge_routing_style` 映射：`Orthogonal→orthogonal / Curved→circular / Straight→straight / Spline→spline / SelfLoop|Unspecified→plan.edge_routing`。

### 4.7 friendliness 模式（plan.rs:14-51）

- `Off`：跳过 V1+V2，零开销。
- `Diagnose`：仅 V1 评估，写 `hints.friendliness_report`，不调整布局。
- `Adjust`（默认）：V1 + V2。V2 可被环境变量 `PLOTGRAM_NO_V2_ADJUST=1` 禁用。

---

## 5. 架构图布局 architecture_v2

路径：`layout/node/architecture_v2/`。仅适用于 `DiagramType::Architecture`。

### 5.1 入口（layout/mod.rs:26-142）

`ArchitectureV2Layout { config }` 实现 `LayoutStrategy`。`compute_with_overlay` 阶段顺序：

1. 空图短路。
2. 节点尺寸 & 图索引 & 分组映射（`node_sizing::standard_node_sizes`、`GraphIndex::build`、`build_group_map`）。
3. **Phase 1 去环**：`acyclic::find_edges_to_reverse` 调用 `common::acyclic::greedy_fas`。
4. **拓扑意图注入**：`acyclic::inject_intent_edges`。**关键约束**：有顶层分组时，跨组意图边会被跳过（仅同组意图边才注入，`acyclic.rs:31-38`），且意图边永不反转。
5. **分支路由**：有顶层分组 → `two_phase::compute_two_phase_layout`；无顶层分组 → 全局 Sugiyama 管线。
6. 返回 LayoutResult，hints 携带 `edge_routing_style: Orthogonal` 和 `sugiyama_ranks`。

### 5.2 两阶段分治（two_phase.rs）

#### Phase A: 组内布局（递归）— `layout_intra_group_recursive`（two_phase.rs:391-492）

- **叶子组**：走 `layout_intra_group`。
- **容器组**：递归布局每个子组 → 直接实体作为"无组节点块"用 `layout_ungrouped_cluster` 布局 → 构建 `IntraMacroBlock` → 用 `build_super_graph_for_group` 构建容器组内超级图 → 宏观 rank + sibling 等宽等高 + sizing 策略 → `position_intra_macro_blocks` 定位 → `compose_intra_layout_recursive` 合并。

#### Phase B: 宏观超级节点分层 + 定位（two_phase.rs:138-165）

1. `build_super_graph`（two_phase.rs:1050-1107）：超级节点 = 顶层 group + 无组节点；超级边 = 跨越不同超级节点的有效边；产出 `super_edges` / `pair_edge_counts` / `edge_weights`。
2. `assign_super_macro_ranks`（rank.rs:149-229）：
   - **加权裁决双向对**（`resolve_bidirectional_pairs_by_weight`）：比较有向边权，反转权重小的方向；权重平局时 DSL 声明序更早的组视为上游。
   - **超级图 FAS 去环**：`common::acyclic::greedy_fas` 处理剩余长环。
   - **拓扑排序**（Kahn 算法，零入度节点排序后入队）。
   - **最长路径 rank**。
3. `build_macro_blocks`（含 intra_by_group）。
4. `apply_group_sizing_policy`（fit / uniform）。
5. `position_macro_blocks`（two_phase.rs:1272-1351）：按 macro rank 自上而下逐行布局，同 rank 内按 block id 排序后水平排列，**自适应间距**（有跨组边：`GROUP_GAP_X + min(edge_count × 6, 40)`；无跨组边：`GROUP_GAP_X × 0.5`），**自适应垂直层间距**（综合 rank 总跨组边密度与上下行组对最大边数）。`RowAlign::Start` 默认左对齐。

#### Phase C: 回填全局坐标 — `compose_global_layout`（two_phase.rs:1355-1403）

- group 块：组内节点坐标 = `block.x + padding.left + local.x`，`block.y + padding.top + local.y`。
- 无组块：节点坐标 = `block.x + local.x`，`block.y + local.y`。

#### Phase C+: 两阶段 spacing 微调（two_phase.rs:170-183）

`nudge_intra_nodes_toward_cross_group_edges`（two_phase.rs:1477-1663）是"先定组框再微调组内节点"的反转步骤：
- **P2.1 y 对齐**：同 macro rank 内跨组边两端节点 y 中心向中间值靠拢，上限 `CROSS_GROUP_Y_ALIGN_MAX = 20`。
- **x 微调**：跳过组内 hub；动态位移上限 = `min(available_width × 0.3, 48).max(16)`；按 desired_x 排序后强制保持 `NODE_GAP` 间距；最终 clamp 到组框内。

#### 后处理（two_phase.rs:185-279）

- `rebuild_layers_from_metadata`（two_phase.rs:1757-1803）：从 `macro_ranks + intra.layers` 元数据重建（旧版从 y 坐标反推易误合并）。
- `rebalance_infrastructure_layers` + `clamp_to_canvas`。
- **EGB（Edge Gutter Budget）**：`estimate_side_gutters_with_hierarchy` → `compute_group_bounds_with_side_gutters` → `merge_egb_groups` → `group_frame::resolve_all_sibling_overlaps` → `expand_groups_to_contain_contents`。
- `build_sibling_corridors + merge_corridors`（group routing hints）。
- 导出 `sugiyama_ranks` 供拓扑意图评估。

### 5.3 叶子组内布局（two_phase.rs:284-382）

1. `resolve_group_layout_hint` + `resolve_group_layout_mode`。
2. `assign_ranks_for_mode` 按 mode 分派（见 5.5）。
3. `build_layers + order_layers_group_aware`。
4. `assign_coordinates_intra`（two_phase.rs:926-1025）：局部原点，邻接拉力**仅限组内成员**，迭代 6 轮（全局 8 轮），`INTRA_LAYER_GAP = 56.0`（小于全局 `LAYER_GAP = 80.0`）。
5. hub 居中 + client 对齐。
6. Vertical 模式额外列对齐。
7. 过扁组回退 Grid（宽高比 < 0.25）。
8. 归一化到原点 + 计算 content_bbox。

### 5.4 group_layout_hint（group_layout_hint.rs）

**两层结构**：
- `GroupLayoutHint`（DSL 可写）：`Auto / Horizontal / Vertical / FanOut / FanIn / Grid`。
- `GroupLayoutMode`（解析后）：增加 `FanOut{hub}` / `FanIn{sink}` / `Sugiyama` 等携带参数的模式。

**解析优先级**：DSL 显式 `layout:` 属性优先 → 架构图子网命名启发式（`public_subnet`→Vertical，`data_subnet`→Grid）→ Auto。

**Auto 推断**（`detect_auto_mode`）：1 节点→Horizontal；fan-out 优先（max_fanout ≥ 2）；fan-in（max_fanin ≥ 2 且 max_fanout < 2）；无内部边 + 3+ 节点→Grid；简单链→Vertical；兜底→Sugiyama。

### 5.5 group_sizing（group_sizing.rs）

两种策略：
- `Fit`：组宽 = 组内内容 + padding（默认）。
- `Uniform`：所有顶层 group 拉齐到最宽者，组内内容水平居中。

`apply_equal_sibling_dimensions_per_rank`：**同 macro rank 内** sibling 块拉齐到最宽/最高，内容居中。确定性：rank 升序、块 id 升序迭代。

### 5.6 post_layout（post_layout.rs）

`center_single_group_rows`：**触发条件**：顶层 group 数量 = 1。按 y 聚类成行，单 group 行水平居中到画布宽度。多个顶层 group 保持左对齐。

### 5.7 architecture_v2 的 layout 子模块（Sugiyama 各阶段）

| 文件 | 职责 |
|---|---|
| `layout/acyclic.rs` | `find_edges_to_reverse` 委托 `common::acyclic::greedy_fas`；`inject_intent_edges` 有顶层分组时跨组意图边跳过 |
| `layout/rank.rs` | `assign_macro_group_ranks` 宏观+微观两层 rank 合并；`assign_super_macro_ranks` 加权双向对裁决 + 超级图 FAS + 拓扑序最长路径 |
| `layout/order.rs` | `order_layers_group_aware` 自适应 sweep 数 + 加权中位数 + 分组吸附 + 相邻交换；Fenwick Tree O(E log V) 交叉数 |
| `layout/coordinate.rs` | `assign_coordinates` BK 四趟分配 x 中心（复用 `sugiyama_v2::coordinate::assign_layer_centers_for_string_graph`）+ 组质心引力 + hub 居中 + client 对齐 |
| `layout/postprocess.rs` | `remove_node_overlaps`（力导向+BruteForce 串联）/ `clamp_to_canvas` / `resolve_group_overlaps` / `compute_total_size` |
| `layout/pipeline.rs` | 7 Phase pipeline（无分组场景）：OverlapRemoval → Clamp → NeighborAlignment → HubCentering → GroupBounds → GroupOverlap → GroupAlignment |

### 5.8 architecture_v2 关键常量（layout/constants.rs）

```
LAYER_GAP = 80.0          (宏观层间距)
INTRA_LAYER_GAP = 56.0    (组内层间距)
NODE_GAP = 48.0
GROUP_GAP_X = 50.0
MIN_GROUP_GAP = 8.0
CROSSING_SWEEPS_MIN/MAX = 4/16
COORDINATE_REFINE_ITERATIONS = 8 (全局) / 6 (组内)
NEIGHBOR_PULL_FACTOR = 0.4
GROUP_CENTER_PULL_FACTOR = 0.25
LONG_EDGE_BARYCENTER_WEIGHT = 1.8
NEIGHBOR_ALIGN_MAX_PASSES = 4
```

### 5.9 与 sugiyama_v2 的关键差异

| 维度 | architecture_v2 | sugiyama_v2 |
|------|-----------------|-------------|
| 适用图类型 | 仅 Architecture | Flowchart / State / Er |
| 分组处理 | group 一等公民，两阶段（组内→组间） | group 为后验包围盒 |
| 路由风格 | 固定 Orthogonal | 支持多种 |
| 去环 | 节点级 FAS + **超级图 FAS**（双层去环）+ 加权双向对裁决 | 仅节点级 FAS |
| rank 分配 | 宏观 rank + 微观 rank 合并 | network-simplex 风格 rank compaction |
| 坐标分配 | BK 四趟（复用 sugiyama_v2）+ 组内独立 6 轮 + hub/client 特化 | BK 四趟 |
| 拓扑意图 | 跨组意图边**跳过**（仅同组注入） | 全图注入 |
| 特化优化 | fan-out/fan-in 模式、hub 居中、client 对齐、跨组边端口微调、基础设施行居中、sibling 等宽等高、EGB gutter | 通用，无分组特化 |
| sizing 策略 | Fit / Uniform（图级属性）+ per-rank sibling 等宽等高 | 无 |
| layout hint | DSL `layout:` + 子网命名启发式 | 无 |
| 组间间距 | 自适应（按跨组边数，无边自动靠拢 0.5×） | 固定 |
| 嵌套分组 | 递归 `layout_intra_group_recursive` | 不支持 |

---

## 6. 流程图布局 sugiyama_v2 + flowchart 分治

### 6.1 整体架构

| 层级 | 路径 | 职责 |
|---|---|---|
| 门面层 | `layout/node/flowchart/mod.rs` | `FlowchartLayout` 策略入口，按是否有 group 分派 |
| 引擎层 | `layout/node/sugiyama_v2/` | Sugiyama 四阶段共享引擎（去环→分层→排序→坐标） |
| 公共层 | `layout/node/common/` | 跨算法共享工具（FAS、交叉数、分治框架、尺寸、包围框等） |

### 6.2 sugiyama_v2 四阶段（engine.rs `compute_with_preset_and_overlay`）

```
build_graph_with_overlay     (graph.rs:49)      阶段0：建图 + 意图边注入
greedy_cycle_reversal        (graph.rs:88)      阶段1：贪心 FAS 去环
build_dag                    (graph.rs:115)     阶段1.5：构造 DAG
assign_ranks_network_simplex_style (rank.rs:15) 阶段2：NS 紧边压缩分层
apply_*_rank_constraints     (engine.rs:318/349/255) 阶段2.5：语义/group rank 约束
build_proper_layer_graph     (graph.rs:158)     阶段3前置：长边 dummy 拆分
order_layers_weighted_median (order.rs:19)      阶段3：加权中位数 + 转置
assign_coordinates_brandes_koepf (coordinate.rs:10) 阶段4：BK 四趟坐标分配
compute_group_bounds / bounds_from_layout        后处理
```

#### 阶段 0+1：建图与去环（graph.rs）

- `build_graph_with_overlay`：构建 `DiGraph<String, EdgeMeta>`。真实边 `reversible=true`；意图边 `reversible=false`，`Below(A,B)` 注入 `B→A`，`Above(A,B)` 注入 `A→B`。
- `greedy_cycle_reversal`：**意图边保护**——构建 FAS 邻接表时过滤 `reversible=false` 的边，意图边永远不会被反转。调用 `acyclic::greedy_fas`。
- `build_proper_layer_graph`：**长边 dummy 链拆分**——对每条边 `(from, to)`，若 `to_rank > from_rank + 1`，在中间每个 rank 插入 `Dummy { source, target, segment }` 节点串联成链。

#### 阶段 2：NS-style 紧边压缩（rank.rs）

常量：`NS_MAX_ITERATIONS = 256`（安全阀）、`NS_NO_IMPROVEMENT_THRESHOLD = 5`（连续无改进早停）。

主循环（`assign_component_ranks_network_simplex`，rank.rs:61-179）：

```
longest_path_ranks          → 初始可行分层（最长路径）
build_feasible_tight_tree   → 构造初始紧边生成树
for _ in 0..256:
    root_tree + simplex_state_key + seen_states → 状态去重防振荡
    compute_all_cut_values          → 批量计算所有树边的 cut value (O(V+E))
    best_pivot_candidate_incremental → 选最佳 pivot 边对
    apply_pivot_shift               → 平移子树
    tree_edges.remove(leaving).insert(entering) → 换树边
    tree_is_connected ? : rebuild
    no_improvement_count >= 5 ? break
```

关键子函数：
- `longest_path_ranks`（rank.rs:193）：拓扑序遍历，`rank[succ] = max(rank[succ], rank[node]+1)`。
- `build_feasible_tight_tree`（rank.rs:265）：从根节点 `tight_component_nodes`（slack==0 的连通区域）出发，反复找 `minimum_cross_slack_edge`，平移使该边变紧，扩展树。
- `compute_all_cut_values`（rank.rs:548）：利用 `RootedTree.order` 的逆序自底向上合并子树成员，再对每条非根树边调用 `cut_value_for_subtree`（`incoming - outgoing`），整体 O(V+E)。
- `best_pivot_candidate_incremental`（rank.rs:624）：从预计算 `cut_values` 表读取，跳过 `cut_value == 0`（已最优）的边；`shift = cut_value < 0 ? 1 : -1`。
- `apply_pivot_shift`（rank.rs:737）：`shift > 0` 平移子树；`shift < 0` 优先下移子树，否则上移 complement 集合（保证 rank 非负）。
- `simplex_state_key`（rank.rs:796）：`(rank 序列, 排序后的树边索引序列)`，用于 `seen_states` 去重防振荡。

#### 阶段 2.5：语义 / Group rank 约束（engine.rs）

- `apply_state_semantic_rank_constraints`（engine.rs:318）：initial → rank 0，final → max rank。
- `apply_sink_rank_constraints`（engine.rs:349）：`type=end` 节点 → max rank。
- `apply_group_rank_constraints`（engine.rs:255）：**group 感知 rank 重分配**。按 `(min_rank, group_id)` 排序 group，为每个 group 分配不重叠的 rank 窗口（`window_size = max(orig_span, nodes.len())`，窗口间留 +1 空隙给 dummy 链）。

#### 阶段 3：加权中位数 + 转置（order.rs）

常量：`GROUP_BIAS_EPSILON = 1.0`、`ORDERING_SWEEP_MAX = 16`、`ORDERING_NO_IMPROVE_STOP = 2`。

`order_layers_weighted_median`（order.rs:19）：`max_sweeps = ordering_sweeps.clamp(1, 16)`，每轮前向扫 + 后向扫 + `transpose_adjacent`，连续 2 轮无改进早停。

**比较器 `compare_nodes_for_layer`**（order.rs:102）多键排序：
1. `median`（主键）
2. **group 偏置**：`|median_diff| < 1.0` 时同 group 节点优先相邻
3. `barycenter`
4. `degree`（高度数优先）
5. `is_dummy`（**dummy 优先**于真节点，使长边 dummy 链更易竖直对齐）
6. `spread`
7. `layer_node_sort_key`（Real 优先于 Dummy，按原始 index）
8. 原始位置（稳定排序兜底）

**`weighted_median_stats`**（order.rs:177）：
- **Eades-Sugiyama 偶数偏移**：奇数度取中位数；偶数度时 `Incoming` 取左中位数 `pos[degree/2 - 1]`，`Outgoing` 取右中位数 `pos[degree/2]`，避免偶数邻居中位数不确定导致抖动。
- **加权 barycenter**：dummy 邻居权重 = `long_edge_barycenter_weight`（FLOWCHART_PRESET 为 1.8），鼓励节点向长边 dummy 链对齐。

**`transpose_adjacent`**（order.rs:262）：交换优化，比较三个指标（字典序）：
1. `crossing_score_around`（Fenwick 树 O(E log V) 交叉数）
2. `alignment_penalty_around`（偏离 barycenter 惩罚，dummy 且 degree==1 时减半）
3. `long_edge_crossing_score`（跨层长边几何交叉估计）

交换条件：`after_cross < before_cross` OR (`cross 相等` AND `penalty 更小`) OR (`cross/penalty 都相等` AND `long_edge 更小`)。

#### 阶段 4：BK-style 四趟坐标分配（coordinate.rs）

`assign_coordinates_brandes_koepf`（coordinate.rs:10）：

```
spine = compute_spine_nodes(dag)                    // 识别主干节点
centers = assign_layer_centers_brandes_koepf(.., &spine)  // BK 四趟
compact_layer_centers(&mut centers, .., passes=2)   // 后续阻尼紧凑化
resolve_real_node_overlaps(..)                      // 逐层消除真节点重叠
normalize_layout_to_padding(..)
```

**四趟 BK**（coordinate.rs:169）：`down_left / down_right / up_left / up_right`，按"布局宽度最小（最紧凑）"选取最优趟，同宽时按 `down_left > down_right > up_left > up_right` 固定优先级（确定性）。

**单趟 `run_coordinate_pass_bk`**（coordinate.rs:215）：
1. `orient_layers`：按方向调整层序。
2. `detect_alignment_conflicts`：检测 type-0/1/2 冲突，标记 inner segment 交叉时非 inner 那条边为冲突边。
3. `vertical_alignment_blocks`：并查集构造垂直对齐块，`median_candidates_with_spine` 优先选 spine 上的候选，跳过冲突边、保持单调、避免环。
4. `horizontal_compaction`：块放置，前向扫 + 后向扫，受 `lower_bound`/`upper_bound` 约束。
5. `mirror_coordinates`：right-to-left 时镜像。

**`compact_layer_centers`**（coordinate.rs:426）：BK 四趟后的阻尼紧凑化。`DAMPING = 0.35`，每层节点向邻居（入+出，仅 Real）barycenter 靠拢，然后按 min_gap 强制最小间距。2 趟。

#### 密度感知间距（engine.rs）

- `apply_density_aware_spacing`（engine.rs:150）：统计最密集层的平均度数，超过 `DENSITY_DEGREE_THRESHOLD=2.0` 时放大 `node_gap`（每超 1.0 加 8px，上限 32px）。
- `compute_per_layer_gaps`（engine.rs:205）：逐层间统计跨越边数，每条跨层边贡献 `DENSITY_LAYER_GAP_SCALE=2.0` 像素（上限 40px），稀疏层不被无谓拉大。

### 6.3 flowchart 分治布局（group_divide.rs）

`FlowchartLayout.compute`（flowchart/mod.rs:69）：
```rust
if group_divide::should_divide(diagram) {
    return group_divide::divide_flowchart_with_groups(diagram, self.config);
}
engine::compute_with_preset(diagram, &preset::FLOWCHART_PRESET, self.config)
```

`should_divide`：`!diagram.groups.is_empty()`。注意：分治路径**不支持拓扑意图叠加**（有 group 时忽略 overlay）。

#### `divide_flowchart_with_groups`（group_divide.rs:447）

```
1. GroupTree::build(diagram)
2. 识别顶层 group + 无 group 节点（UNGROUPED_ID = "__ungrouped__"）
3. build_entity_to_top_group 构建跨组边映射
4. 每个顶层 group 调 layout_intra；无 group 节点作为 UNGROUPED 虚拟组
5. collect_cross_edges 收集跨 group 边
6. read_arrangement_config + StackingArrangement.arrange → 各 group 偏移
7. 合并全局坐标：intra 节点 + group 偏移
8. finalize_routing_groups + build_stacking_corridors + merge_corridors → 路由提示
9. 计算总尺寸
```

**组内布局 `FlowchartIntraGroupLayouter`**（group_divide.rs:121）：
- `build_sub_diagram`：过滤出 members 中的 entities 和内部边，**清空 group_id**（子图不再嵌套），继承外层方向属性。
- `layout_intra`：单节点直接返回；多节点调用 `engine::compute_with_preset(&sub_diagram, &FLOWCHART_PRESET, config)`；从 `sugiyama_ranks` 重建层结构。

**组间排列 `StackingArrangement`**（group_divide.rs:274）：
- `topological_sort_groups`（group_divide.rs:363）：Kahn 算法 + `BinaryHeap`（O(G log G)），入度 0 的 group 按声明顺序入队，环残留按声明顺序追加（确定性）。
- `Vertical` 模式：group 自上而下堆叠，水平方向按 `align`（Center 居中 / Left 左对齐）。
- `Horizontal` 模式：group 从左到右排列（泳道图）。

### 6.4 sugiyama_v2 preset（preset.rs）

| Preset | 关键差异 |
|---|---|
| `BASE` | 160×50, padding=40, layer_gap=84, node_gap=56, sweeps=16, weight=1.0 |
| `FLOWCHART_PRESET` | `long_edge_barycenter_weight=1.8`（鼓励长边 dummy 链竖直对齐） |
| `ER_PRESET` | layer_gap=96, node_gap=64, `NodeSizing::Er`, 有 finish 回调 |
| `STATE_PRESET` | layer_gap=72, node_gap=48, `NodeSizing::State`, weight=1.5 |
| `GENERIC_PRESET` | `NodeSizing::InferFromDiagram` |

### 6.5 common 公共模块

| 文件 | 职责 |
|---|---|
| `acyclic.rs` | `greedy_fas<N>` 泛型贪心反馈边集（剥 sinks/sources + 启发式选 out-in 最大节点） |
| `crossings.rs` | `FenwickTree` 树状数组 + `count_crossings_from_edges` O(E log V) 扫描线 |
| `divide_and_conquer.rs` | `IntraLayout` / `GroupTree` / `CrossGroupEdge` / `IntraGroupLayouter` / `GroupArrangement` trait |
| `group_bounds.rs` | `GroupPadding` / `SideGutter` / `compute_group_bounds`（按 depth 降序，先叶子后容器）/ `detect_group_layout_warnings` |
| `group_map.rs` | `build_node_to_top_group` 节点→顶层组映射 |
| `node_sizing.rs` | `NodeSizing` enum / `standard_node_size`（`width = clamp(unicode_width * 11 + 44, 96, 240)`） |
| `overlap.rs` | `OverlapResolver` trait / `ForceDirectedResolver` / `BruteForceResolver` / `ChainedResolver`（sugiyama_v2 自带 `resolve_real_node_overlaps`，不依赖本模块） |
| `pack.rs` | `pack_components`（Row / Shelf 二维装箱，多连通分量场景） |
| `edge_gutter.rs` | `estimate_side_gutters`（architecture 专用，LCA + 路径累积 + 主 0.7/次 0.3 权重） |
| `graph_index.rs` | `DirectedGraphIndex`（架构图用，过滤 Passive 边） |
| `barnes_hut.rs` | `BarnesHutTree` 四叉树（力导向专用，O(V log V)） |

---

## 7. 边路由入口与分派

### 7.1 分派核心（registry.rs:57-81）

```rust
let strategy: Box<dyn EdgeRoutingStrategy> = match algo {
    "straight"  => Box::new(edge_routing::StraightRouting),
    "bezier"    => Box::new(edge_routing_bezier::BezierRouting::from_options(...)),
    "spline"    => Box::new(edge_routing_spline::SplineRouting::from_options(...)),
    "circular"  => Box::new(edge_routing_circular::CircularRouting),
    "orthogonal"=> Box::new(edge_routing_orthogonal::OrthogonalRouting::from_options(...)),
    "organic"   => Box::new(edge_routing_organic::OrganicRouting::from_options(...)),
};
```

`EDGE_ROUTING_NAMES = &["straight", "bezier", "spline", "circular", "orthogonal", "organic"]`。

> **注意**：`edge/edge_routing.rs` 实际是 **StraightRouting**（仅用于 ER 图），并非主分派器。

### 7.2 visibility.rs（可见性图，供 spline/bezier/circular 用）

- `Obstacle`（膨胀矩形，padding=`DEFAULT_NODE_MARGIN`）。
- `ObstacleGrid`：64px 均匀网格空间索引。
- `ObstacleIndex::build`：全图只建一次，预计算所有角点两两的阻挡障碍物列表。
- `shortest_path`：先查直线可见 → 否则 Dijkstra（BinaryHeap，等距用 node 作稳定 tiebreak）。
- **正交路由不使用 visibility graph**，它用 `SegmentGrid`（在 `context.rs`）+ 候选路径打分。

---

## 8. 正交路由 pipeline

路径：`layout/edge/edge_routing_orthogonal/`。采用「固定磁吸点（slot）」方案。

### 8.1 主 pipeline 编排顺序（mod.rs:273-958 `route_edges_orthogonal_inner`）

#### 阶段 0: 预计算（mod.rs:281-313）
- `self_loop::self_loop_indices(relations)` — 标记自环边。
- `OrthoRoutingProfile::for_diagram_type(diagram_type)` — 选择 profile（flowchart/architecture）。
- `GroupRoutingContext::from_layout(...)` — 构建分组路由上下文。
- `PreparedObstacles::build(&result.nodes, &group_ctx)` — 预排序节点/分组 ID，避免路由循环内重复排序。
- `feedback_side::assign_feedback_sides(...)` — 回环边侧向通道分配。
- `corridor_route::plan_corridor_routes(...)` — 跨组走廊链 BFS 规划。

#### 阶段 1: 端口选择（mod.rs:315-374）
- 按 `undirected_pair_key` 将边分到无向节点对组。
- 每组调用 `choose_pair_sides_with_group(a_nl, b_nl, can_from, can_to, Some(&group_ctx))`（slot.rs:69）确定 `(side_a, side_b)`。
- **Step 1b**：`coordinate_port_sides(...)` 做"同侧偏好"全局协调，让少数派边在几何可接受时切换到多数派侧。
- `apply_feedback_side_overrides(...)` 再次应用 feedback 覆盖。

#### 阶段 2: Slot 分配（mod.rs:376-524）
- 按 `endpoint_bundling_key = (node_id, side, is_from, arrow_type, line_style)`（mod.rs:1559）构建并线子组。
- 聚合到 `side_groups: (node_id, side) → Vec<Vec<Endpoint>>`。
- 子组内沿切线方向排序；子组间按 `(arrow_tag, line_style, min_edge_index)` 排序——**不含 is_from**，保证同一 edge 在两端获得相同 `base_frac`。
- `choose_docking_strategy(count)`（slot.rs:32）选择汇流策略：
  - `Single`（1 条）：单点居中。
  - `Compact`（2-3 条）：紧凑分布（pitch 上限 16px）。
  - `Concentrate`（4+ 条）：共享中心入口点。
- 计算 `base_frac`：单子组居中 0.5；多子组用 `slot_fraction(group_rank, k, edge_len, pitch)`。
- 子组内每个端点：根据策略用 `slot_fraction` 或 `slot_fraction_around` 计算 `frac`，再用 `slot_anchor(nl, side, frac)`（slot.rs:365）得到锚点坐标。
- 写入 `endpoint_map: HashMap<(edge_index, is_from), Endpoint>`。

#### 阶段 2 后处理: 平行边偏移（mod.rs:526-573）
对 A↔B 正反向对应用切线偏移，使对称错开。

#### 阶段 3: 边序（mod.rs:588-596）
`layer_order::compute_edge_order`：有 Sugiyama rank 时按端点最小 rank 升序分批（低层先占通道），层内按连接度降序；无 rank 时退化为连接度降序。

#### 阶段 4: 逐边构建路径（mod.rs:613-726）
对 `edge_order` 中每条边：
- 自环边走 `self_loop::route_self_loop`。
- 保留边直接复用并加入 grid。
- 构建 `RoutingContext::new(...).with_strict_group_transit(corridor_plan.chains.contains_key(&i))`。
- **优先**调用 `validated_corridor_path(...)` 尝试走廊路径。
- 否则调用 `select_best_path_with_scorer_stats(...)` 走通用候选生成。
- `grid.insert_path(&path, i)` 注册到段网格。
- 用 `build_edge_labels` 计算标签位置。

#### 阶段 4b: Slot 重规划（mod.rs:735-749）
`replan_slots(...)`（mod.rs:1016-1258）— 按实际出口方向全局重排 slot。

#### 阶段 4c: 直连偏好对齐（mod.rs:754-838）
`straighten_preferred_alignments(...)` — 正对端口边的 slot 锚点对齐。找出 anchor 被修改的边，移除 grid 后重路由。

#### 阶段 4d: X-1 多轮冲突消解（mod.rs:842-857）
`reroute_conflicting_edges(...)` — 最多 `MAX_REROUTE_ROUNDS=3` 轮。

#### 阶段 4e: X-2 反向 stub 检测与端口翻转（mod.rs:868-883）
`fix_reverse_stub_ports(...)` — 检测反向 stub 与侧向接入，尝试翻转/旋转端口。

#### 阶段 4f: X-3 车道分配（mod.rs:889-941）
`assign_lanes(...)` — 对残余平行段通过 cross-axis 平移分离。

#### 阶段 5: 标签自动避让（mod.rs:950-952）
`resolve_label_overlaps_with_config(...)`。

### 8.2 GroupRoutingContext 预计算（group/context.rs:58-70）

```rust
pub struct GroupRoutingContext {
    pub groups: HashMap<String, GroupLayout>,
    pub node_to_groups: HashMap<String, Vec<String>>,
    pub border_shell_pad: f64,
    pub stub_clearance: f64,
    pub corridor_misalignment_penalty: f64,
    pub repulse_max_rounds: usize,
    pub corridors: Vec<GroupCorridor>,
    pub side_gutters: BTreeMap<String, SideGutter>,
    pub node_leaf_group: HashMap<String, String>,        // 节点→最深 leaf group
    pub sibling_sets: Vec<Vec<String>>,                  // 同父 leaf group 集合
    pub sibling_orientation: HashMap<(String, String), SiblingOrientation>,
    pub group_ancestors: HashMap<String, Vec<String>>,   // group→祖先链（含自身）
}
```

关键访问方法：`node_leaf_group` / `is_same_leaf_group` / `sibling_orientation` / `corridor_between_groups` / `endpoint_group_set`（含祖先链）/ `segment_violates_border_shell`。

### 8.3 磁吸点 Slot 分配算法

**`slot_fraction`**（slot.rs:329-338）— 实际实现用「固定间距 + 居中」策略，而非简单 `(rank+1)/(count+1)`：

```rust
pub fn slot_fraction(rank, count, edge_len, pitch) -> f64 {
    if count <= 1 { return 0.5; }
    let usable = edge_len * (1.0 - 2.0 * SLOT_MARGIN_RATIO);  // SLOT_MARGIN_RATIO = 0.12
    let span = (pitch * (count as f64 - 1.0)).min(usable);
    let pitch = span / (count as f64 - 1.0);
    let offset = (rank as f64 - (count as f64 - 1.0) / 2.0) * pitch;
    0.5 + offset / edge_len.max(EPS)
}
```

以 0.5 为中心，按 `pitch` 间距向两侧展开；边长不足时自动压缩到 `usable` 范围内。

`slot_fraction_around`（slot.rs:346）：围绕给定 `base_frac` 而非 0.5 展开，用于同一节点同一侧多个并线子组。

`slot_anchor(nl, side, frac)`（slot.rs:365-372）：frac 映射到节点边线坐标。

### 8.4 `replan_slots` 逻辑（mod.rs:1016-1258）

**目的**：slot 排序按对端节点中心坐标排列，但当实际路由方向与对端位置方向不一致时，排序会导致出边交叉。路由完成后按"实际出口方向"全局重排 slot。

**算法步骤**：
1. 按 `(node_id, side)` 分组所有端点。
2. 对每个 (node_id, side) 组（≥2 端点）：
   - `compute_effective_exit_dir` 获取有效出口方向（垂直端口返回水平位移，水平端口返回垂直位移，跳过 stub 段）。
   - 构建**锚点块**（AnchorBlock）：共享相同 tangent 坐标的端点为不可拆分单元（Concentrate 策略下 4+ 边共享锚点）。
   - 块内按 `sort_key`（=effective_dir）排序；块间按 `dir_key → center_tangent → min(edge_index)` 全序排序保证确定性。
   - 块按新顺序重新分配 tangent 值。
3. 收集所有 anchor 被修改的边到 `edges_to_reroute`。
4. 一次性 `grid.remove_by_edges` + 逐条重路由（`phase1_only=true`，跳过阶梯候选）。

### 8.5 候选路径生成与评分（path.rs）

`select_best_path_with_scorer_stats`（path.rs:314-455）采用**渐进式候选**策略：

**Level 0**: `build_candidate_paths`（path.rs:457-531）— 基础 L 形 + 混合端口扩展
- 若 `can_go_straight`（对端正对且共轴对齐）→ 直线 `[start, end]`。
- 否则计算 `start_stub`、`end_stub`（沿端口外延 `PORT_CLEARANCE=16`）。
- `compute_orthogonal_path_variants`（path.rs:1019）生成中间折点：混合端口 L 形、同轴反向端口 8 种比例 Z 形、同侧端口绕回。
- 混合端口时额外用 `[2.5, 4.0, 6.0]` 倍 stub 长度扩展候选。
- 若 `profile.prefer_trunk_fork`（flowchart）：额外评估 fork 候选（一侧 stub_len=0）。

**Level 1**: `build_channel_detours`（path.rs:926-960）— 侧通道绕行（仅同轴端口）
- 检测 travel band 内阻塞节点/分组，必须有 `blocking_in_corridor=true` 才生成候选。
- 通道坐标：默认距障碍 `margin`，若附近有分组边框则取"节点列边缘"和"分组边框"中点。
- 渐进 margin：`base_margin` → `EXTRA_CHANNEL_MARGINS=[28, 40]`，仅当 `best_strict` 为空时才尝试更高档。

**Level 2**: `build_obstacle_aware_z_folds`（path.rs:595-632）— 障碍物感知 Z 折点
- 在障碍物边界处生成折点，`generate_axis_folds` 收集 `collect_obstacle_boundaries_on_axis` + `group_gap_midpoints_on_axis` 的坐标。

**Level 3**: `build_staircase_candidates`（path.rs:656-752）— 阶梯路径（开销最高）
- 结合主轴折点和交叉轴通道，`FoldOrder::VerticalFirst` 和 `HorizontalFirst` 两种折叠顺序。
- 坐标经 `prepare_coords` 排序、去重、下采样到 `MAX_PER_AXIS=6`。

**评分机制**：`PathEvalState`（path.rs:218）维护三档候选：
- `best_strict` — 通过 `path_is_clean`（不穿节点）+ `path_avoids_group_interiors`（不穿组内部）。
- `best_nodes_only` — 仅通过 `path_is_clean`（不穿节点，但穿组）。
- `best_dirty` — 穿节点的退化候选。

最终选择优先级：`best_strict → (strict_group_transit ? None : best_nodes_only) → best_dirty → [start, end]`。

`DefaultScorer`（scoring.rs:43-75）评分公式：

```
score = path_length * w.path_length
      + (bends * BEND_PENALTY=16) * w.bend
      + obstacle_penalty * w.obstacle
      + edge_overlap_penalty
      + channel_load_penalty * w.channel_load    (仅 reroute 时)
      + corridor_misalignment_penalty * w.corridor_misalignment
```

`obstacle_penalty` 累加：`NODE_CROSSING_PENALTY=10000`（穿节点）、`NODE_NEAR_MISS_PENALTY=2500`（擦过节点）、`GROUP_TRANSIT_PENALTY=3000`（违反分组边框壳层）、`GROUP_NEAR_MISS_PENALTY=2000`（擦过分组边框）。

`edge_overlap_penalty` 用 `SegmentGrid` 空间索引加速，每条冲突段 `EDGE_OVERLAP_PENALTY=1200`。

### 8.6 路径简化（simplify.rs）

- `simplify_path`（simplify.rs:33）— 标准简化：去重 + 删除共线中间点。
- `simplify_path_preserving_stubs`（simplify.rs:6）— 保留首尾 stub 段：仅当 `len > 4` 时简化，且强制保留 `first_stub_index=1` 和 `last_stub_index=len-2`。
- `is_collinear(a, b, c)`（simplify.rs:55）— 叉积判定，阈值 0.1。

### 8.7 走廊路由（corridor_route.rs）

**计划阶段**：`plan_corridor_routes`（corridor_route.rs:38-94）
- 仅当 `group_ctx.corridors` 非空才执行。
- 对每条边：取 `from_g = node_leaf_group(from)`、`to_g = node_leaf_group(to)`，若 `from_g == to_g` 跳过。
- `find_corridor_chain`（corridor_route.rs:278-315）— BFS 最短链。
- `assign_merge_aware_lanes`（corridor_route.rs:97-153）按 merge policy 分配车道：
  - `semantic_merge=false`（flowchart）：每条边独占一个 lane。
  - `semantic_merge=true`（architecture）：仅 `edges_may_share_trunk=true` 的边可共用同一 lane。

**路径构建**：`try_build_corridor_path`（corridor_route.rs:174-276）三段式串联：源节点 → 源组边框 → 走廊车道 → 目标组边框 → 目标节点。
- `corridor_lane_coord` = `corridor.coord + (lane - (count-1)/2) * CORRIDOR_LANE_PITCH=18`。
- `corridor_sides`：Vertical 走廊 group_a 用 Right、group_b 用 Left；Horizontal 走廊 group_a 用 Bottom、group_b 用 Top。
- `append_stub_leg` + `ortho_connect` 串联正交路径点。
- 最后用 `simplify_path_preserving_stubs` 简化。

`validated_corridor_path`（mod.rs:961-1007）额外校验：`path_is_clean` + `path_avoids_group_interiors`，否则返回 None 由通用路由兜底。

### 8.8 feedback_side 侧选择（feedback_side.rs）

入口：`assign_feedback_sides`（feedback_side.rs:32-84），为 Greedy FAS 反转边分配侧向通道。

**4 步决策树**：

**Step 1: Hard constraints — 识别反转边**（feedback_side.rs:39, 86-120）
- `reversed_edge_indices` 调用 `greedy_fas` 计算反转边集合（节点按 id 排序保证确定性）。

**Step 2: Single candidate — 按 centroid 分桶**（feedback_side.rs:44-72）
- `graph_center` 计算图中线（所有节点中心的平均值）。
- 每条反转边计算 centroid，`centroid < graph_center` → left_bucket；否则 → right_bucket。
- 每个 bucket 按 `(rank_span, edge_index)` 排序。

**Step 3: 4 组关系类别 — balance_buckets**（feedback_side.rs:159-172）
- `MAX_SAME_SIDE_FEEDBACK=3`：同侧回环边超过 3 条时溢出到另一侧。

**Step 4: Exit validation — 分配 hint**（feedback_side.rs:79-83, 192-209）
- `detour_side(horizontal, low_side)`：TB 布局 low_side=true → Left，false → Right；LR 布局 low_side=true → Top，false → Bottom。
- `assign_bucket_hints`：bucket 内按 lane 序号分配，`same_side_ports(side)` 让 from_side=to_side=side（同侧绕行）。

### 8.9 Profile 区分（profile.rs）

| 字段 | flowchart | architecture |
|---|---|---|
| `parallel_gap` | `ORTHO_PARALLEL_GAP` | `ORTHO_PARALLEL_GAP_ARCHITECTURE`（更大） |
| `corridor_lane_offsets` | false | true |
| `separate_unrelated_trunks` | false | true |
| `semantic_merge` | false | true |
| `prefer_trunk_fork` | true | false |

- **flowchart**：`prefer_trunk_fork=true` 让候选生成额外评估 fork 候选（一侧 stub_len=0），适配 fan-out 形态；`semantic_merge=false` 让 corridor lane 分配时每条边独占 lane。
- **architecture**：启用 `corridor_lane_offsets` + `separate_unrelated_trunks`，对通用路由回退的走廊边应用 planned offset，并强制分离无关边对的重合 trunk；`semantic_merge=true` 允许同源 fan-out 共用 corridor lane；`parallel_gap` 更大（视觉上更宽松）。

### 8.10 关键常量速查（mod.rs）

```
PORT_CLEARANCE=16                 端口外延 stub 长度
SLOT_MARGIN_RATIO=0.12            slot 边界余量比例
NODE_CROSSING_PENALTY=10000       穿节点惩罚
EDGE_OVERLAP_PENALTY=1200         边段重叠惩罚
BEND_PENALTY=16                   每折点惩罚
STUB_GUARD_LENGTH=24              stub 段保护长度（X-1 硬检查豁免）
MAX_REROUTE_ROUNDS=3              X-1 最大重路由轮数
REROUTE_EXTRA_CHANNEL_MARGIN=40   重路由额外 channel margin
EPS=0.1                           坐标比较容差
REVERSE_STUB_THRESHOLD=24         反向 stub 判定阈值（X-2）
COMPACT_SLOT_PITCH=16             紧凑分布模式 slot 间距
CHANNEL_MARGIN=18（默认）          侧通道距障碍节点留白
```

---

## 9. 边后处理：bundling / lane_assignment / X-1 / X-2

### 9.1 Bundling（并线）— 逻辑内联在 mod.rs

#### 启用条件

通过 `OrthoRoutingProfile`（profile.rs:34-48）控制：
- `semantic_merge: bool` — architecture=true, flowchart=false。
- **注意**：endpoint bundling（slot 共享锚点）对所有图种都执行，`semantic_merge` 只影响 trunk 共享判定和 corridor lane 分配。

#### 并线分组键 `endpoint_bundling_key`（mod.rs:1559-1570）

```rust
fn endpoint_bundling_key(node_id, side, is_from, rel) -> String {
    format!("{node_id}|{side:?}|{is_from}|{}|{}",
        arrow_type_tag(&rel.arrow), edge_line_style_signature(rel))
}
```

三条并线原则（mod.rs:376-388 注释）：
1. **不同箭头类型不并线**（Active/Passive/Bidirectional）。
2. **不同线型不并线**（虚线/实线/dash pattern）。
3. **仅同源出边或同宿入边才并线**（OR 语义，`is_from` 区分）。

#### DockingStrategy（slot.rs）

- `Single`（1条）/ `Concentrate`（4+条扇形汇流）：所有边共享子组中心 `base_frac`，实现入口合并。
- `Compact`（2-3条）：围绕中心紧凑分布（pitch 上限 16px）。

#### 子组排序稳定性（mod.rs:475-484）

`sub_group_sort_key` 返回 `(arrow_tag, line_style, min_edge_index)`，**故意不含 is_from**：同一 edge 在 from 端 is_from=true、to 端 is_from=false，若纳入排序会导致两端排名不一致 → base_frac 不同 → 路径弯折。

#### 安全回退（mod.rs:669-689、961-1007）

`validated_corridor_path` 返回 `Option<Vec<Point>>`，候选路径必须同时通过 `path_is_clean` + `path_avoids_group_interiors`，任一失败返回 None，由 `.unwrap_or_else(|| select_best_path_with_scorer_stats(...))` 回退到通用候选路径打分。

### 9.2 edge_merge_policy 的 `edges_may_share_trunk`（edge_merge_policy.rs:88-104）

**segment-level 语义判定**（非几何 direction 判定）：

```rust
pub fn edges_may_share_trunk(e1, e2, diagram_type) -> bool {
    if !requires_semantic_merge(diagram_type) { return true; }  // 非architecture直接true
    if e1.edge_index == e2.edge_index { return true; }
    let g1 = merge_groups_for_edge(e1);
    let g2 = merge_groups_for_edge(e2);
    g1.iter()
        .filter(|k| !matches!(k, MergeGroup::SuperEdgePair { .. }))  // SuperEdgePair被排除
        .any(|k| g2.contains(k))
}
```

- `requires_semantic_merge`：**仅 Architecture 启用**；flowchart 等几何优先，直接返回 true。
- `MergeGroup`：`SameSourceFanOut` / `SameTargetFanIn` / `ParallelPair`（canonical_pair 规范化）/ `SuperEdgePair`（leaf group 对）。
- **`SuperEdgePair` 不参与 trunk 合并**，仅用于 corridor lane 相邻分配。

### 9.3 X-1 多轮冲突消解（mod.rs:1269-1484）

`reroute_conflicting_edges` — `MAX_REROUTE_ROUNDS = 3` 轮：

- 每轮检测所有边的间距违规（`path_edge_spacing_violations`，已豁免 stub 段 ≤24px）。
- 按违规数降序排列冲突边。
- 递增 margin 档位 `[channel_margin+10, +25, +40]`（mod.rs:1294-1298）。
- 新路径必须通过 `path_is_clean` + `path_avoids_group_interiors` + `path_is_clean_from_edges` 三重硬检查。
- 找不到干净路径 → 恢复原路径并标记 `failed_edges`（优雅降级）。
- 每轮构建 `ChannelLoadMap` 让 scorer 偏好低负载通道。

### 9.4 X-2 反向 stub 端口翻转（mod.rs:2312-2605）

`fix_reverse_stub_ports` — 检测反向 stub（`has_reverse_stub` 两种情况：直接反向 / U型折返）与侧向接入（`detect_side_approach`），尝试翻转/旋转端口。

`REVERSE_STUB_THRESHOLD=24` 为反向 stub 判定阈值。

### 9.5 X-3 车道分配（lane_assignment.rs）

#### 主入口 `assign_lanes`（lane_assignment.rs:233-408）

**Step 1**（250-285）：收集所有 interior 段（`1 ≤ si ≤ n_segs-2`，保护端口锚点）。

**Step 2**（292-321）：O(N²) 检测冲突对（同方向 + layer 差 < min_gap + 投影重叠），Union-Find 分组。

**Step 3**（323-394）：BTreeMap 保证组遍历顺序确定；每组按 `(is_positive, ei, si)` 排序（Negative 在前），对称偏移：
```rust
let offset = (pos as f64 - (n_group as f64 - 1.0) / 2.0) * min_gap;  // 中心对称
```

**Step 4 - 三重验证 `validate_shift`**（lane_assignment.rs:142-191）：
- a. **邻段不反转**：方向符号对比偏移前后 si-1/si+1 方向符号不反号。
- b. **邻段不退化**：长度 ≥ `MIN_ADJACENT_LEN = 4.0`。
- c. **无节点穿透**：`segment_hits_node` 检查 si-1/si/si+1 均不穿节点。

**Step 5**：重建 SegmentGrid。

#### cross-axis 偏移（"中段修改"）

```rust
if seg.is_horizontal {
    new_points[seg.si].y += offset;       // H段移y
    new_points[seg.si + 1].y += offset;
} else {
    new_points[seg.si].x += offset;       // V段移x
    new_points[seg.si + 1].x += offset;
}
```

被移段仍为 H/V，相邻段仅长度变化，自动保持正交性，**无需 Z 字弯补偿**。

#### 其他相关后处理

- `apply_corridor_planned_offsets`（lane_assignment.rs:411-491）：对通用路由回退的走廊边，按 plan 的 cross-axis offset 平移最长干线 interior 段。
- `separate_unrelated_trunk_overlaps`（lane_assignment.rs:494-574）：对 `edges_may_share_trunk == false` 的边对，强制分离仍重合的干线，尝试 `[min_gap, +6, 18, 24, 36, 48]` × `[+/-]` 多档位偏移。

---

## 10. refine 精修与 spline 兜底

路径：`layout/refine/`。在节点布局+边路由完成后，检测折线边穿过非端点节点的情形，局部推开问题节点并重新路由。

### 10.1 配置（refine/mod.rs:24-41）

```rust
pub struct RefineConfig {
    pub enabled: bool,       // 默认 true
    pub max_passes: usize,   // 默认 3
    pub push_distance: f64,  // 默认 40px
    pub node_shrink: f64,    // 默认 2px（穿障检测时节点 AABB 内缩）
}
```

综合评分 `combined_crossing_score` = `edge_node_crossings × 10 + edge_overlaps`。

### 10.2 主流程 `run_refine`（refine/mod.rs:65-151）

```
1. analyze_crossings → best_score;若 edge_node_crossings==0 直接返回
2. 循环 max_passes:
   a. analyze_crossings;若 0 则 break
   b. 收集 edges_to_reroute(问题节点涉及的边 + 端点为问题节点的边)
   c. push_problem_nodes(推节点)
   d. reroute_subset(增量重路由受影响边)
   e. analyze_crossings → new_score
   f. new_score < best_score → 接受并更新 best;否则回退 break
3. 写入 RefineDebugStats
4. P2-2 兜底:最终仍有穿障的边 → spline_fallback 可见性图绕障
```

### 10.3 子模块

| 文件 | 职责 |
|---|---|
| `crossing.rs` | `analyze_edge_node_crossings` 遍历每条 Polyline 边的每段，`segment_intersects_aabb` 检测穿过非端点节点；`accumulate_push` 累积法线推力 |
| `geometry.rs` | `segment_intersects_aabb`（委托 `rect.intersects_segment`）/ `segment_intersects_node`（含 0.5px 容差） |
| `overlap.rs` | `analyze_edge_overlaps` 仅检测 Polyline 边，`SegmentSpatialIndex`（64px 网格，O(S²)→O(S·k)）；`segments_conflict_xy` 检测平行重叠或垂直交叉 |
| `push.rs` | `MomentumHistory` 记录节点上一轮方向，方向反转时衰减为 0.5 倍；`push_problem_nodes` 按 id 排序（确定性）沿累积推力方向位移 |
| `reroute.rs` | `reroute_subset` 只重路由受影响边，`MIN_PRESERVE_RATIO=0.15` 低于此阈值回退全量 `router.route`，否则 `router.route_preserve` |
| `spline_fallback.rs` | `reroute_edges_with_spline` 对仍穿障的边降级为 spline 可见性图绕障；构建 `ObstacleIndex` + `shortest_path`；无绕障路径时退化为 Bezier |

---

## 11. grid_snap 节点对齐与像素量化

路径：`layout/grid_snap.rs`。模块头注释（grid_snap.rs:1-10）明确职责拆分。

### 11.1 NodeAlignConfig（节点结构对齐，路由前执行）

```rust
pub struct NodeAlignConfig {
    pub enabled: bool,
    pub rank_axis: bool,           // 流向轴同层对齐
    pub layer_axis: LayerAxisAlign, // 垂直流向分布修正
    pub node_gap: f64,
    pub max_snap_distance: f64,
    pub layer_tolerance: f64,
    pub padding: f64,
}
```

预设工厂：
- `default_sugiyama`：`rank_axis=true, layer_axis=OverlapOnly, node_gap=56`。
- `default_flowchart`：同 sugiyama。
- `default_er`：`layer_axis=Off`（保留 Sugiyama 原始分布）。
- `default_architecture`：`node_gap=48, layer_axis=Centroid`。

`DiagramAlignOverride`：`Off / Default / RankOnly / LayerOnly / Full`，由 diagram 顶层 `align` 属性控制。

### 11.2 `align_nodes`（grid_snap.rs:285-314）

1. `cluster_by_rank_axis`：按 rank 轴中心聚类同层节点（容差 `layer_tolerance`）。
2. 若 `rank_axis` 开启：`snap_rank_axis_centers` — 取层内中位数中心，位移 ≤ `max_snap_distance` 才 snap；`pinned.is_rank_pinned` 的跳过。
3. 若 `layer_axis != Off`：`apply_layer_axis_align` — `OverlapOnly` 仅消除重叠，`Centroid` 均匀化至 `node_gap`；**保持层重心不变**，`pinned.is_layer_pinned` 的跳过。

### 11.3 EdgeSnapConfig（边像素量化，路由末尾执行）

```rust
pub struct EdgeSnapConfig {
    pub enabled: bool,
    pub grid_step: f64,
    pub shell_pad: f64,
    pub stub_clearance: f64,
    pub repulse_max_rounds: usize,
}
```

`default_orthogonal`：`grid_step=8, shell_pad=GROUP_BORDER_SHELL_PAD, stub_clearance=PORT_STUB_CLEARANCE, repulse_max_rounds=2`。

### 11.4 `snap_edge_waypoints`（grid_snap.rs:324-360）

对每条 Polyline 边：
1. 跳过 Bezier 和 `path_len <= 2`。
2. `snap_edge_path_channels(points, grid_step)`（grid_snap.rs:424-534）三阶段量化：
   - **Phase A**（433-461）：通道段（竖线 snap x、横线 snap y），用 `snap_channel_value` — 端点锚定 + minimax 位移最小化。
   - **Phase B**（463-497）：protected 段邻接处理，只 snap 可动端的主轴坐标。
   - **Phase C**（499-531）：内部拐角重对齐。
3. `group::project_path_off_group_borders_with_stub`（壳层投影）。
4. `simplify_polyline_path_preserving_stubs` — 用放大容差 `POST_QUANTIZE_SIMPLIFY_EPS=2.0` 消除量化微小折点，保护 stub 索引。

### 11.5 关键设计要点

- **P1 量化延后**：像素量化（`snap_and_repulse_edges`）只在管道末尾执行一次；中间所有路由后/组框修复后只做 `repulse_edges_only`（纯几何排斥）。
- **NodeAlignConfig / EdgeSnapConfig 拆分**：节点对齐是结构修正（改坐标、影响路由输入，路由前执行，`align` 属性控制）；边量化是视觉优化（不改拓扑，路由后执行，`snap` 属性控制）。两者独立开关、独立配置。
- **自适应 grid_step**：`adaptive_grid_step`：`<20 节点→4px / 20-50→8px / >50→16px`。
- **protected_path_indices**（grid_snap.rs:373-382）：保护 `path[0]`、`path[last]`（磁吸锚点）和 `path[1]`、`path[last-1]`（stub clearance）。

### 11.6 edge_postprocess.rs

两个独立函数：
- `repulse_edges_only(edges, groups, config)`：仅执行 `group::repulse_edges_from_group_borders`（几何投影推开贴边路径到合法通道）。**不含量化**。在路由后、组框修复后调用（多次）。
- `snap_and_repulse_edges(edges, groups, config)`：`grid_snap::snap_edge_waypoints`（像素量化）+ `group::repulse_edges_from_group_borders`。**在管道最末尾执行，仅一次**。

---

## 12. group 模块与 group_frame 三层模型

### 12.1 group_frame 三层模型（group_frame/mod.rs:8-16）

```
L1  Group Frame（组间）   — 顶层/同级 group 的 track 几何   ← 本模块
L2  Intra Frame（组内）   — 单 group 内节点的排列模式         group_layout_hint
L3  Node Frame（节点）   — rank/layer 对齐 + 像素量化        grid_snap
```

### 12.2 `GroupFrameSpec`（group_frame/mod.rs:56-77）

```rust
pub struct GroupFrameSpec {
    pub arrangement: GroupArrangement,   // Stack { axis } / Matrix { rows, cols }
    pub track_sizing: TrackSizing,       // Fit / Equal / Fixed(f64)
    pub cross_align: CrossAlign,         // Start / Center / End / Stretch
    pub gap: f64,
    pub padding: GroupPadding,
    pub border_align: BorderAlign,       // None / SharedLines
    pub quantize: QuantizeSpec,          // { enabled, step=8.0, quantize_groups }
}
```

`resolve_group_frame_spec`（mod.rs:172-183）：优先消费 `group_frame:` 配置块，否则按算法默认：
- architecture：`Stack(H) + Fit + Start + gap=50 + SharedLines`（`group_sizing: uniform` 时 Equal）。
- flowchart/通用：`Stack(V) + Fit + Center + gap=60 + None`。

### 12.3 `apply_group_frame`（group_frame/mod.rs:465-571）L1 整形主入口

- **嵌套 sub-frame**：`collect_sibling_sets` BFS 自顶向下，同一 Spec 应用于所有层级。
- 步骤1 arrangement：Matrix 二维网格 / Stack（cross_align Start + track_sizing Equal）。
- 步骤2 border_align SharedLines：只改框不动节点，聚类阈值=step，簇内取中位数。
- 步骤3 quantize groups：floor 原点 / ceil 远端，只改框不动节点。
- 步骤4 `resolve_sibling_overlaps` 安全网：检测 recompute_group_bounds + quantize 导致的 sibling 重叠，同步平移组内节点与嵌套 group 框。

**节点联动分级**：
- track_sizing / cross_align / Matrix → **必须同步平移组内节点**（跳过 PinSet）。
- border_align / quantize 微调（≤1 step）→ **只改 GroupLayout**。

### 12.4 `GroupFramePass`（group_frame/pass.rs）

```rust
pub struct GroupFramePass { pub spec: GroupFrameSpec, pub padding: GroupPadding }
```

- `resolve(diagram, plan, algo)`：从 diagram + plan 解析。
- `apply_after_node_snap`：L3 snap 后 → recompute_group_bounds + apply_group_frame + architecture 单行居中。
- `refresh_before_route`：路由前同步权威 group rect（debug 断言 `routing_groups_contain_members`）。
- `restore_after_node_moves`：V2/refine 推开后 → recompute + `realign_group_rows` + apply_group_frame + 居中。

### 12.5 `realign_group_rows`（group_frame/realign.rs）

V2/refine 后 `recompute_group_bounds` 从 min_node_y 重算 group y，若同 rank group 内节点被推开幅度不同会导致 y 不一致。算法：按 recompute 前的 y 聚类（1.0px 容差）→ recompute 后统一对齐到 rank 内最小 recomputed y，保留各 group 的 bottom 不缩。按 group id 字典序处理（确定性）。

### 12.6 group 模块

#### `group/context.rs` — GroupRoutingContext

见 8.2 节。`from_layout`：`GroupRoutingProfile::for_algo` 选参数 → `build_node_to_groups` → 合并注入走廊（`merge_corridors`，注入优先）与几何 fallback → `build_group_hierarchy` 填充 leaf/sibling/ancestor。

#### `group/corridor.rs` — 走廊

**`GroupCorridor`**（corridor.rs:19-27）：`{ axis: CorridorAxis, coord, span_min, span_max, group_a, group_b }`。竖走廊 coord=x，横走廊 coord=y。

构建函数：
- `build_corridors_from_groups`：O(N²) 对所有组对，检测 y_overlap+x_gap 或 x_overlap+y_gap，取中线。
- `build_sibling_corridors`：同父 sibling 组，仅在相邻对导出走廊。
- `build_stacking_corridors`：流程图堆叠排列，按拓扑序在 adjacent group 间导出走廊。
- `merge_corridors`：注入优先，补全未覆盖邻接对。

辅助：`prefer_corridor_coord`（找最近走廊坐标）、`corridor_misalignment_penalty`（路径段未对齐走廊 ALIGN_EPS=6.0 时加软惩罚）。

#### `group/border_shell.rs` — Border Shell 几何判定

**`SegmentGroupRelation`**：`Free / Interior / Crossing / Transit`。

关键函数：
- `segment_within_port_stub_zone`：从 path[0] 累计到 segment_index 的长度 ≤ stub_clearance 则豁免。
- `segment_intersects_group_shell`：段与 pad 膨胀后的壳层相交。
- `segment_hugs_group_border`：贴边平行（段坐标距 group 边 < pad）。
- `group_segment_violates_border_shell`：**硬违规 = 贴边平行（非 stub 区）或 Transit/Interior/Crossing（endpoint_in_group 不满足）**。

#### `group/post_route.rs` — snap 后投影

- `project_path_off_group_borders_with_stub`：遍历路径段，跳过 stub 区段，将贴边坐标投影到 `gl ± pad` 的合法格点。
- `repulse_edges_from_group_borders`：多轮（max_rounds）调用 project 直到稳定。

#### `group/post_route_shell.rs` — PRS 路由后单次壳层外扩

`post_route_shell_expand`：路由+标签完成后，若几何越出 group border shell，**向外扩壳一次（不重路由）**。`PRS_MAX_PER_SIDE = 48.0`（单侧最大补扩）。`scan_shell_overflow` 遍历所有边的路径段 + 标签 bbox，对 relevant_groups（端点 leaf group + 祖先）累计溢出量，取 max 并 cap 到 48。`grow_border_outward` 按 GutterSide 外扩，只改 GroupLayout 不动节点。

#### `group/hierarchy.rs` — 分组层级索引

**`GroupHierarchy`**：`node_leaf_group` / `parent_of` / `group_ancestors` / `sibling_sets` / `sibling_orientation`。

`build_group_hierarchy`：
- `node_leaf_group`：节点 → 最深 depth 的所属 group（leaf）。
- `sibling_sets`：同父且 ≥2 子的集合。
- `sibling_orientation`：基于 ox/oy overlap 与 `GROUP_GAP_THRESHOLD=48.0` 判断 Horizontal/Vertical，fallback 用中心点 dx/dy。

#### `group/rect.rs` — 路由权威 group rect

- `routing_group_padding(algo, group_padding)`：architecture→`GroupPadding::architecture_v2()`，force-directed→`force_directed()`，其他→`uniform(group_padding, 16.0)`。
- `finalize_routing_groups`：委托 `group_bounds::compute_group_bounds`。
- `debug_assert_routing_groups_contain_members`（debug 构建）：成员节点应在 group rect 内（MEMBER_EPS=2.0）。

#### `group/config.rs` — GroupRoutingProfile

`routing_algo_for_diagram`：Architecture→"architecture"，Flowchart→"flowchart"，其他→"sugiyama-v2"。

参数：`border_shell_pad`（=12）、`stub_clearance`（=16）、`corridor_misalignment_penalty`（architecture=120, flowchart=80）、`repulse_max_rounds`（=2）。

#### `group/constants.rs`

`GROUP_BORDER_SHELL_PAD = 12.0` / `PORT_STUB_CLEARANCE = 16.0`（与 orthogonal `PORT_CLEARANCE` 对齐）/ `EPS = 0.1`。编译期断言 `PORT_STUB_CLEARANCE == 16.0`。

---

## 13. friendliness 友好性评估

路径：`layout/friendliness/`。在**节点布局完成后、边路由前**基于拓扑与位置快速预测布局对路由是否友好。

### 13.1 V1 评估（mod.rs）

`FriendlinessReport`：复合分数 + 五维子分数 + 热点列表。分数越低越友好，0=完美（可为负）。

**五维子度量**：

| 维度 | 文件 | 评估目标 |
|---|---|---|
| congestion | congestion.rs | 正交通道占用度（节点行/列间隙被多少边跨越的峰值） |
| long_edge | long_edge.rs | 长边跨层度（Sugiyama 专属，`|rank差| > 1` 的边） |
| group_gap | group_gap.rs | group 间距充裕度（跨 group 边所需通道宽度 vs 实际间距） |
| predicted_crossings | crossing_predict.rs | 穿障预测度（直线连接穿过非端点节点次数，**最强单维预测器** r=0.62） |
| port_conflict | port_conflict.rs | 端口冲突度（同侧多边汇入 slot 容量） |

**复合分数**：对各子分数做 z-score 归一化（`CalibrationParams::zscore`），加权求和。z-score 可为负（比均值更友好），0=该族平均水平。

**权重预设**：
- `for_hierarchical()`：predicted_crossings 主导（r=0.62），权重 0.66。
- `for_force_directed()`：port_conflict 最强（r=0.69），权重 0.36。
- `for_radial()`：predicted_crossings + congestion 强。

### 13.2 V2 adjuster（adjuster.rs）

V2 反馈模式：在 V1 评估后、正式路由前，对预测穿障热点做局部节点位移。

**配置**：`max_passes=5`、`push_distance=80px`、`momentum_damping=0.5`、`min_crossings_to_adjust=2`。

**主流程**：
1. `compute_crossing_details`：对每条边检测直线穿过的非端点节点。
2. 若总穿障数 `< min_crossings_to_adjust` 直接返回（低穿障由路由器自行绕行）。
3. 迭代最多 max_passes：
   - `push_crossed_nodes`：对穿障节点沿边法线方向推送。
   - 重新评估；**重叠守卫**：`overlap_pairs` 检测是否引入新重叠对。
   - 若穿障减少且无新重叠→接受；否则回退停止。

**momentum 阻尼**：记录节点历史位移，若当前推送方向与历史位移方向相反（dot<0），按 `1 - momentum_damping` 衰减，抑制振荡。

---

## 14. intent 意图系统

路径：`layout/intent/`。布局算法之上的可选修正层，允许调用方在不修改 DSL 前提下对自动布局做局部调整。**作为独立参数透传，不变异 Diagram，保持 `relations[i] ↔ edges[i]` 索引契约**。

### 14.1 两类意图

```rust
pub enum TopologyIntent {
    Below { from: String, to: String },  // from 应在 to 下游(rank 更大) → 注入 B→A
    Above { from: String, to: String },  // from 应在 to 上游(rank 更小) → 注入 A→B
}

pub enum GeometricIntent {
    Pin { node: String, axis: PinAxis },          // 固定当前坐标,跳过 snap
    AlignVertical { nodes: Vec<String> },         // x 中心一致
    AlignHorizontal { nodes: Vec<String> },       // y 中心一致
}
```

`PinSet`：记录被 Pin/Align 保护的节点，供 grid snap 跳过。分 5 个集合：`full` / `x_only` / `y_only` / `aligned_vertical` / `aligned_horizontal`。

### 14.2 拓扑意图校验（topology.rs）

`validate_topology_intents`（topology.rs:52-164）：在注入意图边前做校验。

校验流程（逐条意图）：
1. **节点存在性**：`from`/`to` 必须在 `diagram.entities`，否则 `NotFound`。
2. **自环检查**：`Below(A,A)` → `Conflicted`。
3. **矛盾去重**：同方向重复或反方向冲突 → 后声明者 `Conflicted`，保留先声明者。
4. **环检测**：`has_path`（DFS）检查注入边后是否产生环 → `Conflicted`。

**FAS 保护**：真实边成环时 FAS 仅反转真实边破环，意图边保留（`reversible=false`）。

`evaluate_topology_satisfaction`（topology.rs:200-242）：布局后比对 rank 映射判断是否满足。

### 14.3 几何意图精修（geometric.rs）

`apply_geometric_refinement`（geometric.rs:33-65）：在 `compute_with_overlay` 产出 LayoutResult 后、grid snap 前执行。

- **Pin**：仅标记节点为 pinned（轴约束），跳过后续 snap。
- **AlignVertical / AlignHorizontal**：
  1. 过滤存在节点。
  2. **跨组检测**：若节点分属不同顶层组，仅对齐同组节点（第一个组），标记 Partial。
  3. 计算对齐轴中心均值，设置每个节点中心到均值。
  4. **单轮重叠消除** `resolve_alignment_overlap`：沿非对齐轴排序，前向扫描确保相邻节点间距 ≥ `ALIGN_OVERLAP_GAP(24px)`；单轮未完全消除→Partial（不级联）。
  5. 对齐节点加入 `pinned.aligned_vertical/horizontal`。

**穿障后对齐完整性检查** `check_alignment_after_refine`（geometric.rs:326-360）：在 `refine::run_refine` 后调用，若节点集在对齐轴上的中心极差 > `ALIGN_BREAK_TOLERANCE(1.0)`，仅降级该条为 Partial（不回滚 refine 修改）。

---

## 15. lint 质量检查

路径：`layout/lint/`。在 `LayoutResult` 上运行一组确定性几何规则，输出可追溯到 DSL 实体的违规列表。不依赖 SVG 渲染，供 CLI/测试/eval 框架消费。

### 15.1 检测的违规（11 条规则）

| 规则 | 检测内容 | 默认级别 |
|------|---------|---------|
| `NodeOverlap` | 两节点 AABB 重叠 | Error |
| `GroupOverlap` | 两无嵌套关系分组 AABB 重叠 | Error |
| `NodeOutsideGroup` | 节点超出所属分组边界 | Error |
| `ChildGroupOutsideParent` | 子分组超出父分组边界 | Error |
| `EdgeThroughNode` | 边路径穿过非端点节点（段数>2 时跳过首尾段） | Error |
| `EdgeCrossing` | 两条边在非共享端点处交叉（采样 16 点 + `polylines_cross`） | Warning |
| `EdgeOnGroupBorder` | 边路径与分组边框重合/贴边 | Warning |
| `EdgeCrossesGroupInterior` | 边穿过某分组内部但端点均不属于该分组（含祖先链） | Error |
| `UnrelatedEdgeTrunkMerge` | 架构图不同源/宿边共享非语义 trunk 段（仅 Architecture 图，`MIN_SHARED_TRUNK_LEN=24`） | Warning |
| `LabelNodeOverlap` | 标签与节点 AABB 重叠 | Error |
| `LabelLabelOverlap` | 两标签 AABB 重叠 | Warning |

### 15.2 配置（config.rs）

`LintProfile`：Default / Strict / Verbose 三档预设。
- `default_preset()`：全开但关闭 `EdgeOnGroupBorder`。
- `strict()`：CI 门禁，仅硬约束（NodeOverlap/GroupOverlap/NodeOutsideGroup/ChildGroupOutsideParent/EdgeThroughNode/EdgeCrossesGroupInterior/LabelNodeOverlap）。
- `verbose()`：全开。

### 15.3 几何工具（geometry.rs）

- `rect_overlap_area`：两矩形 AABB 重叠面积（含 `OVERLAP_EPS=0.5` 容差）。
- `point_in_rect_interior`：点在矩形内部（含 `GROUP_INTERIOR_INSET=2.0` 内缩）。
- `segment_on_group_border`：线段与分组四条边之一重合（含 `BORDER_EPS=2.0` 容差）。
- `segments_cross`：两线段真正交叉（跨立试验，不含共线重叠）。

---

## 16. 关键设计要点与确定性保障

### 16.1 关键设计要点

1. **P1 量化延后**：像素量化（`snap_and_repulse_edges`）只在管道末尾执行一次；中间所有路由后/组框修复后只做 `repulse_edges_only`（纯几何排斥）。
2. **NodeAlignConfig / EdgeSnapConfig 拆分**：节点对齐是结构修正（路由前执行，`align` 属性控制）；边量化是视觉优化（路由后执行，`snap` 属性控制）。两者独立开关、独立配置。
3. **Group Frame 三层模型**：L2 组内节点排列（算法内）→ L3 节点对齐 → L1 组框 → friendliness+route+refine → L1 幂等恢复 → Edge Pixel Snap。
4. **friendliness 解耦**：`off/diagnose/adjust` 三档，V1 评估零成本跳过，V2 调整可被环境变量 `PLOTGRAM_NO_V2_ADJUST=1` 禁用。
5. **增量重路由**：`route_after_node_moves` 在组框修复和 PRS 壳层扩展后基于实际位移节点集增量重路由，避免全图重算。
6. **Layout↔Route 反馈闭环**：布局结果 → friendliness V1 诊断 → 可选 V2 节点微调 → 路由 → refine。V2 微调会改变节点坐标，从而影响路由输入。
7. **Bundling 三档汇流策略**（Single/Compact/Concentrate）+ endpoint_bundling_key 保证同方向同箭头同线型的边自然并线。
8. **Lane Assignment 替代 Nudge**：cross-axis shift 无需 Z 字弯，自动保持正交性，三重验证（邻段不反转/不退化/无节点穿透）。
9. **X-1 多轮冲突消解**：递增 margin 档位 + 三重硬检查（path_is_clean + path_avoids_group_interiors + path_is_clean_from_edges）+ 优雅降级。
10. **安全回退**：走廊路径失败回退通用候选打分；refine 后仍穿障回退 spline 可见性图绕障；spline 无路径退化为 Bezier。

### 16.2 确定性保障（符合 AGENTS.md §2）

所有迭代/排序点都显式避免 HashMap key 顺序驱动，使用 `BTreeMap`/`BTreeSet`/显式排序：

- **NS 主循环**：组件、节点、边按 `index()` 排序；`simplex_state_key` 用排序后的 `(rank 序列, 树边索引序列)` 去重防振荡。
- **排序阶段**：`compare_nodes_for_layer` 的 8 级 tiebreaker 最终落到 `layer_node_sort_key`（按 `original.index()`）和原始位置。
- **BK 坐标**：4 趟按固定优先级 `down_left > down_right > up_left > up_right` 选取；`union_blocks` root 取 index 小者。
- **group_divide**：`GroupTree` 子组/实体按 ID 排序；`topological_sort_groups` 用声明顺序作优先级；环残留按声明顺序追加。
- **FAS**：sinks/sources 每轮 `sort()`；启发式选节点用 `then_with(|| b.cmp(a))` 确定性 tiebreak。
- **architecture_v2**：超级图邻接表 sort、rank_indices、super_edges_sorted、group_ids、sorted_targets 均显式排序。
- **正交路由**：`replan_slots` 的 AnchorBlock 块间按 `dir_key → center_tangent → min(edge_index)` 全序排序；`lane_assignment` 用 BTreeMap 保证组遍历顺序确定。
- **grid_snap**：`cluster_by_rank_axis` 按 `(center, id)` 稳定排序。
- **visibility shortest_path**：Dijkstra 等距用 node 作稳定 tiebreak。
- **group_frame**：ranks 升序、块 id 升序迭代；realign 按 group id 字典序。

---

## 附录：关键文件路径索引

### 布局主 pipeline
- `crates/plotgram-core/src/layout/mod.rs` — 入口、核心数据结构、Trait
- `crates/plotgram-core/src/layout/pipeline.rs` — LayoutPipeline 中枢
- `crates/plotgram-core/src/layout/plan.rs` — LayoutPlan / ResolvedAlgoOptions
- `crates/plotgram-core/src/layout/registry.rs` — 算法注册与工厂
- `crates/plotgram-core/src/layout/algorithm_config.rs` — OptionsReader / 各算法 config
- `crates/plotgram-core/src/layout/postprocess.rs` — 画布尺寸与居中
- `crates/plotgram-core/src/layout/edge_postprocess.rs` — 边后处理（量化+边框排斥）
- `crates/plotgram-core/src/layout/grid_snap.rs` — 节点对齐 + 边像素量化
- `crates/plotgram-core/src/layout/route_feedback.rs` — 布局↔路由反馈闭环
- `crates/plotgram-core/src/layout/geometry.rs` — 几何原语
- `crates/plotgram-core/src/layout/constants.rs` — 集中常量

### 架构图布局
- `crates/plotgram-core/src/layout/node/architecture_v2/layout/mod.rs` — ArchitectureV2LayoutStrategy
- `crates/plotgram-core/src/layout/node/architecture_v2/two_phase.rs` — 两阶段分治
- `crates/plotgram-core/src/layout/node/architecture_v2/group_layout_hint.rs` — 组内布局模式
- `crates/plotgram-core/src/layout/node/architecture_v2/group_sizing.rs` — 顶层分组宽度策略
- `crates/plotgram-core/src/layout/node/architecture_v2/post_layout.rs` — 单 group 行居中
- `crates/plotgram-core/src/layout/node/architecture_v2/layout/rank.rs` — 宏观+微观 rank
- `crates/plotgram-core/src/layout/node/architecture_v2/layout/order.rs` — 排序
- `crates/plotgram-core/src/layout/node/architecture_v2/layout/coordinate.rs` — 坐标分配
- `crates/plotgram-core/src/layout/node/architecture_v2/layout/pipeline.rs` — 7 Phase pipeline

### 流程图布局
- `crates/plotgram-core/src/layout/node/flowchart/mod.rs` — FlowchartLayout 门面
- `crates/plotgram-core/src/layout/node/flowchart/group_divide.rs` — 分治主逻辑
- `crates/plotgram-core/src/layout/node/sugiyama_v2/mod.rs` — SugiyamaV2LayoutStrategy
- `crates/plotgram-core/src/layout/node/sugiyama_v2/engine.rs` — 主引擎
- `crates/plotgram-core/src/layout/node/sugiyama_v2/graph.rs` — 图结构 + proper layer graph
- `crates/plotgram-core/src/layout/node/sugiyama_v2/rank.rs` — NS 紧边压缩
- `crates/plotgram-core/src/layout/node/sugiyama_v2/order.rs` — 加权中位数 + 转置
- `crates/plotgram-core/src/layout/node/sugiyama_v2/coordinate.rs` — BK 四趟坐标
- `crates/plotgram-core/src/layout/node/sugiyama_v2/preset.rs` — 预设参数
- `crates/plotgram-core/src/layout/node/common/` — 公共模块（acyclic/crossings/divide_and_conquer 等）

### 正交路由
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/mod.rs` — OrthogonalRouter + 主 pipeline + bundling + replan_slots + X-1 + X-2
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/slot.rs` — 磁吸点 slot
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/path.rs` — 候选路径生成
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/scoring.rs` — 评分
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/simplify.rs` — 路径简化
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/corridor_route.rs` — 走廊路由
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/lane_assignment.rs` — X-3 车道分配
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/feedback_side.rs` — 4 步决策树
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/profile.rs` — profile 配置
- `crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/context.rs` — RoutingContext / PreparedObstacles / SegmentGrid

### 边路由入口与公共
- `crates/plotgram-core/src/layout/edge/edge_routing.rs` — StraightRouting（ER 图）
- `crates/plotgram-core/src/layout/edge/edge_merge_policy.rs` — edges_may_share_trunk
- `crates/plotgram-core/src/layout/edge/visibility.rs` — 可见性图（spline/bezier/circular）

### group 与 group_frame
- `crates/plotgram-core/src/layout/group_frame/mod.rs` — GroupFrameSpec / apply_group_frame
- `crates/plotgram-core/src/layout/group_frame/pass.rs` — GroupFramePass
- `crates/plotgram-core/src/layout/group_frame/realign.rs` — realign_group_rows
- `crates/plotgram-core/src/layout/group/context.rs` — GroupRoutingContext
- `crates/plotgram-core/src/layout/group/corridor.rs` — 走廊
- `crates/plotgram-core/src/layout/group/border_shell.rs` — Border Shell 几何判定
- `crates/plotgram-core/src/layout/group/post_route.rs` — snap 后投影
- `crates/plotgram-core/src/layout/group/post_route_shell.rs` — PRS 壳层外扩
- `crates/plotgram-core/src/layout/group/hierarchy.rs` — 分组层级索引

### 精修、友好性、意图、lint
- `crates/plotgram-core/src/layout/refine/mod.rs` — run_refine 主流程
- `crates/plotgram-core/src/layout/refine/spline_fallback.rs` — spline 可见性图兜底
- `crates/plotgram-core/src/layout/friendliness/mod.rs` — V1 评估
- `crates/plotgram-core/src/layout/friendliness/adjuster.rs` — V2 节点微调
- `crates/plotgram-core/src/layout/intent/topology.rs` — 拓扑意图校验
- `crates/plotgram-core/src/layout/intent/geometric.rs` — 几何意图精修
- `crates/plotgram-core/src/layout/lint/mod.rs` — 11 条规则检测
