# 布局与路由优化方案（架构图 / 流程图）

> 日期：2026-07-09  
> 状态：Iteration 1–3 已落地  
> 前置文档：[layout-routing-pipeline-full-analysis.md](./layout-routing-pipeline-full-analysis.md)  
> 原则：**性能可控、架构合理、不过度复杂**；不推翻现有 Sugiyama-v2 / architecture_v2 / orthogonal 管线。

---

## 0. 一句话结论

当前管线骨架是对的（分层布局 → L3 节点对齐 → L1 组框 → 正交路由 → 幂等恢复），**不需要重写引擎**。  
真正拉开与 D2 / Graphviz / ELK 观感差距的，是三件事：

1. **架构图 group 缺少「同级条带」审美约束**（默认 Fit + Start，尺寸与对齐漂移）  
2. **布局与路由之间的空间预算仍偏被动**（EGB/PRS/corridor 已有，但 sizing 与通道预算未统一）  
3. **流程图在回环 / 长边 / 自环场景下 rank 与坐标仍会塌缩**（stress-dag 可复现）

下面方案按「收益 / 复杂度 / 性能」排序，优先做 **P0–P1**，刻意避开 Network Simplex 坐标分配、全局 ILP、全图 A* 等重方案。

---

## 1. 现状诊断（含实验）

### 1.1 架构图：group 大小不一、对称性差

对 `showcase/architecture/*.svg` 的 group 框量化（同 y 行宽高比）：

| 样例 | 现象 |
|------|------|
| `c.layout-stress-nested` | 同容器内 subnet：`public=322` / `private=544` / `data=512`；左缘对齐但右缘参差 |
| `c.ecommerce-platform` | 层宽跨度 `296 → 1058`（约 3.6×） |
| `c.k8s-multi-namespace-overview` | 同行 `platform_ns` vs `data_ns` 宽比 ≈ 1.62 |
| `c.payment-clearing-platform` | 同行高比 ≈ 1.90 |
| `c.ai-agent-docops-pipeline`（已写 `track: equal`） | 宽比 ≈ 1.0，观感明显更整齐 |

**根因（按优先级）**：

1. 默认 `GroupFrameSpec` = `Fit + Start`（`group_frame/mod.rs` `resolve_architecture`）  
2. 组尺寸由组内内容 bbox 驱动；节点数 / `layout:` hint 不同 → 框天然不等  
3. `apply_equal_sibling_dimensions_per_rank` 只在**嵌套容器内、同 macro rank** 生效；顶层默认不做  
4. Equal 采用「左锚向右生长」：`x` 不变、`width += extra`，配合 `cross: start` → 右缘锯齿  
5. `adaptive_group_gap` 按跨组边数放大间距，破坏均匀节奏  
6. EGB / PRS / V2 / refine 事后扩壳或推节点，进一步打破对称  

这不是单纯 bug，而是 **「内容贴合」与「条带审美」目标冲突**；需要默认策略偏向后者，同时保留 Fit 逃生舱。

### 1.2 对照实验：盲目 Equal ≠ 更好

对 `c.layout-stress-nested` 三种配置重渲染：

| 配置 | 画布 | 关键 group 宽 | lint warnings | 观感 |
|------|------|---------------|---------------|------|
| 默认 Fit | 845×1714 | public 322 / private 544 / data 512 | 6 | 紧凑，但 subnet 右缘不齐 |
| `group_frame { track: equal, cross: center }` | **1502×1748** | external 被拉到 968；subnet 仍不完全齐 | 5 | 画布膨胀，顶层与嵌套 sibling 语义混淆 |
| `group_sizing: uniform` | 1070×1786 | external/cloud 同宽 816；subnet ≈656/704/656 | **3** | **最均衡**：条带感强、lint 更好 |

**实验结论**：

- 需要的是 **「同级 sibling 条带化」**，不是「全图所有 group 强行等宽」。  
- 顶层 `uniform`（同级拉齐）比全局 `track: equal` 更稳。  
- Equal 必须配合 **居中扩展** 与 **固定 gap**，否则只是把空白堆到一侧。

