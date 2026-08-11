# Hierarchical · 重建阶段路线

> 目的：在 [architecture.md](architecture.md) §12 里程碑之上，写清 **MVP 之后各阶段做什么、大致朝哪走**。  
> 不替代目标架构；不写进度日记。实现取舍真源仍见 [notes/2026-08-02-mvp-scope.md](notes/2026-08-02-mvp-scope.md)。  
> 写权尺子：[write-authority.md](../write-authority.md)。  
> 组策略选型：[expectations.md](expectations.md) §7 · StrongMacro 方案：[phases/strong-macro.md](phases/strong-macro.md)。

---

## 0. 当前位置（2026-08）

### 0.1 已交付主路径

主路径已远超当初 MVP 子集：

```text
FAS(+环 reroot) → rank / properify / order
  → ports（FREE / FixedSide）→ Metric（J(x)+VPSC / PortLane / Track）
  → Channel（Substrate + TrackOrder + rip-up + Corridor Allocator）
  → Ink 展开 + diagnostics
```

| 能力 | 状态 |
|------|------|
| 阶段 A–C（次轴拉直 / 端口 / 诊断） | ✅ 已收口（见 §2–§4） |
| **D₁** Channel（D1.0–D1.3.5） | ✅ 已交付；余量见 §5.1 |
| **Weak** 组策略（默认） | ✅ 边界 dummy + Gate/ScopeMask + finalize 后验框 + post-VPSC compact |
| **StrongMacro**（`group_policy: strong-macro`） | ✅ SM-0..4：MacroBlockWriter 写框；与 Weak **同一 Plan schema**；共享 Channel/Ink 尾 |
| 穿组 verifier | ✅ `verify_no_group_penetration`（strong 硬门禁 / weak 观测） |
| 边参数三档 + `auto_edge_grouping` + `weight`/`critical` | ✅ |
| PortLane / 对称轴 `J(x)` | ✅ |

硬不变量（无重叠、端口落界、组包含）由 `hier_eval` 守住；全正交断言仅对默认/`orthogonal` 成立（`polyline`/`curved` 见 [edge-parameters](edge-parameters.md)）。

### 0.2 相对目标架构的主要缺口

| 缺口 | 观感 / 产品影响 | 归属 |
|------|-----------------|------|
| **Weak 组框仍是 finalize 后验 bbox**（非 VPSC 真源）；兄弟分隔靠 compact 止血 | 框可漂移 / 穿组未由构造成立；weak ~11 fixture 穿组仅观测 | **D₂** |
| Gate 容量 → Metric 缝宽；`channel-group-fallback` 仍偏多 | fallback 后等同无组，交叉易爆 | D₂ ∩ Channel 收口 |
| `group_anchor` 仍当普通节点分层（ADR-004 未闭合） | 贴框语义不纯 | 组专项 |
| strong-port projection；label / loop reserve | 端口序不进列位；标签/自环空间未建模 | 余项（原 M2） |
| ordering 落选孩子 / 多父列位 | 分支边大 Z | Compose `order` |
| **PartitionGrid** 未消费 | 泳道 / 矩阵不到位 | **E / M5** |
| architecture profile 默认 strong | 本轮不做（语料小）；须显式 `group_policy` | SM-4 缓项 |
| Integrated labeling；octilinear / 真 MCF / from-sketch | 不挡主路径闭环 | E / 后置 |
| 全局 Grid | 与 PortLane 正交，单开 | §10 |

**总方向**：A–C / D₁ / StrongMacro 已过；下一块产品主菜是 **缩小后的 D₂（Weak 框写权）**，或 **PartitionGrid**，按痛点二选一。Strong 路径框写者已是 MacroBlockWriter——**禁止**再叠一套 VPSC 组框变量。

---

## 1. 阶段总览

```text
MVP ──► A 次轴 ──► B 端口 ──► C 诊断     ✅
              │
              ├─► D₁ Channel（D1.0–D1.3.5）  ✅ 主路径；余量收口
              │
              ├─► D₂ Weak 组框写权           ✅ 已关闭（见 §5.2）
              │
              └─► E StrongMacro ✅ · Partition / labeling …
```

