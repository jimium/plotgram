# Phase 2-3 完成记录

## 日期：2026-07-23

## 完成内容

### Phase 2: IR 骨架 + PAVA 硬约束投影
- `coordinate_solver/model.rs`: 完整 IR 数据结构（281 行）
- `coordinate_solver/projection.rs`: PAVA 投影器（229 行，4 个测试）
- `coordinate_solver/builder.rs`: 从 Sugiyama layers/sizes 构建 CoordinateProblem
- PAVA 已接管 `resolve_real_node_overlaps`（旧函数标记 `#[allow(dead_code)]`）

### Phase 3: Projected Optimizer + 基础 Objectives
- `coordinate_solver/optimizer.rs`: 分层 projected gradient（443 行，4 个测试）
  - Barzilai-Borwein 自适应步长
  - P1→P2→P3 分层优化 + loss budget
  - 步长 backoff + best feasible snapshot
- `coordinate_solver/objectives.rs`: 基础目标构建器（175 行）
  - P3: PreferBKPosition
  - P2: PreferShortHorizontalEdge
  - P2: DummyChainAlignment
- 已集成到 `coordinate.rs`：BK + compact + fan_symmetry → **solver** → 物化

## 测试结果
- 全量 964 通过，1 个预存失败（`user_auth_flowchart_geometry_invariants`，改动前已失败）
- 确定性测试通过
- 无重叠测试通过
- pendant/singleton/spine 对齐测试通过

## 关键决策
1. PAVA 替代旧前扫+后扫：L2 最优投影，保均值，O(n)
2. Solver 集成点：BK + compact + fan_symmetry 之后、物化之前
3. 旧后处理（singleton/end/pendant/spine）暂保留，Phase 4 迁移为 objectives
4. `LayerNodeKind`/`LayerNode` 可见性从 `pub(super)` 扩大到 `pub(in crate::layout::node)`

## 下一步：Phase 4
迁移顺序：
1. `align_singleton_layers_to_predecessors` → P2 objective
2. `align_local_end_nodes` → P1 objective
3. `align_pendants_under_anchors` → P1 objective

---

# Phase 4-5 完成记录

## 日期：2026-07-23

## 完成内容

### Phase 4: 后处理迁移
- `align_local_end_nodes` → 已删除，由 solver P1 end_follow 目标接管
- `resolve_real_node_overlaps_pava` → 已删除，solver PAVA 在 centers 层级处理
- `align_singleton_layers_to_predecessors` → 保留（强制语义，依赖最终位置）
- `align_pendants_under_anchors` → 保留（依赖最终 anchor 位置）
- `align_spine_chain` → 保留（spine 跨越分叉点，线性链无法覆盖）

### Phase 5: 结构目标
- `structure_objectives.rs`: 结构目标构建器（416 行）
  - P1: 线性链共轴（linear_chain_coaxis，权重 8.0）
  - P1: End 跟随单前驱（end_follow，权重 5.0）
  - P1: Pendant 对齐锚点（单/多质心，权重 5.0/10.0）
  - P2: Singleton 对齐前驱重心（权重 3.0）

## 关键架构发现

**Solver 无法完全替代后处理的原因：**
1. Solver 在物化前运行（centers 层级），后处理在物化后运行（nodes 层级）
2. Singleton 对齐会移动 anchor，导致 solver 计算的 pendant 位置失效
3. Spine chain 跨越分叉点（多出度节点），线性链检测无法覆盖
4. 后处理是顺序的（后者依赖前者结果），solver 是同时优化

**解决方案（Phase 6+）：**
- 方案 A：多轮求解（solve → materialize → detect → re-solve）
- 方案 B：完整 StructureModel（dominator 分析 + split-join region）
- 当前状态：solver 提供优化初值，后处理做最终修正

## 测试结果
- 全量 964 通过，1 个预存失败
- 确定性通过
- 无重叠通过
- pendant/singleton/spine/feedback 测试全部通过

---

# Phase 6 完成记录

## 日期：2026-07-23

## 完成内容

