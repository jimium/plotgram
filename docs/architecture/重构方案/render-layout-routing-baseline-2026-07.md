# 渲染管线 / 架构图布局与路由 — 基线分析笔记

> 日期：2026-07-10  
> 性质：**Agent 工作笔记**（总体分析，为后续重构改造升级打基础）  
> 状态：基线快照；不替代 `layout/readme.md` 与各专项方案  
> 相关：  
> - [render-pipeline.md](../../guides/render-pipeline.md)  
> - [layout-routing-pipeline-full-analysis.md](./layout-routing-pipeline-full-analysis.md)  
> - [layout-routing-optimization-proposal-2026-07.md](./layout-routing-optimization-proposal-2026-07.md)  
> - [orthogonal-split-and-bundling-removal.md](../../已经实现的方案/orthogonal-split-and-bundling-removal.md)  
> - `crates/plotgram-core/src/layout/readme.md`

---

## 0. 一句话结论

FlowML（Plotgram）是 **纯 Rust 多阶段管线**：DSL → parse → prepare → validate → **layout（含边路由）** → build_scene → encode。前端只消费 SVG/文本，不做布局。

架构图默认路径是 **`architecture`（两阶段 Sugiyama）+ `orthogonal`（Architecture Profile）**。管线骨架正确，**不需要推倒重写**；后续升级应围绕：

1. **双套 Sugiyama 收敛**（`architecture_v2/layout/*` vs `sugiyama_v2/*`）
2. **布局↔路由空间预算统一**（EGB / PRS / corridor / group_sizing）
3. **正交路由编排拆分**（`edge_routing_orthogonal/mod.rs` 过重）
4. **组框审美默认策略**（Fit vs 同级条带化）

---

## 1. 渲染 Pipeline 总览

### 1.1 阶段顺序

```text
DSL 源码 (.pgm)
  │
  ▼ parse()                         RawDiagram
  ▼ prepare(StyleRequest)           PreparedDiagram { diagram, layout_plan }
  ▼ validate() [可选]               诊断 errors/warnings
  ▼ compute_layout_with_plan()      LayoutResult
  │   ├─ LayoutStrategy.compute()        节点 + 分组几何
  │   └─ EdgeRoutingStrategy.route()     边路径（sequence 等内置则跳过）
  ▼ build_scene(RenderRequest)      ExportScene
  ▼ encode_scene / encode_from_diagram
  ▼ RenderOutput  Text(SVG/ASCII/JSON/…) | Binary(PNG/WebP)
```

| 阶段 | 关键模块 | 输出类型 |
|------|----------|----------|
| parse | `dsl/parser/` | `RawDiagram` |
| prepare | `pipeline/prepare.rs`, `prepare/`, `theme/` | `PreparedDiagram` |
| validate | `validation/`, `kinds/` | 诊断 |
| layout | `layout/pipeline.rs` | `LayoutResult` |
| scene | `render/scene.rs` | `ExportScene` |
| encode | `render/encode/` | `RenderOutput` |

### 1.2 边界原则

| 层 | 职责 |
|----|------|
| **plotgram-core** | 解析、校验、布局、路由、样式物化、编码（全部业务） |
| **CLI / Server / WASM** | 薄适配：组装 `StyleRequest` / `RenderRequest`，返回产物或 JSON |
| **TS 前端** | 编辑器、debounce、选项 JSON、SVG DOM 展示；**不布局、不自绘** |

共享核心：playground / agent-demo / studio / website / VS Code / CLI / Server 都走同一套 `pipeline`。

### 1.3 编码双路径

| EncodingPath | 格式 | 是否走 ExportScene |
|--------------|------|-------------------|
| Scene | SVG, PNG, WebP, JSON, Draw.io | 是 |
| Diagram | ASCII, md-outline, OPML, FreeMind | 否（diagram + layout 直编） |

PNG/WebP = SVG 中间产物 + `usvg`/`resvg` 栅格化。WASM 桥接**不支持**二进制格式。

### 1.4 LayoutPipeline 内部（布局子系统）

实现：`crates/plotgram-core/src/layout/pipeline.rs`

