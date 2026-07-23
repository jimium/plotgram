# 统一坐标求解与 Kernel/Recipe 架构：执行推进计划

> 日期：2026-07-23  
> 状态：执行中  
> 分支：全新重构分支，门禁关闭，不考虑暂时退化  
> 设计依据：
> - [`11-统一坐标约束求解架构与执行计划-2026-07.md`](./11-统一坐标约束求解架构与执行计划-2026-07.md)（CoordinateKernel 详细设计）
> - [`12-多图类型布局共享内核与独立配方架构-2026-07.md`](./12-多图类型布局共享内核与独立配方架构-2026-07.md)（上层 Kernel/Recipe/Coordinator 架构）
>
> 验收策略：最终验收时统一看指标，过程中只守正确性底线（穿组 + 确定性）。

---

## 0. 执行原则

1. **先文档 11，后文档 12**：文档 11 是具体可执行的 CoordinateKernel 实现路径；文档 12 是架构北极星，在 Kernel 稳定后再抽取边界。
2. **每阶段独立可编译**：`cargo check -p plotgram-core` 必须通过。
3. **正确性底线不豁免**：穿组（`edge_crosses_group_interior`）和确定性（`det=true`）仍然硬。
4. **不保留双轨**：每迁移一个旧 pass，立即删除其 direct mutation，不做长期新旧并存。
5. **验证用 `cargo run -p plotgram-cli`**：不信任陈旧 binary。
6. **固定排序、固定迭代、禁止时间停机、禁止图名特判**。

---

## 1. 阶段总览

```text
Phase 1 ─ 写权地图与只读诊断（M0）
Phase 2 ─ IR 骨架 + PAVA 硬约束投影（M1 + M2）
Phase 3 ─ Projected Optimizer + 基础 Objectives（M3）
Phase 4 ─ 迁移简单后处理为 Intent（M4）
Phase 5 ─ StructureModel + Split-Join（M5）
Phase 6 ─ 统一剩余写权 + 删除旧 pass（M6）
Phase 7 ─ SpaceBudget 收口 + Route Feedback（M7）
Phase 8 ─ Finalize / 审计 / 旧代码清理（M8）
Phase 9 ─ Kernel/Recipe 边界抽取（文档 12 的 M3-M6）
Phase 10 ─ Architecture 可选复用（文档 11 M9 / 文档 12 M4）
```

---

## 2. Phase 1：写权地图与只读诊断

**对应**：文档 11 §10.2 M0

### 目标

不改变任何布局行为，让所有坐标覆盖关系可观察。

### 任务

1. 枚举 Sugiyama 后所有移动节点的函数，建立写权清单：
   - `coordinate.rs` 内：`compact_layer_centers`、`enforce_fan_symmetry`、`resolve_real_node_overlaps`、`align_singleton_layers_to_predecessors`、`align_spine_chain`、`align_local_end_nodes`、`align_pendants_under_anchors`、`normalize_layout_to_padding`
   - `space_budget.rs`：`enforce_horizontal_gaps`
   - `route_feedback` / `refine` 中可能推节点的入口
   - grid snap
2. 定义只读 diagnostic snapshot 结构：
   - 每层节点中心坐标
   - 最小间距 violation
   - fan centroid deviation
   - split/join axis deviation（如果可识别）
   - 标记"哪个阶段移动了哪些节点"
3. 诊断通过现有 `perf_log!` 或 debug flag 输出，默认关闭。
4. 选取 3-5 个目标样例，记录各阶段前后坐标证据。

### 产物

- 写权清单（标注每个写者的未来归属：删除 / 迁移为 intent / 保留为 finalize）
- 诊断结构定义
- 样例证据

### 退出判据

- 能回答"onboarding 图在哪一步开始偏离父轴"
- 能列出 solver 上线后必须删除或改造的全部直接写者
- 如发现未纳入计划的写者，先更新文档 11 写权边界

### 风险

- **隐藏写者**：可能存在 `refine` 或 pipeline 通用层中未被注意到的节点移动。发现后必须先补入清单再继续。

---

