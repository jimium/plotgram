# Hierarchical · StrongMacro 实现方案

> 父页：[architecture](../architecture.md) §8.2 · [roadmap](../roadmap.md) §6 阶段 E  
> 组合相契约：[composition](composition.md) §2 StrongMacro  
> Atlas 四步形状（只读参考）：[atlas-reference/strong-macro-expansion](../atlas-reference/strong-macro-expansion.md)  
> 现状盘点：[notes/2026-08-09-group-state-review](../notes/2026-08-09-group-state-review.md)  
> 写权尺子：[write-authority](../../write-authority.md)  
> 状态：**现行实现方案**（可执行推进）；与 Atlas 参考文区分——冲突时以本文 + architecture 为准

本文汇总 StrongMacro 相关资料，钉死与 Weak / D₂ 的边界，并给出基于 **2026-08 重建现状** 的实现方案与分步推进计划。

---

## 1. 一句话

**StrongMacro** = `group_policy: strong-macro`：先定组的语义舞台（macro block），再在组内用**同一套** Hier 算法栈摆节点；展开后归一为与 Weak **完全相同**的全局 `Plan` schema。Ink / Channel **不得**按 policy 分叉。

它解决的是架构图痛点（接入层在业务层正上方、等宽条带、子系统块感），**不是** Weak 路径上再拧 clamp / post-VPSC compact，也**不是** D₂「组框进 VPSC 真源」。

---

## 2. 资料地图

| 文档 | 角色 |
|------|------|
| [architecture.md](../architecture.md) §8.2 | 目标语义：Weak vs StrongMacro；同一 Plan；四条组树不变量 |
| [composition.md](composition.md) §2 | 组合相：后序局部图 → macro → 全局 key 重映射 |
| [atlas-reference/strong-macro-expansion.md](../atlas-reference/strong-macro-expansion.md) | v1 四步形状 + **不该照搬**清单（真源参考，非本仓库契约） |
| [roadmap.md](../roadmap.md) §6 E | 阶段归属：E（后置能力）；不挡 D₁/D₂ 主路径闭环 |
| [scope.md](../scope.md) | Weak/StrongMacro 均为 profile 参数，禁止第二套 `ArchitectureLayout` |
| [group-state-review](../notes/2026-08-09-group-state-review.md) | 2026-08 现状：`strong-macro` 仍 `Unsupported` |
| [mvp-scope](../notes/2026-08-02-mvp-scope.md) §0.4 | MVP 刻意不做 StrongMacro 的历史记录 |
| [group-invariants](../atlas-reference/group-invariants.md) | containment / sibling separation / penetration（Strong 与 Weak 共用） |
| 本文 | **实现方案 + 推进步骤（现行）** |

代码钩子：

| 路径 | 现状 |
|------|------|
| `hierarchical/params.rs` · `GroupPolicy::StrongMacro` | 可 bind |
| `hierarchical/mod.rs` | 入口硬 `Unsupported` |
| `crates/v1/.../atlas/dialect/contraction/strong_macro/` | **只读**参考实现 |

---

## 3. 与邻近工作的边界（勿混）

| 工作 | 回答的问题 | 与 StrongMacro |
|------|------------|----------------|
| **Weak**（现行默认） | 全局 Sugiyama + 边界 dummy；框 = finalize 后验 bbox | 对照基线；间距 compact 只服务 Weak |
| **D₂ 组框写权** | 组框进 Metric/VPSC，finalize 不重算 | **正交**；Strong 的框由 macro-block Writer 写。D₂ 若后做，须划清「谁写框」——Strong 路径上 macro-block 已是框写者，不宜再叠一套 VPSC 框变量 |
| **D₁ Channel** | 走廊 / Gate / rip-up | Strong 展开后的 Plan 应能喂现有 Channel；第一期允许 Gate 退化，但 schema 必须同一 |
| **阶段 E · StrongMacro** | 组策略第二档 | **本文** |

产品选型口诀：

- 密边挤、弯多 → D₁（已大部分落地）  
- Weak 下框合法性 / 穿组可证 → D₂  
- 架构分层「舞台感」、上下子系统对齐 → **StrongMacro**

路线图原句「D₁ 与 D₂ 不要并行」**不禁止**推进 E；E 不挡 D 闭环。但 StrongMacro 与 D₂ 都碰「组框写者」时须先裁定单写者（见 §5）。

---

## 4. 现状（2026-08）

### 4.1 已具备（可复用）