| 阶段 | 对齐 architecture | 状态 | 一句话 |
|------|-------------------|------|--------|
| **A** | M1 | ✅ | Metric 长链 / 端口列对齐 |
| **B** | M2 | ✅（余项见 §0.2） | 端口决议；Ink 零猜测 |
| **C** | M0 契约 | ✅ | diagnostics / params_hash |
| **D₁** | M3 | ✅ 主路径 | 走廊 + track + rip-up + allocator |
| **D₂** | M4（Weak 半） | ✅ | **仅 Weak**：框进 Metric；穿组由构造 + 门禁硬化（残留 C 类豁免登记，见 group-frame-d2.md §8.10） |
| **E** | M4+/M5/后置 | Strong ✅；其余 ◻ | PartitionGrid、labeling、… |

**约束**：历史「D₁ 与 D₂ 不要并行」仍成立——D₁ 主路径已完，开 D₂ 时不要再开第二条大 Channel。E 不挡 D；StrongMacro 与 D₂ 都碰「组框写者」时：**Strong = MacroBlockWriter；Weak+D₂ = VPSC 框变量**（[strong-macro.md](phases/strong-macro.md) §3 / §8）。

---

## 2. 阶段 A — 次轴拉直（优先）

**方向**：折点问题的主写者在 Metric，不在 Ink。把「长边更直、端口可列对齐」做成可验收的度量目标。

**做什么（概要）**：

- Ideal 质量向 Brandes–Köpf 靠拢（四候选按固定规则合并）；阻尼重心视为过渡。注：dummy 高权重（virtual 4.0）现状已有，不是本阶段新增项
- 长 proper / dummy 链：**BK block 内硬共线（由构造保证可行）+ block 外强软对齐**；VPSC 从「仅层内」升为跨层统一求解。硬共线等式加在 block 的**同类型相邻对**上：virtual-virtual 保 dummy 主干笔直；real-real 拉直 1:1 主链（主链不得微 Z、叶节点必须落在唯一邻居正下方）。例外有二：**链拖拽守卫**——与 dummy 相邻过的 real 成员（median 抢到了 dummy 的）保持软目标，否则硬等式会把整条链拖离走廊，且第二遍端口锚点拉动会拖着 real 节点走、端口对齐失效；**偶扇守卫**——real 对任一侧有 ≥4 的偶数 real 邻居时松开，junction 改由第二遍居中 desired（中间两个邻居 pass-1 位的中点）落位，因为 BK 只能把 junction 焊在一个 median 孩子列上，偶扇时该列必然偏心中。奇扇（≥3 奇数）与二分叉保持硬化：median 对所在列就是扇的中心，且主链可直穿 junction。两个守卫不是独立特判，而是单写者纪律在等式约束上的推论：**硬等式 = 熔成刚体、共享一个列自由度；任一成员的列已属别的写者（dummy 走廊列位争夺 / 偶扇中心展开）时对不得硬化**。熔断：若将来需要第三个守卫，不再往约束循环加排除条件，而是把「刚体成员资格」上提为显式决策步（单谓词产出成员表，约束构造只消费）。禁止对 block 外的链直接加跨层等式约束——交叉链会使其不可行；不可行时报 `InfeasibleConstraint`，不得静默降级为特判
- 端口对齐进次轴目标（不只拉节点中心）。锚点依赖节点 frame，展开顺序钉死为两遍：先解节点中心 → 按 `along_spec` 展开锚点 → 以锚点为 desired 第二遍拉 dummy 链；写者全程只有 CoordWriter，Metric 不读 Ink 输出
- `hier_eval`：开工前先记录 per-fixture 折点基线，再按 fixture 建回归阈值；同时记 sum_bends，防止「最大折点降、总折点涨」的假改善

**刻意不做**：改 Ink 发明拓扑；上完整 Channel；为消楼梯在 Ink 叠 dogleg 特判。

**验收**：含长回边的目标 fixture max_bends / sum_bends 明显下降（以开工前基线为准）；69 个 fixture 硬不变量全绿；crossings 与画布宽度无显著恶化（拉直链可能拉宽图，需守住）。

---

## 3. 阶段 B — 端口补齐

