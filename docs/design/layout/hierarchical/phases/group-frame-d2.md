# Hierarchical · D₂ Weak 组框写权（D₂.0–D₂.2）

> 父页：[architecture](../architecture.md) §8.3 / §11 · [roadmap](../roadmap.md) §5.2  
> 组不变量：[atlas-reference/group-invariants](../atlas-reference/group-invariants.md)  
> 坐标相契约：[coordinate-and-demand](coordinate-and-demand.md) §6  
> Channel 联调：[channel-d1](channel-d1.md) · [ports-and-channel](ports-and-channel.md)  
> 对照边界：[strong-macro](strong-macro.md)（Strong 框写者 = MacroBlockWriter，**不**进本方案）  
> 选型：[expectations](../expectations.md) §7  
> 写权尺子：[write-authority](../../write-authority.md)  
> 状态：**现行实现方案**（可执行推进）；产品已裁定「Weak 框要可证」→ 开 D₂.0

本文把缩小后的 **D₂ · Weak 组框写权**切成 **D₂.0 → D₂.1 → D₂.2** 三个可串行交付切片，钉死与 StrongMacro / D₁ 的边界、写权、IR、管线序与验收。

---

## 1. 一句话

**D₂** = 仅 **Weak** 路径：组框由 **Metric（VPSC / 主轴）写出真源**；`finalize` **不得**再 `union(members)+pad` 发明框；兄弟分隔与（最终）穿组由构造 + 共用 verifier 可证。

它解决的是「Weak 装饰框与求解/路由几何分叉 → 穿组只能观测」；**不是** StrongMacro 舞台感，也**不是**再开一套 Channel。

---

## 2. 为什么要进 VPSC（产品裁定备忘）

Weak「软包围」指的是 **收缩策略**（全图 Sugiyama + 边界 dummy），不是「框没有自由度」。

现行裂缝：

```text
节点坐标（Metric 真源）
  → GroupBoundary snap / post-VPSC compact（半真源）
  → finalize union+pad（视觉框真源）
Channel / 外轨多半只见节点体
→ 边可穿过「事后才画出来的 pad 带」→ weak 穿组观测
```

进 VPSC 的目标：

| 要可证的事 | 做法 |
|------------|------|
| Containment | 成员 ⊆ 框 由硬约束保证 |
| Sibling separation | 框–框 gap 进求解（取代 compact 主路径） |
| No penetration（Weak） | Channel/外轨对着 **同一份** 框几何避让；门禁硬化 |

软包围语义可保留为 **Fit**：框紧贴 `members ∪ 子框 ± pad`——只是跟法在同一次求解里，而不是 finalize 发明。

---

## 3. 资料地图

| 文档 / 代码 | 角色 |
|-------------|------|
| [roadmap §5.2](../roadmap.md) | 阶段归属；本文为可执行展开 |
| [coordinate-and-demand §6](coordinate-and-demand.md) | 目标契约：框 = Metric 变量；Facade 不重算 |
| [architecture §8.3](../architecture.md) | 边界 dummy（P3）→ 框边 VPSC（P4） |
| [group-invariants](../atlas-reference/group-invariants.md) | 三不变量；禁 Atlas 式 post-Ink 刚体推开 |
| [group-state-review](../notes/2026-08-09-group-state-review.md) | 2026-08 盘点（部分过时；以本文 + roadmap 为准） |
| `plotgram-engine/src/finalize.rs` · `group_frames` | **现行**后验框（D₂.0 必须改） |
| `hierarchical/group_frame.rs` | pad / `GROUP_FRAME_GAP` / shell bands 契约 |
| `metric/symmetry_objective.rs` | Weak-only `compact_sibling_frame_gaps` / `snap_boundaries` |
| `ink/verify.rs` · `verify_no_group_penetration` | SM-4 已落地；D₂.2 硬化 Weak 门禁 |
| `strong_macro/` | **对照**：Strong 框写者；D₂ **禁止**叠 VPSC 框变量到 Strong |

---

## 4. 与邻近工作的边界（勿混）

