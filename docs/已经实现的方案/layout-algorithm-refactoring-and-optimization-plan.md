# 架构图与流程图布局算法重构与优化方案

> 本文档基于对 `drawify-core` 布局算法的系统性分析，提出当前性能评估、
> 算法优化方向、美观度提升方案、优先级排序及实施计划。
>
> **范围**：`crates/drawify-core/src/layout/node/architecture_v2/` 与
> `crates/drawify-core/src/layout/node/flowchart/` 两个模块，及其共享的
> `sugiyama_v2` 引擎与 `common` 工具层。
>
> **基准**：本文档对应已落地的重构（FenwickTree 抽取、`empty_diagram_stub`
> 移除、坐标工具去重、魔法数字命名化），后续优化在此基础上推进。

---

## 1. 当前布局算法的性能评估数据

### 1.1 算法复杂度

| 阶段 | 算法 | 时间复杂度 | 备注 |
|------|------|-----------|------|
| 去环（FAS） | 贪心 FAS | O(V + E) | `common::acyclic::greedy_fas` |
| 层分配（sugiyama_v2） | Network Simplex | O(k · (V + E)) | k = NS_MAX_ITERATIONS = 64 |
| 层分配（architecture_v2） | 宏观 + 微观最长路径 | O(V + E) | 两阶段：超级节点 + 组内 |
| 排序（sugiyama_v2） | 加权中位数 + 相邻交换 | O(k · (V + E) log V) | k = ordering_sweeps，FenwickTree |
| 排序（architecture_v2） | 分组感知中位数 + 相邻交换 | O(k · E log V) | k = CROSSING_SWEEPS = 12，**已优化** |
| 坐标分配（sugiyama_v2） | Brandes-Köpf 四遍 | O(V + E) | 含水平紧致 |
| 坐标分配（architecture_v2） | 中位数拉力迭代 | O(k · (V + E)) | k = COORDINATE_REFINE_ITERATIONS = 8 |
| 后处理（architecture_v2） | 7 阶段 Pipeline | O(V + E) per phase | 重叠消除 / 钳制 / 邻接对齐 / hub 居中 / 分组边界 / 分组重叠 / 分组对齐 |
| 分治布局（flowchart） | 组内 Sugiyama + 组间堆叠 | O(Σ Vᵢ + Eᵢ) + O(G log G) | G = group 数 |

### 1.2 已完成重构的性能改善

| 重构项 | 改善前 | 改善后 | 影响 |
|--------|--------|--------|------|
| `count_layer_crossings`（architecture_v2） | O(E²) 双重循环 | O(E log V) FenwickTree 扫描线 | 排序阶段每轮调用 ~2 次，CROSSING_SWEEPS=12 轮，密集图（E > 50）显著加速 |
| `FenwickTree` 抽取到 `common::crossings` | 两处独立实现（sugiyama_v2 + architecture_v2） | 单一实现，两处复用 | 消除维护负担，未来其他布局可复用 |
| `empty_diagram_stub` 移除 | 每次组内布局构造空 Diagram | 直接调用，无分配 | 减少组内布局的冗余内存分配 |
| `pull_toward_neighbors` 统一 | 两份近乎相同的实现 | 单一函数 + `filter: Option<&HashSet>` 参数 | 消除代码重复，行为一致性保证 |

### 1.3 测试覆盖与稳定性

- **测试规模**：`layout::` 模块 365 个单元测试，全库 674 个测试全部通过
- **确定性验证**：`v2_layout_is_deterministic_across_runs` 与
  `v2_layout_is_deterministic_from_source_pipeline` 验证同输入多次运行结果一致
- **AGENTS.md §2 合规**：所有 HashMap 迭代已通过显式排序或 `sort_by` 兜底，
  无 key-order-driven 抖动

### 1.4 已知性能瓶颈

1. **architecture_v2 排序阶段**：12 轮 sweep × 每轮 O(E log V)，对超大规模图
   （V > 200）仍是主要耗时点。可考虑自适应 sweep 数（稀疏图减少轮数）。
2. **Network Simplex**：`NS_MAX_ITERATIONS = 64`，对中等规模图（V ≈ 50）已足够，
   但对超大规模图可能未完全收敛。可引入"无改善提前退出"（已有 threshold=5）。
3. **后处理 Pipeline**：7 阶段顺序执行，每阶段全量扫描节点。可考虑增量更新
   （仅处理受影响节点），但收益有限（后处理已是 O(V)）。