### 1.3 流程图：主干尚可，压力图仍崩

| 样例 | 观察 |
|------|------|
| `c.aml-case-investigation` | 主链 cx≈306 对齐良好，lint 全绿 |
| `c.layout-stress-dag` | `n8/n9` 与 `start` 同层顶部；`end` 在右下；自环路径退化（几乎零长度）；8 个 crossing warning |
| `c.software-release` | 主干大致垂直，但 `commit/build/done` 层序与语义流不完全一致 |

说明：**日常流程图质量已可用**；优化重点应放在回环、长边、自环、sink 约束，而不是重做整个 Sugiyama。

### 1.4 路由质量（架构图仍是短板）

实测 lint：

- `c.cloud-native`：`edge_through_node` + `edge_crosses_group_interior`  
- `c.k8s-multi-cluster-federation`：4 error（穿组）+ 43 warning  
- `c.ecommerce-platform`：3 error（穿组）  

corridor / X-1 / X-3 / PRS 已存在，但 **穿组硬约束与通道预算仍不够**，大图仍会退化到「绕外圈 / 穿无关组」。

### 1.5 性能基线（可控）

showcase 量级路由多在 **3–20ms**；大图（如 tenant-isolation）可达数百 ms。热点仍是：

1. 正交候选生成（staircase / 多档 margin）  
2. X-1 重路由轮次  
3. Friendliness V2（默认 adjust）  
4. refine 推节点后的增量重路由  

优化方案必须带 **复杂度上限与早停**，避免为美观引入超线性爆炸。

---

## 2. 设计原则

| 原则 | 含义 |
|------|------|
| **保留管线，收敛职责** | 不新增第四套 sizing 语言；把 two_phase sizing 收敛进 L1 Group Frame |
| **审美默认，内容可选** | 架构图默认偏向「同级条带」；Fit 作为显式选项 |
| **同级约束，跨级不强制** | 只对同一 parent 下的 sibling set 做等宽/等高；跨 rank 用「行居中 + 共享边」代替强行等宽 |
| **空间先预算，路由后修补** | 组间 gap / side gutter 在布局期按边负载预留；PRS 只做小幅补扩 |
| **性能有预算** | 每阶段有候选上限、轮次上限、节点数阈值；大图自动降级 |
| **确定性** | 继续遵守 `AGENTS.md` §2：显式排序，不用 HashMap 迭代序 |

参考（借鉴思想，不照搬实现）：

- **Graphviz / ELK**：分层 + 通道预算；compound 图对 sibling 做对齐  
- **D2**：容器优先、嵌套 group 视觉节奏强（等宽条带感）  
- **Brandes–Köpf / dagre**：spine 对齐 + compaction（流程图已有雏形，需加强）  
- **Orthogonal edge routing 文献**（Wybrow et al. / Nöllenburg）：port + channel + lane，对应现有 slot/corridor/lane，应强化而非替换  

---

## 3. 目标架构（轻量演进）

```
┌──────────────────────────────────────────────────────────────┐
│ Layout Strategy（architecture_v2 / flowchart+sugiyama_v2）   │
│  - 产出节点坐标 + 初始 group bounds + LayoutHints            │
│  - 不再各自维护第二套「最终 sizing」语义                      │
└────────────────────────────┬─────────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────────┐
│ L3 Node Frame（grid_snap）                                   │
└────────────────────────────┬─────────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────────┐
│ L1 Group Frame（唯一权威）                                   │
│  RankBand 模型：同级 sibling → 等宽/可选等高 + 居中扩展      │
│  + 固定/半固定 gap + SharedLines + 行居中                    │
│  + EdgeGutterBudget（把 EGB 收编为 Frame 的通道预算）        │
└────────────────────────────┬─────────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────────┐
│ Pre-route（friendliness：大图默认 diagnose）                 │
└────────────────────────────┬─────────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────────┐
│ Orthogonal Router                                            │
│  corridor 优先 → 候选（有上限）→ X-1/X-2/X-3 → labels        │
└────────────────────────────┬─────────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────────┐
│ Post-route：L1 幂等恢复 → 小幅 PRS → 增量重路由 → 末尾 snap  │
└──────────────────────────────────────────────────────────────┘
```

