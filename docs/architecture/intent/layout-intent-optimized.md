# Layout Intent 优化方案（需求 + 实现重评估）

> 状态：v2.0（合并重写）
>
> 本文档基于 [layout-intent-refinement.md](./layout-intent-refinement.md) v0.4 需求稿与
> [layout-intent-implementation.md](./layout-intent-implementation.md) v1.0 实现稿，
> 结合 `drawify-core` 当前代码现状进行二次评估后重写。原两份文档已被本文件取代并删除。

---

## 0. 评估摘要

### 0.1 评估方法

逐文件比对两份原始文档与以下代码实体的实际契约：

- [crates/drawify-core/src/layout/mod.rs](../../crates/drawify-core/src/layout/mod.rs) — `compute_layout` / `compute_layout_with_plan` 调度
- [crates/drawify-core/src/layout/refine.rs](../../crates/drawify-core/src/layout/refine.rs) — 已存在的 layout↔route 反馈循环
- [crates/drawify-core/src/layout/grid_snap.rs](../../crates/drawify-core/src/layout/grid_snap.rs) — Grid Snap Phase 1/2
- [crates/drawify-core/src/ast.rs](../../crates/drawify-core/src/ast.rs) — `Relation` / `PreparedDiagram` / `LayoutPlan`
- [crates/drawify-core/src/pipeline.rs](../../crates/drawify-core/src/pipeline.rs) — parse → prepare → validate
- [crates/drawify-core/src/render/scene.rs](../../crates/drawify-core/src/render/scene.rs) — `export_scene` 边索引契约
- [crates/drawify-core/src/layout/edge/edge_routing.rs](../../crates/drawify-core/src/layout/edge/edge_routing.rs) — 路由器与 relations 的索引对应
- [crates/drawify-core/src/layout/node/sugiyama_v2/](../../crates/drawify-core/src/layout/node/sugiyama_v2/) — 分层引擎
- [crates/drawify-wasm/src/lib.rs](../../crates/drawify-wasm/src/lib.rs) / [crates/drawify-server/src/api.rs](../../crates/drawify-server/src/api.rs) — 对外 API

### 0.2 总体结论

| 维度 | 原需求稿 | 原实现稿 | 评估结论 |
|------|----------|----------|----------|
| 需求边界 | 清晰、克制 | 基本对齐 | 需求稿质量高，但实现稿在 `Pin{x,y}` 上越界 |
| 架构兼容性 | — | 偏离 | 实现稿绕过 `LayoutPlan` / `PreparedDiagram`，伪代码无法编译 |
| 数据结构 | — | 有硬伤 | `Relation` 无 `id` 字段；`inject_phantom_relations` 代码错误 |
| 算法可行性 | — | 高估 | `SameRank` / `Near` 在现有 Sugiyama 引擎上无原生支持 |
| 路由/渲染契约 | — | 未覆盖 | 幽灵边与 `route_edges` / `export_scene` 的索引契约冲突 |
| API 兼容性 | — | 部分冲突 | Server 成功响应为裸字节流，无法内嵌 `refinement_report` |

**核心判断：** 原需求稿（v0.4）方向正确、边界克制，应作为需求基线保留；原实现稿（v1.0）需大幅重写以对齐真实代码架构，并收敛算法范围。

---

## 1. 需求基线（沿用 v0.4，仅做边界收紧）

本节复述 [layout-intent-refinement.md](./layout-intent-refinement.md) v0.4 的核心需求，并标注本次重评估的收紧点。

### 1.1 适用范围

- 仅 `architecture` 与 `flowchart` 两类图
- 仅在自动布局已基本可读的前提下做局部修正

### 1.2 首期核心需求（P1）

| 编号 | 需求 | 原稿定位 | 本次评估 |
|------|------|----------|----------|
| N1 | 主方向与上下游关系修正 | P1 核心 | ✅ 保留，但首期仅做 `below` / `above` |
| N2 | 局部聚类修正 | P1 核心 | ⚠️ 降级为 P2：`near` 在现有引擎无原生支持 |
| N3 | 局部稳定与最小扰动 | P1 核心 | ✅ 保留，由 `pin`（仅轴约束，不含绝对坐标）承担 |
| N4 | 简单对齐与轻量美化 | P1.5 | ✅ 保留为 P1.5：`align_vertical` / `align_horizontal` |

### 1.3 明确的非目标（收紧）

以下能力在原需求稿 §7 已列为非目标，本次重申并补充：

- ❌ 绝对坐标控制（`x` / `y`）—— 原实现稿的 `Pin { x: Option<f64>, y: Option<f64> }` **违反此非目标**，必须移除
- ❌ 像素级偏移
- ❌ 通用布尔约束求解
- ❌ 图形编辑器式自由拖拽
- ❌ 承诺所有意图都能严格满足

### 1.4 验收标准（沿用原稿 §10）

- 用户在 1–2 次交互内完成局部修正
- 修正后整体布局基本稳定
- 相比"重生成 DSL + 重布局"，整图跳变明显减少
- 系统能明确反馈修正结果（成功 / 部分成功 / 未满足）

---

## 2. 现状代码基线

实施任何意图能力前，必须理解以下既有契约。

### 2.1 布局调度链路