| 工作 | 回答的问题 | 与 D₂ |
|------|------------|-------|
| **Weak（今日）** | 软聚类 + 后验框 + compact | **改造对象** |
| **StrongMacro** | 舞台 / MacroBlockWriter | **正交**；同 policy 禁止双写者 |
| **D₁ Channel** | 走廊 / Gate / rip-up | D₂.0 后框可作障碍；容量/fallback = D₂.2 联调 |
| **PartitionGrid** | 泳道矩阵 | E / M5；勿用 group 冒充 |

产品口诀：密边 → D₁；架构舞台 → Strong；**Weak 框可证 → D₂（本文）**。

---

## 5. 现状（开 D₂.0 前）

### 5.1 已具备（可复用）

- Weak：`GroupBoundary`、跨层软拉齐、`group_shell_bands` → LayerGap demand  
- post-VPSC drawn-frame compact（仅 rank 相交兄弟；Weak-only）  
- Channel Gate / ScopeMask / `verify_route_scope`（D1.2）  
- `verify_no_group_penetration` + hier_eval（strong 硬 / weak 观测）  
- Strong 外轨 `group_obstacles` 先例（Weak 目前传空表）  
- pad 常量单一契约：`GROUP_PAD` / `GROUP_LABEL_TOP_PAD` / `GROUP_FRAME_GAP`

### 5.2 缺口（本文要关）

| 缺口 | 影响 |
|------|------|
| 框真源在 `finalize::group_frames` | Metric/Channel 与视觉框分叉 |
| 兄弟分隔靠 compact | 写权不干净；极端图仍可叠 |
| Weak 穿组 ~11 fixture 仅观测 | 「可证」未达标 |
| Gate 无容量；fallback 偏多 | 组不纯 → 当无组 |

---

## 6. 目标架构（Weak + D₂）

### 6.1 管线形状

```text
Compose（不变）：boundary / order / ports …
        │
        ▼
Channel（读节点帧 + 可选组壳；D₂.0 后逐步读 Metric 框）
        │
        ▼
┌─ Metric（Weak + D₂）─────────────────────────────────────────┐
│  节点 J(x)+VPSC（既有）                                        │
│  + 组框变量（L/R/T/B 或等价 Rect 自由度）                      │
│  + 含约束：member ⊆ frame（pad / label top）                   │
│  +（D₂.1）sibling separation；title / min-size demand          │
│  写出：node frames + GroupPlacement[] 真源                     │
└──────────────────────────────────────────────────────────────┘
        │
        ▼
Ink 展开（只读框；不发明框）
        │
        ▼
finalize / Facade：`owns_group_frames` → **透传**（空向量也不兜底）；禁止重算真源
```

Strong 路径：**不**走本 Metric 框变量；MacroBlockWriter 直接产出 `GroupPlacement`，经 `TailFrames::Fixed { frames, groups }` 注入尾部。

### 6.2 写权表（Weak + D₂）

| 自由度 | 写者 | 禁止 |
|--------|------|------|
| 组内/全局节点坐标 | Metric `J(x)`+VPSC | Ink / finalize 挪节点消框债 |
| **组框几何（Weak）** | **Metric 组框 Writer** | finalize `union+pad` 当真源；Ink 平移组 |
| 组框几何（Strong） | MacroBlockWriter（直接产出 `GroupPlacement`） | 本方案 VPSC 框变量叠在 Strong 上 |
| 兄弟框缝 | Metric 硬分隔（D₂.1） | 第三趟 ad-hoc compact 作主路径 |
| Gate 拓扑 | Channel（已有） | D₂ 另起路由栈 |
| Gate 容量 → 缝宽 | DemandBoard ← Channel 计数（D₂.2） | 容量 = 跨界边数恒真公式 |
| 穿组检查 | 共用 `verify_no_group_penetration` | Atlas post-Ink 刚体推开 |

### 6.3 明确禁止

1. Strong 路径叠 VPSC 组框变量。  
2. Ink / Channel **模块内**读 `group_policy`（orchestrator hook 除外，同 Strong 先例）。  
3. 用 post-Ink / post-finalize 平移组框「修」sibling / containment。  
4. 静默 `channel-group-fallback` 且不进 `relaxations`。  
5. 双真源：Layout 已输出 groups 时 finalize 再算一遍并覆盖。

