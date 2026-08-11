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
finalize / Facade：若 Layout 已带 groups → **透传**；禁止重算真源
```

Strong 路径：**不**走本 Metric 框变量；继续 MacroBlockWriter + `TailFrames::Fixed`。

### 6.2 写权表（Weak + D₂）

| 自由度 | 写者 | 禁止 |
|--------|------|------|
| 组内/全局节点坐标 | Metric `J(x)`+VPSC | Ink / finalize 挪节点消框债 |
| **组框几何（Weak）** | **Metric 组框 Writer** | finalize `union+pad` 当真源；Ink 平移组 |
| 组框几何（Strong） | MacroBlockWriter | 本方案 VPSC 框变量 |
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
  groups: [GroupPlacement]   // D₂.0+ Weak：Metric 已填；Strong：既有 Fixed
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

- [ ] 记录 weak 穿组清单（fixture × edge × depth）与 hier_eval baseline 哈希  
- [ ] 选定 3–5 张代表图：无组 / 单组 / 兄弟并排 / 嵌套 / 已知穿组各一  
- [ ] 钉死 pad 契约：与 `group_frame.rs` + 现行 finalize 公式一致（含嵌套双 pad）

#### 8.1 写权与 IR

- [ ] **裁定** `GroupBoundary` vs 框变量：默认走 §7.2 **选项 B**（文档 + 代码注释写死）  
- [ ] Metric 产出 `Vec<GroupPlacement>`（组树后序，确定性序）  
- [ ] `LayoutOutput.groups`（或等价通道）Weak 必填；空组策略与今日一致（不画空框）  
- [ ] Strong：继续 Fixed frames；**零** VPSC 框变量

#### 8.2 finalize / Facade

- [ ] `plotgram-engine::finalize`：若输入已含非空 `groups` 真源 → **透传**（可做坐标系整体平移若 Stage 需要）  
- [ ] 仅当 layout **未**提供 groups（非 Hier / 旧路径）时保留后验计算，或 Hier Weak 硬要求必带——**禁止** Hier Weak「带了又覆盖」  
- [ ] 单测：同一节点几何下，透传框 ≠ 再算一遍不得被覆盖

#### 8.3 Metric 最小约束（Fit）

- [ ] 引入组框自由度 + containment 硬约束（成员 + 子框）  
- [ ] Fit 目标：框贴近外包络（避免无故胀框）  
- [ ] 主轴：与既有 `group_shell_bands` / LayerGap 一致，避免框顶底与走廊 demand 打架  
- [ ] 失败：不可行 → `InfeasibleConstraint`，禁止静默丢约束

#### 8.4 Channel / 外轨钩子（只做可读，不做满容量）

- [ ] Weak 路径亦可把 Metric 框注入 `group_obstacles`（或等价），对齐 Strong 外轨避让先例  
- [ ] **不**在本切片做 Gate 容量公式；fallback 逻辑不变，但框纯度应开始改善（观测）

#### 8.5 验收（D₂.0 Done）

1. Hier Weak：finalize 源码路径对已提供 groups **不重算真源**（代码审查 + 单测）。  
2. 代表图 containment：成员 ⊆ 框（EPS）；嵌套子框 ⊆ 父框（已有门禁应仍绿）。  
3. Strong + 无组图：几何 bit-identical（或仅 params_hash / 无关字段变）。  
4. Weak 全量 hier_eval：硬不变量绿；穿组 **仍可观测**，但鼓励深度下降（不强制清零）。  
5. 文档：roadmap / expectations 指向本文；`group_frame.rs` 模块头更新写者说明。

**非目标（D₂.0）**：sibling 硬分隔主路径、穿组硬门禁、Gate 容量、删 compact、title 美学。

---

### D₂.1 · Sibling + Demand；退役 compact 主路径 ≈ 1–1.5 周

**目标**：兄弟框分隔由求解保证；post-VPSC compact 降为可选/删除；title / min-size 进 Demand（可薄）。

#### 8.6 Sibling

- [ ] 无祖先关系的组对：若 cross 投影相交（或 rank span 相交，与现行 compact 过滤对齐）→ 硬分隔 ≥ `GROUP_FRAME_GAP`  
- [ ] 主轴重叠兄弟：沿次轴推开（VPSC）；禁止 Ink 刚体平移整组  
- [ ] 嵌套：父子是含约束，**不是** sibling  

#### 8.7 Demand / title

- [ ] 有 label：顶边至少 `GROUP_LABEL_TOP_PAD`（已在含约束）+ 可选 `GroupMinWidth` 吃长标题（可第二周）  
- [ ] `group_sizing` / `group_align`：若仍从 params 删除则本切片**不**恢复空字段；有需求再 bind 真消费  

#### 8.8 退役 compact

- [ ] `compact_sibling_frame_gaps`：求解已保证分隔后 **删除** 或缩成 debug-only assert「无剩余超额缝」  
- [ ] `snap_boundaries_to_members`：与选项 B 对齐——boundary 跟框，或跟成员且框已含 pad（二选一，禁止双源打架）  

#### 8.9 验收（D₂.1 Done）

1. 构造兄弟并排 fixture：框间隙 ≥ `GROUP_FRAME_GAP`，且关闭 compact 仍成立。  
2. Weak 代表图无非法兄弟重叠（MetricVerifier 或 hier_eval）。  
3. compact 主路径已删或测试证明为 no-op。  
4. Strong 仍 bit-identical。

**非目标**：穿组硬门禁清零、Gate 容量。

---

### D₂.2 · 穿组硬化 + Gate 容量 / fallback 收口 ≈ 1.5–2 周

**目标**：Weak 穿组 hier_eval **硬失败**（清存量或 relaxation 可解释）；Gate 容量进 Demand；fallback 可观测且下降。

#### 8.10 穿组存量治理

- [ ] 分类 ~11 fixture：  
  - **A 类**：外轨 / 框障碍可清（复用 Strong `group_obstacles` 思路）  
  - **B 类**：须经 Gate；ScopeMask / cut_line 与框真源不一致 → 修 derive  
  - **C 类**：暂时不可达 → 显式 `relaxations` + 单图豁免清单（禁止静默）  
- [ ] hier_eval：Weak 默认硬门禁；仅 C 类带源码标注豁免  
- [ ] 禁止用「放大框」假清穿组而破坏 sibling / 画布暴涨（阈值宽度）

#### 8.11 Gate 容量 → Metric

- [ ] 相 I：按 `GateKey` 累计穿行边 demand（车道 × `edge_gap`，封顶策略对齐 Macro demand）  
- [ ] 相 II：DemandBoard → 相关 LayerGap / 段长下界  
- [ ] **禁止**「容量 = 跨界边数」恒真无预算  

#### 8.12 fallback 收口

- [ ] 统计 `channel-group-fallback` 触发率（diagnostics）  
- [ ] 框真源后：EmptyGroup / OverlappingGroups / ForeignNode 分类修复或硬失败  
- [ ] fallback 必须写 `relaxations`；目标：代表 showcase 零 fallback 或白名单  

#### 8.13 验收（D₂.2 Done = D₂ 里程碑）

1. Weak 穿组：全量硬门禁绿，或仅文档化 C 类豁免且条数 → 0 趋势。  
2. Gate 容量：至少 1 张密跨组图缝宽随边数单调（表驱动单测）。  
3. fallback：基线对比触发次数下降；无静默。  
4. MetricVerifier 最低集（coordinate-and-demand §9）含：含成员、兄弟分隔、Demand 下界。  
5. 文档：roadmap §5.2 勾完；group-state-review 过时句打补丁或加「以 D₂ 文为准」。

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

1. Weak：框真源 = Metric；finalize 不重算。  
2. Containment + sibling separation 由构造（+ verifier）成立。  
3. Weak 穿组门禁硬化（豁免可解释且趋零）。  
4. Gate 容量进 Demand；fallback 可观测。  
5. Strong / 无组路径不被破坏。  
6. 无 Ink 事后挪组；无 Strong 上 VPSC 框双写者。

---

## 13. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-08-11 | 初版：产品裁定「要可证」；D₂.0–D₂.2 可执行方案；与 StrongMacro / 现状债划界 |