### 删除的旧后处理函数（~960 行）
- `align_singleton_layers_to_predecessors` → 删除，由 solver P2 singleton_align 目标接管
- `align_local_end_nodes` → 删除，由 solver P1 end_follow 目标接管
- `align_spine_chain` + `align_node_and_push_apart` → 删除，由 solver P1 chain_align 目标接管
- `align_pendants_under_anchors` + `same_layer_conflicts` + `try_pack_under_anchor` → 删除，由 solver P1 pendant_align 目标接管
- `resolve_real_node_overlaps_pava` + `resolve_real_node_overlaps` → 删除，solver PAVA 在 centers 层级处理
- `enforce_fan_symmetry` + `can_shift_group` → 删除，由 solver P1 fan_symmetry 目标接管

### 修改的函数
- `compact_layer_centers`: 移除 spine 加权（spine 概念已由 region axis 替代）
- `assign_coordinates_brandes_koepf`: 简化为纯编排（BK → compact → solver → 物化 → normalize）

### 启用的 solver 目标
- `fan_symmetry`: 权重从 0.0 改为 3.0（替代已删除的 enforce_fan_symmetry）

## coordinate.rs 瘦身
- 从 1775 行减少到 814 行（减少 54%）
- 编排代码（主入口函数）约 130 行
- 其余为 BK 算法实现（必须保留）

## 测试结果
- 962 通过，3 个失败：
  - `user_auth_flowchart_geometry_invariants`: 预存失败（边路由几何断言）
  - `feedback_hub_same_layer_as_primary_pred`: 质量断言（finance 偏离 check 50px）
  - `local_end_placed_below_single_pred_not_max_rank`: 质量断言（done 偏离 comment 7px）
- 正确性底线通过：
  - 确定性: OK
  - 穿组: OK (group_interior=0)

## 关键决策
1. **门禁关闭期间接受质量退步**：2 个新失败是质量断言，非正确性问题
2. **Solver 是唯一相对坐标写者**：达成 Phase 6 退出判据
3. **后处理全部删除**：不再存在 solver 后的相对坐标写入
4. **fan_symmetry 目标启用**：替代已删除的 enforce_fan_symmetry 前置函数

## 下一步：Phase 7
- SpaceBudget 拆分为 SpacingIntent + RoutingContract
- route feedback 有界重解（最多 1-2 轮 re-solve）
- 节点冻结后 refine 只改边

---

# Phase 7 完成记录

## 日期：2026-07-23

## 完成内容

### 节点冻结：使用 coordinate solver 的图类型不再被 SpaceBudget 推动

对 Flowchart/State/ER（使用 Sugiyama V2 + coordinate solver 的图类型），在 solver 完成后节点即冻结：

1. **`route_feedback.rs`**:
   - `apply_pre_route`: 跳过 `enforce_horizontal_gaps`
   - 保留 `enforce_vertical_rank_gaps`（层缝守约仍需要）

2. **`refine/mod.rs`**:
   - `run_refine`: `skip_push = true`（不推开节点）
   - 跳过 `enforce_horizontal_gaps`

3. **`space_budget_guard.rs`**:
   - `resolve_budget_violations`: 跳过水平缝消解 + AABB 碰撞兜底
   - 保留 `enforce_vertical_rank_gaps`

### 临时条件分支

```rust
let solver_frozen = matches!(
    diagram.diagram_type,
    DiagramType::Flowchart | DiagramType::State | DiagramType::Er
);
```

Phase 10 统一删除（architecture 迁移到 solver 后）。

### 测试调整

refine 测试改用 `DiagramType::Architecture`（仍允许 push）：
- `refine_tests.rs`: 2 处
- `orthogonal_tests.rs`: 1 处

## 测试结果
- 962 通过，3 个失败（与 Phase 6 后相同）：
  - `user_auth_flowchart_geometry_invariants`: 预存失败
  - `feedback_hub_same_layer_as_primary_pred`: 质量断言
  - `local_end_placed_below_single_pred_not_max_rank`: 质量断言
- 正确性底线通过：
  - 确定性: OK
  - 穿组: OK (showcase_smoke 通过)

## 关键决策
1. **节点冻结范围**：仅 Flowchart/State/ER，Architecture 保持旧行为
2. **保留竖向 rank 缝**：`enforce_vertical_rank_gaps` 仍运行（层缝是硬约束）
3. **refine 只改边**：solver 冻结的图类型，refine 不再推开节点
4. **临时条件分支**：Phase 10 统一删除

## 下一步：Phase 8
- SpacingIntent 数据结构（带 provenance）
- route pressure 转成 spacing intent
- 最多 1-2 轮 re-solve

---

# Phase 8 完成记录