- 单一 Hier 管线：Compose → Metric（`J(x)` + VPSC）→ Channel → Ink  
- Weak：边界 dummy、`group_boundary_weight`、跨层软拉齐、Gate/Scope（部分 fallback）  
- Weak 间距止血：post-VPSC drawn-frame compact（仅 rank 相交兄弟；**不**作为 Strong 解）  
- `group_policy` 已进 `HierarchicalParams` / `params_hash`  
- architecture profile 语义上应对齐 StrongMacro（[`architecture.md`](../architecture.md) 表），但解析展开后仍跑 Weak 或未消费 strong

### 4.2 缺口

- ~~`group_policy: strong-macro` → 硬失败（`mod.rs`）~~ **SM-0 已解绑**：入口分派 `strong_macro::layout(...)`  
- ~~无 intra / super-graph / macro-block / expand 模块~~ **SM-1 已落地**（顶层-only）；嵌套递归 **SM-2 已落地**  
- ~~无与 Weak 共享的「收缩 meta → 展开」IR~~ **SM-1 形态落地**：`strong_macro` 内部 IR（`IntraResult`/`Block`）不泄漏到 Ink，expand 产出与 Weak 同一 `PlanGraph` schema，共享 Channel/Ink 尾部（`compute_channel_ink_tail`；orchestrator 经 `TailFrames` / `group_obstacles` 等 hook 注入 Strong 定帧，`channel/`/`ink/` 内不读 policy）  
- `group_sizing` / `group_align` 已从 params 删除（曾 Unsupported）；Strong 第一期**不**恢复，避免空字段  
- ~~`verify_no_group_penetration` 仍未做（D₂/E 可共享验收）~~ **SM-4 已落地**（`ink/verify.rs`，经 crate 根导出；hier_eval 穿组门禁 strong 硬 / weak 观测）
- ~~跨组边进目标函数 / DemandBoard 行间 gap（SM-3）~~ **SM-3 已落地**（macro 行对齐 soft 项 + `MacroRowGap`/`MacroColGap` demand）；architecture profile 默认 strong（SM-4）**本轮决定不做**（选型说明见 expectations.md）

### 4.3 典型验收图（Strong 应对齐的观感）

| 图 | Strong 期待 |
|----|-------------|
| `product.rest-api-backend.pgm` | 接入 / 业务 / 数据 **上下叠放、水平大致同轴**，而非左右错开 |
| `stress.layout-stress-nested.pgm` | 云端 vs 外部有明确块感与间隙；嵌套子网在父舞台内 |
| `demo.k8s-platform-stack.pgm` | 顶层子系统块排列可读；不依赖 Weak compact 硬拉 |

---

## 5. 目标架构（新实现）

### 5.1 管线形状（保留 Atlas 四步拓扑，换写者）

```text
Graph + HierarchicalParams{ group_policy: strong-macro }
        │
        ▼
┌─ SM-A  Intra（组树后序）─────────────────────────────────────┐
│  叶子组：同一套 rank/order/坐标栈（局部 Plan，局部 key）      │
│  容器组：子 macro 装箱 → 组内 super → 局部堆叠 → 合成 intra  │
│  写者：Compose(局部) + Metric(局部)；禁止读 diagram_type      │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
┌─ SM-B  Super-graph（顶层）───────────────────────────────────┐
│  超节点 = 顶层 group ∪ 未入组 real                             │
│  超边 = 跨超节点有效边（带权重 / 计数供 gap demand）           │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
┌─ SM-C  Macro-block 定位（框写者）────────────────────────────┐
│  装箱：intra bbox + GROUP_PAD / label top pad                  │
│  超节点分层 + 逐 rank 堆叠（主轴）+ 行内 gap（cross）          │
│  **唯一写组框几何**（Strong 路径）；finalize 只展开已有框     │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
┌─ SM-D  Expand → 全局 Plan────────────────────────────────────┐
│  内容原点 = block 原点 + pad（与 Atlas「Super 点 = 内容原点」）│
│  重映射 NodeKey / SegmentKey / GateKey → 全局稳定 key          │
│  产出与 Weak **同一** PlanGraph schema                         │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
共享 Channel / Ink / finalize 尾部（`compute_channel_ink_tail`）
  · `channel/` / `ink/` **模块内**禁止读 `group_policy`
  · orchestrator 可经参数化 hook 注入 Strong 定帧：
    `TailFrames::Fixed`、`group_obstacles`（外轨避让）等
```

### 5.2 写权表（Strong 路径）