---

## 7. IR 草图

### 7.1 Layout 输出

```text
LayoutOutput / LayoutResult {
  nodes: [NodePlacement]
  edges: …
  groups: [GroupPlacement]   // D₂.0+ Weak：Metric 已填；Strong：MacroBlockWriter 已填
  owns_group_frames: true      // Hier 恒 true → finalize 禁止 union+pad 兜底
  …
}
```

`GroupPlacement { id, frame }` 类型不变；**写者**从 finalize 迁到 layout。

### 7.2 Metric 内部（建议，实施可微调命名）

```text
GroupFrameVar {
  group_id
  // 次轴：left, right（或 center+width）
  // 主轴：top, bottom（或与 layer band 耦合）
}

Constraints (最低集，D₂.0):
  ∀ member m of g:
    m.left  ≥ g.left  + GROUP_PAD
    m.right ≤ g.right - GROUP_PAD
    m.top   ≥ g.top   + top_pad(g)
    m.bottom≤ g.bottom- GROUP_PAD
  ∀ child group c of g:   // 嵌套：子框已含 pad
    c.frame ⊆ g.frame 内缩 0（或再 +0；与现行 finalize 双 pad 契约对齐）

Objective / soft (D₂.0 可极简):
  Fit：最小化 (g.right-g.left) + (g.bottom-g.top)
  或等价：框边软贴「成员外包络 ± pad」（与今日视觉接近，bit 变化可控）
```

与 `GroupBoundary` 关系（开 D₂.0 必须裁定，见 §8.1）：

| 选项 | 含义 | 建议 |
|------|------|------|
| **A. 合一** | boundary 左右 = 框 L/R 在层上的投影 | 长期干净；改动面大 |
| **B. 边界仍夹节点；框变量另解后对齐** | 过渡：先框 = 外包络约束解，再令 boundary snap 到框 | **D₂.0 推荐** |
| **C. 删 boundary，只留框** | 秩序与交叉可能回退 | 不做首切片 |

### 7.3 DemandKey（D₂.1+）

```text
GroupMinWidth(g) / GroupMinHeight(g)   // title / 显式 min
GroupSiblingGap(a,b)                    // 或复用 GROUP_FRAME_GAP 常量硬分隔
GateLaneDemand(gate_key)                // D₂.2：跨 Gate 边计数 → 缝宽
```

---

## 8. 推进步骤

### D₂.0 · 框真源进 Metric（最小可证闭环）≈ 1.5–2.5 周

**目标**：Weak 路径 `GroupPlacement` 由 layout/Metric 写出；finalize **透传**；视觉与含约束与今日 Fit 接近；Strong bit-identical。

#### 8.0 开工前基线

- [x] 记录 weak 穿组清单（fixture × edge × depth）与 hier_eval baseline 哈希  
- [x] 选定 3–5 张代表图：无组 / 单组 / 兄弟并排 / 嵌套 / 已知穿组各一  
- [x] 钉死 pad 契约：与 `group_frame.rs` + 现行 finalize 公式一致（含嵌套双 pad）  

#### 8.1 写权与 IR

- [x] **裁定** `GroupBoundary` vs 框变量：默认走 §7.2 **选项 B**（文档 + 代码注释写死）  
- [x] Metric 产出 `Vec<GroupPlacement>`（组树后序，确定性序）  
- [x] `LayoutOutput.groups`（或等价通道）Weak 必填；空组策略与今日一致（不画空框）  
- [x] Strong：继续 Fixed frames；**零** VPSC 框变量  

#### 8.2 finalize / Facade

- [x] `plotgram-engine::finalize`：若输入已含非空 `groups` 真源 → **透传**（可做坐标系整体平移若 Stage 需要）  
- [x] 仅当 layout **未**提供 groups（非 Hier / 旧路径）时保留后验计算，或 Hier Weak 硬要求必带——**禁止** Hier Weak「带了又覆盖」  
- [x] 单测：同一节点几何下，透传框 ≠ 再算一遍不得被覆盖  

