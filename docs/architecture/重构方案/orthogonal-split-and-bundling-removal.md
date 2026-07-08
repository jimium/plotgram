# Orthogonal 分型重构与 Bundling 退役方案

> 版本：1.0  
> 日期：2026-07-09  
> 状态：**Phase 1–4 已实施**（持续优化见 Phase 4 后续子项）  
> 范围：`edge_routing_orthogonal/`、`edge_bundling/`、`pipeline.rs`、`edge_merge_policy.rs`、标签流水线  
> 验证用例：`showcase/architecture/c.layout-stress-nested.pgm`、showcase flowchart 多 fan-out 样例、`plotgram-eval` showcase baseline

---

## 一、背景与动机

### 1.1 问题来源

在 `c.layout-stress-nested.pgm`（`bundling: 1.0`）上观测到两类退化：

1. **边线过度拥挤**：多条跨子网边被合并到 `cloud` 左缘窄走廊（x ≈ 148–184），平行间距仅满足 `ORTHO_PARALLEL_GAP = 8px` 下限。
2. **标签布局异常**：边标签被推到 group 外缘或组间缝隙（已通过「group 边框壳层」修复一部分）；长边标签（如 Webhook）仍挤在狭窄区域。

根因分析表明：**主要矛盾不在节点布局，而在 orthogonal 后置 bundling 与 architecture 场景语义冲突**，而非 organic / bezier 等曲线路由。

### 1.2 战略判断

| 判断 | 依据 |
|------|------|
| flowchart / architecture **必须继续以 orthogonal 为主路径** | 产品默认、`orthogonal-routing-investment-plan.md` 已投入 P0–P2 全阶段 |
| **不宜分裂 organic** | organic 为 mindmap 曲线路由；arch/flowchart 用户预期为正交折线 |
| **宜分裂 orthogonal 内部策略** | 两图种需求差异大，但可共享 path/scoring/grid 等内核 |
| **宜退役 edge_bundling 模块** | 默认关闭、仅 1 个 showcase 启用；与 lane assignment / P1-1 trunk+fork 职责重叠；引入路径二次重写与标签特例 |

### 1.3 本方案目标

1. 将 `edge_routing_orthogonal` 重构为 **共享内核 + FlowchartProfile / ArchitectureProfile** 两套预设。
2. **移除 `edge_bundling/` 模块**及 pipeline 后置链，统一标签流水线。
3. 对外 DSL **保持** `edge_routing: orthogonal` 不变；图种差异由内部分型自动选择。
4. 可度量验收：stress-nested、eval baseline、flowchart fan-out 回归。

---

## 二、现状审计

### 2.1 代码规模（约 2026-07）

| 模块 | 行数（约） | 职责 |
|------|-----------|------|
| `edge_routing_orthogonal/mod.rs` | 2,626 | 主流程编排、端口/slot/路由/后处理 |
| `path.rs` | 1,103 | 候选生成、走廊绕行、P1-1 trunk+fork |
| `scoring.rs` | 1,057 | 路径打分、障碍/重合惩罚 |
| `lane_assignment.rs` | 1,001 | X-3 车道分配、无关 trunk 分离 |
| `corridor_route.rs` | 828 | 组间走廊规划（architecture 为主） |
| `edge_bundling/` 合计 | ~3,700 | 聚类、trunk 定位、path 重写、标签后置 |
| **合计 orthogonal + bundling** | ~13,500 | |

### 2.2 当前流水线（`pipeline.rs` + `orthogonal/mod.rs`）

```text
节点布局（flowchart → Sugiyama-v2 / architecture → architecture-v2）
  ↓
orthogonal 路由（单入口 OrthogonalRouting::route）
  ├── 端口选择 + slot 分配
  ├── 逐边寻路 + replan_slots + reroute
  ├── lane assignment
  ├── [architecture] corridor offsets + separate_unrelated_trunk_overlaps
  └── [bundling 关] resolve_label_overlaps
  ↓
[可选] apply_edge_bundling（path 全文重写）
  ↓
[可选] relayout_edge_labels_after_bundling（SegmentAware）
  ↓
[architecture + bundling] separate_unrelated_architecture_trunks_after_bundling
  ↓
snap_and_repulse_edges
```

### 2.3 已存在的「隐性分型」

代码中已有按 `DiagramType` 分支的逻辑，但未抽象为 Profile：