**方向**：Compose 继续是端口唯一写者；Metric 只展开；减少无意义微偏。

**做什么（概要）**：

- FREE 单边 / 少边时偏向中心（或稳定中心 slot）
- 在 model 字段允许的范围内补强约束档位；缺字段则先扩 IR，禁止静默 no-op
- 保持四向 Orientation 下 fixed side 正确（已有回归须守住）

**刻意不做**：Ink `unwrap_or` 默认侧；用 slot 同时当像素真源。

**已收口（阶段 B 落地摘要）**：边端口收敛为 FREE / `FixedSide`（DSL 仅 `from_side`/`to_side`）；`FixedOrder` 仅 group_anchor；已移除边级 slot/ratio/pos/sides。Compose 决议（FREE 走 shape→port policy + 拓扑选侧、G3 仅对无 dummy 链回边选 East/West）；Metric 按 `Ordered` 相对序稠密居中 / `LocalOffset` 展开；Ink 只读零猜测。多 rank 回边的 East/West 与 strong-port projection / label-loop reserve 归入 §0.2 缺口表。

---

## 4. 阶段 C — 诊断与契约出口

**方向**：没有 diagnostics，后面的 Channel / Gate 返工只能靠肉眼。

**做什么（概要）**：

- `LayoutOutput`（或等价通道）暴露 warnings / relaxations / params_hash
- Unsupported / Infeasible 硬失败语义保持；软放宽必须进诊断
- 为后续 verifier（Plan / Metric / Ink）留稳定报告字段

**刻意不做**：用墙钟超时改变布局结果；用日志替代结构化诊断。

**已收口（阶段 C 落地摘要）**：`plotgram-model::diagnostics::LayoutDiagnostics`
（warnings / relaxations / params_hash）进 `LayoutOutput` 与 `LayoutResult`；
hierarchical bind 的未知 option warning 透出（此前被丢弃），`params_hash`
为 FNV-1a 64 对 canonical 参数串的确定性哈希（归因参数 vs 代码）。
Unsupported / Infeasible 硬失败语义不变；relaxations 已由 Channel rip-up 等消费。出口：CLI render stderr warnings、measure
JSON `diagnostics` 段；与 `LayoutDebugTrace` 并列产出、不合并
（debug-inspector.md §5.5）。验收：hier_eval 与全部快照几何零变化。

---

## 5. 阶段 D

### 5.1 D₁ · Channel 正交（密边 / 走廊）— 主路径已交付

**方向**：组合相写路径拓扑与 track 序；Metric 写 track 像素；Ink 只展开。

**可执行契约**：[phases/channel-d1.md](phases/channel-d1.md)；走廊分配：[phases/channel-corridor-allocator.md](phases/channel-corridor-allocator.md)。

| 子里程碑 | 状态 | 摘要 |
|----------|------|------|
| **D1.0** | ✅ | 层间走廊 TrackOrder + 最小 DemandBoard；修假 bus；消费 `edge_gap` |
| **D1.1** | ✅ | Substrate + 词典序搜索；`RouteTopology::Orthogonal` |
| **D1.2** | ✅ | Gate / ScopeMask / 有界 rip-up；端总线 `BundlePlan`；段长参数 |
| **D1.3.1–D1.3.5** | ✅ | SpanAffinity → 内层优先 → RouteOrder+rip-up → Corridor Demand → 回边侧别代价 |

**仍属收口（非新开大阶段）**：

- Gate 容量累计 → Metric 缝宽（架构有、`GateCapacity::Fixed` 已删，尚未进 Demand）
- 降低 `channel-group-fallback` 频率（依赖组矩形纯度；与 D₂ 框真源联动）
- 与 Weak 框真源对齐后的外轨 / Scope 一致性

**何时还要动 D₁**：密边路由回归、fallback 噪声、容量预算——**不要**为「架构舞台感」再开 Channel 特判（那是 StrongMacro）。

### 5.2 D₂ · Weak 组框写权（现行主缺口）