| 自由度 | 写者 | 禁止 |
|--------|------|------|
| 组内 layer / order | 局部 Compose | 父层事后改组内序 |
| 组内节点坐标 | 局部 Metric（同 `J(x)`+VPSC） | post-expand nudge 改组内点 |
| macro 块位置 / 组框 | **MacroBlockWriter（SM-C）** | finalize 重算包围盒当真源；Ink 平移组 |
| 全局 Plan schema | Expand（SM-D）归一 | 第二套 `IntraLayout` IR 泄漏到 Ink |
| 跨组边方向偏好 | 进局部 / 超图 **objective 或 Demand** | `nudge_intra_nodes_*` 事后修 |
| Gate / 路径拓扑 | 展开后既有 Channel | Strong 专用第二路由栈 |

### 5.3 明确不照搬（摘自 Atlas 参考，本文升格为硬约束）

1. **禁止** `nudge_intra_nodes_toward_cross_group_edges` 类 post-solve 改组内坐标。  
2. **禁止** `phase_d_postprocess` / gutter 事后扩缝——走廊需求走 DemandBoard。  
3. **禁止** 组内另起一套 Sugiyama 委托栈——共享主算法（rank / order / `symmetry_objective`）。  
4. **禁止** `diagram_type` / architecture recipe 耦合；mode 与 hub 语义只经 profile → params。  
5. **禁止** 全局可变 override 传 sizing。  
6. **禁止** Ink / Channel 模块内读取 `group_policy` 分支（orchestrator 参数化 hook 除外，见 §5.1 尾部）。

### 5.4 与 Weak finalize 双 pad 的关系

Weak 现行 finalize：`union(成员 ∪ 子组已 pad 框) + pad`，父框比「成员±pad」多一圈。  
Strong 路径上框由 SM-C 一次写定（成员内容 + 一层 pad + label band）；**finalize 不得再发明第二圈 pad**。若引擎 finalize 暂不可分 policy，则 Strong expand 写入的 `GroupPlacement` 已被视为终态，finalize 对 Strong 结果应透传或只做与 Weak 相同的坐标系平移——实施时在 SM-0/SM-1 验收中钉死，避免「macro 已 pad、finalize 再 pad」。

---

## 6. 推进步骤（可交付切片）

### SM-0 · 解绑与骨架（约 0.5–1 周）

**目标**：`group_policy: strong-macro` 不再硬失败；走空壳分支并落入同一 `LayoutOutput` 类型。

- [x] `mod.rs`：Strong 入口改为调用 `strong_macro::layout(...)`（可先 stub 转调 Weak 并打 `relaxation`/`warning`，但 **禁止静默当 Weak**——stub 阶段保留显式 `Unsupported` **或** 明确的 `strong-macro-stub` diagnostics；推荐直接 SM-1 最小闭环，不做假 stub）  
- [x] 模块落点：`hierarchical/strong_macro/{mod,intra,super_graph,macro_block,expand}.rs`  
- [x] 文档/错误信息指向本文，不再指向 mvp-scope「本轮不做」  
- [x] `params_hash` 已含 `group_policy`（已有）— 加一条 bind 测试：`strong-macro` 可解析

**验收**：单元测试 bind + 入口可区分 policy；showcase 指定 strong 时行为可预期（失败信息或真布局）。✅ 按推荐项未做假 stub，骨架直接由 SM-1 实现填充。

### SM-1 · 顶层-only 最小闭环（主切片，约 1–2 周）

**目标**：仅处理**顶层 group**（忽略嵌套或把嵌套当扁平成员）；macro 垂直堆叠；组内跑共享 Hier 子集。

范围：

1. **Intra（叶子 = 顶层组）**：组诱导子图 → `assign_ranks` / `order` / 局部 `solve_symmetry_objective`（或等价 cross 赋值）→ 得到组内相对坐标与内容 bbox。  
2. **Super-graph**：顶层组 + 未入组节点；跨组边聚合成超边。  
3. **Macro-block**：主轴按超图 rank **纵向堆叠**（解决 `rest-api-backend` 接入在业务之上）；行内 `GROUP_FRAME_GAP`；框 = 内容 + `GROUP_PAD` / `GROUP_LABEL_TOP_PAD`。  
4. **Expand**：平移到全局；填 `PlanGraph`（或先填 `LayoutResult` 节点/组 frame，若 Compose 全量 Plan 过重——**优先仍产出 Plan**，以便 Channel 复用）。  
5. Channel/Ink：复用现有路径；Gate 不完善时可 fallback，但须打 relaxation。