#### 8.3 Metric 最小约束（Fit）

- [x] 引入组框自由度 + containment 硬约束（成员 + 子框）  
- [x] Fit 目标：框贴近外包络（避免无故胀框）  
- [x] 主轴：与既有 `group_shell_bands` / LayerGap 一致，避免框顶底与走廊 demand 打架  
- [x] 失败：不可行 → `InfeasibleConstraint`，禁止静默丢约束  

#### 8.4 Channel / 外轨钩子（只做可读，不做满容量）

- [x] Weak 路径亦可把 Metric 框注入 `group_obstacles`（或等价），对齐 Strong 外轨避让先例  
- [x] **不**在本切片做 Gate 容量公式；fallback 逻辑不变，但框纯度应开始改善（观测）  

#### 8.5 验收（D₂.0 Done）

1. Hier Weak：finalize 源码路径对已提供 groups **不重算真源**（代码审查 + 单测）。  
2. 代表图 containment：成员 ⊆ 框（EPS）；嵌套子框 ⊆ 父框（已有门禁应仍绿）。  
3. Strong + 无组图：几何 bit-identical（或仅 params_hash / 无关字段变）。  
4. Weak 全量 hier_eval：硬不变量绿；穿组 **仍可观测**，但鼓励深度下降（不强制清零）。  
5. 文档：roadmap / expectations 指向本文；`group_frame.rs` 模块头更新写者说明。

落地记录（2026-08-11）：框真源 = `metric/group_frames.rs`（选项 B）；finalize 非空 groups 透传、仅空时兜底；Weak/Strong 均注入 Metric 框 obstacles。验收结果：hier_eval 76 fixture 对基线零 delta（Strong bit-identical、Weak 几何不变），weak 穿组观测 68 处与基线一致（存量待 D₂.2）。

**非目标（D₂.0）**：sibling 硬分隔主路径、穿组硬门禁、Gate 容量、删 compact、title 美学。

---

### D₂.1 · Sibling + Demand；退役 compact 主路径 ≈ 1–1.5 周

**目标**：兄弟框分隔由求解保证；post-VPSC compact 降为可选/删除；title / min-size 进 Demand（可薄）。

#### 8.6 Sibling

- [x] 无祖先关系的组对：若 cross 投影相交（或 rank span 相交，与现行 compact 过滤对齐）→ 硬分隔 ≥ `GROUP_FRAME_GAP`  
- [x] 主轴重叠兄弟：沿次轴推开（VPSC）；禁止 Ink 刚体平移整组  
- [x] 嵌套：父子是含约束，**不是** sibling  

#### 8.7 Demand / title

- [x] 有 label：顶边至少 `GROUP_LABEL_TOP_PAD`（已在含约束）+ 可选 `GroupMinWidth` 吃长标题（可第二周）——`GroupMinWidth` **推迟**：现有 DemandBoard 只有主轴 key，次轴 Demand 是新机制，成本高，按「可选」延后  
- [x] `group_sizing` / `group_align`：若仍从 params 删除则本切片**不**恢复空字段；有需求再 bind 真消费  

#### 8.8 退役 compact

- [x] `compact_sibling_frame_gaps`：求解已保证分隔后 **删除** 或缩成 debug-only assert「无剩余超额缝」——**落地偏差（裁定）**：未全删，缩为 `close_sibling_frame_slack` 只收缝写者（只向左拉、永不右推，带安全钳制），推开写权归硬约束；全删随 D₂.2 J(x) 紧凑项一并收口  
- [x] `snap_boundaries_to_members`：与选项 B 对齐——boundary 跟框，或跟成员且框已含 pad（二选一，禁止双源打架）——裁定选项 B（boundary 跟成员、框已含 pad），保留为 clamp 的唯一事后写者  

#### 8.9 验收（D₂.1 Done）

1. 构造兄弟并排 fixture：框间隙 ≥ `GROUP_FRAME_GAP`，且关闭 compact 仍成立。  
2. Weak 代表图无非法兄弟重叠（MetricVerifier 或 hier_eval）。  
3. compact 主路径已删或测试证明为 no-op。  
4. Strong 仍 bit-identical。