**方向（缩小后）**：仅 **Weak** 路径——组框由 Metric 写出，finalize 不重算；穿组由构造 + 已有 verifier 门禁硬化。  
**不再**把 StrongMacro /「同一 Plan」/ verifier 首实现算进 D₂——这些已由 E / SM-4 / 共享尾完成。

**可执行方案**：[phases/group-frame-d2.md](phases/group-frame-d2.md)（**D₂.0 → D₂.1 → D₂.2**）。已交付：D₂.0 框真源进 Metric；D₂.1 sibling 硬分隔；D₂.2 穿组硬门禁 + Gate 容量 + fallback 收口（**D₂ 里程碑关闭**，验收记录见 group-frame-d2.md §8.13）。

| 子切片 | 摘要 | 状态 |
|--------|------|------|
| **D₂.0** | 框真源进 Metric（Fit 含约束）；finalize 透传；外轨可读框 | ✅ 76 fixture 零 delta |
| **D₂.1** | sibling 硬分隔 + title/min Demand；退役 compact 主路径 | ✅ VPSC 硬分隔 + hier_eval 门禁 |
| **D₂.2** | Weak 穿组硬门禁；Gate 容量 → Demand；fallback 收口 | ✅ a 门禁硬化 + b 容量/观测/verifier（per-edge 试验撤回；穿组清零归基片 blocked region 未来工作） |

| 原 D₂ 条目 | 现状 | D₂ 是否还做 |
|------------|------|-------------|
| StrongMacro 同 Plan schema | ✅ expand → 全局 `PlanGraph` | 否 |
| `verify_no_group_penetration` | ✅ SM-4；strong 硬 / weak 观测 | ✅ D₂.2a Weak 硬门禁（C 类豁免登记） |
| Gate / ScopeMask / `verify_route_scope` | ✅ 大体在 D1.2 | 否（从零做 Gate）；D₂.2b 补 Gate 容量 demand |
| **组框进 VPSC**（含 / 分隔 / title demand） | Weak = finalize 后验 + compact | ✅ D₂.0–D₂.1 |
| 兄弟框硬分隔 | compact 止血 | ✅ D₂.1 |
| Weak 穿组由构造成立 | ~11 fixture 仍观测穿组 | ✅ D₂.2 硬门禁；残留 68 处 C 类豁免登记（清零归基片 blocked region 未来工作） |

**刻意不做**：第二套 `ArchitectureLayout`；Ink 事后挪组；用 StrongMacro 冒充 Weak 框真源；Strong 路径叠 VPSC 框变量。

**何时选**：产品痛点是 **Weak 下**组框合法性、跨组穿框可证——不是架构分层舞台感（那用 `strong-macro`，见 [expectations §7](expectations.md)）。

---

## 6. 阶段 E — 后置能力

**方向**：图种差异只经 profile 进参数；不新增第二套布局器。

| 项 | 状态 | 方向摘要 |
|----|------|----------|
| **StrongMacro** | ✅ SM-0..4 | 组树后序局部 Plan → macro → 与 Weak 同一 Plan。方案：[phases/strong-macro.md](phases/strong-macro.md)。缓项：architecture profile 默认 strong |
| **PartitionGrid** | ◻ | 引擎消费 cell / band；Orientation 轴语义；非 `group` 冒充泳道（ADR-008） |
| **Bundle / auto_edge_grouping** | ✅ 端总线主路径 | Compose 写合流；Ink 接合；美学可渐进 |
| **Integrated labeling** | ◻ | 至少 label 需求进 Demand；完整联合求解可渐进 |
| **真 MCF / from-sketch / octilinear** | ◻ | 明确不挡主路径闭环；`octilinear` bind 仍硬失败 |

---

## 7. 原则（各阶段共用）

1. **单写者 / 落笔零新决策** — 修楼梯先问 Metric，不在 Ink 加特判。  
2. **有界返工优先于真全局优化** — Channel 用 rip-up，不上真 MCF 挡闭环。  
3. **禁止图名特判** — architecture = Hierarchical + profile 参数，不是 `ArchitectureLayout`。  
4. **参数能 bind 就必须被消费** — 否则 bind 硬失败（见 MVP 对 `edge_gap` 等的处理）。  
5. **Weak / Strong 不得永久两套产出类型** — 收缩策略不同，Plan schema 必须同一（已满足）。  
6. **组框单写者按 policy** — Strong: MacroBlockWriter；Weak(+D₂): Metric 框变量；禁止同一 policy 双写。

