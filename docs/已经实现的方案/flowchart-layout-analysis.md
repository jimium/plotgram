# 流程图布局算法分析

## 一、大白话逻辑：整体思路

流程图布局 (`layout/node/flowchart/`) 本身只是个**门面**，真正干活的是共享的 Sugiyama v2 引擎 (`layout/node/sugiyama_v2/engine.rs`)。整套算法就是经典的"分层布局"流水线，分 6 步：

### 第 1 步：建图 + 去环（把带环的图变成 DAG）

`graph.rs` 把 entity 当节点、relation 当边建图。流程图里可能有环（比如"判断→处理→判断"的循环），但分层布局要求所有边方向一致，所以得先把环破掉。

去环用贪心 FAS 算法（`common/acyclic.rs`），大白话就是：

- 先把"没有出边"的节点（sink）摘出来放右边
- 再把"没有入边"的节点（source）摘出来放左边
- 剩下的都在环里，挑一个"出度 - 入度"最大的节点强行剥到左边
- 最后拼成拓扑序，凡是"从后往前"走的边就标记为要反转

这套比旧版 DFS 反转回边更聪明，反转的边更少，保留的原始方向语义更多。

### 第 2 步：分层（给每个节点分配一个 y 坐标层级）

`rank.rs` 用的是"Network Simplex 风格"分层。大白话：

1. 先使用**最长路径法**给初始层级：源节点在第 0 层，其他节点层级 = max(前驱层级) + 1
2. 再做**紧边压缩**：如果一条边 `A→B` 的层级差 > 1（说明中间有空层），就尝试把 B 往上挪，挪到 `rank(A)+1`，只要不违反其他边的约束
3. 用 feasible tree + pivot shift 迭代优化，最多 64 轮，目标是让总边长最短

这一步决定了图的"高度"（有多少层）。压缩得好，图就更紧凑。

### 第 3 步：密度感知调间距

`engine.rs#apply_density_aware_spacing` 在分层后、排布前，根据图密度动态放大间距：

- **跨层边越多** → 层间距 `layer_gap` 越大（给正交路由留通道）
- **平均度数越高** → 同层间距 `node_gap` 越大（避免端口拥挤）

这是个粗粒度但有效的启发式。

### 第 4 步：拆长边 + 排序（决定同层节点的左右顺序）

`graph.rs#build_proper_layer_graph`：跨多层的长边被拆成一串 dummy 节点链（每层一个），这样所有边都只跨一层。

`order.rs` 做**加权中位数 + 转置**排序，8 轮上下扫描：

- **中位数排序**：每层节点按"邻居位置的中位数"排序，减少交叉
- **转置**：排完序后，尝试交换相邻节点对，交叉数减少就保留
- 交叉计数用 Fenwick Tree 加速到 O(E log V)
- dummy 节点优先于真节点（让长边 dummy 链更容易竖直对齐，减少折弯）

这一步决定了图的"美观度"（交叉数多少）。

### 第 5 步：坐标分配（给每个节点算精确 x/y）

`coordinate.rs` 用 Brandes-Kopf 风格的 4 趟对齐 + 压缩：

1. **4 趟对齐**：down-left、down-right、up-left、up-right，每趟构建垂直对齐块（让 dummy 链尽量竖直）
2. **水平压缩**：块之间按约束传播，紧凑排列
3. **4 趟取平均**：`(down_left + down_right + up_left + up_right) / 4`
4. **重叠消除**：`resolve_real_node_overlaps` 做后验的相邻节点推开

### 第 6 步：分组包围框 + 归一化

最后 `group_bounds.rs` 算分组包围框，`postprocess.rs` 把整体平移到 padding 内。

---

## 二、Group 在流程图布局里的考虑

**核心结论：流程图布局几乎完全忽略 group，group 只是"事后画的框"。**

### 当前 group 的处理方式

在 `engine.rs` 中，group 的处理是在**所有节点坐标都算完之后**才做的：

```rust
let groups = group_bounds::compute_group_bounds(
    diagram,
    &nodes,
    GroupPadding::uniform(layout_config.group_padding, 16.0),
);
```

`compute_group_bounds` 的逻辑就是：