**与现状的关键差异**：L1 成为 sizing/对齐的唯一出口；two_phase 只负责「拓扑位置初值」。

---

## 4. 优化方案（按优先级）

### P0-A 架构图 RankBand：同级条带化（核心美观项）

**问题**：默认 Fit 导致 sibling 框大小不一；盲目全局 Equal 又会撑破画布。

**方案**：引入 **RankBand**（概念层，实现可落在现有 `apply_group_frame` / `apply_equal_sibling_dimensions_per_rank`）：

1. **作用域**：每个 sibling set（同 parent）内，再按 macro rank 分 band。  
2. **默认策略（architecture）**：
   - `track_sizing = Equal`（仅 sibling band 内等宽）  
   - `equalize_height = true`（同 band 等高；跨 band 不强制）  
   - `cross_align = Center`（行居中，替代纯 Start）  
   - `gap = Fixed(48)`（关闭按边数自适应的水平 gap；垂直层距仍可轻度密度感知）  
   - `border = SharedLines`  
3. **扩展方式改为居中**：`x -= extra/2; width += extra`（高同理），避免左锚右伸。  
4. **跨 rank 不强制等宽**：只做「画布/父容器中线对齐」+ SharedLines；避免 public(2 节点) 被 data(4 节点) 无脑拉宽到难看（历史决策保留，但用居中缓解视觉不平衡）。  
5. **DSL**：
   - 新默认等价于「智能 uniform」  
   - `group_frame: stack { track: fit }` 或 `group_sizing: fit` 可退回旧行为  

**涉及文件**：

- `group_frame/mod.rs`（`resolve_architecture` 默认、`apply_equal_sizing` 居中扩展、可选等高）  
- `architecture_v2/group_sizing.rs` / `two_phase.rs`（删除或降级为「初值 hint」，最终以 L1 为准）  
- `architecture_v2/post_layout.rs`（多顶层 group 也做行居中，不限单 group）  

**性能**：O(G log G)，可忽略。  

**验收**：

- `c.layout-stress-nested`：三个 subnet 同宽（容差 ≤ 8px），左/右缘共线或对称内缩  
- `c.cloud-native`：ingress / observability / data 顶层条带观感接近 D2  
- 画布面积相对 Fit 增长 **≤ 35%**（用 uniform 实验的 ~27% 作参考上限）  
- lint error（穿组/穿节点）不回归恶化  

---

### P0-B 统一 sizing 入口，消灭双通道

**问题**：two_phase 在定位前改 MacroBlock 尺寸，L1 在定位后再改 GroupLayout；语义重复且易打架。

**方案**：

1. `two_phase` 只输出 **content-fit 初值** + macro rank / sibling 元数据。  
2. 所有 Equal / Uniform / SharedLines / quantize **只在 `GroupFramePass` 执行**。  
3. 废弃或内部化 `group_sizing: uniform` → 映射为 `track: equal`（对外可保留别名一期）。  
4. 路由后 `restore_after_node_moves` 继续幂等调用同一套 L1（已有）。  

**复杂度**：中等重构，但删代码多于加代码。  
**性能**：减少重复拉齐次数（目标：主路径 L1 ≤ 2 次）。  

---

### P0-C 组间通道预算前移（布局期，而不是只靠 PRS）

**问题**：跨组边仍大量穿组 / 贴边假母线；PRS 事后扩壳治标。

**方案**（在已有 EGB + corridor 上做「预算闭环」）：