## 3. Phase 2：IR 骨架 + PAVA 硬约束投影

**对应**：文档 11 §10.3 M1 + §10.4 M2

### 目标

建立 `CoordinateProblem` 数据模型和 PAVA 投影器，行为仍保持不变（双跑比较）。

### 任务

#### 2A：IR 骨架

1. 新建 `crates/plotgram-core/src/layout/node/coordinate_solver/` 模块。
2. 实现 `model.rs`：`NodeVariable`、`AxisVariable`、`HardConstraint`、`ObjectiveTerm`、`ConstraintSource`、`CoordinateProblem`、`InitialCoordinates`。
3. 从现有 layers/sizes 构建 `CoordinateProblem`（只读转换）。
4. BK 坐标填入 `InitialCoordinates`。
5. 现有层内 node gap 编译为 `MinSeparation`。
6. 所有枚举和 Vec 显式排序，不依赖 HashMap 迭代。
7. 加入 auditor 骨架，输出旧坐标在 hard constraint 下的 violation 报告。
8. **仍返回旧坐标**，不启用新结果。

#### 2B：PAVA 投影器

1. 实现累计距离变换：`s[i] = Σ d[k]`，`y[i] = x[i] - s[i]`。
2. 实现 weighted PAVA（Pool Adjacent Violators）。
3. 支持变宽节点、per-layer gap。
4. 支持 `LowerBound` / `UpperBound` / `Fixed` 及冲突报告。
5. 双跑：旧 `resolve_real_node_overlaps` 与新 PAVA projection 并行，只比较输出。
6. 验证新结果确定性（同输入多次相同）。
7. 稳定后由 PAVA 接管 overlap 职责，删除 `resolve_real_node_overlaps` 的坐标写入。

### 退出判据

- 同输入多次生成完全相同的变量和 constraint 顺序
- 所有层满足 min separation
- `resolve_real_node_overlaps` 不再写坐标
- SVG 输出无穿组、确定性通过

### 风险

- **PAVA 与旧行为差异**：旧 overlap 是前扫+后扫+回中，PAVA 是精确投影。结果会不同，但应更好。门禁关闭期间接受差异。
- **Dummy 节点处理**（⚠️ 设计空白）：dummy 宽度极小或为 0，其 min_separation 与真实节点不同。`NodeVariable` 需要 `kind: Real | Dummy` 字段，PAVA 中 dummy 使用专属 gap。首期 dummy 保持 BK 初值不单独优化，但其 separation 必须正确编码。

---

## 4. Phase 3：Projected Optimizer + 基础 Objectives

**对应**：文档 11 §10.5 M3

### 目标

实现统一求解器，让 BK 初值、边拉直和紧凑度进入统一优化。

### 任务

1. 实现稀疏 residual loss 与 gradient 计算。
2. 实现 projected optimizer：
   - 固定最大迭代次数（首期 P1: 80, P2: 40, P3: 30）
   - 确定性的初始步长
   - 固定次数二分 backoff
   - 每步后 PAVA 投影
   - best feasible snapshot
3. 实现 P1/P2/P3 分层 loss budget。
4. 实现未收敛 degraded 诊断。
5. 加入基础 objectives：
   - P3: `PreferBKPosition` — `(x[node] - initial_x[node])²`
   - P2: `PreferShortHorizontalEdge` — `(x[from] - x[to])²`
   - P2: dummy chain alignment — dummy 链共线
6. 输出相对坐标，全图 normalize 只做刚性平移。
7. 关闭新增 objective 时，结果应等价于 BK 初值的 PAVA 硬投影。

### 退出判据

- P0 始终满足（PAVA 后无 separation violation）
- 无 wall-clock timeout
- WASM 编译路径无不兼容依赖
- 确定性通过
- 关闭 objectives 时结果 = BK + PAVA

### 风险