```text
strategy.compute()                    # 节点/分组
  → apply_node_frame                  # L3 grid_snap + L1 GroupFramePass
  → [若 produces_edge_geometry] 直接 finalize  # sequence
  → run_routing_pipeline:
      apply_pre_route                 # 当前透传
      resolve_effective_edge_routing
      router.route + refine
      repulse_edges_only
      post_route_group_frame          # 组框恢复 + 增量重路由
      [architecture] PRS 扩壳
      snap_and_repulse_edges          # 末尾像素量化（仅一次）
  → finalize_canvas_bounds
```

**Group Frame 三层模型**（布局侧契约）：

| 层 | 职责 | 模块 |
|----|------|------|
| L2 Intra | 组内排列（hint / Sugiyama） | `group_layout_hint`, two_phase |
| L3 Node | rank/layer 轴对齐 | `grid_snap` |
| L1 Group | 组间排列 / 等宽 / 边框对齐 | `group_frame` |

---

## 2. 架构图布局算法

### 2.1 默认与可选

| 项 | 值 |
|----|-----|
| Profile 默认 layout | `architecture` |
| Profile 默认 routing | `orthogonal` |
| 默认方向 | `top-to-bottom` |
| 显式可选 layout | `architecture`, `force-directed` |

入口：`ArchitectureV2Layout` → `layout/node/architecture_v2/`

### 2.2 两条主路径

```text
有顶层 group?
  ├─ 是 → two_phase（核心路径，~2200 行）
  └─ 否 → 全局分组感知 Sugiyama（FAS → rank → order → coord → 7 Phase 后处理）
```

#### 路径 A：无顶层 group — 全局 Sugiyama

| 阶段 | 文件 | 要点 |
|------|------|------|
| 建图 | `architecture_v2/layout/types.rs` | `GraphIndex` |
| 去环 | `acyclic.rs` | greedy FAS + constrain 不可反转 |
| 分层 | `rank.rs` | 组感知：超级节点宏观 + 组内微观 |
| 排序 | `order.rs` | 加权中位数 + transpose；声明序 + id 字典序 |
| 坐标 | `coordinate.rs` | 中位数拉力迭代（**非** Brandes-Köpf） |
| 后处理 | `pipeline.rs` | Overlap → Clamp → NeighborAlign → HubCenter → GroupBounds → GroupOverlap → GroupAlign |

#### 路径 B：有顶层 group — 两阶段（主路径）

| 阶段 | 说明 |
|------|------|
| Phase A 组内 | 递归嵌套；hint（H/V/Fan/Grid）或组内 Sugiyama |
| Phase B 宏观 | 顶层 group / 游离节点 → 超级图 → macro rank → 块定位 |
| Phase C 回填 | 超级块 → 全局坐标 |
| C+ 微调 | 跨组边方向 nudge 组内节点 |
| D–F | 基础设施重平衡、同 rank y 微对齐 |
| EGB | 侧 gutter 预算，扩框给边留通道 |
| Group Frame | 兄弟重叠消解、内容包络 |

组内 hint：`group { layout: horizontal | vertical | fan-out | fan-in | grid | auto }`  
组宽：`group_sizing: fit`（默认）| `uniform`（同级条带）

### 2.3 与 sugiyama_v2 的关系（重要技术债）

| | architecture_v2 内置 Sugiyama | sugiyama_v2（flowchart/er/state） |
|--|------------------------------|----------------------------------|
| 分层 | 组感知最长路径式 | Network Simplex **风格**紧边压缩 |
| 坐标 | 中位数拉力 | Brandes-Köpf **风格**四遍 |
| 长边 | 较弱 | dummy 链 Proper Layer Graph |
| 用途 | 架构图组内 / 无组全局 | 流程图等 |

**结论**：两套平行实现，策略不同、维护成本高。重构优先方向是 **组内布局更多委托 sugiyama_v2**，或抽出统一 rank/order 接口；不要在 architecture 内再复制一套 BK。

### 2.4 其他布局（对照）

| 算法 | 架构图相关？ | 备注 |
|------|-------------|------|
| flowchart | 否（同族） | 有 group 时分治；共享 `divide_and_conquer` |
| force-directed | 可选 | FR + Barnes-Hut；推荐 straight |
| sequence | 否 | `produces_edge_geometry=true`，跳过通用路由 |
| mindmap / circular / er / state | 否 | 各有 profile |

---

## 3. 架构图边路由算法

### 3.1 算法清单（全库）

