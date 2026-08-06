# Hierarchical · 重建阶段路线

> 目的：在 [architecture.md](architecture.md) §12 里程碑之上，写清 **MVP 之后各阶段做什么、大致朝哪走**。  
> 不替代目标架构；不写进度日记。实现取舍真源仍见 [notes/2026-08-02-mvp-scope.md](notes/2026-08-02-mvp-scope.md)。  
> 写权尺子：[write-authority.md](../write-authority.md)。

---

## 0. 当前位置（MVP 已交付）

已具备可跑的 Sugiyama 主路径子集：

```text
FAS(+环 reroot) → Network Simplex → properify → median(+权+snapshot)
  → 阻尼重心 + 每层 VPSC → 端口（Free / FixedSide）→ dummy 链 Ink 正交展开
```

FAS 含环入口规则：声明序靠前的节点优先靠上（环 reroot，[architecture.md](architecture.md) §3.1）；
反转数回涨由 `hier_eval` 基线的 `reversed_count` 门禁守住。

硬不变量（无重叠、端口落界）已由 `hier_eval` 守住；全正交断言仅对
默认/`orthogonal` 风格成立（`routing_style: polyline/curved` 见
[edge-parameters](edge-parameters.md)）。

边参数产品补齐（不归入新阶段，属既有批次的参数收口，见
[edge-parameters](edge-parameters.md) §4）：`routing_style` 三档
（orthogonal/polyline/curved）、`auto_edge_grouping` 端口合流（bus-style）、
边级 `critical` 权重均已落地。

相对目标架构的主要缺口：

| 缺口 | 观感 / 产品影响 |
|------|-----------------|
| 多 rank 反向走廊边仍走 rank 方向侧别（写者 ports.rs）；跨轴出针需 Channel 消费才不增折点（阶段 B 已落地无 dummy 链回边的 East/West，见 G3） | 长回边走 dummy 链走廊；Channel（D₁）前跨轴侧别会增折点，故暂不启用 |
| strong-port projection（port dummy 进 ordering）与 label/loop reserve（M2 余项） | 端口序不参与列位决议；标签/自环保留空间未建模（留 C/D 阶段） |
| ordering 落选孩子 / 多父节点的列位（写者 order.rs） | 分支边大 Z（如落选孩子不在父节点出端口正下方） |
| 无完整 Channel / track / rip-up | 密边拥塞只能硬挤 |
| 组框后验 bbox；无 Gate/Scope | 跨组边可贴框 / 穿框 |
| StrongMacro / PartitionGrid 未消费 | 架构图 / 泳道类能力未到位 |

**总方向**：先收口「长边更直」与可观测性，再按产品主痛点二选一推进 Channel 或组框写权；StrongMacro / Partition / 完整 labeling 后置。

---

## 1. 阶段总览

```text
MVP ──► A 次轴拉直 ──► B 端口补齐 ──► C 诊断出口
              │
              └─► D₁ Channel 正交    或    D₂ 组框 + Gate
                              │
                              └─► E StrongMacro / Partition / labeling …
```

| 阶段 | 对齐 architecture | 一句话方向 |
|------|-------------------|------------|
| **A** | 收口 M1 视觉债 | Metric 把长链与端口列对齐；折点明显下降 |
| **B** | 补齐 M2 | 端口决议更稳；Ink 零猜测 |
| **C** | 补齐 M0 契约债 | Layout 输出可诊断、可回归 |
| **D₁** | M3 | 走廊拓扑 + track；拥塞有界返工 |
| **D₂** | M4 | 组框进求解；跨组只经 Gate |
| **E** | M4+/M5/后置 | StrongMacro、PartitionGrid、integrated labeling |

**约束**：A → B → C 顺序建议串行收口；**D₁ 与 D₂ 不要并行开两条**——先定产品主痛点（密边路由 vs 架构组框）再选。E 不挡 D 的主路径闭环。

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

**已收口（阶段 B 落地摘要）**：边端口收敛为 FREE / `FixedSide`（DSL 仅 `from_side`/`to_side`）；`FixedOrder` 仅 group_anchor；已移除边级 slot/ratio/pos/sides。Compose 决议（FREE 走 shape→port policy + 拓扑选侧、G3 仅对无 dummy 链回边选 East/West）；Metric 按 `Ordered` 相对序稠密居中 / `LocalOffset` 展开；Ink 只读零猜测。多 rank 回边的 East/West 与 strong-port projection / label-loop reserve 归入 §0 缺口表（留 C/D）。

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
Unsupported / Infeasible 硬失败语义不变；relaxations 通道已立、暂无生产方
（D 阶段 rip-up 等首批消费）。出口：CLI render stderr warnings、measure
JSON `diagnostics` 段；与 `LayoutDebugTrace` 并列产出、不合并
（debug-inspector.md §5.5）。验收：hier_eval 与全部快照几何零变化。

---

## 5. 阶段 D — 二选一深挖

### D₁ · Channel 正交（密边 / 走廊）

**方向**：组合相写路径拓扑与 track 序；Metric 写 track 像素；Ink 只展开。