**非目标**：穿组硬门禁清零、Gate 容量。

落地记录（2026-08-11）：推开写权进 VPSC——`symmetry_objective.rs` 层内硬分离链中无亲缘组 boundary clamp 对 extra 由 `GROUP_PAD` 改 `GROUP_FRAME_GAP`（`pair_extra` helper，表驱动单测覆盖无亲缘/嵌套/同组/普通节点四类 pair）。compact 未按原计划全删（裁定偏差）：缩为 `close_sibling_frame_slack` 只收缝写者——只向左拉超额缝、永不右推，配安全钳制（仅 Real 成员驱动碰撞；同 rank 用 rank 局部表面 `group_rank_edge` 判定，跨 rank 用精确 y_band 相交才按全局矩形钳制；clamp/Virtual 零宽元素豁免），每次位移同步平移绘制框。`snap_boundaries_to_members` 保留为 clamp 唯一事后写者。hier_eval 新增 sibling 分隔硬门禁（无祖先关系组对、主轴投影重叠 → 次轴间隙 ≥ `GROUP_FRAME_GAP−EPS`，否则 hard failure；LR 别名扫描兜底）。验收：cargo check 零警告；layout/engine 测试全绿；hier_eval 全量——Strong bit-identical、Weak bbox 最大 +16（更紧凑）、穿组观测 68 处与基线逐 fixture 一致；sibling 门禁 26 fixture 有 pair 覆盖非空转；showcase 二次强制渲染 0 changed（确定性）。GroupMinWidth 推迟（见 §8.7）。

---

### D₂.2 · 穿组硬化 + Gate 容量 / fallback 收口 ≈ 1.5–2 周

**目标**：Weak 穿组 hier_eval **硬失败**（清存量或 relaxation 可解释）；Gate 容量进 Demand；fallback 可观测且下降。

#### 8.10 穿组存量治理

- [x] 分类 ~11 fixture：  
  - **A 类**：外轨 / 框障碍可清（复用 Strong `group_obstacles` 思路）——已由 D₂.0 收编（两 policy 均传框障碍给外轨），本切片核实无残留 A 类  
  - **B 类**：须经 Gate；ScopeMask / cut_line 与框真源不一致 → 修 derive——本切片核实无纯 B 类（残留均归 C，见落地记录）  
  - **C 类**：暂时不可达 → 显式 `relaxations` + 单图豁免清单（禁止静默）——11 fixture / 68 处全部 C 类，`// d2-exempt: group-penetration` 源码标注 + 下方登记表  
- [x] hier_eval：Weak 默认硬门禁；仅 C 类带源码标注豁免  
- [x] 禁止用「放大框」假清穿组而破坏 sibling / 画布暴涨（阈值宽度）——canvas-bloat 护栏：weak bbox 任一维超基线 +10% 硬失败（复用 `hier_eval_baseline.json`，未另建快照）  

落地记录（2026-08-11，D₂.2a）：穿组分类结论——存量 68 处 / 11 fixture **全部 C 类**，两类根因：① **C-fallback**（5 fixture / 58 处）：gate 路由不可行（ScopeMask 限制）→ 整图回退 root-scope，路由无组感知；② **C-lane**（6 fixture / 10 处）：gate 启用但基片轨道无外框避让，轨道直接穿越外组框内部。A 类机制已由 D₂.0 耗尽（外轨框障碍两 policy 均生效），残留穿透均为基片/内轨层 = §8.12 机制范畴 → C。门禁落地：hier_eval 穿组 weak 分支由观测硬化为默认 hard failure；豁免判据 = fixture 源码含 `// d2-exempt: group-penetration — <原因>`，被豁免者保持喧响观测（计数 + 逐条明细 + relaxation 原因）；画布暴涨护栏按基线 +10% 硬门禁（落地偏差：复用已检入 `hier_eval_baseline.json` 而非新建快照，等价护栏）。C 类豁免登记表：