- **⚠️ 收敛质量**：固定步长 projected gradient 对病态问题收敛慢。
  - **缓解**：实现简单的 Barzilai-Borwein 自适应步长（用前两步梯度差估计 Lipschitz 常数），不增加架构复杂度。
  - **兜底**：如果 MAX_ITER 内未收敛，返回 best feasible snapshot 并标记 degraded。
  - **后续**：Phase 3 稳定后评估是否需要 active-set 快路径（对几十节点的小图，直接解 KKT 可能更快更精确）。

- **⚠️ 分层 Tolerance 设定**：
  - 首期方案：`tolerance_p1 = max_p1_term_weight * (0.5 * node_gap)²`，即"P1 loss 增加不超过半个节点间距对应的能量"。
  - 如果实际效果过紧/过松，在 Phase 5 引入结构目标后再微调。

- **Dummy 节点在目标函数中的角色**：
  - dummy chain alignment 目标：`(x[dummy_i] - x[dummy_{i+1}])²`，权重低于真实边。
  - dummy 不进入 `PreferBKPosition`（它们没有用户语义）。

---

## 5. Phase 4：迁移简单后处理为 Intent

**对应**：文档 11 §10.6 M4

### 目标

将语义明确的局部后处理迁移为 objective，删除对应直接写者。

### 迁移顺序

1. `align_singleton_layers_to_predecessors` → P2 objective：singleton 对齐前驱重心
2. `align_local_end_nodes` → P1 objective：单前驱 end 跟随所属 chain
3. `align_pendants_under_anchors` → P1 objective：pendant pack 质心对齐 anchor

### 每个迁移的步骤

1. 旧函数改为生成 `ObjectiveTerm`（不写坐标）。
2. 双跑：solver 结果 vs 旧函数结果，输出 deviation。
3. 验证 hard separation 仍满足。
4. 删除旧函数的坐标写入。
5. 更新写权清单。

### 退出判据

- 上述三个函数不再直接写布局
- solver 之后不存在对应补救 pass
- 穿组 + 确定性通过

### 风险

- **低风险**：这三个函数语义明确，迁移为二次目标自然。主要注意 pendant pack 可能包含多个节点，需要生成质心目标而非单节点目标。

---

## 6. Phase 5：StructureModel + Split-Join

**对应**：文档 11 §10.7 M5

### 目标

从"贪心选 spine"升级为"识别 region axis"，实现 split-join 共轴和 fan 对称。

### 任务

1. 构建 effective DAG 结构输入（复用现有 FAS 结果）。
2. 标记 feedback edges（不参与普通主轴）。
3. 实现确定性 dominator/post-dominator（bitset，小图 O(V²) 足够）。
4. 识别严格 split-join：
   - 多出度节点 → 候选 split
   - 最近共同 post-dominator → 候选 join
   - 检查 SESE 近似
   - 收集分支节点
   - 稳定分支顺序（rank → order → 声明序 → id）
5. 构建嵌套 region 层次（laminar）。
6. 为 region 创建 virtual axis 变量。
7. 生成 objectives：
   - P1: split → axis 共轴
   - P1: join → axis 共轴
   - P1: continuation → axis
   - P1: child centroid → axis
   - P1: 双分支 mirror pair
8. 置信度分级：只有 Exact/High 进入 P1，Medium/Low 进入 P2 或降级 BK。
9. 无法识别时回退 BK，不猜测。

### 目标样例

- `showcase/flowchart/product.employee-onboarding.pgm`
- 普通菱形决策图
- 嵌套 fan-out/fan-in
- 有 feedback edge 的审批图

### 退出判据

- onboarding 中 split、join、end continuation 共轴
- IT/行政关于 axis 对称
- 不再由 id tie-break 选择分支作为主干
- 反馈边不进入普通 fan-in median
- 确定性通过

### 风险

- **⚠️ 结构误识别**：错误 split-join 比没有更糟。
  - 缓解：严格 SESE 检查 + 置信度分级 + laminar 验证。
  - 非 laminar 交叠 region → 降级为低置信 soft intent。
  - 首期宁可漏识别（回退 BK），不可误识别。