**可执行契约**：[phases/channel-d1.md](phases/channel-d1.md)（分三子里程碑，禁止 Ink `mid_y` 特判）。

| 子里程碑 | 摘要 |
|----------|------|
| **D1.0** | 层间走廊 TrackOrder + 最小 DemandBoard；修关 `auto_edge_grouping` 后的假 bus；恢复消费 `edge_gap` |
| **D1.1** | 顶层 scope-only Substrate + 词典序搜索；完整 `RouteTopology::Orthogonal` |
| **D1.2** | Gate / ScopeMask / 有界 rip-up；`min_first/last_segment`、回边外侧走廊、端总线 `BundlePlan` 升格 |
| **D1.3** | Corridor Allocator：span 亲和 → 内层走廊优先 → RouteOrder+多轮 rip-up → Corridor Demand → 回边侧别统一代价。可执行方案见 [phases/channel-corridor-allocator.md](phases/channel-corridor-allocator.md)。**D1.3.1–D1.3.5 已交付** |

**做什么（概要）**：

- Substrate + Channel 搜索（折点优先于长度）——自 D1.1
- track order + DemandBoard（MetricBudget）——自 D1.0
- 有界 rip-up / history cost；InkVerifier（不穿节点、非 bundle 不非法重合）——D1.2
- 恢复并真正消费 `edge_gap` 等 track 相关参数——`edge_gap` 在 D1.0；其余见 [edge-parameters.md](edge-parameters.md) §4 第四批

**何时选**：产品痛点是边挤、弯多、需要走廊分配，而不是组框合法性。

### D₂ · 组框写权 + Gate（架构 / 跨组）

**方向**：组框由 Metric 写出，finalize 不重算；跨组边只经合法 Gate。

**做什么（概要）**：

- 组框进 VPSC（含 / 分隔 / title demand）
- Gate / Scope 三道防线；`verify_no_group_penetration`
- Weak 连续块与目标 Plan schema 对齐；为 StrongMacro 留同一产出类型

**何时选**：产品痛点是组框位置、跨组穿框、架构分层子系统。

---

## 6. 阶段 E — 后置能力

**方向**：图种差异只经 profile 进参数；不新增第二套布局器。

| 项 | 方向摘要 |
|----|----------|
| **StrongMacro** | 组树后序局部 Plan → macro 进父层 → 展开为与 Weak **同一 Plan schema** |
| **PartitionGrid** | 引擎消费 cell / band；与 Orientation 轴语义一致；非 `group` 冒充泳道 |
| **Bundle / auto_edge_grouping** | Compose 写合流事实；Ink 只接合干线 |
| **Integrated labeling** | 至少 label 需求进 Demand；完整联合求解可渐进 |
| **真 MCF / from-sketch / octilinear** | 明确不挡主路径闭环 |

---

## 7. 原则（各阶段共用）

1. **单写者 / 落笔零新决策** — 修楼梯先问 Metric，不在 Ink 加特判。  
2. **有界返工优先于真全局优化** — Channel 用 rip-up，不上真 MCF 挡闭环。  
3. **禁止图名特判** — architecture = Hierarchical + profile 参数，不是 `ArchitectureLayout`。  
4. **参数能 bind 就必须被消费** — 否则 bind 硬失败（见 MVP 对 `edge_gap` 等的处理）。  
5. **Weak / Strong 不得永久两套产出类型** — 收缩策略不同，Plan schema 必须同一。

---

## 8. 与现有文档的关系

| 文档 | 角色 |
|------|------|
| **本文** | MVP 之后的**阶段路线与方向** |
| [architecture.md](architecture.md) | 目标架构与 M0–M5 设计里程碑 |
| [notes/2026-08-02-mvp-scope.md](notes/2026-08-02-mvp-scope.md) | MVP 实现相对架构的取舍记录 |
| [scope.md](scope.md) | 能力 / 非目标 / 典型域 |
| [edge-parameters.md](edge-parameters.md) | 边参数支持研究（对照 yFiles Edges 分组）+ 分批实施路线 |
| [phases/](phases/) | 相级可执行契约 |
| [phases/channel-d1.md](phases/channel-d1.md) | D₁ Channel 分阶段契约（D1.0–D1.2） |

代码入口（重建）：`crates/plotgram-layout/src/layout/hierarchical/`。

---

## 9. 改善性重构 backlog（轻量）

**现状**：硬化判定经具名谓词 [`hardenable_real_pair`](../../../crates/plotgram-layout/src/layout/hierarchical/metric/cross_axis.rs)（`build_constraints` 只调用谓词）。**凡 fan `deg≥2`（奇偶皆然）松开**，由 pass-2 `fan_centers` 居中；链拖拽守卫仍排除 dummy-aligned 成员。

**做什么**：

- 谓词表驱动测试钉死判定矩阵：virtual-virtual 恒硬化 / 1:1 real 链 / 凡 fan≥2 松开 / dummy 相邻 real（链拖拽守卫）
- **不做**完整版上提（显式决策步产出成员表）——那是第三个守卫触发时的熔断动作，见阶段 A 硬共线条目

**验收**：行为与现行规则一致；谓词测试绿。

**何时做**：已落地（flat 夯实 M1）。