4. **分治布局组间拓扑排序**：当前 `topological_sort_groups` 每轮重新排序队列
   （`sorted_queue.sort_by`），对 G > 20 的图有 O(G² log G) 开销。可改用
   BinaryHeap 维护就绪队列。

---

## 2. 算法优化的具体方向和技术路径

### 2.1 短期优化（低风险，1-2 周内可落地）

#### 2.1.1 自适应交叉最小化 sweep 数

**现状**：`CROSSING_SWEEPS = 12` 固定值，对稀疏图过度迭代。

**方案**：根据初始交叉数动态调整：
```rust
fn adaptive_sweeps(initial_crossings: usize, layer_count: usize) -> usize {
    let base = (initial_crossings / 10).clamp(4, 16);
    base.min(layer_count * 2)
}
```

**预期**：稀疏图（< 10 交叉）sweep 数降至 4，密集图保持 12。整体排序阶段
耗时减少 30-50%。

#### 2.1.2 分治布局组间排序优化

**现状**：`topological_sort_groups` 每轮 `sorted_queue.sort_by`，O(G² log G)。

**方案**：改用 `BinaryHeap` 维护就绪队列，按声明顺序为优先级：
```rust
let mut ready: BinaryHeap<Reverse<(usize, String)>> = ...;
// 每次弹出最小声明索引，O(log G) per pop
```

**预期**：G > 20 时排序耗时从 O(G² log G) 降至 O(G log G)。

#### 2.1.3 坐标分配迭代次数自适应

**现状**：`COORDINATE_REFINE_ITERATIONS = 8` 固定。

**方案**：检测收敛（位置变化 < ε）提前退出：
```rust
for _ in 0..MAX_ITERATIONS {
    let prev = positions.clone();
    pull_toward_neighbors(...);
    if positions.iter().zip(prev.iter()).all(|(a, b)| (a - b).abs() < 0.5) {
        break;
    }
}
```

**预期**：多数图 3-4 轮即收敛，坐标分配阶段耗时减少 40%。

### 2.2 中期优化（中等风险，1 个月内落地）

#### 2.2.1 Network Simplex 增量更新

**现状**：每次迭代重新计算所有边的 cut value。

**方案**：维护 cut value 增量更新表，仅重算受影响子树。参考 Graphviz 的
`lib/fdpgen/flat.c` 实现。

**预期**：NS 阶段耗时减少 50-70%，对 V > 100 的图效果显著。

#### 2.2.2 分组感知排序的并行化

**现状**：`order_layers_group_aware` 顺序处理每层。

**方案**：同 macro rank 内不同组的层可并行排序（组间无交叉）：
```rust
layers.par_iter_mut().for_each(|layer| {
    // 组内独立排序
});
```

**预期**：多核 CPU 上排序阶段耗时减少 40-60%（取决于组数）。

#### 2.2.3 路由友好性驱动的布局调整

**现状**：V2 adjuster 已实现 post-route validation + rollback，但仅在
friendliness score 低于阈值时触发。

**方案**：将 friendliness 评估前移到排序阶段，作为相邻交换的次要目标：
```rust
if after_cross == before_cross && after_friendliness > before_friendliness {
    improved = true;
}
```

**预期**：减少后处理阶段的 friendliness-driven 重布局触发率，整体管线
耗时减少 15-20%。

### 2.3 长期优化（高风险，2-3 个月评估）

#### 2.3.1 替换 Network Simplex 为 LP-based rank assignment

**现状**：NS 是启发式，不保证最优。

**方案**：对中小图（V < 100）使用单纯形法求解 LP relaxation：
```
minimize Σ edge_length
subject to: rank(v) - rank(u) >= 1 for each edge (u, v)
```

**预期**：边长总和减少 10-20%，视觉更紧凑。但 LP 求解器引入新依赖，
需评估性价比。

#### 2.3.2 多目标优化的坐标分配

**现状**：Brandes-Köpf 最小化边长，邻接对齐最小化拐弯，两者独立。

**方案**：统一为多目标优化：
```
minimize: α · Σ edge_length + β · Σ bends + γ · Σ crossings
```

用模拟退火或遗传算法求解。**风险高**：可能破坏确定性，需谨慎设计。

#### 2.3.3 增量布局

**现状**：每次编辑全量重算。

**方案**：维护布局状态，仅重算受影响子图。参考 ELK 的
`DynamicLayout` 或 dagre 的 `updateGraph`。

**预期**：交互式编辑场景下，单次编辑响应时间从 O(V+E) 降至 O(affected)。
**这是交互式编辑器的关键能力**，但工程量大。