| fixture | 类 | 条数 | 根因 | 清零去向 |
|---|---|---|---|---|
| k8s-blue-green-release-topology | C-fallback | 13 | gate 不可行(e20) → root-scope | 基片 blocked region + gate 可行性（未来工作） |
| k8s-multi-namespace-overview | C-fallback | 15 | gate 不可行(e3) → root-scope | 基片 blocked region + gate 可行性（未来工作） |
| k8s-platform-stack | C-fallback | 11 | gate 不可行(e23) → root-scope | 基片 blocked region + gate 可行性（未来工作） |
| plotgram-core-mod-deps | C-fallback | 12 | gate 不可行(e26) → root-scope | 基片 blocked region + gate 可行性（未来工作） |
| k8s-tenant-isolation | C-fallback | 7 | gate 不可行(e6) → root-scope | 基片 blocked region + gate 可行性（未来工作） |
| ai-agent-docops-pipeline | C-lane | 1 | 基片轨道穿外框 | 基片 blocked region（未来工作） |
| ci-cd-security-pipeline | C-lane | 1 | 同上 | 基片 blocked region（未来工作） |
| hybrid-cloud-dr-topology | C-lane | 3 | 同上 | 基片 blocked region（未来工作） |
| realtime-recommendation | C-lane | 3 | 同上 | 基片 blocked region（未来工作） |
| supply-chain-control-tower | C-lane | 1 | 同上 | 基片 blocked region（未来工作） |
| product.cdn-cache | C-lane | 1 | 同上 | 基片 blocked region（未来工作） |

验收：cargo check 零警告；layout/engine 全绿（含豁免标注解析 + bbox 护栏表驱动单测）；hier_eval 12 测试全绿（非豁免穿组 = 0，豁免计数与登记表逐项一致）；Strong bit-identical；bbox 护栏无越界；showcase 二次强制渲染 0 changed（标注为纯注释不影响几何）。

#### 8.11 Gate 容量 → Metric

- [x] 相 I：按 `GateKey` 累计穿行边 demand（车道 × `edge_gap`，封顶策略对齐 Macro demand）  
- [x] 相 II：DemandBoard → 相关 LayerGap / 段长下界  
- [x] **禁止**「容量 = 跨界边数」恒真无预算  

落地记录（2026-08-11，D₂.2b）：`demand::publish_gate_capacity_demand`（demand.rs）。计数口径：每组按成员 rank extent `[min_r, max_r]` 取两个主轴 gate（顶缝 `min_r−1`、底缝 `max_r`），统计两端点在该缝内外分离且外端超出 extent 的边（同 rank 侧向跨界 = cross-axis gate，不计）。公式 `base_layer_gap + min((count−1)×edge_gap, MACRO_DEMAND_MAX_EXTRA_LANES×edge_gap)`（封顶 4 lanes，对齐 Macro 先例）；count ≤ 1 不发布（基础缝容纳单车道）——即「有预算」而非恒真。orchestrator 在 freeze 前发布（mod.rs Weak arm），带语义闸门：整图已回退 root-scope（`used_gates == false`）的图没有 gate 泳道，不发布。DemandBoard max-merge 与 channel / shell producer 自然共存：gate 容量通常被 channel demand（tracks ≥ 穿行数）支配 → 几何不变；全量仅 6 fixture 上 gate demand 胜出合法抬缝（均在 canvas-bloat +10% 护栏内）：data-lineage / fintech-payment / insurance-claim / mcp-server-cluster +16h，microservices / layout-stress-nested +8h。21 个 gate-on fixture 发布（缝数 1–6；mcp-server-cluster 与 message-queue-pipeline 各 6 缝）。表驱动单测 4 个：count→demand 表（1/2/5/99 → 不发布/56/104/104）、穿行数单调、嵌套组顶/底缝映射、侧向跨界不发布。

#### 8.12 fallback 收口

- [x] 统计 `channel-group-fallback` 触发率（diagnostics）  
- [x] 框真源后：EmptyGroup / OverlappingGroups / ForeignNode 分类修复或硬失败——全量触发 = 0，保留软回退 + relaxation（见落地记录）  
- [x] fallback 必须写 `relaxations`；目标：代表 showcase 零 fallback 或白名单——relaxations 全量可观测；**偏差**：触发数未降（6 fixture 保持），per-edge 递减试验无效撤回（见落地记录）  