**状态（2026-08-10）✅ 已落地**，含两个实施中新增的必要机制（未见于初版方案，补记）：

- **块级 FAS**（`super_graph::break_block_cycles`）：节点级 FAS 后工作图无环，但块聚合后可重新成环（泳道回边）；须在超图上再跑一次 `greedy_fas`，翻转选中块对间实际边的 working 方向并 toggle `e.reversed`（Ink `chain_in_original_order` 依赖），`original_*` 不动。
- **走廊隔离**（`intra::layout_intra` isolate）：块级 FAS 翻转边的两个 working 端点若与兄弟共享局部层，侧廊进场段必穿透兄弟；故该两端点在组内求解时各移入底部独占层。

**非目标**：递归嵌套、equal-track、`group_align`、穿组 verifier、跨组边进组内 J。

**验收图**：

- `product.rest-api-backend.pgm` + `group_policy: strong-macro`：接入 / 业务 / 数据中心 x 偏差小于阈值；垂直顺序正确。  
- 与 Weak 同图对比：Strong 块对齐，Weak 允许现有行为。  
- `cargo test -p plotgram-layout`；相关 showcase `measure` 不炸。

### SM-2 · 嵌套递归（约 1–2 周）

**目标**：组树后序；容器组 = 子 macro 的 super + 堆叠；`stress.layout-stress-nested` 云端内子网。

- [x] `layout_intra_group_recursive` 形状（共享算法栈，无第二 Sugiyama）  
- [x] 子框进入父内容区；父 macro 装箱含嵌套  
- [x] 展开后全局 key 稳定、确定性（禁止 `HashMap` 迭代序）

**状态（2026-08-10）✅ 已落地**，实施要点（对初版方案的必要具体化，补记）：

- **Block 树递归**：嵌套组自身即 macro block；容器块 scope entries = 子组块 + 直属成员节点单块（声明序：节点先于子组）。节点归属最深块；空组整棵跳过（与 finalize 不画空框一致）。
- **SM-B 作用域化**（`super_graph::break_scope_cycles` / `assign_scope_ranks`）：slot 映射（`scope_slots` 沿 parent 链解析嵌套后代）；每作用域（容器后序 → 顶层）独立块级 FAS + super ranks；走廊隔离端点集跨作用域累积。
- **SM-C 递归**（`macro_block::place_scope` + `propagate_origins`）：后序 pack（容器包络 = 已定子框并集）+ 行堆叠（每层同一 mechanics）；全局内容原点自顶向下累积。finalize `union(成员 ∪ 子框) + pad` 逐层还原框树（一圈 pad/层，§5.4 不变）。
- **SM-D**（`expand::block_layers`）：容器行按 **band 式 offset** 展开（每行起于前序行全深之后，空层留白）——这是跨行边 span ≥ 1 的必要条件，与顶层 band 同一论证；层序归一按 top-block section + `(edge_id, ordinal)` 确定性排序。
- **验证**：顶层-only 几何 bit 级不变（SM-1 快照原样通过）；weak 75 条 baseline 相对 SM-2 前重建版零变化；新增 strong `stress.layout-stress-nested`（3 层嵌套 + 跨边界回边）进常规硬门禁；showcase 81 张全 ok。

**验收**：`stress.layout-stress-nested` Strong 下父子框无错误重叠；嵌套 label 可读。

### SM-3 · 跨组边进目标 + Demand（约 1 周）

**目标**：消灭「先定框再 nudge」压力。

- [x] 超边 / 跨组边对 intra 或 macro 定位的 soft 项（typed 权重进 params）  
- [x] 行间 / 行内 gap 下限由边计数经 DemandBoard 发布（替换 Atlas 经验 `*8/*12` 常量）  
- [x] 仍禁止 post-expand 改组内坐标

**状态（2026-08-11）✅ 已落地**，soft 项按「intra 或 macro 二选一」取 **macro 行对齐**（intra 不动）：