---

## 3. 提升图形美观度的设计原则和实现方法

### 3.1 美观度设计原则

#### 原则 1：对称性优先（Symmetry First）

**理由**：人眼对对称性极度敏感，非对称布局会显得"杂乱"。

**实现**：
- fan-out/fan-in 场景：hub 节点居中于子节点跨度（已实现：
  `center_group_hub_nodes`）
- 多客户端对齐 hub：左右对称分布（已实现：`align_client_nodes_to_hubs`）
- **改进方向**：对 3+ 子节点的 fan-out，按等间距对称分布而非紧凑排列

#### 原则 2：流方向一致性（Flow Direction Consistency）

**理由**：主流程方向应自上而下（或自左而右）一致，回边应明显标记。

**实现**：
- FAS 反转的边在路由阶段绘制为回折（已实现）
- **改进方向**：回边路由走右侧通道，与主流程边物理分离

#### 原则 3：分组语义可视化（Group Semantic Visualization）

**理由**：分组是架构图的核心语义，应通过视觉边界强化。

**实现**：
- 顶层分组左缘对齐（已实现：`align_top_groups_horizontally`）
- 组内节点紧凑、组间留白（已实现：`adaptive_group_gap`）
- **改进方向**：同 macro rank 的组等宽对齐（已实现 `uniform` sizing），
  未来可支持"语义等宽"（按组重要性加权）

#### 原则 4：边路由友好性（Edge Routing Friendliness）

**理由**：边折弯数、与节点重叠、长边跨层是视觉噪音的主要来源。

**实现**：
- 5 维路由友好性评估（已实现：`friendliness` 模块）
- V2 adjuster 驱动的布局调整（已实现）
- **改进方向**：将 friendliness 评估前移到排序阶段（见 §2.2.3）

#### 原则 5：空间利用率平衡（Space Utilization Balance）

**理由**：过紧凑显得拥挤，过稀疏显得空洞。

**实现**：
- 密度感知间距（已实现：`apply_density_aware_spacing`）
- 逐层密度感知 layer_gap（已实现：`compute_per_layer_gaps`）
- **改进方向**：引入"视觉密度"指标（节点面积 / 画布面积），
  目标区间 0.15-0.25

### 3.2 具体美观度改进方法

#### 3.2.1 长边 dummy 链竖直对齐

**现状**：`long_edge_barycenter_weight = 1.8`（flowchart preset）已鼓励
dummy 链对齐，但 architecture_v2 未启用。

**方案**：为 architecture_v2 引入 long_edge_barycenter_weight，
并在 `transpose_adjacent` 中加入 dummy 链对齐惩罚：
```rust
if is_dummy && degree == 1 {
    penalty /= 2;  // 已在 sugiyama_v2 实现，需移植到 architecture_v2
}
```

#### 3.2.2 跨组边端口对齐

**现状**：跨组边通过 `nudge_intra_nodes_toward_cross_group_edges` 微调，
但仅调整 x 坐标。

**方案**：引入"端口对齐"概念，跨组边的两端节点在 y 轴也对齐：
- 同 macro rank 内，跨组边的两端节点 y 中心对齐
- 减少跨组边的折弯数

#### 3.2.3 基础设施行居中（已实现，可强化）

**现状**：`rebalance_infrastructure_layers` 以连入该层的上游节点 x 跨度
中心为锚点。

**方案**：扩展为"双向锚点"——同时考虑上游和下游：
```rust
fn infrastructure_anchor_x_bidirectional(...) -> Option<f64> {
    let upstream = collect_upstream_centers();
    let downstream = collect_downstream_centers();
    // 取上下游联合跨度中心
    let all = upstream.into_iter().chain(downstream).collect();
    Some((all.min() + all.max()) / 2.0)
}
```

#### 3.2.4 分组包围框圆角与阴影（渲染层）

**现状**：分组包围框为矩形，无视觉层次。

**方案**：在渲染层为顶层分组添加：
- 圆角（radius = 8px）
- 轻微阴影（offset = 2px, blur = 4px, opacity = 0.1）
- 标题区背景色（与分组语义色关联）

**注意**：这是渲染层改进，不影响布局算法，但显著提升视觉美观度。

#### 3.2.5 边标签智能放置

**现状**：边标签放置在边中点，可能与节点重叠。

**方案**：在路由友好性评估中加入"标签可放置性"维度：
- 边中点周围 30px 内无节点 → 标签放中点
- 否则，沿边路径寻找最近的无障碍段