1. 按 depth 降序排序（叶子组先算，容器组后算）
2. 对每个 group，取它所有直接 entity 的包围框 ∪ 所有子组的包围框
3. 外扩一个 padding，得到 group 的 `x/y/width/height`

**也就是说：group 不参与去环、不参与分层、不参与排序、不参与坐标分配。** 节点该在哪层在哪层，该在哪个 x 位置在哪个 x 位置，group 只是在外面套个框。

### 这会导致什么问题

1. **同组成员可能散落在不同层**：比如 group A 里有 `web` 和 `db`，但 `web` 在第 0 层、`db` 在第 3 层，group 框会拉得很长，中间夹着其他组的节点。
2. **group 框可能互相重叠**：两个 group 的成员在层间交错，包围框必然重叠，视觉上很混乱。
3. **group 框可能压住非组节点**：group 框只是 entity 包围框 + padding，完全不考虑框内是否有其他组的节点。
4. **group 框可能被边穿过**：因为布局时根本不知道 group 边界，边路由也不会避让 group 框。

### 与 architecture_v2 的对比

`architecture_v2/two_phase.rs` 才是"group 一等公民"的做法：

- **Phase A（组内布局）**：组内独立做 Sugiyama（每个 group 内部自己分层、排序、分配坐标）
- **Phase B（组间宏观定位）**：组间把 group 当"超级节点"做宏观 Sugiyama 分层
- **Phase C（坐标回填）**：把组内坐标叠加到组间坐标上

流程图布局完全没有这套机制。

---

## 三、优化方向

按优先级排序：

### 优先级 1：Group 感知布局（最大短板）

这是流程图布局**最明显的缺陷**。建议分两步走：

**短期（低成本）**：在排序阶段加 group 偏置。`order.rs` 的 `compare_nodes_for_layer` 里，当 median/barycenter 接近时，优先把同 group 的节点排在一起。这不需要改架构，只在排序比较函数里加一个 tiebreaker。

**长期（高收益）**：引入 architecture_v2 的两阶段布局作为可选模式。当检测到 diagram 有 group 时，走 two_phase 路径；无 group 时走当前路径。这样流程图也能获得"group 内紧凑 + group 间分层"的效果。

### 优先级 2：Group 包围框重叠检测与消除

`compute_group_bounds` 算完包围框后，应该检测：

- group 框之间是否重叠
- group 框内是否有非组节点

如果重叠，要么局部推开节点重新算包围框，要么至少在 `LayoutHints` 里标记为警告。

### 优先级 3：Brandes-Kopf 坐标分配的完整度

当前 `coordinate.rs` 的 4 趟是**取平均**：

```rust
(down_left + down_right + up_left + up_right) / 4.0
```

标准 Brandes-Kopf 是**取 4 趟中交叉数最少的那一趟**，不是取平均。取平均会模糊各趟的优势，导致坐标不够紧凑。另外 `resolve_real_node_overlaps` 的存在本身就说明压缩阶段没完全消除重叠，这是个补丁。

### 优先级 4：Network Simplex 的完整度

`rank.rs` 的 NS-style 是简化版：

- `cut_value_for_subtree` 只数边的出入数，没考虑边权重
- `best_entering_edge` 选 slack 最小的边，但没做完整的 cut value pivot
- readme 也承认"不等同于 Graphviz dot 的完整 Network Simplex"

升级到完整 NS 能让大图的层级更紧凑，但工程量大，建议先评估收益。

### 优先级 5：长边权重针对流程图调优

`preset.rs` 里 `FLOWCHART_PRESET = BASE`，`long_edge_barycenter_weight = 1.0`，意味着流程图的长边（跳连、回边）在排序时和普通边同权。

流程图经常有"判断节点跳过几层直接指向远端节点"的长边。把 `long_edge_barycenter_weight` 调到 1.5~2.0，能让 dummy 链更竖直对齐，减少折弯。这是个改 preset 常量就能验证的低成本优化。

### 优先级 6：密度感知间距的细粒度化

`apply_density_aware_spacing` 使用的是**全局**跨层边数和**全局**平均度数。但密度往往是局部的——某几层特别密集，其他层很稀疏。

可以改成**逐层**密度感知：对每一层单独评估密度，只放大密集层的间距。这样稀疏层不会被无谓拉大，整体更紧凑。

### 优先级 7：确定性审计