- **⚠️ 同层边 / 反馈边的坐标影响**（设计空白）：
  - 同层边两节点已在同一 PAVA 链中，其间距由 min_separation 保证。
  - 反馈边不生成普通共轴目标；FeedbackRegion 生成"侧支保持侧向"的 P2 目标。
  - 具体规则：feedback hub 对齐主链 axis，feedback 侧支偏好位于主链一侧（通过不等式 soft bound 表达）。
  - 首期如果无法清晰表达，feedback 结构暂不生成目标，保持 BK 初值。

- **Virtual axis 无 PAVA 约束**：
  - axis 变量不属于任何层，不受 PAVA 投影。
  - 其更新仅由梯度步驱动，可能被相连节点的 PAVA 投影"拉扯"。
  - 缓解：axis 使用与节点相同的步长策略；如果 axis 振荡（连续两步方向相反），缩小步长。

---

## 7. Phase 6：统一剩余写权 + 删除旧 pass

**对应**：文档 11 §10.8 M6

### 目标

将 `enforce_fan_symmetry`、`align_spine_chain`、`compact_layer_centers` 的 spine 加权全部迁移为 intent，删除所有 solver 后相对坐标写入。

### 任务

1. `enforce_fan_symmetry` → P1 SymmetryRegion objective（已由 Phase 5 部分覆盖，此处处理非 split-join 的普通 fan）。
2. `align_spine_chain` → P1 LinearChain objective（高置信 chain 共轴）。
3. `compact_layer_centers` 中 spine 加权 → 删除（spine 概念已被 region axis 替代）。
4. 删除所有 solver 后相对坐标写入。
5. `coordinate.rs` 仅保留编排入口：构建 IR → 调 solver → 物化坐标 → finalize。

### 退出判据

- CoordinateSolver 是 Sugiyama 相对主轴坐标唯一写者
- 旧 fan/spine/singleton/end/pendant direct mutation 已删除
- `coordinate.rs` 行数大幅缩减（目标 < 300 行编排代码）
- 穿组 + 确定性通过

### 风险

- **中风险**：这是删除量最大的阶段，可能遗漏某些边界情况的旧逻辑。
  - 缓解：Phase 1 的写权清单必须完整；每删一个函数后跑全量 flowchart showcase 验穿组。

---

## 8. Phase 7：SpaceBudget 收口 + Route Feedback

**对应**：文档 11 §10.9 M7

### 目标

将坐标管线外部写者收口，SpaceBudget 只生成 intent，route feedback 有界重解。

### 任务

1. `SpaceBudget` 改为生成 `SpacingIntent`（数据），不再直接推节点。
2. `enforce_horizontal_gaps` 不再用于冻结后的 flowchart 节点。
3. route pressure 转成带 provenance 的 spacing intent。
4. 实现最多 1-2 轮 re-solve：
   - solve → route probe → 新增 spacing intent → re-solve affected components → final route
5. re-solve 后重建 group frame 和路由输入。
6. 节点冻结后 refine 只改边。

### 退出判据

- route/refine 后节点坐标不再变化
- 所有节点移动都能追溯到一次 solver run
- 不出现 layout-route 无限反馈
- 穿组 + 确定性通过

### 风险

- **⚠️ 1-2 轮反馈可能不够**：
  - 对极复杂图（大量交叉边），1-2 轮后仍有压力。
  - 处理：保留最佳正确结果，标记 degraded，不无限振荡。
  - 门禁关闭期间接受"有些图间距不完美但正确"。

- **Architecture 的 SpaceBudget 依赖**：
  - 架构图当前也使用 `enforce_horizontal_gaps`。
  - 本阶段只改 flowchart 路径；architecture 保持旧行为，等 Phase 10 再迁移。
  - 需要条件分支：`if flowchart → 新路径; else → 旧路径`（临时，Phase 10 删除）。

---

## 9. Phase 8：Finalize / 审计 / 旧代码清理

**对应**：文档 11 §10.10 M8

### 目标

完成新架构收口，删除所有旧代码。

### 任务

1. 全图 normalize 只做刚性平移。
2. grid snap 并入 finalize：
   - 确定性量化
   - 量化后 PAVA 硬修复
   - 不做"网格域内重优化"（首期保守）