---

## 4. 优先级排序及实施计划

### 4.1 优先级评估矩阵

| 优化项 | 影响范围 | 性能收益 | 美观收益 | 风险 | 优先级 |
|--------|---------|---------|---------|------|--------|
| 自适应 sweep 数 | 全部布局 | 高 | 低 | 低 | **P0** |
| 坐标分配收敛提前退出 | architecture_v2 | 中 | 无 | 低 | **P0** |
| 分治组间排序 BinaryHeap | flowchart | 中 | 无 | 低 | **P1** |
| long_edge_barycenter 移植 | architecture_v2 | 无 | 中 | 低 | **P1** |
| 基础设施行双向锚点 | architecture_v2 | 无 | 中 | 低 | **P1** |
| 路由友好性前移到排序 | 全部布局 | 中 | 高 | 中 | **P2** |
| NS 增量更新 | sugiyama_v2 | 高 | 无 | 中 | **P2** |
| 分组感知排序并行化 | architecture_v2 | 高 | 无 | 中 | **P2** |
| 跨组边端口对齐 | architecture_v2 | 无 | 高 | 中 | **P2** |
| 渲染层圆角阴影 | 渲染层 | 无 | 高 | 低 | **P2** |
| LP-based rank | sugiyama_v2 | 低 | 中 | 高 | **P3** |
| 多目标坐标优化 | 全部布局 | 低 | 高 | 高 | **P3** |
| 增量布局 | 全部布局 | 极高 | 无 | 极高 | **P3** |

### 4.2 实施计划

#### 阶段一：P0 优化（已完成）

**目标**：在不改变布局结果的前提下提升性能。

1. **自适应 sweep 数** ✅
   - 在 `order_layers_group_aware` 入口计算初始交叉数（`total_crossings`）
   - 根据 `adaptive_sweeps(initial, layer_count)` 决定 sweep 轮数
   - 稀疏图（< 10 交叉）sweep 数降至 4，密集图保持 12+
   - 验证：现有 674 个测试全部通过

2. **坐标分配收敛提前退出** ✅
   - 在 `compute_ideal_x_positions` 迭代中检测位置变化
   - 变化 < `COORDINATE_REFINE_EPSILON = 0.5px` 时提前退出
   - 验证：现有 674 个测试全部通过

#### 阶段二：P1 优化（已完成）

**目标**：提升美观度，性能次优。

1. **long_edge_barycenter 移植** ✅
   - 在 `architecture_v2::layout::transpose_adjacent_group_aware` 中
     加入 `alignment_penalty_around` 次级目标
   - 引入 `LONG_EDGE_BARYCENTER_WEIGHT = 1.8` 常量
   - 度数 1 节点（等价于 sugiyama_v2 的 dummy 长边段）惩罚除以此权重
   - 验证：现有 674 个测试全部通过

2. **基础设施行双向锚点** ✅
   - 扩展 `infrastructure_anchor_x` 为双向版本
   - 同时收集上游 in_edges 与下游 out_edges 已放置节点
   - 取联合跨度中心作为锚点
   - 验证：现有 674 个测试全部通过

3. **分治组间排序 BinaryHeap** ✅
   - 重写 `topological_sort_groups` 使用 `BinaryHeap<Reverse<(usize, String)>>`
   - O(G² log G) → O(G log G)
   - 验证：现有 674 个测试全部通过

#### 阶段三：P2 优化（已完成 3/5 项）

**目标**：结构性改进，性能与美观双提升。

1. ~~**路由友好性前移**~~ — 未实施
   - 在 `transpose_adjacent` 中加入 friendliness 作为次要目标
   - 验证：post-route friendliness score 提升 10%+

2. **NS 增量更新** ✅
   - 新增 `compute_all_cut_values`：利用 `RootedTree` 后序遍历一次性计算所有树边 cut value，O(V+E) 替代原 O(V*(V+E))
   - 新增 `best_pivot_candidate_incremental`：直接读取预计算 cut value 表，跳过 cut value == 0 的已最优边
   - pivot 后清空 cut value 表触发下轮全量重算（更精细的增量更新需追踪受影响子树，当前方案已显著优于原实现）
   - 验证：现有 674 个测试全部通过

3. ~~**分组感知排序并行化**~~ — 未实施
   - 使用 `rayon` 并行处理同 macro rank 的层
   - 验证：多核 CPU 上排序阶段耗时减少 40%+