| 注册名 | 几何 | 架构图 | 避障方式 |
|--------|------|--------|----------|
| **orthogonal** | Polyline | **默认主路径** | 候选枚举打分 + SegmentGrid；走廊 BFS |
| spline | Polyline/Bezier | 可用 | 可见性图 + Dijkstra |
| bezier | Bezier | 可用 | **无**障碍避让 |
| straight | Straight | 少用 | 简单 |
| organic / circular | Bezier | 否 | mindmap / state 配套 |

**无网格 A\***。正交 = 有限候选 + 启发式；极端情况 refine 推节点或 spline 兜底。

### 3.2 Orthogonal 内部阶段（摘要）

实现：`layout/edge/edge_routing_orthogonal/`

```text
PreparedObstacles（确定性排序）
  → feedback_side（回环边外通道）
  → corridor_route（跨组 BFS 走廊 + 语义车道）
  → 端口选择 + slot 磁吸（端口级并线）
  → layer_order（sugiyama_ranks）
  → select_best_path（L / Channel / Z / Staircase 候选打分）
  → replan_slots / straighten / reroute / reverse-stub / assign_lanes
  → [Architecture] corridor offsets + unrelated trunk 分离
  → label_avoidance
```

**Architecture vs Flowchart Profile**（`profile_architecture.rs` / `profile_flowchart.rs`）：

| | Architecture | Flowchart |
|--|--------------|-----------|
| parallel_gap | 更大 | 标准 |
| semantic_merge | 开 | 几何优先 |
| corridor lanes | 开 | 关 |
| unrelated trunk 分离 | 开 | 关 |
| trunk+fork 偏好 | 关 | 开 |

### 3.3 Bundling 现状（勿走回头路）

- Post-route `edge_bundling/` **已退役**（见 `orthogonal-split-and-bundling-removal.md`）
- DSL `bundling` 选项会告警
- 并线能力由：**slot 磁吸 + edge_merge_policy + lane assignment + corridor lane** 承担
- 新功能应走上述路径，**不要恢复全文路径 bundling**

### 3.4 布局→路由耦合契约（改一边必验另一边）

| LayoutHints / 产物 | 路由消费方 |
|--------------------|------------|
| `sugiyama_ranks` | layer_order、feedback_side |
| `group_routing`（corridors, side_gutters） | GroupRoutingContext、corridor、EGB |
| 节点/组 bounds | 障碍膨胀、slot、PRS |
| `route_after_node_moves` / preserve | refine、GroupFrame、PRS 增量重路由 |

---

## 4. 关键类型速查

| 类型 | 位置 | 角色 |
|------|------|------|
| `Diagram` / `RawDiagram` / `PreparedDiagram` | `ast.rs` | AST 三态 |
| `LayoutPlan` | `layout/plan.rs` | 已解析 algo + routing + options |
| `LayoutResult` | `layout/mod.rs` | nodes/groups/edges/hints |
| `PathGeometry` | `layout/mod.rs` | Straight / Bezier / Polyline |
| `LayoutStrategy` / `EdgeRoutingStrategy` | `layout/mod.rs` | 可插拔策略 |
| `ExportScene` | `render/scene.rs` | 格式无关视觉 IR |
| `RenderOutput` | `render/output.rs` | Text \| Binary |
| `OrthoRoutingProfile` | `edge_routing_orthogonal/profile.rs` | Arch/Flow 分型 |

---

## 5. 确定性（AGENTS.md §2）

- **禁止**用未排序 `HashMap` 迭代驱动布局/路由顺序
- 已有实践：声明序 + id 字典序、`PreparedObstacles` 预排序、`BTreeMap`、显式 tie-breaker
- `LayoutResult.nodes/groups` 仍是 `HashMap`（存储无序）；算法内部与 export/测试签名需再排序
- `force-directed` 为弱确定性例外（浮点迭代）

---

## 6. 质量与性能观测面

| 工具 | 用途 |
|------|------|
| `OrthoDebugStats` / `RefineDebugStats` / `GutterBudgetDebug` | 运行时 hints |
| `plotgram-eval` `LayoutMetrics` | 穿障、交叉、弯折、面积、拥堵预测 |
| layout lint | CLI `lint`、穿组/穿节点等 |
| `[perf] layout/routing/...` | 分段耗时 |
| `benchmark-data/baseline.md` | 基线（例：k8s-tenant routing ~12ms） |

架构图已知短板（来自 2026-07-09 优化方案实测）：穿组、同级 group 宽高不齐、Fit 默认与条带审美冲突。日常流程图质量已可用；压力图（回环/长边/自环）仍是 flowchart 侧重点。

