# Phase 1：坐标写权地图

> 日期：2026-07-23  
> 状态：完成  
> 目的：枚举 flowchart 管线中所有能移动节点的函数，标注未来归属。

---

## 管线时序（flowchart / sugiyama-v2）

```text
strategy.compute(diagram)
  └─ sugiyama_v2::coordinate::assign_coordinates_brandes_koepf
       ├─ [W1] compute_spine_nodes             → 只读（生成 spine set）
       ├─ [W2] assign_layer_centers_brandes_koepf  → BK 四趟初值
       ├─ [W3] compact_layer_centers           → 向邻居重心靠拢（spine 加权）
       ├─ [W4] enforce_fan_symmetry            → fan-out/fan-in 质心对齐
       ├─ 物化 centers → NodeLayout
       ├─ [W5] resolve_real_node_overlaps      → 前扫+后扫+回中 消重叠
       ├─ [W6] align_singleton_layers_to_predecessors → 单节点层对齐前驱
       ├─ [W7] align_spine_chain (条件)        → 反馈主干拉直+侧支推开
       ├─ [W8] align_local_end_nodes           → end 对齐唯一前驱
       ├─ [W9] normalize_layout_to_padding     → 全图刚性平移
       └─ [W10] align_pendants_under_anchors   → 悬挂叶对齐锚点

pipeline.apply_node_frame
  └─ [W11] grid_snap::align_nodes             → rank 轴量化 + layer 轴对齐

pipeline.run_routing_pipeline
  ├─ feedback.apply_pre_route
  │    ├─ PressureSnapshot::compute           → 只读
  │    ├─ budget.enrich_from_pressure         → 只写 budget 数据
  │    ├─ [W12] enforce_horizontal_gaps       → 水平推开节点
  │    └─ [W13] enforce_vertical_rank_gaps    → 竖向整带下移
  │
  ├─ router.route / run_refine
  │    ├─ [W14] refine::push_problem_nodes    → 问题节点沿力方向推开
  │    └─ [W12] enforce_horizontal_gaps (refine 内再调)
  │
  ├─ [W15] space_budget_guard::resolve_budget_violations
  │    ├─ resolve_residual_with_budget_and_ranks → 水平+AABB 消重叠
  │    └─ [W13] enforce_vertical_rank_gaps
  │
  ├─ group_frame::restore_after_node_moves
  │    └─ [W16] 组框恢复（可移动节点以满足 containment）
  │
  ├─ architecture only:
  │    ├─ [W17] reassert_multi_client_hub_centroids
  │    └─ [W18] align_cross_scope_pendant_chains
  │
  └─ node_freeze = NodeFreeze::capture        → 冻结屏障
      └─ 此后只改边，node_freeze.assert_unchanged 校验
```

---

## 写权清单与未来归属

### Sugiyama 内部（coordinate.rs + postprocess.rs）

| # | 函数 | 文件:行 | 写什么 | 未来归属 |
|---|------|---------|--------|----------|
| W2 | `assign_layer_centers_brandes_koepf` | coordinate.rs:810+ | BK 四趟中心 | **保留为 BK Initializer**（只产 initial_x） |
| W3 | `compact_layer_centers` | coordinate.rs:1065 | 向邻居重心靠拢 | **删除** → 由 P2/P3 objective 替代 |
| W4 | `enforce_fan_symmetry` | coordinate.rs:1480 | fan 质心对齐 | **删除** → 由 P1 SymmetryRegion objective 替代 |
| W5 | `resolve_real_node_overlaps` | coordinate.rs:717 | 前扫+后扫消重叠 | **删除** → 由 PAVA 硬约束投影替代 |
| W6 | `align_singleton_layers_to_predecessors` | coordinate.rs:124 | 单节点层对齐 | **删除** → 由 P2 objective 替代 |
| W7 | `align_spine_chain` | coordinate.rs:270 | 主干拉直+推开 | **删除** → 由 P1 LinearChain/RegionAxis 替代 |
| W8 | `align_local_end_nodes` | coordinate.rs:215 | end 对齐前驱 | **删除** → 由 P1 objective 替代 |
| W9 | `normalize_layout_to_padding` | postprocess.rs:28 | 全图刚性平移 | **保留为 Finalize**（合法：只平移） |
| W10 | `align_pendants_under_anchors` | coordinate.rs:395 | pendant 对齐锚点 | **删除** → 由 P1 PendantRegion objective 替代 |