3. auditor 在冻结点复核全部 P0。
4. 删除旧 overlap/fan/singleton/spine/end/pendant 修补函数残余。
5. 删除只为旧 pass 存在的状态和开关。
6. 更新核心手册的坐标写权地图。

### 退出判据

- solver 后只有 finalize 的整体平移/合法量化
- 不存在"检测成功但由后阶段覆盖"的坐标目标
- 诊断可以说明每个降级结构
- 全部旧 pass 函数已删除
- 穿组 + 确定性通过

### 风险

- **⚠️ Grid Snap 复杂度**：
  - 首期策略：量化 → PAVA 修复 → 接受微小 P1 偏差 → 标记 degraded。
  - 不实现"网格可行域内重新优化"（那是整数 QP，复杂度不匹配首期投入）。
  - 如果 grid snap 后 P1 偏差超过 1px，记录但不阻塞。

---

## 10. Phase 9：Kernel/Recipe 边界抽取

**对应**：文档 12 的 M1-M6（在文档 11 完成后执行）

### 目标

将已稳定的 CoordinateKernel 从 flowchart 代码中抽取为共享内核，建立 Recipe/Coordinator 架构。

### 任务

1. 将 `coordinate_solver/` 移至 `layout/kernel/coordinate/`。
2. 引入 `PreparedLayoutInput` 和 `StableGraph`（先提供 adapter，不删旧结构）。
3. 建立 `FlowchartRecipe` 显式编排（compile → solve → product）。
4. 引入 `LayoutCoordinator` 和 `run_recipe` 统一生命周期。
5. 引入 `NodeLayoutProduct → FrozenNodeLayout` typestate。
6. 路由签名逐步改成只读 nodes。
7. 拆分 SpaceBudget 为 `SpacingDemand` + `RoutingContract`。

### 退出判据

- FlowchartRecipe 拥有显式逻辑链
- 公共 Coordinator 不包含图类型业务分支
- Kernel API 不接受 `DiagramType`
- freeze 后节点不可修改（类型层面或 fingerprint 守卫）

### 风险

- **过度抽象**：
  - 防护：只有 flowchart 一个真实消费者时，不提前为 architecture 设计 hook。
  - Recipe 是普通 Rust 编排代码，不是动态插件系统。
  - 目录可以后移，先建立逻辑边界。

---

## 11. Phase 10：Architecture 可选复用

**对应**：文档 11 §10.11 M9 / 文档 12 M4

### 目标

仅在 flowchart 稳定后执行。Architecture 复用 solver/model/projection/audit，不复用 flowchart 语义。

### 任务

1. 保留 architecture macro/intra 两阶段语义。
2. 新建 `ArchitectureRecipe` + `ArchitectureIntentBuilder`。
3. 组内 layered path 委托 LayeredKernel。
4. macro block 坐标尝试使用同一 CoordinateKernel。
5. 删除 architecture 旧 post-layout direct mutations。
6. 删除 Phase 7 中的临时条件分支。

### 退出判据

- architecture 没有被 flowchart 的 split/end 语义污染
- shared solver 没有 architecture 图名分支
- 穿组 + 确定性通过

---

## 12. 风险登记表