```text
PreparedDiagram（携带 diagram + layout_plan）
  ↓ render::scene::export_scene
  ↓ layout::compute_layout_with_plan(diagram, plan)
      ├─ validate_layout_config(diagram)
      ├─ registry::build_layout_strategy(algo, plan)   ← 算法实例化依赖 plan
      ├─ strategy.compute(diagram)                      ← 产出 nodes/groups，edges 通常为空
      ├─ grid_snap::snap_layout_to_grid(result, cfg, horizontal)
      ├─ grid_snap::refresh_layout_bounds(result, diagram, algo, group_padding)
      ├─ grid_snap::snap_group_bounds / update_canvas_bounds
      ├─ registry::build_edge_routing_strategy(edge_routing, plan)
      ├─ router.route(diagram, result)                  ← 产出 edges，索引 == relations 索引
      ├─ refine::run_refine(diagram, result, router, cfg)  ← 已存在的穿障修正循环
      └─ grid_snap::snap_edge_waypoints(result.edges, cfg)
```

关键事实：

1. **`LayoutPlan` 是算法实例化的必经输入**：`registry::build_layout_strategy(algo, plan)` 需要 plan 解析出的 options。原实现稿的 `resolve_layout_algo` / `layout_strategy_for(algo)` 不存在。
2. **`PreparedDiagram` 在 `prepare()` 阶段已 resolve plan**：下游不应再次 resolve。
3. **`LayoutResult.edges` 由路由器产出**，非 strategy 产出（`sequence` 除外）。路由器 `route_edges` 按 `diagram.relations[i]` → `result.edges[i]` 一一对应。
4. **`export_scene` 按 `diagram.relations[i]` → `layout.edges[i]` 映射**（[scene.rs#L102-L112](../../crates/drawify-core/src/render/scene.rs#L102-L112)）。任何破坏该索引对应关系的方案都会导致渲染错位。
5. **`refine::run_refine` 已存在**：做 edge-node 穿障检测 → 推开问题节点 → 全量 re-route。原实现稿完全未提及与此模块的交互。

### 2.2 `Relation` 结构（关键约束）

```rust
// crates/drawify-core/src/ast.rs
pub struct Relation {
    pub from: Identifier,
    pub to: Identifier,
    pub arrow: ArrowType,
    pub label: Option<String>,
    pub attributes: AttributeMap,
    pub span: Span,
}
```

**`Relation` 没有 `id` 字段。** 原实现稿 `inject_phantom_relations` 中 `Relation { id: id.clone(), ... }` 无法编译。

### 2.3 Sugiyama V2 引擎现状

- `build_graph(diagram)` 遍历 `diagram.relations` 构图，无 edge kind 区分
- `greedy_cycle_reversal` 会反转边以破环 —— **非环的幽灵边也可能被反转**，破坏 `below`/`above` 语义
- `assign_ranks_network_simplex_style` 基于 longest-path + network-simplex，**无原生 same-rank 约束**
- `order_layers_weighted_median` 层内排序，**无原生 order hint 入口**
- `assign_layer_centers_brandes_koepf` 坐标分配，**无原生 per-pair 距离权重**
- 引擎已具备确定性（测试 `v2_layout_is_deterministic_across_runs` 验证），无需 `layout_seed`

### 2.4 Grid Snap 现状

- `snap_layout_to_grid(layout, config, horizontal) -> SnapReport`，按 rank 轴聚类 + 层内槽位吸附
- `refresh_layout_bounds(layout, diagram, algo, group_padding)` —— **4 个参数**，原实现稿漏写 `group_padding`
- 已支持 `snap: false` 顶层开关

### 2.5 对外 API 现状

| 入口 | 现状 | 携带 overlay 的难点 |
|------|------|---------------------|
| WASM `render_with_options` | 返回 `RenderResult { svg, ascii, scene_json, ... }` | overlay 需经 `RenderRequest` 透传至 `export_scene` 内部的 `compute_layout_with_plan` |
| Server `/render` | 成功返回裸字节流（SVG/PNG），失败返回 JSON | 无法在成功响应体内追加 `refinement_report` |
| `RenderRequest` | 仅持有 `&PreparedDiagram` + 主题/样式 | 无 layout overlay 字段 |

---

## 3. 原方案问题清单

### 3.1 编译级硬伤（必须修复）

| # | 问题 | 位置 | 严重度 |
|---|------|------|--------|
| C1 | `Relation` 无 `id` 字段，`inject_phantom_relations` 伪代码无法编译 | 实现稿 §4.3 | 🔴 阻断 |
| C2 | `compute_layout_with_intents` 调用 `resolve_layout_algo` / `layout_strategy_for(algo)`，二者不存在 | 实现稿 §6.3 | 🔴 阻断 |
| C3 | `refresh_layout_bounds` 调用漏写 `group_padding` 参数 | 实现稿 §6.3 | 🟡 编译失败 |
| C4 | `LayoutIntent::AlignVertical` / `AlignHorizontal` 的 `or-pattern` 绑定 `nodes` 不合法（Rust or-pattern 不能绑定不同变体的同名字段） | 实现稿 §6.3 | 🟡 编译失败 |

### 3.2 架构契约冲突（必须重新设计）

| # | 问题 | 影响 |
|---|------|------|
| A1 | 绕过 `LayoutPlan` / `PreparedDiagram`，与 `export_scene` → `compute_layout_with_plan` 链路脱节 | overlay 无法到达布局阶段 |
| A2 | 幽灵边注入 `diagram.relations` 后，`route_edges` 与 `export_scene` 的 `relations[i] ↔ edges[i]` 索引契约被破坏 | 渲染错位或幽灵边被画出 |
| A3 | 未处理与既有 `refine::run_refine` 的交互顺序 | P1b 移动节点后，穿障修正可能反推节点，破坏对齐 |
| A4 | `Pin { x, y }` 绝对坐标违反需求稿 §7 非目标 | 需求-实现不一致 |
| A5 | Server 成功响应为裸字节流，无法内嵌 `refinement_report` | API 设计不可行 |

### 3.3 算法可行性高估（必须收敛范围）

| # | 问题 | 真实复杂度 |
|---|------|-----------|
| F1 | `SameRank` 在 network-simplex rank 分配上无原生支持 | 需引入约束传播或节点合并，非"注入幽灵边"可解决 |
| F2 | `Near`（Proximity）在 Brandes-Kopf 坐标分配上无 per-pair 距离权重 | 需改造坐标优化目标函数 |
| F3 | `greedy_cycle_reversal` 可能反转非环幽灵边，破坏 `below` 语义 | 需在 FAS 阶段标记幽灵边为不可反转 |
| F4 | Architecture V2 两阶段布局中，跨组幽灵边需影响组级 rank | 需改造 `two_phase` 模块 |
| F5 | `layout_seed` 无实际消费者（引擎已确定性） | 死基础设施 |

### 3.4 范围漂移（必须对齐需求）

| # | 问题 | 需求稿定位 |
|---|------|-----------|
| S1 | `AlignVertical/Horizontal` 被放在 P1b，与 `Pin` 同期 | 需求稿 §9.3 明确为 P1.5 |
| S2 | `Pin { x, y }` 提供绝对坐标 | 需求稿 §7 明确为非目标 |
| S3 | `SameRank` / `RightOf` / `Near` 一次性全上 | 需求稿 §8.2 要求"低复杂度、最小能力闭环" |

### 3.5 可维护性问题

| # | 问题 | 建议 |
|---|------|------|
| M1 | `layout/intent/refinement.rs` 与既有 `layout/refine.rs` 命名混淆 | 改名 `layout/intent/geometric.rs` |
| M2 | 幽灵边通过 mutate `diagram.relations` + 事后 retain 清理，副作用面大 | 改为 strategy 透传 overlay，不 mutate diagram |
| M3 | `LayoutIntent` 枚举混装拓扑意图与几何意图，序列化 tag 易混淆 | 拆分为 `TopologyIntent` + `GeometricIntent` 两个枚举 |

---

## 4. 优化策略总览

### 4.1 架构层：overlay 透传而非 diagram 变异

**原方案：** clone diagram → push 幽灵 relations → compute → retain 清理 → route。
**优化方案：** overlay 作为独立参数透传，strategy 原生消费，diagram 不被变异。

```text
RenderRequest { diagram, overlay: Option<&LayoutIntentOverlay>, ... }
  ↓ export_scene
  ↓ compute_layout_with_plan_and_overlay(diagram, plan, overlay)
      ├─ strategy.compute_with_overlay(diagram, overlay)   ← 算法原生解读意图
      ├─ apply_geometric_refinement(result, overlay)        ← P1.5 几何微调
      ├─ grid_snap(pinned_nodes)
      ├─ router.route(diagram, result)                      ← diagram 未变异，索引契约保持
      ├─ refine::run_refine(...)                            ← 既有穿障修正
      └─ snap_edge_waypoints
```

**收益：**
- 消除 A2 索引契约冲突
- 消除 C1 `Relation.id` 硬伤
- 消除 M2 副作用面

### 4.2 算法层：分阶段引入，原生 hint 而非幽灵边

放弃"统一幽灵边 + 算法适配"的过度抽象，改为每类意图由对应算法原生支持：

| 意图 | 实现方式 | 首期是否做 |
|------|----------|-----------|
| `below` / `above` | Sugiyama `build_graph` 阶段注入 rank 约束边，并在 `greedy_cycle_reversal` 中标记为不可反转 | ✅ P1 |
| `pin`（轴约束） | P1.5 几何微调阶段固定节点轴坐标，跳过 grid snap | ✅ P1.5 |
| `align_vertical` / `align_horizontal` | P1.5 几何微调，对齐后局部重叠消除 | ✅ P1.5 |
| `right_of` | Sugiyama `order_layers_weighted_median` 增加 order hint 入口 | ⚠️ P2 |
| `same_rank` | 需 network-simplex 约束传播或节点合并 | ⚠️ P2 |
| `near` | 需 Brandes-Kopf 坐标目标函数改造 | ❌ 暂不做 |

### 4.3 数据结构层：拆分意图类型

```rust
pub enum TopologyIntent {
    Below { from: String, to: String },
    Above { from: String, to: String },
}

pub enum GeometricIntent {
    Pin { node: String, axis: PinAxis },           // 仅轴约束，无绝对坐标
    AlignVertical { nodes: Vec<String> },
    AlignHorizontal { nodes: Vec<String> },
}

pub enum PinAxis { X, Y, Both }

pub struct LayoutIntentOverlay {
    pub topology: Vec<TopologyIntent>,
    pub geometric: Vec<GeometricIntent>,
}
```

**收益：**
- 消除 A4（`Pin` 不再带绝对坐标）
- 消除 M3（拓扑与几何分离，序列化清晰）
- 对齐 S1（几何意图归 P1.5）

### 4.4 API 层：报告走 header / 扩展字段

| 入口 | 优化方案 |
|------|----------|
| WASM | `RenderResult` 增加 `refinement_report: Option<RefinementReport>` 字段；`render_with_options` 扩展 `WasmRenderOptions` 增加 `layout_intents` |
| Server | 成功响应增加 `X-Drawify-Refinement-Report` 响应头（JSON），body 保持裸字节流；`RenderRequestBody` 增加 `layout_intents` 字段 |
| `RenderRequest` | 增加 `layout_overlay: Option<&'a LayoutIntentOverlay>` 字段 |

### 4.5 命名层：消除冲突

- `layout/intent/mod.rs` — 数据结构
- `layout/intent/topology.rs` — 拓扑意图解读（替代原 `phantom.rs`）
- `layout/intent/geometric.rs` — 几何微调（替代原 `refinement.rs`，避免与 `layout/refine.rs` 混淆）

---

## 5. 优化后方案详情

### 5.1 数据结构

```rust
// crates/drawify-core/src/layout/intent/mod.rs

use serde::{Deserialize, Serialize};

/// 拓扑意图：影响分层/排序，在 strategy.compute 内部消费
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TopologyIntent {
    /// from 应在 to 的下游（rank 更大），注入约束边 from→to
    Below { from: String, to: String },
    /// from 应在 to 的上游（rank 更小），等价于 Below { from: to, to: from }
    Above { from: String, to: String },
}

/// 几何意图：布局后修正坐标，在 apply_geometric_refinement 中消费
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeometricIntent {
    /// 固定节点当前坐标（跳过 grid snap 与后续几何调整）
    Pin { node: String, axis: PinAxis },
    /// 多节点垂直对齐（x 中心一致）
    AlignVertical { nodes: Vec<String> },
    /// 多节点水平对齐（y 中心一致）
    AlignHorizontal { nodes: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinAxis { X, Y, Both }

/// 意图叠加层
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayoutIntentOverlay {
    #[serde(default)]
    pub topology: Vec<TopologyIntent>,
    #[serde(default)]
    pub geometric: Vec<GeometricIntent>,
}

/// 单条意图满足状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    Satisfied,
    Partial,
    Conflicted,
    NotFound,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentResult {
    pub index: usize,
    pub kind: String,
    pub status: IntentStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RefinementReport {
    pub results: Vec<IntentResult>,
    pub satisfied: usize,
    pub partial: usize,
    pub conflicted: usize,
    pub not_found: usize,
}
```

### 5.2 拓扑意图实现（P1）

#### 5.2.1 Sugiyama V2 适配

在 `sugiyama_v2/graph.rs` 的 `build_graph` 中增加 overlay 入口，将 `Below` 翻译为约束边：

```rust
// 伪代码：build_graph_with_overlay
pub(super) fn build_graph_with_overlay(
    diagram: &Diagram,
    overlay: Option<&LayoutIntentOverlay>,
) -> DiGraph<String, EdgeMeta> {
    let mut graph = DiGraph::<String, EdgeMeta>::new();
    // ... 添加真实节点 ...
    // 真实边：EdgeMeta { kind: Real, reversible: true }
    for relation in &diagram.relations {
        graph.add_edge(from, to, EdgeMeta { kind: EdgeKind::Real, reversible: true });
    }
    // 拓扑意图边：EdgeMeta { kind: Intent, reversible: false }
    if let Some(ov) = overlay {
        for intent in &ov.topology {
            match intent {
                TopologyIntent::Below { from, to } => add_intent_edge(&mut graph, from, to),
                TopologyIntent::Above { from, to } => add_intent_edge(&mut graph, to, from),
            }
        }
    }
    graph
}
```

#### 5.2.2 关键：保护意图边不被 FAS 反转

`greedy_cycle_reversal` 当前对所有边一视同仁。需改造为：

```rust
pub(super) fn greedy_cycle_reversal(
    graph: &DiGraph<String, EdgeMeta>,
) -> HashSet<(NodeIndex, NodeIndex)> {
    // FAS 优先反转 reversible=true 的真实边，
    // 仅当无法仅靠真实边破环时才反转 intent 边
    // ...
}
```

**这是原方案完全遗漏的关键点**：不保护意图边，`below(A,B)` 注入的 A→B 边可能被 FAS 反转为 B→A，导致 A 排在 B 上游，与意图相反。

**实施路径**：`greedy_cycle_reversal` 当前先重建邻接表再调用 `acyclic::greedy_fas`（见 [graph.rs#L49-L62](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/sugiyama_v2/graph.rs#L49-L62)）。改造有两种路线：

- **路径 A（推荐）**：在 `greedy_cycle_reversal` 层构建邻接表时排除意图边（不作为 FAS 候选），仅当仅靠真实边无法破环时才降级处理意图边。改动局部、不对 `acyclic::greedy_fas` 通用函数引入意图概念。
- **路径 B**：改造 `acyclic::greedy_fas` 增加 protected edges 参数。通用性更高但改动面更广。

首期建议走路径 A，验证 FAS 行为后再考虑是否需要路径 B。

#### 5.2.2.1 涟漪效应：Graph 边类型变更

将 `build_graph` 的返回类型从 `DiGraph<String, ()>` 改为 `DiGraph<String, EdgeMeta>`，以下下游函数也需要相应调整：

| 函数 | 文件 | 影响 |
|------|------|------|
| `build_graph` / `build_graph_with_overlay` | `sugiyama_v2/graph.rs` | 创建边时携带 `EdgeMeta` |
| `greedy_cycle_reversal` | `sugiyama_v2/graph.rs` | 签名从 `&DiGraph<String, ()>` 改为 `&DiGraph<String, EdgeMeta>` |
| `build_dag` | `sugiyama_v2/graph.rs#L64-L85` | 当前 `add_edge(new_from, new_to, ())`，需改为携带 `EdgeMeta`；反转边时需调整 `reversible` 标记 |
| `build_proper_layer_graph` | `sugiyama_v2/graph.rs#L104-L165` | dummy 边（跨层长边拆分出的虚拟边）需决定 `EdgeMeta` 值（建议统一为 `EdgeKind::Real, reversible: false`，因为 dummy 边在 proper layer graph 中不需要参与 FAS） |
| rank / order / coordinate 等下游 | `sugiyama_v2/` | 这些阶段只读拓扑结构（节点邻接关系），不读边权重，影响可控 |

此变更是**纯内部实现细节**，不影响公开 API。建议在 Phase 0 搭建 `EdgeMeta` 类型后，先完成 graph 模块的全链路编译验证，确认无遗漏再进入 Phase 1。

#### 5.2.3 环检测

注入前对 `真实边 + 意图边` 做环检测。若意图边引入环：

- 标记该意图为 `Conflicted`
- 跳过该意图边，不参与布局

```rust
fn validate_topology_intents(
    diagram: &Diagram,
    overlay: &LayoutIntentOverlay,
) -> (Vec<ValidIntent>, Vec<IntentResult>) { ... }
```

#### 5.2.4 Architecture V2 适配

Architecture V2 在无分组时走全局 Sugiyama，可直接复用上述机制。有分组时走 `two_phase`，首期**仅支持组内拓扑意图**（`from` 与 `to` 同组），跨组意图标记为 `Partial` 并跳过。这避免改造 `two_phase` 的组级 rank 逻辑，控制首期范围。

### 5.3 几何微调实现（P1.5）

#### 5.3.1 执行位置

```text
strategy.compute_with_overlay(diagram, overlay)
  ↓
apply_geometric_refinement(&mut result, overlay)   ← 此处
  ↓
grid_snap::snap_layout_to_grid(result, cfg, horizontal, &pinned)
  ↓
router.route(diagram, result)
  ↓
refine::run_refine(diagram, result, router, cfg)   ← 既有穿障修正
  ↓
snap_edge_waypoints
```

**与 `refine::run_refine` 的顺序：** 几何微调在路由前，穿障修正在路由后。若穿障修正推开节点破坏对齐，报告中对齐状态降级为 `Partial`。首期不做两者联动，仅观测。

#### 5.3.2 `Pin` 实现（轴约束，无绝对坐标）

```rust
fn apply_pin(result: &mut LayoutResult, node_id: &str, axis: PinAxis) -> IntentStatus {
    let Some(node) = result.nodes.get(node_id) else {
        return IntentStatus::NotFound;
    };
    // Pin = 锁定当前坐标，不修改值，仅标记为 pinned 跳过后续 snap
    // axis 决定 snap 时跳过哪个轴
    IntentStatus::Satisfied
}
```

`Pin` 的语义从"设置绝对坐标"改为"锁定当前坐标"。用户通过先调整 DSL 让节点接近目标位置，再 `Pin` 锁定，符合需求稿"不要求绝对坐标控制"的边界。

#### 5.3.3 `AlignVertical` / `AlignHorizontal`

沿用原方案均值对齐 + 局部重叠消除，但增加约束：

- 对齐节点集合若跨分组，标记 `Partial` 并仅对齐同组节点
- 重叠消除仅做一轮，失败则标记 `Partial`，不级联
- 对齐后的节点加入 `pinned` 集合，跳过 grid snap

#### 5.3.4 Grid Snap Phase 3 集成

```rust
pub fn snap_layout_to_grid(
    layout: &mut LayoutResult,
    config: &GridSnapConfig,
    horizontal: bool,
    pinned: &PinSet,  // ★ 新增
) -> SnapReport { ... }

pub struct PinSet {
    full: HashSet<String>,      // PinAxis::Both
    x_only: HashSet<String>,    // PinAxis::X
    y_only: HashSet<String>,    // PinAxis::Y
    aligned: HashSet<String>,   // 对齐意图保护的节点
}
```

`pinned` 节点在 rank 轴 / layer 轴 snap 时按 axis 跳过；`aligned` 节点作为 snap 锚点参考。

### 5.4 调度入口

```rust
// crates/drawify-core/src/layout/mod.rs

/// 既有入口：保持不变
pub fn compute_layout(diagram: &Diagram) -> Result<LayoutResult, DiagnosticError> {
    let profile = profile_for(&diagram.diagram_type);
    let plan = LayoutPlan::resolve(diagram, profile);
    compute_layout_with_plan(diagram, &plan)
}

/// 既有入口：保持不变
pub fn compute_layout_with_plan(
    diagram: &Diagram,
    plan: &LayoutPlan,
) -> Result<LayoutResult, DiagnosticError> {
    compute_layout_with_plan_and_overlay(diagram, plan, None).map(|(r, _)| r)
}

/// 新增入口：带意图
pub fn compute_layout_with_plan_and_overlay(
    diagram: &Diagram,
    plan: &LayoutPlan,
    overlay: Option<&LayoutIntentOverlay>,
) -> Result<(LayoutResult, Option<RefinementReport>), DiagnosticError> {
    validate_layout_config(diagram)?;

    let algo = plan.layout_algo.as_str();
    let strategy = registry::build_layout_strategy(algo, plan).ok_or_else(|| { ... })?;

    // P1：拓扑意图在 strategy 内部消费
    let produces_edges = strategy.produces_edge_geometry();
    let mut result = strategy.compute_with_overlay(diagram, overlay);

    // P1.5：几何微调（在 grid snap 前）
    let mut pinned = PinSet::default();
    let mut report = RefinementReport::default();
    if let Some(ov) = overlay {
        let geo_report = apply_geometric_refinement(&mut result, ov, &mut pinned);
        report.merge(geo_report);
    }

    // Grid Snap Phase 1+3
    if !produces_edges && grid_snap::should_snap(algo) {
        let horizontal = diagram.layout_direction() == "left-to-right";
        let snap_config = grid_snap::GridSnapConfig::for_diagram(algo, diagram);
        if snap_config.enabled {
            grid_snap::snap_layout_to_grid(&mut result, &snap_config, horizontal, &pinned);
            let group_padding = snap_group_padding(plan, algo);
            grid_snap::refresh_layout_bounds(&mut result, diagram, algo, group_padding);
            grid_snap::snap_group_bounds(&mut result, &snap_config);
            grid_snap::update_canvas_bounds(&mut result, constants::DEFAULT_PADDING);
        }
    }

    if produces_edges {
        return Ok((result, Some(report)));
    }

    // 边路由（diagram 未变异，索引契约保持）
    let edge_routing_style = plan.edge_routing.as_str();
    let router = registry::build_edge_routing_strategy(edge_routing_style, plan).ok_or_else(|| { ... })?;
    let mut result = router.route(diagram, result);

    // 既有穿障修正
    let refine_config = refine::RefineConfig::default();
    result = refine::run_refine(diagram, result, router.as_ref(), &refine_config);

    // Grid Snap Phase 2
    if grid_snap::should_snap(algo) {
        let snap_config = grid_snap::GridSnapConfig::for_diagram(algo, diagram);
        if snap_config.enabled {
            grid_snap::snap_edge_waypoints(&mut result.edges, &snap_config);
        }
    }

    Ok((result, Some(report)))
}
```

> **注意**：边路由的障碍物膨胀间距由 `constants::DEFAULT_NODE_MARGIN` / `constants::DEFAULT_GROUP_MARGIN` 统一提供，不再从 per-element `style.margin` 读取。

### 5.5 `LayoutStrategy` trait 扩展

为避免破坏既有实现，采用默认方法而非修改 `compute` 签名：

```rust
pub trait LayoutStrategy {
    fn compute(&self, diagram: &Diagram) -> LayoutResult {
        self.compute_with_overlay(diagram, None)
    }

    /// 新增默认方法：支持意图的布局入口
    fn compute_with_overlay(
        &self,
        diagram: &Diagram,
        overlay: Option<&LayoutIntentOverlay>,
    ) -> LayoutResult {
        // 默认实现：忽略 overlay，等价于 compute
        let _ = overlay;
        let mut result = self.compute(diagram);
        // 默认不消费意图，report 由调度层填充 NotFound
        result
    }

    // ... 其余 trait 方法不变 ...
}
```

`SugiyamaV2Layout` / `FlowchartLayout` / `ArchitectureV2Layout` override `compute_with_overlay` 以原生消费拓扑意图。其余算法使用默认实现（忽略 overlay）。

### 5.6 渲染层透传

```rust
// crates/drawify-core/src/render/request.rs
pub struct RenderRequest<'a> {
    pub diagram: &'a PreparedDiagram,
    pub format: RenderFormat,
    // ... 既有字段 ...
    pub layout_overlay: Option<&'a LayoutIntentOverlay>,  // ★ 新增
}

// crates/drawify-core/src/render/scene.rs
pub fn export_scene(request: &RenderRequest) -> Result<ExportScene> {
    let layout = layout::compute_layout_with_plan_and_overlay(
        request.diagram.inner(),
        request.diagram.layout_plan(),
        request.layout_overlay,
    ).map_err(|e| DrawifyError::Render(e.to_string()))?;
    // ... 其余不变 ...
}
```

`ExportScene` 增加 `refinement_report: Option<RefinementReport>` 字段，供编码器写入响应。

> **注意**：当前 `export_scene` 在 [scene.rs#L193-L194](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/render/scene.rs#L193-L194) 调用的是 `compute_layout(request.diagram)`，这会通过 `Deref` 重新从 `Diagram` resolve `LayoutPlan`，忽略了 `PreparedDiagram` 中已缓存的 plan。本方案改为直接调用 `compute_layout_with_plan_and_overlay(_, request.diagram.layout_plan(), _)`，顺便修复了此效率问题。

### 5.7 对外 API

#### 5.7.1 WASM

```rust
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct WasmRenderOptions {
    pub theme_id: Option<String>,
    pub graphic_style: Option<String>,
    pub dark_mode: Option<bool>,
    pub ascii: Option<AsciiExportOptions>,
    pub layout_intents: Option<LayoutIntentOverlay>,  // ★ 新增
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RenderResult {
    pub success: bool,
    pub svg: Option<String>,
    pub ascii: Option<String>,
    pub scene_json: Option<String>,
    pub ascii_metadata: Option<AsciiExportMetadata>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub refinement_report: Option<RefinementReport>,  // ★ 新增
}
```

`render_with_options` 内部将 `options.layout_intents` 透传至 `RenderRequest::layout_overlay`。

#### 5.7.2 Server

```rust
#[derive(Debug, Deserialize)]
pub struct RenderRequestBody {
    pub source: String,
    #[serde(default = "default_format")]
    pub format: String,
    pub theme_id: Option<String>,
    pub graphic_style: Option<String>,
    pub dark_mode: Option<bool>,
    pub layout_intents: Option<LayoutIntentOverlay>,  // ★ 新增
}
```

成功响应：

- Body：保持裸字节流（SVG/PNG/ASCII/JSON）
- Header：增加 `X-Drawify-Refinement-Report`（JSON 序列化的 `RefinementReport`，仅当存在时设置）

失败响应：保持现有 JSON 结构不变。

### 5.8 冲突检测与报告

| 冲突类型 | 检测时机 | 处理 |
|----------|----------|------|
| 意图节点不存在 | overlay 解析时 | `NotFound`，跳过 |
| 拓扑意图引入环 | `build_graph_with_overlay` 后 DFS | `Conflicted`，跳过该意图边 |
| `Below(A,B)` + `Above(A,B)` 矛盾 | overlay 解析时去重 | 保留先声明者，后者 `Conflicted` |
| 跨组拓扑意图（Architecture V2） | strategy 内部判断 | `Partial`，仅组内生效 |
| 对齐节点跨组 | `apply_geometric_refinement` | `Partial`，仅对齐同组节点 |
| 对齐后重叠无法消除 | 重叠消除单轮失败 | `Partial` |
| 穿障修正破坏对齐 | `refine::run_refine` 后比对 | `Partial`（首期仅观测，不回滚） |

---

## 6. 实施路线

### Phase 0：基础设施（1–2 天）

- [ ] 创建 `layout/intent/mod.rs`：`TopologyIntent` / `GeometricIntent` / `LayoutIntentOverlay` / `RefinementReport`
- [ ] `LayoutStrategy` trait 增加 `compute_with_overlay` 默认方法
- [ ] `compute_layout_with_plan_and_overlay` 入口（暂不消费 overlay，仅透传 + 空 report）
- [ ] `RenderRequest` 增加 `layout_overlay` 字段
- [ ] 验证：`overlay = None` 时所有既有测试通过

### Phase 1：拓扑意图 MVP（3–5 天）

- [ ] `layout/intent/topology.rs`：意图校验（节点存在性、矛盾去重、环检测）
- [ ] `sugiyama_v2/graph.rs`：`build_graph_with_overlay` + `EdgeMeta` 区分真实/意图边
- [ ] `sugiyama_v2/graph.rs`：`greedy_cycle_reversal` 保护意图边不被反转
- [ ] `FlowchartLayout` / `SugiyamaV2Layout` override `compute_with_overlay`
- [ ] `ArchitectureV2Layout` override `compute_with_overlay`（仅组内意图）
- [ ] 测试：`below` 影响 rank 排序；环检测拒绝；FAS 不反转意图边

### Phase 1.5：几何微调（2–3 天）

- [ ] `layout/intent/geometric.rs`：`apply_pin` / `apply_align_vertical` / `apply_align_horizontal`
- [ ] `PinSet` 结构 + `grid_snap::snap_layout_to_grid` 增加 `pinned` 参数
- [ ] 对齐后单轮重叠消除
- [ ] 测试：pin 节点跳过 snap；对齐满足且不重叠

### Phase 2：API 暴露（1–2 天）

- [ ] WASM：`WasmRenderOptions.layout_intents` + `RenderResult.refinement_report`
- [ ] Server：`RenderRequestBody.layout_intents` + `X-Drawify-Refinement-Report` 响应头
- [ ] 集成测试

### Phase 3：Playground 集成（2–3 天）

- [ ] 前端 `LayoutIntentOverlay` 类型定义
- [ ] 意图编辑面板（节点选择 + 意图类型 + 增删改）
- [ ] 报告展示区
- [ ] 视觉验证

### Phase 4（暂不启动）：扩展意图

- [ ] `right_of`：`order_layers_weighted_median` 增加 order hint
- [ ] `same_rank`：network-simplex 约束传播
- [ ] `near`：Brandes-Kopf 坐标目标函数改造
- [ ] DSL `arrange` 语法

---

## 7. 性能影响评估

| 操作 | 复杂度 | 备注 |
|------|--------|------|
| 意图校验 | O(I + V + E) | I = 意图数，环检测 DFS |
| `build_graph_with_overlay` | O(V + E + I) | 额外 I 条意图边 |
| `greedy_cycle_reversal` 改造 | O(V + E + I) | 额外判断 reversible 标记 |
| `apply_pin` | O(1) per node | |
| `apply_align_*` | O(K log K) | K = 对齐节点数 |
| 重叠消除 | O(K²) 最坏 | K 通常 < 10 |
| `snap_layout_to_grid` pinned 跳过 | O(1) 查找 | HashSet |

**结论：** 性能影响可忽略（< 1ms），远小于布局算法本身。

---

## 8. 风险与缓解

| 风险 | 严重度 | 缓解 |
|------|--------|------|
| FAS 反转意图边破坏 `below` 语义 | 🔴 高 | `EdgeMeta.reversible` 标记，FAS 优先反转真实边 |
| 拓扑意图边引入环导致布局失败 | 🟡 中 | 注入前 DFS 环检测，拒绝引入环的意图 |
| 对齐后级联重叠 | 🟡 中 | 单轮消除，失败标记 `Partial`，不级联 |
| 穿障修正破坏对齐 | 🟡 中 | 首期仅观测，报告降级；后续可考虑联动 |
| 跨组意图在 Architecture V2 失效 | 🟡 中 | 首期仅支持组内意图，跨组标记 `Partial` |
| 向后兼容 | 🟢 低 | `compute_layout` / `compute_layout_with_plan` 签名不变；trait 默认方法不破坏既有实现 |
| Server 响应头过大 | 🟢 低 | `RefinementReport` 通常 < 2KB，header 可承载 |
| Architecture V2 两阶段布局复用 Sugiyama graph 构建路径，需确保 `architecture-v2` 的 Sugiyama 引擎也能正确消费 `EdgeMeta` | 🟡 中 | Phase 0 搭建 `EdgeMeta` 后，在 `architecture_v2` 模块的 graph 构建路径加编译验证；首期仅支持组内意图，避免触及 `two_phase` 的组级 rank 逻辑 |

---

## 9. 测试策略

### 9.1 单元测试

| 模块 | 测试内容 |
|------|----------|
| `intent/mod.rs` | 序列化/反序列化、overlay 默认值 |
| `intent/topology.rs` | 节点存在性校验、矛盾去重、环检测 |
| `intent/geometric.rs` | pin 跳过 snap、对齐均值计算、单轮重叠消除 |
| `sugiyama_v2/graph.rs` | `build_graph_with_overlay` 边 kind 标记、FAS 不反转意图边 |
| `grid_snap.rs` | `PinSet` 跳过逻辑、aligned 锚点 |

### 9.2 集成测试

| 场景 | 验证内容 |
|------|----------|
| flowchart + `below` | rank 排序符合意图 |
| flowchart + `below` 引入环 | 意图标记 `Conflicted`，布局不崩 |
| architecture + 组内 `below` | 组内 rank 符合意图 |
| architecture + 跨组 `below` | 意图标记 `Partial`，布局不崩 |
| `pin` + grid snap | 被 pin 节点不被 snap 移动 |
| `align_vertical` | x 中心一致，无重叠 |
| `align_vertical` 跨组 | 标记 `Partial`，仅同组对齐 |
| overlay = None | 行为与 `compute_layout_with_plan` 完全一致 |
| WASM `render_with_options` | overlay 透传，report 返回 |
| Server `/render` | overlay 透传，响应头含 report |

### 9.3 回归

- 运行 `showcase/` 下所有 architecture / flowchart 用例（无 overlay）
- 确认输出与优化前完全一致
- 新增带 overlay 的 showcase 用例

---

## 10. 与既有系统的关系

| 系统 | 影响 | 说明 |
|------|------|------|
| `LayoutPlan` | 无 | overlay 与 plan 正交，plan 仍由 `prepare()` resolve |
| `PreparedDiagram` | 无 | 不变异 diagram，overlay 独立透传 |
| `LayoutStrategy` trait | 增加默认方法 | 既有实现不破坏 |
| `EdgeRoutingStrategy` | 无 | diagram 未变异，索引契约保持 |
| `refine::run_refine` | 顺序明确 | 几何微调在前，穿障修正在后 |
| `grid_snap` | `snap_layout_to_grid` 增加 `pinned` 参数 | 其余 snap 逻辑不变 |
| `export_scene` | 增加 `refinement_report` 字段 | 编码器按需消费 |
| `RenderRequest` | 增加 `layout_overlay` 字段 | 默认 None |
| catalog | 无 | 意图是布局之上的可选层 |

---

## 11. 修订记录

| 版本 | 日期 | 说明 |
|------|------|------|
| 2.0 | 2026-06-17 | 合并重写：基于代码现状重评估原需求稿 v0.4 + 原实现稿 v1.0，修正架构契约冲突、收敛算法范围、拆分意图类型、消除命名冲突。原两份文档已删除。 |
| 2.1 | 2026-06-18 | 补充涟漪效应分析：Graph 边类型变更对 `build_dag` / `build_proper_layer_graph` 的波及（§5.2.2.1）；FAS 保护的两条实施路径（§5.2.2）；`inject_margins` 遗漏提醒（§5.4）；`export_scene` 中 plan 复用优化（§5.6）；Architecture V2 graph 复用风险（§8）。 |