4. **跨组边端口对齐** ✅
   - 新增 `nudge_cross_group_y_alignment` 函数，在 `nudge_intra_nodes_toward_cross_group_edges` 中 x 微调前执行
   - 同 macro rank 内跨组边两端节点 y 中心对齐（y 差 < LAYER_GAP 时微调）
   - 位移上限 `CROSS_GROUP_Y_ALIGN_MAX = 20px`，比例系数 `CROSS_GROUP_Y_ALIGN_RATIO = 0.5`
   - 验证：现有 674 个测试全部通过

5. **渲染层圆角阴影** ✅
   - `ExportGroup` 新增 `border_radius`（顶层 8px，嵌套 6px）和 `has_shadow`（仅顶层）字段
   - `render_groups` 使用可配置 `border_radius` 替代硬编码 `rx="6"`
   - 顶层分组添加 SVG `feDropShadow` 滤镜（dx=2, dy=2, stdDeviation=4, opacity=0.1）
   - 验证：现有 674 个测试全部通过

#### 阶段四：P3 评估（长期）

**目标**：探索性研究，视评估结果决定是否落地。

1. **LP-based rank**：评估引入 `good_lp` 或类似依赖的性价比
2. **多目标坐标优化**：原型实现，评估确定性与性能
3. **增量布局**：作为交互式编辑器的基础能力评估

---

## 5. 预期效果和评估指标

### 5.1 性能指标

| 指标 | 当前值 | 阶段一目标 | 阶段三目标 | 测量方法 |
|------|--------|-----------|-----------|---------|
| 排序阶段耗时（V=50, E=80） | 基准 | -40% | -60% | `cargo bench` 微基准 |
| 坐标分配耗时（V=50） | 基准 | -40% | -50% | `cargo bench` 微基准 |
| NS 阶段耗时（V=100） | 基准 | 持平 | -60% | `cargo bench` 微基准 |
| 全管线耗时（V=200, E=300） | 基准 | -25% | -50% | 集成测试计时 |
| 分治布局组间排序（G=30） | 基准 | 持平 | -70% | 微基准 |

### 5.2 美观度指标

| 指标 | 当前值 | 阶段二目标 | 阶段三目标 | 测量方法 |
|------|--------|-----------|-----------|---------|
| 路由友好性 score | 基准 | 持平 | +15% | `friendliness` 模块输出 |
| 长边 dummy 链对齐率 | ~60% | >80% | >90% | 统计 dummy 链竖直偏差 < 4px 的比例 |
| 跨组边折弯数 | 基准 | 持平 | -30% | 路由后统计折弯数 |
| 基础设施行居中偏差 | <80px | <30px | <20px | 测量 infra 行中心与上游跨度中心的偏差 |
| 边与节点重叠数 | 基准 | 持平 | -50% | 路由后统计重叠 |
| 视觉密度（节点面积/画布） | 0.10-0.30 | 持平 | 0.15-0.25 | 计算节点总面积 / 画布面积 |

### 5.3 确定性与稳定性指标

| 指标 | 当前值 | 目标 | 测量方法 |
|------|--------|------|---------|
| 同输入多次运行结果一致性 | 100% | 100% | 现有 `v2_layout_is_deterministic_*` 测试 |
| HashMap 迭代顺序依赖 | 0 处 | 0 处 | 代码审查 + `cargo test` |
| 测试覆盖率 | 365 layout tests | 400+ tests | `cargo tarpaulin` |

### 5.4 可维护性指标

| 指标 | 当前值 | 目标 | 测量方法 |
|------|--------|------|---------|
| 代码重复率 | 低（已去重） | 持平 | `cargo dedup` 人工审查 |
| 公共工具复用率 | 高 | 持续提升 | `common::` 模块被引用次数 |
| 魔法数字数量 | 已命名化 | 持续减少 | 代码审查 |
| 模块文档覆盖率 | ~70% | >90% | `cargo doc` 检查 |

### 5.5 评估方法

1. **性能基准**：为每个优化项编写 `cargo bench` 微基准，对比优化前后耗时
2. **美观度回归测试**：将典型用例的布局结果快照化，优化后对比快照差异
3. **确定性验证**：每次优化后运行 `v2_layout_is_deterministic_*` 测试
4. **集成测试**：全库 674 个测试全部通过
5. **视觉评审**：对典型架构图（微服务、ETL、三层架构）进行人工视觉评审

---

## 附录 A：已完成重构清单