| # | 风险 | 影响阶段 | 严重度 | 缓解策略 |
|---|------|----------|--------|----------|
| R1 | Projected gradient 收敛慢/质量差 | Phase 3 | 中高 | Barzilai-Borwein 自适应步长；兜底 best feasible snapshot；后续评估 active-set |
| R2 | 分层 tolerance 过紧/过松 | Phase 3+ | 中 | 按物理意义设定（0.5×node_gap 能量）；Phase 5 后根据结构目标效果微调 |
| R3 | Dummy 节点处理空白 | Phase 2-3 | 中 | NodeVariable 加 kind 字段；dummy 使用专属 gap；dummy chain 共线目标 |
| R4 | 同层边/反馈边坐标规则不明 | Phase 5 | 中 | 首期 feedback 不生成目标保持 BK；同层边只走 min_separation；后续迭代补充 |
| R5 | Grid snap 破坏 P1 | Phase 8 | 中 | 首期只做量化+PAVA修复，不做整数域重优化；偏差标记 degraded |
| R6 | 结构误识别（split-join） | Phase 5 | 中高 | 严格 SESE + 置信度分级 + laminar 验证；宁可漏识别不误识别 |
| R7 | 隐藏坐标写者遗漏 | Phase 1/6/7 | 中 | Phase 1 全面枚举；每删一个函数后跑全量验穿组；freeze 后 fingerprint 守卫 |
| R8 | 1-2 轮 route feedback 不够 | Phase 7 | 低 | 保留最佳正确结果标记 degraded；不无限振荡 |
| R9 | Architecture 临时双轨 | Phase 7-10 | 低 | 条件分支明确标注临时；Phase 10 统一删除 |
| R10 | 过度抽象 / 提前泛化 | Phase 9 | 低 | 只有第二个真实消费者出现后才抽 Kernel；Recipe 是显式 Rust 代码 |
| R11 | Virtual axis 振荡 | Phase 5 | 低 | 与节点共享步长策略；检测连续反向则缩步 |
| R12 | 性能退化（大图） | Phase 3+ | 低 | 首期串行足够（几百节点）；后续用分量分解优化 |

---

## 13. 每阶段通用验证清单

```bash
# 编译检查
cargo check -p plotgram-core

# 单元测试
cargo test -p plotgram-core

# 渲染验真（目标样例）
cargo run -p plotgram-cli -- render showcase/flowchart/product.employee-onboarding.pgm -o /tmp/onboarding.svg
cargo run -p plotgram-cli -- render showcase/flowchart/basic.diamond-decision.pgm -o /tmp/diamond.svg

# 确定性验证（同图跑两次，diff SVG）
cargo run -p plotgram-cli -- render <图> -o /tmp/run1.svg
cargo run -p plotgram-cli -- render <图> -o /tmp/run2.svg
diff /tmp/run1.svg /tmp/run2.svg

# 穿组 + 全量（可选，阶段完成后）
cargo run -p plotgram-cli -- render-all showcase/flowchart/ -o /tmp/flow-out/
```

### 不变量断言优先级

1. rank 不变
2. layer order 不变
3. 相邻节点间距满足约束（P0）
4. 同输入多次结果确定
5. solver 后节点不被 route/refine 修改
6. split/join axis deviation 在阈值内（Phase 5+）
7. 具体像素坐标（辅助证据，非唯一判据）

---

## 14. 阶段间依赖关系

```text
Phase 1 ──→ Phase 2 ──→ Phase 3 ──→ Phase 4 ──→ Phase 5 ──→ Phase 6
                                                                    │
                                                                    ▼
Phase 10 ◀── Phase 9 ◀── Phase 8 ◀── Phase 7 ◀────────────────────┘
```

- Phase 1-6 是文档 11 的核心路径（flowchart coordinate 统一写权）。
- Phase 7-8 收口外部写者和旧代码。
- Phase 9-10 是文档 12 的架构抽取（可选节奏，视 Phase 8 完成后的状态决定）。

---

## 15. 执行节奏建议

| 阶段 | 估计工作量 | 核心交付 |
|------|-----------|----------|
| Phase 1 | 小 | 写权清单 + 诊断结构 |
| Phase 2 | 中 | IR + PAVA，旧 overlap 删除 |
| Phase 3 | 中 | Optimizer 上线，BK 变为初值 |
| Phase 4 | 小 | 三个简单 pass 迁移 |
| Phase 5 | 大 | StructureModel + split-join（核心创新点） |
| Phase 6 | 中 | 全部旧 pass 删除 |
| Phase 7 | 中 | SpaceBudget 收口 |
| Phase 8 | 小 | 清理 + finalize |
| Phase 9 | 中 | 架构边界抽取 |
| Phase 10 | 大 | Architecture 复用（可选） |

**关键里程碑**：Phase 6 完成 = "CoordinateSolver 是唯一写者" 达成。  
**最终验收**：Phase 8 完成后统一看 product-gate 指标。