---

## 7. 重构改造升级 — 基础判断

### 7.1 什么是对的（保留）

1. **多阶段纯函数管线** + `PreparedDiagram` / `ExportScene` 契约清晰  
2. **LayoutStrategy / EdgeRoutingStrategy 注册表** 可插拔  
3. **架构图 two_phase「先舞台后节点」** 语义正确  
4. **Orthogonal 分 Profile** + slot/lane/corridor 替代 bundling 的方向正确  
5. **L1/L2/L3 Frame 分层** 职责可理解  
6. **前端零布局** — 升级算法不碰 TS 渲染逻辑  

### 7.2 什么是债（优先处理）

| 优先级 | 债 | 说明 |
|--------|-----|------|
| P0 | 双套 Sugiyama | architecture 内置 vs sugiyama_v2；坐标/分层策略分裂 |
| P0 | 布局↔路由预算被动 | EGB/PRS/corridor/sizing 未统一；事后扩壳破坏对称 |
| P1 | `two_phase.rs` / orthogonal `mod.rs` 过大 | 编排与算法缠在一起，难测难改 |
| P1 | Group 默认 Fit+Start | 与「同级条带」审美冲突；`uniform` 实验更稳 |
| P2 | Pipeline 硬编码 architecture 分支 | PRS 等与通用管线交织 |
| P2 | 旧 `sugiyama` 仍注册 | 扩大 API 面 |
| P2 | 文档超前 | overlay/intent 等部分文档描述未落地 |

### 7.3 明确不做（当前阶段）

- 不重写整个引擎  
- 不引入全图 A\* / 全局 ILP / 完整 Graphviz Network Simplex 坐标  
- 不恢复 post-route edge_bundling  
- 不为消除全部 lint warning 牺牲性能（AGENTS.md §4）  

### 7.4 建议的升级切入顺序（供后续会话）

```text
1. 基线固化：showcase 关键样例 + lint/eval 指标快照（本笔记 + baseline）
2. 组框审美：默认偏向同级条带（uniform / equal+center），保留 Fit 逃生舱
3. 空间预算：把 EGB 与 group_sizing / corridor 预算前移到布局阶段协商
4. Sugiyama 收敛：组内布局委托 sugiyama_v2 或统一接口（小步、可 A/B）
5. 正交编排拆分：path/scoring/slot/corridor 已模块化，继续拆 mod.rs 后处理
6. 反馈环收紧：减少 PRS/refine 后「推节点→全图重路由」的次数与范围
```

### 7.5 改动时必验清单

- [ ] 确定性：同输入多次 layout signature 一致  
- [ ] architecture showcase：穿组/穿节点 lint、group 同级宽高比  
- [ ] 增量重路由：GroupFrame / PRS / refine 后边几何仍干净  
- [ ] flowchart 回归：日常样例不退化；stress-dag 可接受  
- [ ] perf：routing/refine 相对 `benchmark-data/baseline.md` 不显著恶化  
- [ ] WASM/CLI 同一核心路径仍通  

---

## 8. 关键文件索引（绝对路径习惯：仓库相对）

```
crates/plotgram-core/src/pipeline/          # 总编排
crates/plotgram-core/src/layout/pipeline.rs # 布局+路由编排
crates/plotgram-core/src/layout/registry.rs # 算法工厂
crates/plotgram-core/src/layout/plan.rs     # LayoutPlan
crates/plotgram-core/src/layout/node/architecture_v2/
crates/plotgram-core/src/layout/node/sugiyama_v2/
crates/plotgram-core/src/layout/edge/edge_routing_orthogonal/
crates/plotgram-core/src/layout/group_frame/
crates/plotgram-core/src/layout/refine/
crates/plotgram-core/src/render/scene.rs
crates/plotgram-core/src/profile/mod.rs     # 图类型默认 algo
crates/plotgram-wasm/src/lib.rs
crates/plotgram-eval/src/metrics.rs
```

---

## 9. 笔记维护约定

- 本文件是 **基线认知**，不是实施任务单；具体改造写独立方案文档并回链此处。  
- 代码与文档冲突时以 **代码** 为准，并在本节或相关专项文档标注「文档过时」。  
- 重大管线变更后更新 §0 结论、§7 债表与必验清单。  