落地记录（2026-08-11，D₂.2b）：

- **可观测**：`HierarchicalObs.gate_fallback_events` = rule=`channel-group-fallback` 的 relaxation 计数（覆盖 route 级整图回退与 derive 级不纯框回退两入口）；hier_eval FileMetrics 加 `fb` 列 + `[fallback]` 逐条明细喧响打印；表驱动测试 `d22b_fallback_observability_and_demand_floor` 断言计数与 relaxations 一致。
- **基线对比**：全量 6 fixture × 1 event——D₂.2a 已登记的 5 fixture（blue-green e20 / multi-namespace e3 / platform-stack e23 / tenant-isolation e6 / plotgram-core e26）+ product.d2-cell-tower-network（e5），**全部 route 级**「group-gate routing infeasible → 整图 root-scope 回退」。derive 级 EmptyGroup / OverlappingGroups / ForeignNodeInGroupRect 三分类：全量 showcase 0 触发（框真源后不纯框已消失）；软回退 + relaxation 保留，不升级硬失败（避免误伤未来合法用例）。
- **per-edge 降级试验 → 撤回**：曾尝试不可行边单条以 `ScopeMask::unrestricted()` 重试。结果：放宽后的边仍走组基片上的 root 轨道，穿组不降（69 vs 基线 68），且 4 fixture bends 翻倍（tenant-isolation 52→104）、canvas-bloat 破护栏（+105/+121px）→ 按切片边界条款撤回机制。结论：**ScopeMask 放宽不解决穿组**，清零的实际机制是基片 blocked region（未来工作）；试验结论留存于 route_all.rs doc 注释。

#### 8.13 验收（D₂.2 Done = D₂ 里程碑）

验收记录（2026-08-11，D₂.2b）：

1. Weak 穿组：全量硬门禁绿（非豁免 = 0）；C 类豁免保持 68 处 / 11 fixture，计数与基线一致。**偏差**：「→ 0 趋势」未兑现——per-edge 试验证明 ScopeMask 放宽非清零机制（§8.12 落地记录），清零归基片 blocked region（未来工作）；登记表清零去向列已同步，豁免保持喧响不静默。
2. Gate 容量：表驱动单调测试绿（count 1/2/5/99 → 不发布/56/104/104，封顶 4 lanes）；6 fixture 实际抬缝（§8.11 落地记录）。
3. fallback：无静默（`gate_fallback_events` + hier_eval `[fallback]` 喧响 + d22b 测试断言）；**偏差**：触发数未降（6→6）——递减机制（per-edge）试验无效撤回，根因如实记录。
4. MetricVerifier 最低集：含成员（containment）、兄弟分隔（sibling separation）已是 hier_eval 硬门禁（D₂.0/D₂.1）；**新增 Demand 下界** `check_demand_floor`——每个已发布 LayerGap demand 的缝 resolved ≥ demand − EPS（`DemandBoard::layer_gap_lower_bounds()` → `HierarchicalObs.layer_gap_demands/layer_gaps` 暴露，防未来 producer 绕过 DemandBoard）。
5. 文档：本节勾选 + roadmap §5.2 勾完 + group-state-review 过时句补丁。

整体验收：cargo check 零警告；workspace 566 passed 0 failed（含新增 gate 容量 / demand floor / fallback 观测测试）；hier_eval 全量绿；Strong bit-identical；showcase 首渲 6 changed（gate 容量合法抬缝，env 探针逐 fixture 精确归因，均 < +10% 护栏）/ 二渲 0 changed（确定性）。

---

## 9. 模块落点（建议）

```text
crates/plotgram-layout/src/layout/hierarchical/
  group_frame.rs          // pad 契约；D₂.0 更新写者说明
  metric/
    group_frames.rs       // 新增：框变量 + 含/分隔约束（或并入 symmetry_objective）
    symmetry_objective.rs // 删/收 compact；调用 group_frames
  demand.rs               // GroupMin* / GateLaneDemand
  mod.rs                  // Weak tail 带出 groups；obstacles 注入
crates/plotgram-engine/src/finalize.rs
  // 透传 Layout 已写 groups
crates/plotgram-compile/tests/hier_eval.rs
  // D₂.2 Weak 穿组硬门禁
```