| 机制 | Flowchart | Architecture |
|------|-----------|--------------|
| `requires_semantic_merge` | false | true |
| `edges_may_share_trunk` | 几何优先，宽松 | 仅同源 fan-out / 同宿 fan-in / 平行对 |
| `plan_corridor_routes` | 基础走廊 | 组间走廊 + merge-aware lane |
| `apply_corridor_planned_offsets` | 不执行 | 执行 |
| `separate_unrelated_trunk_overlaps` | 不执行（routing 内） | 执行 |
| `semantic_gate`（bundling） | false | true（显式 bundling 时） |
| bundling 默认 | off | off |

**结论**：分型需求真实存在，当前以 scattered `if diagram_type == Architecture` 实现，可维护性差，且 bundling 层叠加后产生二次共线与标签特例。

### 2.4 Bundling 实际使用情况

- DSL 默认：`bundling: 0`（`ORTHOGONAL_OPTIONS[2].default`）
- showcase 全库：**仅** `c.layout-stress-nested.pgm` 设置 `bundling: 1.0`
- `layout-routing-improvement-proposal.md` A5：**architecture 默认不 bundling**（已实现）
- 对比实验：`bundling: 0` 时 stress-nested 左廊 x 从 ~164 扩到 ~215–251，标签更靠近边路径

---

## 三、目标架构

### 3.1 设计原则

沿用 `slot-post-routing-proposal.md` 的分层分治 + 确定性迭代（`AGENTS.md` §2）：

1. **单一路径原则**：边几何在 orthogonal `route()` 内一次定型，禁止后置 path 全文重写。
2. **Profile 显式化**：图种差异收敛到 `OrthoRoutingProfile`，禁止在子模块内新增裸 `DiagramType` 分支。
3. **合并语义单一来源**：`edge_merge_policy` 为 trunk 共享判定的唯一模块；bundling 删除后仍保留。
4. **标签流水线统一**：始终走 `resolve_label_overlaps`（路由内或 pipeline 末尾一次），删除 SegmentAware / trunk keepout 特例。
5. **对外零破坏**：`edge_routing: orthogonal` 名称与 option（`slot_pitch`、`channel_margin`）保留；`bundling` option 标记废弃后删除。

### 3.2 模块结构（目标）

```text
crates/plotgram-core/src/layout/edge/
  edge_routing_orthogonal/
    mod.rs                 # 门面：解析 DiagramType → Profile，编排流水线
    profile.rs             # OrthoRoutingProfile trait + 两套实现
    profile_flowchart.rs   # FlowchartProfile
    profile_architecture.rs# ArchitectureProfile
    context.rs             # 共享（现有）
    path.rs                # 共享
    scoring.rs             # 共享；惩罚权重可由 Profile 注入
    slot.rs                # 共享；DockingStrategy 默认值可 per-profile
    corridor_route.rs      # architecture profile 强依赖；flowchart 弱依赖
    lane_assignment.rs     # 共享
    channel_load.rs        # 共享
    feedback_side.rs       # 共享
    simplify.rs            # 共享
    layer_order.rs         # 共享
    orthogonal_tests.rs    # 按 profile 分测试模块

  edge_merge_policy.rs     # 保留，作为合并语义 SSOT

  [删除] edge_bundling/    # 整目录移除
```

### 3.3 `OrthoRoutingProfile` 接口（草案）

```rust
/// 图种相关的正交路由策略预设（不可变配置 + 阶段开关）。
pub struct OrthoRoutingProfile {
    pub diagram_type: DiagramType,
    /// 平行边最小间距（默认 8，architecture 可提高到 12–16）
    pub parallel_gap: f64,
    /// 侧通道留白（默认 18）
    pub channel_margin: f64,
    /// 是否启用走廊规划后 lane 偏移
    pub corridor_lane_offsets: bool,
    /// 是否在 lane 阶段分离无关 trunk
    pub separate_unrelated_trunks: bool,
    /// trunk 共享是否走语义门控（architecture = true）
    pub semantic_merge: bool,
    /// 打分权重倍率（path_length、bend、group_transit…）
    pub scoring: ScoringWeights,
    /// 是否偏好 P1-1 trunk+fork 路径形态（flowchart fan-out）
    pub prefer_trunk_fork: bool,
    /// slot 默认 docking 策略
    pub default_docking: DockingStrategy,
}
```

**选择逻辑**（`mod.rs`）：