虽然 `AGENTS.md` 强调不能用 HashMap 迭代顺序驱动布局，代码里大部分地方也确实做了排序，但仍有几处值得复查：

- `engine.rs` 的 `sugiyama_ranks` 从 `HashMap` 收集，虽然只用于 hints 不影响布局，但下游 friendliness 评估如果依赖它可能有抖动
- `rank.rs` 的 `seen_states: HashSet<(Vec<i32>, Vec<usize>)>` 是去重用的，不影响顺序，OK

整体确定性做得不错，有测试 `v2_layout_is_deterministic_across_runs` 守护。

---

## 总结

流程图布局的**算法内核**（去环、分层、排序、坐标分配）已经相当完整，接近 Graphviz dot 的工程实现版。但**group 处理是最大短板**——group 完全不参与布局，只是事后画框，导致有 group 的流程图视觉效果差。

短期建议先在排序阶段加 group 偏置，长期建议引入 two_phase 机制。其次是 BK 坐标分配的"取平均"应改为"取最优"，以及流程图 preset 的长边权重可以调高。

---

## 四、优化任务实施记录

本节记录每个优化任务的代码实现情况。优先级 1~3、5~7 已完成代码实现与测试；优先级 4 与优先级 1 长期因工程量大，给出评估与实施计划，暂不落地完整代码。

### 优先级 1（短期）：排序阶段加 group 偏置 tiebreaker — ✅ 已实现

**改动文件：**

- [order.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/order.rs)：`order_layers_weighted_median` 与 `compare_nodes_for_layer` 签名增加 `node_group: &HashMap<NodeIndex, Option<String>>` 参数；在 median 差异小于 `GROUP_BIAS_EPSILON = 1.0` 时，按 group_id 字典序作为 tiebreaker，使同顶层组节点聚拢。
- [engine.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/engine.rs)：新增 `build_node_group_map` 函数，将 layered graph 节点映射到顶层 group_id（Real 节点取 entity 的扁平化顶层组，Dummy 节点为 `None`）。
- [mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/mod.rs)：调用链更新；新增测试 `group_bias_clusters_same_group_nodes_in_layer` 验证 hub → {a1,a2 (组 A), b1,b2 (组 B)} 场景下同组节点相邻。

**设计要点：**

- 偏置仅在 median 接近时生效，不破坏加权中位数的主排序语义。
- 使用扁平化到顶层组的映射（`build_node_to_top_group`），避免嵌套组导致组内子组被拆散。
- Dummy 节点不参与偏置，长边对齐仍由 BK 阶段负责。

### 优先级 2：Group 包围框重叠检测与警告 — ✅ 已实现

**改动文件：**

- [layout/mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs)：新增 `GroupLayoutWarning` 结构体与 `GroupLayoutWarningKind` 枚举（`GroupOverlap` / `ForeignNodeInside`）；`LayoutHints` 增加 `group_layout_warnings: Vec<GroupLayoutWarning>` 字段。
- [group_bounds.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/group_bounds.rs)：新增 `detect_group_layout_warnings` 函数，检测非嵌套兄弟组包围框重叠（记录重叠面积）与非组节点落入组框两种问题；新增 `rect_overlap_area` / `rect_overlap_area_node` 辅助函数。
- [engine.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/engine.rs)：在 `compute_with_preset_and_overlay` 末尾调用 `detect_group_layout_warnings` 填充 hints。

**新增测试（4 个）：** `detects_overlapping_sibling_groups`、`detects_foreign_node_inside_group`、`does_not_warn_for_nested_groups`、`no_warnings_when_groups_disjoint`。

**设计要点：**

- 仅检测非嵌套兄弟组重叠，避免对合法的嵌套组误报。
- 警告而非自动修复：重叠消除涉及节点重排，成本高且可能破坏已优化的交叉数，留给上层决策（如启用 two_phase 或调整 preset）。

### 优先级 3：BK 坐标分配改为取 4 趟中最优 — ✅ 已实现

**改动文件：** [coordinate.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/coordinate.rs)

**改动内容：** `assign_layer_centers_brandes_koepf` 从 `(down_left + down_right + up_left + up_right) / 4.0` 改为按"布局宽度最小（最紧凑）"选取最优趟。新增 `pass_width` 函数计算每趟的最左中心到最右中心距离。