- **params**：`macro_align_weight`（默认 1.0；`0` = 纯居中回退 SM-2 形态）进 Default / bind / `params_hash`；weak 从不读 → weak 几何不变。
- **Demand**：`MacroRowGap(r)`（scope 内 macro 行缝）/ `MacroColGap(k)`（行内相邻 entry）两 key；producer `publish_macro_pair_demand` 以 `base + min((count−1)×edge_gap, 4×edge_gap)` 发布（`MACRO_DEMAND_MAX_EXTRA_LANES = 4`，以 edge_gap 车道计价替代 Atlas 经验 `CROSS_EDGE_GROUP_GAP_SCALE=8.0` 等常量）。
- **边计数**：每 scope 复用 `scope_slots`，real edge 两端 slot 均 Some 且不同 → pair `(min,max)` 计数 + `e.weight` 权重和（含无向边——demand 是路由关注点非定序）。
- **SM-C 消费**：`place_rows` 先发布需求 → freeze → 行缝 gap = `max(max(layer_gap, GROUP_FRAME_GAP), seam demand)`（跨空行取区间最大值）、行内邻接 gap = `max(GROUP_FRAME_GAP, col demand)`。同行 col demand **仅**计 row-adjacent entry 对（声明序同 rank 且中间无同 rank 槽）；跨槽长边不加宽中间邻接。  
- **行对齐 sweep**：`J = Σ w_ab × ((o_a + cx_a) − (o_b + cx_b))²`，自由变量 = 行 offset（块中心与行 offset 无关，差值中相消）；固定 8 趟 Gauss-Seidel（rank 升序），每趟后首个非空行钉回 0（消平移零空间），落框前再 `min_x` 归一；起点 = SM-2 居中结果 → 无跨行边时 bit 级不变（顶层-only 快照原样通过证明居中是对齐 J 的不动点）。写权不变：SM-C 仍是唯一框写者，无 post-expand nudge。  
- **端口写权（SM-D）**：叶块 intra 的端口 **side** 喂局部 VPSC；expand 后全局 `assign_ports` 负责跨块边与 Ordered/clusters，再 `reconcile_intra_port_sides` 把同叶块边的 side 写回，避免 Channel 与已定坐标分叉。
- **验证**：行缝/行内 gap 随边数单调增 + 封顶表驱动单测；对齐吸引 + `macro_align_weight: 0` 回退（bind + 几何断言）；嵌套 fixture bit-identical；weak 75 条零变化（SM-3 前重建 baseline 对比），strong 仅 5 条按预期变化。

### SM-4 · 加固与产品化（持续）

- [ ] architecture profile 默认或显式展开 `group_policy: strong-macro`（解析层，禁引擎图种分支）——**本轮决定不做**：选型说明进 expectations.md，profile 默认切换留待 strong 语料更大后评估
- [x] `verify_no_group_penetration`（可与 D₂ 共用实现）  
- [x] hier_eval / showcase 门禁：Strong 代表图对齐指标  
- [x] 文档：Weak vs Strong 选型说明进 expectations 或 README  
- [x] （可选）再评估 D₂：Weak 路径框真源；Strong 路径保持 MacroBlockWriter → **已关闭**：见 [group-frame-d2.md](group-frame-d2.md)；Strong = MacroBlock 直接写 `GroupPlacement`，禁止叠 VPSC 框

**状态（2026-08-11）✅ 门禁与 verifier 已落地**：

- **verifier**：`ink/verify.rs` `verify_no_group_penetration(edges, groups, allowed)` + 清单形态 `group_penetration_violations`（门禁选硬/软）；语义对齐 v1 substrate L6：段进入「两端点组祖先链之外」组框**开内部**即违规（OBSTACLE_INSET 容差，边界擦过不算）；经 `hierarchical/mod.rs` → crate 根导出，供 plotgram-compile 侧与未来 D₂ 共用。
- **hier_eval 门禁**：① 穿组——全 fixture 计算，source 含 `group_policy: strong-macro` 硬失败清单，否则打印观测（D₂ 欠账：weak 11 张 fixture 存量穿组，最深 ~190px，如实观测不清零）；② 组包含——子组框 ⊆ 父组框（EPS 容差）硬门禁，finalize 契约决定它恒真，作回归护栏。
- **存量修复**：strong 2 处穿组（swimlane-recruitment e4 / swimlane-order-process e6）根因 = 外侧回边 Main 竖轨只避节点体不避组框，落在组框 GROUP_PAD 带内 8px；修法 = strong 路径把 finalize pad 契约的组包络（`canonical_group_obstacles`）并入外轨 `clear_outside` 避让（weak 传空表 → bit 不变）；修复后 strong 清零，附 `outer_main_rail_clears_group_envelope` 单测。

---

## 7. 模块与 IR 草图

```text
crates/plotgram-layout/src/layout/hierarchical/
  strong_macro/
    mod.rs           // layout(input, params) → LayoutOutput（与 weak 同型）
    intra.rs         // 后序局部求解；调用既有 rank/order/symmetry
    super_graph.rs   // 超节点 / 超边
    macro_block.rs   // 装箱 + 堆叠定位（框写者）
    expand.rs        // → 全局 PlanGraph + frames
```