```text
DiagramType::Architecture  → ArchitectureProfile::default()
DiagramType::Flowchart     → FlowchartProfile::default()
DiagramType::Custom(_)     → FlowchartProfile::default()  // 继承 flowchart
DiagramType::State | Er    → 现有共用 orthogonal（本方案不专门分裂，保持默认）
```

### 3.4 Flowchart vs Architecture Profile 差异表

| 维度 | FlowchartProfile | ArchitectureProfile |
|------|------------------|---------------------|
| **核心诉求** | 同源 fan-out 整齐、决策流可读 | 跨 group 不穿障、子网间隙通道、语义正确 |
| **parallel_gap** | 8px（或 10） | 12–16px（减少「贴边挤线」） |
| **semantic_merge** | false | true |
| **corridor_lane_offsets** | false / 轻量 | true |
| **separate_unrelated_trunks** | false（lane 通用分离足够） | true |
| **prefer_trunk_fork** | **true**（替代 bundling 视觉束） | false（避免无关边共线） |
| **group 通道权重** | 低 | 高（`corridor_misalignment_penalty` 加大） |
| **channel_load 惩罚** | 标准 | 更激进（拥堵通道优先 reroute） |
| **标签** | 路由内避让 | 路由内避让；长边可后续加 whitespace bonus |

---

## 四、Bundling 退役与能力迁移

### 4.1 Bundling 现有能力映射

| Bundling 能力 | 退役后替代 | 说明 |
|---------------|-----------|------|
| 同源 fan-out 共享主干 | **P1-1 trunk+fork**（`path.rs`，路由阶段） | 不 rewrite 全文路径；仅对兼容边对生成 trunk 形态 |
| 平行边 ink 节省 | **lane assignment** + 可选增大 `parallel_gap` | 分离共线而非强制合并 |
| 无关边共线 | **`separate_unrelated_trunk_overlaps`** | 已在 orthogonal 内，architecture 必开 |
| 标签锚定独占段 | **删除**；统一 `resolve_label_overlaps` | 已修复 group 边框壳层；长标签另立任务 |
| SVG trunk 加粗/半透明 | **删除**或改为纯渲染启发式（可选 Phase 4） | 非布局正确性依赖 |
| `edge_bundling` hints / debug | 删除；指标迁入 `orthogonal_debug` | |

### 4.2 删除清单

**目录**

- `crates/plotgram-core/src/layout/edge/edge_bundling/`（7 文件）

**Pipeline / Plan**

- `pipeline.rs::apply_edge_bundling`
- `plan.rs::resolve_edge_bundling_config`、`LayoutPlan.edge_bundling`
- `orthogonal/mod.rs` 中 `cfg.bundling` 分支及 `endpoint_bundling_key` slot 分组（改为 merge_policy 或 profile 驱动）

**渲染**

- `render/paint/standard.rs` 中 `edge_bundling` hints 消费
- `render/paint/svg_utils.rs` bundle stroke 样式（若有）

**DSL / 文档**

- `edge_routing: orthogonal { bundling: … }` — 先 deprecated 警告，后删
- `language-spec.md`、`dsl-writing-manual.md` 相关段落

**测试**

- `layout/mod.rs` 中 bundling 端到端测试 → 改为 trunk+fork + lane 测试
- `render/encode/svg.rs` bundling stroke 测试 → 删除或改写

### 4.3 保留并强化的模块

| 模块 | 动作 |
|------|------|
| `edge_merge_policy.rs` | 保留；作为 Profile 的 SSOT |
| `lane_assignment.rs` | 保留；architecture 提高 `parallel_gap` |
| `corridor_route.rs` | 保留；绑定 ArchitectureProfile |
| `label_avoidance.rs` | 保留；group 边框壳层（已落地） |
| `path.rs` P1-1 | 强化；FlowchartProfile 默认开启 trunk+fork |

---

## 五、分阶段实施计划

### Phase 0：基线与文档（0.5d）

**目标**：冻结对比数据，避免重构过程无法回归。

| 任务 | 说明 |
|------|------|
| P0-1 | `c.layout-stress-nested.pgm` 增加 `bundling: 0` 基准分支（或改默认并记录 diff） |
| P0-2 | 跑 `plotgram-eval` showcase baseline，记录 `edge_parallel_overlap_count`、`unrelated_edge_trunk_merge`、lint |
| P0-3 | 导出 bundling on/off 对照 SVG 归档到 `showcase/.history` 或 eval 附件 |