---

## 8. 与现有文档的关系

| 文档 | 角色 |
|------|------|
| **本文** | MVP 之后的**阶段路线与方向**（含现状快照） |
| [architecture.md](architecture.md) | 目标架构与 M0–M5 设计里程碑 |
| [expectations.md](expectations.md) | 视觉期待 + Weak/Strong 选型 |
| [notes/2026-08-02-mvp-scope.md](notes/2026-08-02-mvp-scope.md) | MVP 实现相对架构的取舍记录 |
| [notes/2026-08-09-group-state-review.md](notes/2026-08-09-group-state-review.md) | 组现状盘点（部分条目已过时，以本文 + strong-macro 为准） |
| [scope.md](scope.md) | 能力 / 非目标 / 典型域 |
| [edge-parameters.md](edge-parameters.md) | 边参数支持研究 + 分批实施路线 |
| [phases/](phases/) | 相级可执行契约 |
| [phases/channel-d1.md](phases/channel-d1.md) | D₁ Channel 分阶段契约 |
| [phases/channel-corridor-allocator.md](phases/channel-corridor-allocator.md) | D1.3 Corridor Allocator |
| [phases/strong-macro.md](phases/strong-macro.md) | StrongMacro 实现方案（SM-0..4） |
| [phases/group-frame-d2.md](phases/group-frame-d2.md) | **D₂ Weak 组框写权**（D₂.0–D₂.2） |

代码入口（重建）：`crates/plotgram-layout/src/layout/hierarchical/`。

---

## 9. 改善性重构 backlog（轻量）

**现状（P4 已落地）**：次轴由 [`symmetry_objective`](../../../crates/plotgram-layout/src/layout/hierarchical/metric/symmetry_objective.rs) 最小化 `J(x)`（加权中位 + VPSC + snap）；[`cross_axis`](../../../crates/plotgram-layout/src/layout/hierarchical/metric/cross_axis.rs) 为唯一入口。旧 `SymmetryPlan` / FanPack / claimed 已删。门禁：`hier_eval::symmetry_axis_d2_*`、`symmetry_axis_d3_*`、`twin_spine_*` 等。契约见 [phases/symmetry-axis.md](phases/symmetry-axis.md)；视觉裁定见 [expectations §6.1](expectations.md)。

**已完成**：

- ~~声明表 D1–D3（历史路径）~~ → ~~P4：目标函数取代声明表（S1–S4）~~  
- ~~代表图主链共线 / 跨层扇 / twin 占脊门禁~~  

**后续**：组边界 / group band 截断细则（组专项，非再开声明表）；与 D₂ Weak 框写权可一并裁定。

**不做**：Channel/Ink 改列；图名特判；VPSC 中点硬等式 / 第三趟 ad-hoc 拉回。

---

## 10. 端口列 / Grid backlog（轻量）

- **PortLane（已落地）**：双胞胎 N/S 走廊绝对端口列，无 grid；见 [phases/port-lanes.md](phases/port-lanes.md)、[expectations §6.2](expectations.md)。  
- **全局 Grid**（单开）：节点参考点贴网 + 端口 `ON_GRID` 类策略；**不**作为 PortLane 的前提。

---

## 11. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-08-11 | 同步现状：A–C / D₁ / StrongMacro / 穿组 verifier 已交付；§0 缺口表重写；D₂ 缩小为 Weak 框写权；E 表标 Strong ✅ |
| 2026-08-11 | 产品裁定 Weak 框要可证；§5.2 挂 [group-frame-d2.md](phases/group-frame-d2.md)（D₂.0–D₂.2） |
| 2026-08-11 | D₂ 里程碑关闭：D₂.0/D₂.1/D₂.2 全部落地（框真源 / sibling 硬分隔 / 穿组硬门禁 + Gate 容量 demand + fallback 可观测 + Demand 下界 verifier）；§5.2 勾完；穿组残留 68 处 C 类豁免登记，清零归基片 blocked region（未来工作） |