**确定性保证：** 同宽时按 `down_left > down_right > up_left > up_right` 的固定索引优先级选取（`idx_a.cmp(idx_b)` 作为 tiebreaker），不依赖 HashMap 迭代顺序，符合 `AGENTS.md` 规则 2。

### 优先级 5：调高流程图 long_edge_barycenter_weight — ✅ 已实现

**改动文件：** [preset.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/preset.rs)

**改动内容：** `FLOWCHART_PRESET` 从 `BASE`（`long_edge_barycenter_weight = 1.0`）改为 `long_edge_barycenter_weight: 1.8`。流程图常有判断节点跨层跳连，调高后 dummy 链更竖直对齐，减少折弯。

### 优先级 6：密度感知间距改为逐层评估 — ✅ 已实现

**改动文件：** [engine.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/engine.rs)、[coordinate.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/coordinate.rs)

**改动内容：**

- 重构 `apply_density_aware_spacing`：只调整 `node_gap`（基于最密集层平均度数），不再全局放大 `layer_gap`。
- 新增 `compute_per_layer_gaps`：为每个层间边界单独计算 `layer_gap`，跨越该 gap 的边数越多 gap 越大（`DENSITY_LAYER_GAP_SCALE` 线性放大，上限 `DENSITY_MAX_EXTRA_LAYER_GAP`），稀疏 gap 保持 base 不变。
- `assign_coordinates_brandes_koepf` 签名增加 `layer_gaps: &[f64]` 参数，按 gap 序列累加层偏移。

**收益：** 稀疏层不再被个别密集层拉大间距，整体更紧凑；密集层间为正交路由预留足够通道。

### 优先级 7：确定性审计 — ✅ 已完成（无需修复）

**审计范围：** `sugiyama_v2` 全部 HashMap 使用 + `sugiyama_ranks` 下游消费方。

**审计结论：**

| 位置 | 用法 | 是否确定性安全 |
|------|------|----------------|
| `engine.rs` `ranks.values().copied().max()` | 聚合 max | ✅ |
| `engine.rs` `for &r in ranks.values()` 计数 | 聚合 count | ✅ |
| `engine.rs` `gap_load.iter().map(...)` | Vec 迭代 | ✅ |
| `coordinate.rs` BK 4 趟选取 | 固定数组 + 索引 tiebreaker | ✅ |
| `coordinate.rs` `coords.values().fold(min/max)` | 聚合 | ✅ |
| `coordinate.rs` `blocks` HashMap 填充 `centers` | 无序填充，无顺序依赖 | ✅ |
| `coordinate.rs` `block_order.iter()` | Vec | ✅ |
| `coordinate.rs` `layers.iter()` | Vec | ✅ |
| `order.rs` `layers_iter_from_pos` | `collect` 后 `sort_by_key` | ✅ |
| `postprocess.rs` `.values().fold(min/max)` | 聚合 | ✅ |
| `postprocess.rs` `for node in values_mut()` 平移 | 均匀平移，无顺序依赖 | ✅ |
| `intent/mod.rs` 6 处 `ranks["key"]` | 测试中按键索引 | ✅ |
| `long_edge.rs` `diagram.relations.iter()` + `ranks.get()` | Vec 顺序 + 按键查找 | ✅ |

**结论：** 所有 HashMap 使用均为聚合操作、按键查找或 Vec 迭代，无依赖迭代顺序驱动布局的情况。`v2_layout_is_deterministic_across_runs` 测试持续守护。无需修复。

### 优先级 4：Network Simplex 完整度 — ⏳ 评估与实施计划（暂不落地）

**现状评估：**

当前 [rank.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/rank.rs) 的 NS-style 实现已是"可行树 + pivot shift"的简化版，与完整 Graphviz dot NS 的差距：

1. **cut_value 简化**：`cut_value_for_subtree` 只数子树出/入边数（`incoming - outgoing`），未考虑边权重与边类型（长边 vs 短边）。完整 NS 的 cut value 是子树内所有边权重加权和的符号判定。
2. **entering edge 选取**：`best_entering_edge` 选 slack 最小的边，但完整 NS 应选"使目标函数下降最多"的边，需结合 cut value 与 slack 联合判定。
3. **无边权重模型**：完整 NS 对每条边赋权重（长边权重低，使其更易被拉伸），当前实现所有边同权。
4. **目标函数**：完整 NS 最小化 `Σ weight(e) * length(e)`，当前实现隐式最小化总边长但无显式目标函数。