**验收**：baseline JSON 有 bundling-off 快照；文档引用本方案。

---

### Phase 1：Bundling 退役（1–2d）

**目标**：功能上不再依赖 bundling，behavior 以 bundling-off 为准。

| 任务 | 文件 | 说明 |
|------|------|------|
| P1-1 | `pipeline.rs` | 移除 `apply_edge_bundling` 调用链 |
| P1-2 | `plan.rs` | 移除 `edge_bundling` 解析；`bundling` DSL 解析时发 diagnostic warning |
| P1-3 | `orthogonal/mod.rs` | 删除 `cfg.bundling` 分支；始终 `resolve_label_overlaps` |
| P1-4 | `showcase/.../c.layout-stress-nested.pgm` | `bundling: 0` |
| P1-5 | 测试 | 删除/改写 bundling 专测；保证 `cargo test -p plotgram-core` 绿 |
| P1-6 | `edge/mod.rs` | 移除 `edge_bundling` 模块声明 |

**暂不删** `edge_bundling/` 目录（标记 `#[deprecated]` 或 `dead_code` 隔离），降低单 PR 风险。

**验收**：

- [ ] 全库无 `apply_bundling` 运行时调用
- [ ] stress-nested SVG 与 bundling-off 对照一致
- [ ] flowchart 4 路 fan-out 测试图仍可读（trunk+fork 或 lane 分离）
- [ ] `edge_bundling` hints 恒为 `None`

---

### Phase 2：Profile 抽象（2–3d）

**目标**：将 scattered `DiagramType` 分支收拢为 `OrthoRoutingProfile`。

| 任务 | 文件 | 说明 |
|------|------|------|
| P2-1 | `profile.rs` | 定义 `OrthoRoutingProfile` + `for_diagram_type()` |
| P2-2 | `profile_flowchart.rs` | `prefer_trunk_fork: true`，`semantic_merge: false` |
| P2-3 | `profile_architecture.rs` | corridor + separate trunks + 更大 `parallel_gap` |
| P2-4 | `mod.rs` | `route()` 入口构造 `profile`，下传 `RoutingContext` |
| P2-5 | `corridor_route.rs`、`lane_assignment.rs` | 用 `profile` 替换裸 `DiagramType` 参数 |
| P2-6 | `scoring.rs` | `ScoringWeights` 从 profile 读取（可选倍率） |

**验收**：

- [ ] `orthogonal/mod.rs` 中 `DiagramType::Architecture` 字面匹配 ≤ 3 处（仅 `for_diagram_type`）
- [ ] flowchart / architecture 单测各 ≥ 2 个 profile 行为测试
- [ ] eval showcase 分数无显著退化（architecture stress-nested 视觉改善）

---

### Phase 3：物理删除 bundling + 清理（1d）

| 任务 | 说明 |
|------|------|
| P3-1 | 删除 `edge_bundling/` 目录 |
| P3-2 | 清理 `LayoutHints.edge_bundling`、渲染层 bundle 样式 |
| P3-3 | 更新 DSL spec / manual，移除 `bundling` option |
| P3-4 | 更新 `orthogonal-routing-comparative-analysis.md` 流水线图 |

**验收**：

- [x] `rg edge_bundling` 仅命中 changelog / 本方案文档
- [x] `cargo test -p plotgram-core` 全绿
- [x] `eval-showcase.sh check` 全绿

---

### Phase 4：图种专项优化（持续，与本重构解耦）

在 Profile 框架落地后，分项迭代：

| 子项 | 图种 | 说明 |
|------|------|------|
| F-1 | Architecture | 增大 `parallel_gap`、强化组间走廊（参见 `slot-post-routing-proposal.md`） |
| F-2 | Architecture | 长边标签 whitespace 奖励 / 端点 group 豁免（标签 Phase 2） |
| F-3 | Flowchart | trunk+fork 覆盖率高亮测试；fan-out ≥ 4 的 showcase |
| F-4 | 两者 | `ORTHO_PARALLEL_GAP` 是否提为 profile 级常量（默认 arch 12、flow 8） |

---

## 六、风险与缓解