1. 宏观定位时：`gap = base_gap + lane_budget(k)`，其中 `k` 为相邻 sibling 对的跨组边数；**同一 band 内 gap 取该 band 最大值**（保证等间距节奏）。  
2. side gutter：继续用 LCA 路径累积，但 **左右对称优先**（`max(left,right)` 再分配），避免「左侧挤、右侧空」。  
3. 路由：corridor 失败才回退通用候选；穿无关组默认硬否决（已有 `path_avoids_group_interiors`），仅当无干净候选时降级。  
4. PRS：`PRS_MAX_PER_SIDE` 保持 48，但若 L1 已预留足够 gutter，PRS 触发率应下降。  

**涉及**：`two_phase.rs`（gap）、`edge_gutter.rs`、`corridor_route.rs`、`scoring.rs`。  
**性能**：O(E + G²) 预计算，showcase 可忽略；大图 G 通常 ≪ V。  

**验收**：`c.cloud-native` / `c.k8s-multi-cluster-federation` 的 `edge_crosses_group_interior` error 显著下降。  

---

### P1-A 流程图：强化 sink / spine / 自环（不重做引擎）

**问题**：`c.layout-stress-dag` 中结束节点、回环层序、自环几何仍差。

**方案**（均有现成钩子）：

1. **Sink 约束加硬**：`type=end` / 出度 0 且非自环 → `rank = max_rank`（已有 `apply_sink_rank_constraints`，检查是否被 NS 后续破坏；若有，在 NS 后再次 clamp）。  
2. **Spine 权重**：`compute_spine_nodes` 已存在；提高 spine 边在 BK 对齐与 compaction 中的优先级（阻尼向 spine 靠拢）。  
3. **回环侧均衡**：沿用 `feedback_side`；确保 flowchart profile 默认启用，并与 layer_order 协同，避免全部挤右侧。  
4. **自环专用几何**：orthogonal 下自环走固定「侧方小矩形 / 半环折线」，禁止退化成近零长度 path（stress-dag 的 n9/n4 自环可复现）。  
5. **Compaction 轮次**：`compact_layer_centers` 从 2 提到 3，或按层密度自适应；保持阻尼 0.35，避免抖动。  

**不做**：完整 Graphviz Network Simplex 坐标分配（收益不确定、实现重）。  

**验收**：stress-dag 中 `end` 在最底层；自环可见且不穿节点；crossing 不显著增加。  

---

### P1-B 正交路由性能闸门（美观不牺牲速度）

在 `path.rs` / `mod.rs` 加显式预算：

| 闸门 | 建议 |
|------|------|
| 每边候选上限 | `MAX_CANDIDATES = 48`（超出按 score 截断） |
| staircase | 仅当 Level0/1 无 `best_strict` 时启用；`MAX_PER_AXIS` 保持 6 |
| X-1 轮次 | 默认 2；仅当违规边 > 阈值升到 3 |
| Friendliness | `\|V\| > 40` 或 `\|E\| > 60` 时默认 `diagnose`（不 adjust） |
| refine | 无穿障早退已有；有穿障 `max_passes=2` |

目标：tenant-isolation 级图路由 **P95 不增**；小图质量不降。  

---

### P1-C 对称性度量进入 lint / eval（防止回退）

新增软指标（Warning，不进 CI strict）：

- `sibling_width_ratio`：同 parent、同 band 的 max/min width  
- `sibling_height_ratio`  
- `band_gap_variance`：同 band 相邻 gap 方差  
- `group_center_offset`：单 group 行相对父容器中线的偏移  

showcase eval 跟踪这些指标，避免「为修边又弄歪框」。  

---

### P2（可选，排期靠后）

| 项 | 说明 | 为何靠后 |
|----|------|----------|
| 流程图 group_divide 与 architecture 共享 RankBand | 泳道图也能等宽 | 流程图 group 场景较少 |
| 标签候选打分全面替换推开 | 旧提案 P0-1 | 对 arch/flow 主诉不如框与穿组紧急 |
| ER/state 专项 | 非本次范围 | 用户明确主攻 arch/flow |