**收益评估：**

- **小图（<50 节点）**：当前简化版已足够，完整 NS 收益不明显，甚至可能因 pivot 次数增加而变慢。
- **中图（50~500 节点）**：完整 NS 能让层级更紧凑 5~15%，减少空层与长边跨度。
- **大图（>500 节点）**：完整 NS 的 O(V·E) 单次 pivot 成本显著，需配合启发式剪枝。

**实施计划（未来迭代）：**

1. **Phase 1：边权重模型**（1~2 天）
   - 在 `DiGraph<String, ()>` 旁维护 `HashMap<EdgeIndex, f64>` 权重表，长边（跨层 >1 的边反转/拆分前）权重设为 0.2~0.5，普通边权重 1.0。
   - `cut_value_for_subtree` 改为权重加权和。
   - `best_entering_edge` 选 `max(|cut_value| / slack)` 的边。

2. **Phase 2：显式目标函数**（1 天）
   - 新增 `total_edge_length(ranks, weights)` 函数，作为收敛判定与早停依据。
   - 替换当前 `score` 启发式为真实目标函数下降量。

3. **Phase 3：性能基准**（1 天）
   - 用现有 fixture 图（小/中/大）跑 before/after，记录层级数、总边长、耗时。
   - 若大图耗时超 2x，加启发式：仅对 cut value < -threshold 的边做 pivot，跳过微劣边。

4. **Phase 4：回归测试**（1 天）
   - 新增 `ns_compact_on_medium_graph` 测试：50 节点图，断言完整 NS 层级数 ≤ 简化版。
   - 保留 `v2_layout_is_deterministic_across_runs` 守护。

**暂不落地理由：** 工程量约 4~6 天，且收益主要在中大图，当前流程图 fixture 以小图为主。建议在收集到更多中大图真实用例后再启动，避免过早优化。

### 优先级 1（长期）：引入 two_phase 机制 — ⏳ 评估与实施计划（暂不落地）

**现状评估：**

[architecture_v2/two_phase.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs) 已有成熟的 two_phase 实现（Phase A 组内递归 Sugiyama → Phase B 组间宏观分层 → Phase C 坐标回填 → Phase C+ 跨组边微调），但与流程图布局完全隔离：

- 流程图走 `engine::compute_with_preset` 单路径，无 group 感知。
- architecture_v2 走 `compute_two_phase_layout`，依赖 `GroupMap`、`GraphIndex` 等 architecture 专属抽象。

**收益评估：**

- **有 group 的流程图**：two_phase 能让同组节点物理聚拢（组内独立布局），组间按宏观层级排列，彻底解决"同组节点散落不同层"问题。
- **无 group 的流程图**：two_phase 退化为单阶段，无额外收益但也无损失。
- **与短期 group 偏置的对比**：短期偏置只在排序阶段聚拢同组节点，不改变层级分配；two_phase 在层级分配阶段就隔离组内/组间，效果更强但侵入更大。

**实施计划（未来迭代）：**

1. **Phase 1：抽象 two_phase 核心**（2~3 天）
   - 从 `architecture_v2/two_phase.rs` 抽取不依赖 architecture 专属抽象的核心逻辑到 `sugiyama_v2/two_phase.rs`：
     - `GroupTree`（已通用）
     - `IntraLayout`（已通用）
     - `MacroBlock`（已通用）
     - `compute_two_phase_layout` 的核心流程
   - 抽象点：`GraphIndex` → 通用 `DiGraph`；`GroupMap` → 通用 group 元数据。

2. **Phase 2：流程图集成**（1~2 天）
   - [flowchart/mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/flowchart/mod.rs) 的 `compute` 方法增加分支：
     ```rust
     if diagram.groups.is_empty() {
         engine::compute_with_preset(diagram, &preset::FLOWCHART_PRESET, self.config)
     } else {
         engine::compute_two_phase(diagram, &preset::FLOWCHART_PRESET, self.config)
     }
     ```
   - 保留短期 group 偏置作为无 group 时的兜底（有 group 时 two_phase 已覆盖）。