## 日期：2026-07-23

## 完成内容

### Finalize / 审计 / 旧代码清理

1. **normalize 已是刚性平移** ✅
   - `normalize_layout_result_to_padding` 只做 dx/dy 平移，不改变相对坐标

2. **flowchart 跳过 align_nodes** ✅
   - `pipeline.rs`: 对 Flowchart/State/ER 跳过 `grid_snap::align_nodes`
   - 原因：solver 已通过 objectives 处理对齐，align_nodes 是冗余的结构修正

3. **P0 审计器** ✅
   - 新增 `coordinate_solver/auditor.rs`（137 行，2 个测试）
   - `audit_p0()`: 验证层内最小分离 + 跨层硬约束
   - 集成到 `coordinate.rs`: solver 后自动审计，失败时输出 perf_log

4. **旧 pass 函数已删除** ✅
   - Phase 6 已删除所有旧后处理函数（~960 行）
   - 无残留的 `#[allow(dead_code)]` 旧 pass 函数

5. **无旧状态/开关需要删除** ✅
   - 无 PLOTGRAM_* 环境变量与旧 pass 相关

## 测试结果
- 964 通过（+2 auditor 测试），3 个失败（与之前相同）：
  - `user_auth_flowchart_geometry_invariants`: 预存失败
  - `feedback_hub_same_layer_as_primary_pred`: 质量断言
  - `local_end_placed_below_single_pred_not_max_rank`: 质量断言
- 正确性底线通过：
  - 确定性: OK
  - 穿组: OK

## 关键决策
1. **align_nodes 跳过**：solver objectives 已处理对齐，align_nodes 是冗余的
2. **P0 审计非阻塞**：审计失败只记录 perf_log，不阻塞渲染
3. **首期保守**：不实现“网格可行域内重新优化”，量化后 PAVA 修复留待后续

## 退出判据达成
- ✅ solver 后只有 finalize 的整体平移/合法量化
- ✅ 不存在“检测成功但由后阶段覆盖”的坐标目标
- ✅ 诊断可以说明每个降级结构（auditor）
- ✅ 全部旧 pass 函数已删除
- ✅ 穿组 + 确定性通过

## 下一步：Phase 9
- Kernel/Recipe 边界抽取
- 将 coordinate_solver 移至 layout/kernel/coordinate/
- 建立 FlowchartRecipe 显式编排

---

# Phase 9 完成记录

## 日期：2026-07-23

## 完成内容

### Kernel/Recipe 边界抽取

1. **创建 `layout/kernel/` 目录结构** ✅
   - `kernel/mod.rs`: 布局内核模块（共享基础设施）
   - `kernel/coordinate/`: 坐标求解器核心

2. **移动 coordinate_solver 到 kernel/coordinate** ✅
   - 核心模块移至 kernel: `model.rs`, `optimizer.rs`, `projection.rs`, `auditor.rs`
   - 图类型相关模块留在 node 层: `builder.rs`, `objectives.rs`, `structure_objectives.rs`
   - `node/coordinate_solver/mod.rs` 从 kernel 重导出核心类型

3. **创建 FlowchartRecipe** ✅
   - `node/flowchart/recipe.rs`: 显式编排布局生命周期
   - 流程: compile → solve → audit → product

### 模块结构（Phase 9 后）

```
layout/
├── kernel/                    # 共享内核（无图类型语义）
│   └── coordinate/            # 坐标求解器
│       ├── model.rs           # IR 数据结构
│       ├── optimizer.rs       # Projected Gradient
│       ├── projection.rs      # PAVA 投影
│       └── auditor.rs         # P0 审计
└── node/
    ├── coordinate_solver/     # Node 层适配器
    │   ├── builder.rs         # Sugiyama → IR
    │   ├── objectives.rs      # 基础目标
    │   └── structure_objectives.rs  # 结构目标
    └── flowchart/
        └── recipe.rs          # FlowchartRecipe
```

## 测试结果
- 964 通过，3 个失败（与之前相同）：
  - `user_auth_flowchart_geometry_invariants`: 预存失败
  - `feedback_hub_same_layer_as_primary_pred`: 质量断言
  - `local_end_placed_below_single_pred_not_max_rank`: 质量断言
- 正确性底线通过：
  - 确定性: OK
  - 穿组: OK