---

## 5. 明确不做什么（控制复杂度）

1. **不重写** architecture_v2 / sugiyama_v2 / orthogonal 主算法。  
2. **不引入** 全局力导向二次布局、ILP 交叉最小化、全图 visibility A* 作为默认路径。  
3. **不做**「所有 group 全图等宽」——实验证明会膨胀画布并混淆层级。  
4. **不为消 warning 而堆逻辑**（遵守 `AGENTS.md` §4）：以 ink、对称、穿组硬错误为准。  
5. **不把 V2 adjust 作为美观主手段**——它会破坏对称；大图应降级。  

---

## 6. 实施顺序（建议 3 个迭代）

### Iteration 1（约 3–5 天）— 对称性立刻可见 ✅ 已落地（2026-07-09）

1. RankBand 默认：sibling equal 宽高 + 居中扩展 + 行居中  
2. 固定 band 内 gap  
3. 收敛 `group_sizing` → L1（默认 `Uniform`/`Equal`，显式 `fit` 可退回）  
4. 回归：stress-nested / cloud-native / three-tier / data-pipeline  

**实测摘要**：
- `c.layout-stress-nested`：顶层 external/cloud 同宽 816；subnet 宽比 1.07；lint warning 6→3；面积 +32%
- `c.cloud-native`：顶层条带化；`edge_crosses_group_interior` error 2→0
- `n.microservices`：frontend/backend 同宽 448  

### Iteration 2（约 3–5 天）— 跨组路由 ✅ 已落地（2026-07-09）

1. band 级统一 lane_budget gap（`band_uniform_gap` = 同行相邻 pair 的 max）  
2. 对称 side gutter（`max(left,right)` / `max(top,bottom)`）  
3. corridor 优先 + 穿组约束收紧：  
   - `GROUP_TRANSIT_PENALTY` 3000→8000；architecture `obstacle`/`corridor_misalignment` 权重上调  
   - `should_strict_group_transit`：有 corridor chain 时拒绝 nodes-only 穿组；dirty 仅在不穿无关组时可用  
4. 回归：k8s-federation / ecommerce / hybrid-cloud-dr  

**实测摘要**（相对 I2 改动前 baseline）：
- `c.k8s-multi-cluster-federation`：error 16→10；穿组 5→4；穿节点 9→6；score 52→59；面积 −25%
- `c.ecommerce-platform`：error 持平 3；穿节点 2→1；crossing 21→12；score 62→65
- `c.hybrid-cloud-dr-topology`：error 2→1；穿节点 2→0；crossing 29→17；score 54→57（穿组 0→1）
- `c.cloud-native` / `c.layout-stress-nested`：I1 成果保持（穿组 error 仍为 0）
- **已知代价**：`c.k8s-multi-namespace-overview` error 上升（通道预算 + strict dirty 过滤在稠密多 namespace 上仍会退化）；留给后续 corridor 覆盖率 / 局部放宽

### Iteration 3（约 2–4 天）— 流程图 + 性能闸门 ✅ 已落地（2026-07-09）

1. sink clamp / spine / 自环几何  
   - group rank 后再 clamp `type=end` / 出度 0  
   - `compact_layer_centers` 2→3；spine 邻居权重 1.5  
   - 正交自环最小段长 8px，避免小节点退化  
2. 候选与 X-1 / friendliness 闸门  
   - `MAX_CANDIDATES=48`；X-1 默认 2 轮，冲突边 >8 升到 3  
   - 大图（|V|>40 或 |E|>60）friendliness 默认 `Diagnose`  
   - refine `max_passes` 3→2（无穿障早退不变）  
3. lint 对称性指标：`SiblingWidthRatio`（warning，阈值 1.08）  
4. 全 showcase eval 已更新 baseline  