| 风险 | 等级 | 缓解 |
|------|------|------|
| flowchart fan-out 失去「束状」美感 | 中 | FlowchartProfile 强制 P1-1 trunk+fork；补 showcase |
| architecture 平行边重合回升 | 中 | ArchitectureProfile 提高 `parallel_gap` + `separate_unrelated_trunks` |
| 大 PR 难以 review | 高 | 严格 Phase 1 / 2 / 3 分 PR |
| 删除 bundling 后 ink 指标下降 | 低 | 产品未对外承诺 ink；eval 以 crossing/overlap 为主 |
| State/Er 走 orthogonal 回归 | 低 | Profile 默认 fallback；专项测试保留 |
| 性能回退 | 低 | bundling 删除应略快；用 `plotgram-eval elapsed_us` 监控 |

---

## 七、验收指标

### 7.1 自动化

| 指标 | stress-nested（architecture） | 说明 |
|------|------------------------------|------|
| `edge_parallel_overlap_count` | ≤ 基线或改善 | eval metrics |
| `unrelated_edge_trunk_merge` | ≤ 1 | lint |
| `label_label_overlap` | 0 | lint |
| `edge_crossings` | 不回升超过 +2 | 分离干线可能略升 |
| 确定性 | 30 次 MD5 一致 | `orthogonal_tests` / eval |

### 7.2 目视（stress-nested）

- [ ] 左廊竖线不再全部叠在 x ≈ 164
- [ ] 底部/外侧「标签一排」不再出现
- [ ] Webhook 等长标签不挤在子网夹缝（Phase 4 可继续优化）

### 7.3 Flowchart 专项

- [ ] 4 路 `a→b` 平行边：共享 trunk 或清晰平行间距，无 exact overlap
- [ ] 决策分支图（playground 示例）端口整齐

---

## 八、与现有文档关系

| 文档 | 关系 |
|------|------|
| [orthogonal-routing-investment-plan.md](../../已经实现的方案/orthogonal-routing-investment-plan.md) | P0–P2 已完成；本方案是 **P3 架构层**延续 |
| [slot-post-routing-proposal.md](../布局优化/slot-post-routing-proposal.md) | 走廊/side/slot 改进仍有效；在 ArchitectureProfile 下实施 |
| [edge-separation-proposal.md](../布局优化/edge-separation-proposal.md) | X-3 lane / nudge；bundling 删除后更依赖此路径 |
| [layout-routing-improvement-proposal.md](../../layout-routing-improvement-proposal.md) | A5「architecture 默认不 bundling」→ 本方案彻底退役 |
| [orthogonal-routing-comparative-analysis.md](../布局优化/orthogonal-routing-comparative-analysis.md) | Phase 3 后需更新流水线图（去掉 bundling 支路） |

---

## 九、决策摘要

| 问题 | 决策 |
|------|------|
| 是否分裂 orthogonal？ | **是**，以 `OrthoRoutingProfile` 显式分型 |
| 是否分裂 organic？ | **否**，mindmap 专用 |
| 是否删除 bundling？ | **是**，分 Phase 1 停用 → Phase 3 删代码 |
| DSL 是否 breaking change？ | `bundling` option 废弃后删除；`orthogonal` 名称不变 |
| 优先实施顺序？ | Phase 0 → **Phase 1**（bundling 退役）→ Phase 2（Profile）→ Phase 3（删代码） |

---

## 十、任务看板（可勾选）

```text
Phase 0  基线
  [ ] stress-nested bundling-off baseline
  [ ] eval showcase snapshot

Phase 1  Bundling 退役
  [ ] pipeline 移除 apply_edge_bundling
  [ ] orthogonal 始终 resolve_label_overlaps
  [ ] showcase stress-nested bundling: 0
  [ ] 测试改写

Phase 2  Profile 抽象
  [ ] profile.rs + flowchart / architecture 预设
  [ ] mod.rs 收拢 DiagramType 分支
  [ ] corridor + lane 接 profile
  [ ] profile 单测

Phase 3  物理删除
  [x] 删 edge_bundling/
  [x] 清 DSL / 渲染 / hints
  [x] 更新流水线文档

Phase 4  专项优化（可选）
  [x] architecture parallel_gap ↑ (12px)
  [x] 长标签 whitespace + 端点 group 壳层豁免
  [x] flowchart trunk+fork 测试 + s.fan-out-four showcase
  [x] profile 级 parallel_gap 常量
```

---

*维护者：布局/边路由组。实施时请在 PR 描述中链接本文件并更新 Phase 勾选状态。*