禁止：`ink/` / `channel/` 内 `match group_policy`；policy 特化留在 `hierarchical/mod.rs` + 参数化 hook。

---

## 10. 风险与裁定

| 风险 | 裁定 |
|------|------|
| 框进 VPSC 后 weak 几何大变 | D₂.0 Fit 贴外包络；baseline 允许更新但要门禁护航 |
| 与 GroupBoundary 双源 | D₂.0 选项 B；D₂.1 收齐 |
| 嵌套双 pad 与 Strong 不一致 | Weak 保持现行 finalize 公式；Strong 不动 |
| 不可行爆炸 | 硬失败 + 冲突链；禁止静默丢含约束 |
| 与 D₁ 抢进度 | D₂.0/1 不改搜索；D₂.2 只加 Demand/障碍 |
| Strong 误伤 | 全程 Strong 回归 bit-identical |

---

## 11. 工作量总览

| 切片 | 粗工期 | 依赖 |
|------|--------|------|
| **D₂.0** | 1.5–2.5 周 | 无（可立即开） |
| **D₂.1** | 1–1.5 周 | D₂.0 |
| **D₂.2** | 1.5–2 周 | D₂.0；受益于 D₂.1 |
| **合计** | ≈ 4–6 周 | 串行主路径 |

---

## 12. Definition of Done（整段 D₂）

1. Weak：框真源 = Metric；finalize 在 `owns_group_frames` 下不重算（含空向量）。  
2. Containment（成员 ⊆ 框 + 子组 ⊆ 父组）+ sibling separation 由构造（+ hier_eval 硬门禁）成立。  
3. Weak 穿组门禁**硬化**：默认 hard failure；存量 C 类以 `d2-exempt` 登记（**非清零**——清零归基片 blocked region 后续工作）。  
4. Gate 容量进 Demand；fallback 可观测（触发次数未强制下降）。  
5. Strong：组框唯一写者 = MacroBlockWriter（**禁止**再跑 `solve_group_frames`）；无组 / Strong 回归不被破坏。  
6. 无 Ink 事后挪组；无同一 policy 下双框写者。

---

## 13. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-08-11 | 初版：产品裁定「要可证」；D₂.0–D₂.2 可执行方案；与 StrongMacro / 现状债划界 |
| 2026-08-11 | D₂.0 落地：框真源进 Metric（`metric/group_frames.rs`）；finalize 透传；76 fixture 零 delta |
| 2026-08-11 | D₂.1 落地：sibling 硬分隔进 VPSC（无亲缘 clamp 对 extra→`GROUP_FRAME_GAP`）；compact 裁定缩为只收缝写者（非全删）+ 安全钳制；hier_eval sibling 分隔硬门禁；Strong bit-identical、Weak 穿组观测与基线一致 |
| 2026-08-11 | D₂.2a 落地：Weak 穿组门禁硬化（默认 hard failure + `d2-exempt` C 类豁免 + canvas-bloat 护栏）；存量 68 处分类全部 C 类（C-fallback 58 / C-lane 10），清零归 D₂.2b |
| 2026-08-11 | D₂.2b 落地，**D₂ 里程碑关闭**：Gate 容量进 Demand（`publish_gate_capacity_demand`，Macro 封顶，6 fixture 合法抬缝）；fallback 全量可观测（`gate_fallback_events` + hier_eval `fb` 列 / `[fallback]` 喧响）；per-edge 降级试验无效撤回（穿组 68 处保持登记，清零归基片 blocked region 未来工作）；MetricVerifier Demand 下界门禁（`check_demand_floor`）；Strong bit-identical、workspace 全绿零警告 |
| 2026-08-11 | 评审修：Strong 跳过 `solve_group_frames`、MacroBlock 直接写 `GroupPlacement`；`owns_group_frames` 禁 finalize 空向量兜底；hier_eval 补 member⊆host；DoD 口径改为「硬化+豁免」非清零；diagnostics 注释去 per-edge |