建议 IR（内部，不泄漏到 Ink）：

```text
IntraResult { local_plan: PlanGraph, content_bbox, member_cross_main }
SuperGraph  { nodes: [GroupId | UngroupedNodeId], edges: [...] }
MacroBlock  { id, is_group, width, height, x, y, /* 封装 setter */ }
ExpandMeta  { block_of: GroupId → MacroBlockId, origin_of: ... }
```

展开后只保留全局 `PlanGraph` + 既有 `LayoutResult` 字段。

---

## 8. 风险与裁定

| 风险 | 裁定 |
|------|------|
| 与 D₂「框进 VPSC」双写者 | Strong：MacroBlockWriter；Weak+D₂：VPSC 框变量。禁止同一 policy 下两写者 |
| 过早做满递归 + Channel | SM-1 顶层-only；Gate 可 fallback + 可观测 |
| 偷运 v1 architecture 特判 | profile → params；引擎无 `diagram_type` 分支（ADR-001） |
| Weak compact 与 Strong 混淆 | compact 留在 `symmetry_objective` Weak 路径；Strong 不调用 |
| finalize 二次 pad | SM-1 验收钉死；Strong 框一次写满 pad |

---

## 9. 验收清单（Definition of Done）

**SM-1 Done**：

1. `layout: hierarchical { group_policy: strong-macro }` 可跑通至少 2 张 architecture showcase。  
2. `rest-api-backend`：三层 group 垂直堆叠，水平中心对齐误差可阈值。  
3. Ink/Channel 源码无 `group_policy` 匹配分支。  
4. 无 post-expand 节点 nudge。  
5. 确定性：两次 run 坐标 bit-identical。

**SM-2 Done**：嵌套 showcase 框树视觉正确 + 上述 3–5。

**E 阶段 StrongMacro Done**：SM-1..3 + profile 可选用 + 文档选型说明；穿组 verifier 可另单列。

---

## 10. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-08-10 | 初版：汇总资料、与 Weak/D₂ 边界、SM-0..4 推进步骤；取代「仅 mvp-scope 写不做」的指向 |
| 2026-08-10 | SM-0 + SM-1 落地：入口分派、共享尾部分拆（`compute_channel_ink_tail`，weak 零变化按「HEAD 重建 vs 当前重建 baseline」对齐验证——注意 checked-in baseline 的观测项存在环境差异，bend gate 只硬管 max_bends/sum/reversed/gates）；`symmetry_objective` compact/snap 守卫为 Weak-only；新增块级 FAS（`break_block_cycles`）与走廊隔离两个计划外机制（见 §6 SM-1 状态）；hier_eval 删除 `expects_strong_macro_unsupported` 特判，5 张 strong fixture 进常规硬门禁与 baseline；showcase render 80 张全 ok |
| 2026-08-10 | SM-2 嵌套递归落地：Block 树递归 + SM-B 作用域化（块级 FAS/走廊隔离下沉到每层容器）+ SM-C 递归装箱堆叠 + SM-D band 式行 offset（见 §6 SM-2 状态）；顶层-only 几何 bit 级不变，weak baseline 零变化（SM-2 前重建 vs 当前重建对比）；新增 strong `stress.layout-stress-nested` 进硬门禁，baseline 81 条；showcase render 81 张全 ok |
| 2026-08-11 | SM-3 + SM-4 落地（见 §6 状态补记）：SM-3 `macro_align_weight` 行对齐 sweep + `MacroRowGap`/`MacroColGap` demand（edge_gap 车道计价，封顶 4 车道）；SM-4 `verify_no_group_penetration` + hier_eval 双门禁（穿组 strong 硬/weak 观测 + 组包含恒真护栏）+ strong 2 处存量穿组经外轨组包络避让清零；architecture profile 默认本轮不改（选型说明进 expectations.md）；weak 75 条零变化（SM-3 前重建 baseline 对比），strong 5 条预期变化（缝变宽 / overlap_len 大降 / relaxations 减少） |
| 2026-08-10 | 评审修：组内端口 side 经 `reconcile_intra_port_sides` 写回全局；expand 虚拟插值 `span==0` 防护；`align_rows` 每趟钉首行；同行 col demand 仅 row-adjacent；文档明确共享尾 + `TailFrames`/`group_obstacles` orchestrator hook |