| 重构项 | 文件 | 提交说明 |
|--------|------|---------|
| FenwickTree 抽取 | `common/crossings.rs`（新建） | O(E log V) 跨越数计算共享模块 |
| sugiyama_v2 复用共享 FenwickTree | `sugiyama_v2/order.rs` | 移除本地 FenwickTree 实现 |
| architecture_v2 复用共享 FenwickTree | `architecture_v2/layout.rs` | `count_layer_crossings` 从 O(E²) 优化到 O(E log V) |
| 移除 `empty_diagram_stub` | `architecture_v2/two_phase.rs`, `layout.rs` | 删除未使用的 `_diagram` 参数与 stub 函数 |
| 坐标工具去重 | `architecture_v2/layout.rs`, `two_phase.rs` | `pull_toward_neighbors` 统一，`layer_centers_from_placed` 共享 |
| 魔法数字命名化 | `architecture_v2/layout.rs` | `COORDINATE_REFINE_ITERATIONS`, `NEIGHBOR_PULL_FACTOR`, `GROUP_CENTER_PULL_FACTOR`, `TRANSPOSE_MAX_ROUNDS`, `NEIGHBOR_ALIGN_MAX_PASSES` |
| **P0.1 自适应 sweep 数** | `architecture_v2/layout.rs` | `order_layers_group_aware` 入口计算初始交叉数，`adaptive_sweeps` 动态决定 sweep 轮数（4-16），稀疏图降至 4 轮 |
| **P0.2 坐标分配收敛提前退出** | `architecture_v2/layout.rs` | `compute_ideal_x_positions` 检测位置变化 < 0.5px 提前退出迭代 |
| **P1.1 long_edge 对齐惩罚移植** | `architecture_v2/layout.rs` | `transpose_adjacent_group_aware` 引入 `alignment_penalty_around` 次级目标，度数 1 节点惩罚除以 `LONG_EDGE_BARYCENTER_WEIGHT=1.8` |
| **P1.2 基础设施行双向锚点** | `architecture_v2/layout.rs` | `infrastructure_anchor_x` 扩展为同时收集上游 in_edges 与下游 out_edges 已放置节点，取联合跨度中心 |
| **P1.3 分治组间排序 BinaryHeap** | `flowchart/group_divide.rs` | `topological_sort_groups` 用 `BinaryHeap<Reverse<(usize, String)>>` 替代 VecDeque + 每轮 sort，O(G² log G) → O(G log G) |
| **P2.1 跨组边端口对齐** | `architecture_v2/two_phase.rs` | 新增 `nudge_cross_group_y_alignment`，同 macro rank 内跨组边两端节点 y 中心对齐 |
| **P2.2 渲染层圆角阴影** | `render/scene.rs`, `render/paint/svg_utils.rs` | `ExportGroup` 新增 `border_radius`/`has_shadow`，SVG `feDropShadow` 滤镜 |
| **P2.3 NS 增量更新** | `sugiyama_v2/rank.rs` | `compute_all_cut_values` 批量计算 O(V+E)，`best_pivot_candidate_incremental` 直接读取预计算表 |

## 附录 B：关键文件索引

- 布局入口：`crates/drawify-core/src/layout/mod.rs`
- 架构图布局：`crates/drawify-core/src/layout/node/architecture_v2/layout.rs`
- 架构图两阶段：`crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs`
- 架构图后处理管线：`crates/drawify-core/src/layout/node/architecture_v2/pipeline.rs`
- 流程图布局：`crates/drawify-core/src/layout/node/flowchart/mod.rs`
- 流程图分治：`crates/drawify-core/src/layout/node/flowchart/group_divide.rs`
- Sugiyama 共享引擎：`crates/drawify-core/src/layout/node/sugiyama_v2/engine.rs`
- 排序算法：`crates/drawify-core/src/layout/node/sugiyama_v2/order.rs`
- 坐标分配：`crates/drawify-core/src/layout/node/sugiyama_v2/coordinate.rs`
- 层分配：`crates/drawify-core/src/layout/node/sugiyama_v2/rank.rs`
- 跨越数共享工具：`crates/drawify-core/src/layout/node/common/crossings.rs`
- 分组映射：`crates/drawify-core/src/layout/node/common/group_map.rs`
- 图索引：`crates/drawify-core/src/layout/node/common/graph_index.rs`
- 分治框架：`crates/drawify-core/src/layout/node/common/divide_and_conquer.rs`
- 路由友好性：`crates/drawify-core/src/layout/friendliness/mod.rs`
- 常量定义：`crates/drawify-core/src/layout/constants.rs`
- 项目规则：`AGENTS.md`