### Pipeline 共享层

| # | 函数 | 文件:行 | 写什么 | 未来归属 |
|---|------|---------|--------|----------|
| W11 | `grid_snap::align_nodes` | grid_snap.rs:283 | rank 轴量化 | **保留为 Finalize**（量化+PAVA 修复） |
| W12 | `enforce_horizontal_gaps` | space_budget.rs:423 | 水平推开 | **删除（flowchart）** → SpacingIntent + re-solve |
| W13 | `enforce_vertical_rank_gaps` | space_budget.rs:493 | 竖向整带下移 | **保留**（rank 轴间距属于 finalize 合法操作） |
| W14 | `refine::push_problem_nodes` | refine/push.rs:41 | 问题节点推开 | **删除（flowchart）** → 冻结后不允许推节点 |
| W15 | `resolve_budget_violations` | space_budget_guard.rs:46 | 兜底消重叠 | **删除（flowchart）** → solver P0 保证无重叠 |
| W16 | `restore_after_node_moves` | group_frame/pass.rs:61 | 组框恢复 | **保留**（flowchart group 是后验装饰，通常不移动节点） |

### Architecture 专属（暂不动）

| # | 函数 | 文件 | 未来归属 |
|---|------|------|----------|
| W17 | `reassert_multi_client_hub_centroids` | architecture_v2/post_layout.rs:284 | Phase 10 迁移 |
| W18 | `align_cross_scope_pendant_chains` | architecture_v2/post_layout.rs:145 | Phase 10 迁移 |
| W19 | `nudge_intra_nodes_toward_cross_group_edges` | architecture_v2/two_phase/phase_d.rs:335 | Phase 10 迁移 |
| W20 | `nudge_cross_group_y_alignment` | architecture_v2/two_phase/phase_d.rs:626 | Phase 10 迁移 |
| W21 | `enforce_horizontal_demand_gaps` | architecture_v2/layout/coordinate.rs:163 | Phase 10 迁移 |

---

## 关键发现

### 1. Flowchart 节点的写者共 15 个（W2-W16）

其中：
- **保留 3 个**：W2（BK 初值）、W9（全图平移）、W11（grid snap finalize）
- **保留但限制 1 个**：W13（竖向 rank 缝，属于 finalize 合法操作）
- **保留但观察 1 个**：W16（flowchart group 通常不触发节点移动）
- **删除/迁移 9 个**：W3、W4、W5、W6、W7、W8、W10、W12、W14、W15

### 2. 破坏链确认

以 onboarding 图为例：
```text
W2 BK 选择单侧 spine（按出度+id 贪心）
  → W3 compact 加权 spine 邻居
  → W4 fan symmetry 尝试拉回质心（与 W3 冲突）
  → W5 overlap 前扫后扫（破坏 W4 对称）
  → W6 singleton 按已偏移前驱 median（累积偏差）
  → W12 enforce_horizontal_gaps 再推（破坏所有对齐）
```

### 3. 隐藏写者

- **W14 `push_problem_nodes`**：在 refine 循环中，每轮都可能推动节点。之前未被文档 11 显式列出。
- **W15 `resolve_budget_violations`**：在 pipeline S3 兜底中，post-route 仍可推节点（在 node_freeze 之前）。
- **W16 `restore_after_node_moves`**：group frame 恢复时可能移动节点（对 flowchart 影响小，但存在）。

### 4. NodeFreeze 屏障位置

当前 `NodeFreeze::capture` 在 pipeline.rs:225，位于：
- W15 resolve_budget_violations **之后**
- W16 restore_after_node_moves **之后**
- grid_snap edge waypoints **之前**

这意味着 W12-W16 都在 freeze 之前合法执行。新架构要把这些全部收口到 solver + finalize。

---

## 下一步

1. 定义只读诊断结构（Phase 1 剩余任务）
2. 选取目标样例记录各阶段坐标证据
3. 进入 Phase 2：IR 骨架 + PAVA