**实测摘要**：
- `c.layout-stress-dag`：`end` 在最底层；n4/n9 自环路径长约 251px（非退化）；error 仍为 0；crossing 8→11（可接受代价）
- showcase check：绿；`sibling_width_ratio` 全库 1 条 warning（跟踪用）

---

## 7. 验收标准（量化）

| 维度 | 指标 | 目标 |
|------|------|------|
| 对称 | 同 band sibling 宽比 | ≤ 1.08（8px 量化容差内视为 1.0） |
| 对称 | 同 band 高比 | ≤ 1.12 |
| 画布 | 相对当前 Fit 面积 | 中位增长 ≤ 30%，P95 ≤ 40% |
| 正确性 | `edge_crosses_group_interior` / `edge_through_node` | showcase architecture error 数下降 ≥ 50% |
| 流程图 | stress-dag：end 在 max rank；自环非退化 | 必须 |
| 性能 | bench-phases 代表图 | route 时间不增超过 15% |
| 确定性 | 同输入两次渲染 | 坐标完全一致 |

---

## 8. 风险与回滚

| 风险 | 缓解 |
|------|------|
| Equal 导致小 group 空洞过多 | 仅 sibling band；跨 rank 不拉齐；提供 `track: fit` |
| 行居中改变用户习惯的左对齐 | DSL `cross: start` 可恢复；文档说明默认变更 |
| 收紧穿组导致更多绕行、画布变宽 | lane_budget 前移；接受「略宽但干净」优于「窄但穿组」 |
| 双通道残留导致行为难测 | Iteration 1 必须删掉 two_phase 最终 sizing |

回滚开关（建议环境变量 / DSL）：

- `PLOTGRAM_GROUP_FRAME_LEGACY_FIT=1` → 恢复 Fit+Start  
- 单图 `group_frame: stack { track: fit, cross: start }`  

---

## 9. 附录：实验原始数据摘要

### 9.1 `c.layout-stress-nested` Fit vs Equal vs Uniform

```
Fit:     canvas 844.5×1714
  public_subnet  322×368
  private_subnet 544×368
  data_subnet    512×344
  lint warnings: 6

Equal(track+center): canvas 1502×1748  ← 膨胀过大
  external       968×296  ← 被错误拉宽
  lint warnings: 5

Uniform: canvas 1070×1786
  external/cloud 同宽 816
  subnet ≈ 656 / 704 / 656
  lint warnings: 3  ← 最好
```

### 9.2 同行宽比偏高的架构图（现状）

`k8s-multi-cluster-federation`、`k8s-multi-namespace-overview`、`payment-clearing-platform`、`plotgram-core-mod-deps`、`n.d2-cell-tower-network`、`n.event-driven`。

### 9.3 关键代码锚点

| 主题 | 路径 |
|------|------|
| 架构默认 Frame | `layout/group_frame/mod.rs` `resolve_architecture` |
| 嵌套等宽等高 | `layout/node/architecture_v2/group_sizing.rs` |
| 宏观定位 / 自适应 gap | `layout/node/architecture_v2/two_phase.rs` |
| 管线时序 | `layout/pipeline.rs` |
| 走廊路由 | `layout/edge/edge_routing_orthogonal/corridor_route.rs` |
| 流程图 sink / spine | `sugiyama_v2/engine.rs`、`coordinate.rs` |
| 性能日志 | `plotgram lint` / `bench-phases` |

---

## 10. 总结

最优路径不是「更复杂的算法」，而是：

1. **把审美约束收成 RankBand（同级条带）**，用居中扩展 + 固定 gap 解决 group 大小与对称问题；  
2. **让 L1 Group Frame 成为唯一 sizing 权威**，去掉 two_phase 双通道；  
3. **把跨组通道预算前移**，让路由少做事后补丁；  
4. **流程图只补 sink/spine/自环与性能闸门**，保持 Sugiyama-v2 主体。  

这与 Graphviz/ELK/D2 的工程经验一致：**分层骨架 + 容器对齐 + 通道预算**，而不是上更重的全局优化器。