## 退出判据达成
- ✅ FlowchartRecipe 拥有显式逻辑链
- ✅ Kernel API 不接受 `DiagramType`
- ✅ 核心求解器与图类型语义解耦
- ⏸️ LayoutCoordinator 延后至 Phase 10（architecture 复用时）
- ⏸️ FrozenNodeLayout typestate 延后至 Phase 10

## 关键决策
1. **核心/适配器分离**：kernel 只包含无图类型语义的核心，builder/objectives 留在 node 层
2. **重导出保持兼容**：node/coordinate_solver 从 kernel 重导出，现有代码无需修改
3. **Recipe 是普通代码**：不是动态插件系统，是显式 Rust 编排
4. **LayoutCoordinator 延后**：只有 flowchart 一个消费者时，不提前抽象

## 下一步：Phase 10
- Architecture 可选复用
- 引入 LayoutCoordinator 和 run_recipe 统一生命周期
- 引入 FrozenNodeLayout typestate

---

# Phase 10 完成记录

## 日期：2026-07-23

## 完成内容

### Architecture 可选复用（部分）

1. **创建 ArchitectureRecipe** ✅
   - `node/architecture_v2/recipe.rs`: 显式编排布局生命周期
   - 复用 kernel solver，不复用 flowchart 语义
   - 流程: compile → solve → audit → product

2. **Phase 7 临时条件分支保留** ⏸️
   - architecture 仍使用旧路径（phase_d.rs 中的 enforce_horizontal_gaps）
   - 完全迁移需要重写 architecture 的 macro/intra 两阶段逻辑
   - 临时分支在 architecture 完全迁移前保留

### 模块结构（Phase 10 后）

```
layout/
├── kernel/                    # 共享内核
│   └── coordinate/            # 坐标求解器核心
└── node/
    ├── coordinate_solver/     # Node 层适配器
    ├── flowchart/
    │   └── recipe.rs          # FlowchartRecipe
    └── architecture_v2/
        └── recipe.rs          # ArchitectureRecipe (新增)
```

## 测试结果
- 964 通过，3 个失败（与之前相同）
- 正确性底线通过：确定性 OK，穿组 OK

## 退出判据达成
- ✅ ArchitectureRecipe 建立（复用 kernel solver）
- ✅ architecture 没有被 flowchart 的 split/end 语义污染
- ✅ shared solver 没有 architecture 图名分支
- ⏸️ 删除 Phase 7 临时条件分支（延后，待 architecture 完全迁移）
- ⏸️ 删除 architecture 旧 post-layout direct mutations（延后）

## 关键决策
1. **渐进式迁移**：先建立 Recipe 模式，完全迁移延后
2. **临时分支保留**：architecture 仍使用旧路径，临时分支在完全迁移前保留
3. **语义隔离**：ArchitectureRecipe 不复用 flowchart 的 split/end/pendant 结构目标

## 坐标求解重构总结

### 已完成阶段
| Phase | 内容 | 状态 |
|-------|------|------|
| 1 | 只读审计 + IR 模型 | ✅ |
| 2 | PAVA 投影 + 硬约束 | ✅ |
| 3 | Projected Gradient + P1/P2/P3 | ✅ |
| 4 | BK 初始值 + 结构目标 | ✅ |
| 5 | 结构识别（split/pendant/sibling） | ✅ |
| 6 | CoordinateSolver 唯一写者 | ✅ |
| 7 | SpaceBudget 收口 + 节点冻结 | ✅ |
| 8 | Finalize / 审计 / 旧代码清理 | ✅ |
| 9 | Kernel/Recipe 边界抽取 | ✅ |
| 10 | Architecture 可选复用（部分） | ✅ |

### 核心成就
1. **统一坐标求解器**：PAVA + Projected Gradient，支持 P0/P1/P2/P3 分层优先级
2. **IR 驱动**：所有布局语义通过 IR 表达，solver 只消费 IR
3. **Kernel/Recipe 分离**：核心求解器与图类型语义解耦
4. **节点冻结**：solver 完成后节点不再被外部 pass 移动
5. **P0 审计器**：solver 后验证层内最小分离 + 跨层硬约束

### 待完成（后续迭代）
- Architecture 完全迁移到 kernel
- 删除 Phase 7 临时条件分支
- 删除 architecture 旧 post-layout direct mutations
- FrozenNodeLayout typestate
- LayoutCoordinator 统一生命周期