3. **Phase 3：preset 适配**（1 天）
   - two_phase 的组内布局复用 `FLOWCHART_PRESET`，组间宏观布局用更宽松的 `MACRO_PRESET`（更大 group_gap）。
   - 新增 `SugiyamaPreset` 字段：`macro_layer_gap`、`macro_node_gap`。

4. **Phase 4：回归与对比测试**（1~2 天）
   - 新增 `two_phase_clusters_groups_on_grouped_flowchart` 测试：有 group 的流程图，断言同组节点 y 跨度 < 单阶段。
   - 新增 `two_phase_falls_back_to_single_phase_without_groups` 测试。
   - 保留 `group_bias_clusters_same_group_nodes_in_layer` 作为单阶段路径的守护。

**暂不落地理由：** 工程量约 5~8 天，且短期 group 偏置已覆盖"同组节点聚拢"的主要视觉诉求。two_phase 的主要额外收益在"组间分层"，需先收集有复杂 group 嵌套的流程图用例验证收益，避免过早引入架构复杂度。

---

## 五、最终报告

### 完成情况总览

| 优先级 | 任务 | 状态 | 改动文件数 | 新增测试数 |
|--------|------|------|-----------|-----------|
| P1 短期 | 排序阶段 group 偏置 | ✅ 已实现 | 3 | 1 |
| P2 | Group 包围框重叠检测 | ✅ 已实现 | 3 | 4 |
| P3 | BK 取最优趟 | ✅ 已实现 | 1 | 0（现有测试守护） |
| P4 | Network Simplex 完整度 | ⏳ 评估+计划 | 0 | 0 |
| P5 | 长边权重调优 | ✅ 已实现 | 1 | 0（现有测试守护） |
| P6 | 密度感知逐层化 | ✅ 已实现 | 2 | 0（现有测试守护） |
| P1 长期 | two_phase 机制 | ⏳ 评估+计划 | 0 | 0 |
| P7 | 确定性审计 | ✅ 已完成 | 0 | 0 |

### 代码改动汇总

**已修改文件（7 个）：**

1. [preset.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/preset.rs) — P5：`long_edge_barycenter_weight: 1.8`
2. [coordinate.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/coordinate.rs) — P3：BK 取最优趟；P6：`layer_gaps` 参数
3. [engine.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/engine.rs) — P6：`compute_per_layer_gaps`；P1s：`build_node_group_map`；P2：调用 `detect_group_layout_warnings`
4. [order.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/order.rs) — P1s：group 偏置 tiebreaker
5. [mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/mod.rs) — P1s：调用链更新 + 新测试
6. [layout/mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs) — P2：`GroupLayoutWarning` 结构体
7. [group_bounds.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/group_bounds.rs) — P2：`detect_group_layout_warnings` + 4 测试

### 关键设计决策

1. **P1s 偏置用顶层组而非直接组**：避免嵌套组导致子组内节点被拆散，扁平化到顶层组使同顶层组的所有后代节点聚拢。
2. **P2 警告而非自动修复**：重叠消除需重排节点，可能破坏已优化的交叉数；警告留给上层决策（启用 two_phase 或调 preset）。
3. **P3 取最优趟用宽度而非交叉数**：4 趟 BK 使用相同层序，交叉数相同；宽度最小即最紧凑，是更合适的择优指标。
4. **P6 逐层 gap 基于边跨越数**：跨越某 gap 的边数 = 该 gap 处的 dummy 段数，直接反映正交路由所需通道数。
5. **P7 不修复**：审计确认所有 HashMap 使用均确定性安全，现有 `v2_layout_is_deterministic_across_runs` 测试持续守护。
6. **P4/P1L 暂不落地**：工程量大（4~8 天），收益需中大图用例验证，短期偏置已覆盖主要视觉诉求。

### 后续建议

1. **收集中大图用例**：为 P4（NS 完整度）和 P1L（two_phase）的收益评估提供数据支撑。
2. **监控 P2 警告**：在有 group 的流程图上线后，观察 `group_layout_warnings` 的触发频率与场景，作为是否启动 P1L 的信号。
3. **P5 调参验证**：`long_edge_barycenter_weight: 1.8` 是经验值，建议用真实流程图 fixture 跑 A/B 对比，确认折弯数下降且无回归。