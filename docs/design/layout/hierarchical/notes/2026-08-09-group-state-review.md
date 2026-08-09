# Hierarchical · Group 现状与概念回顾

> 日期：2026-08-09 · 状态：**阅读笔记（不是契约）**  
> 目的：集中做 group 之前，对齐 DSL、概念词典、布局位置与现行缺口。  
> 裁定权仍在 [`architecture.md`](../architecture.md)、[`expectations.md`](../expectations.md)、[`dsl-spec.md`](../../../../specs/dsl-spec.md)。  
> 相关评审：[`2026-08-08-hier-review.md`](2026-08-08-hier-review.md) §2.6 / §3（组通道 fallback、与 yFiles 差距）。

---

## 0. 一句话总览

**DSL / 模型已成型；布局走 Weak（全图 Sugiyama）+ 边界 dummy + Channel Gate；组框仍是 finalize 后验 bbox，不是 Metric 求解变量。**

当前策略**不是** yFiles 式「组内递归子布局再当 macro 嵌回」。跨组路由靠 Gate / ScopeMask；组矩形不纯时会 `channel-group-fallback`，退回「当这图没有组」。

---

## 1. DSL：group 怎么设计

规范真源：[`docs/specs/dsl-spec.md`](../../../../specs/dsl-spec.md) §6；组间边：§5.7 / §7.6；ADR：[`004-group-anchor-nodes.md`](../../../adr/004-group-anchor-nodes.md)。

### 1.1 规范形态

```plotgram
group frontend {
    label: "Frontend"
    variant: primary          // 主题颜料；group 无 shape / archetype
    // layout: ...            // 规划中，引擎尚未读

    node a { label: "Web" }
    node b { label: "App" }
    a -> b                    // 组内边：两端须为本组后代

    group nested { ... }      // 嵌套深度不限制
}

// 跨组边：写在共同祖先或顶层，连的是 node，不是 group
a -> db
```

### 1.2 硬规则

| 规则 | 说明 |
|------|------|
| group / node id **全局唯一**、互不撞名 | — |
| 组内边两端须为当前组后代 node | — |
| **group 不能作边端点**（IR 层） | 见 model-boundary |
| 区到区连线 | `group_anchor` 或 `@group` 糖（ADR-004） |
| group **没有** `shape` / `icon` / `archetype` | 容器非节点几何；`archetypes.csv` 无 group 行 |

### 1.3 组级属性

| 键 | 状态 | 说明 |
|----|------|------|
| `label` | 已落地 | 提升为 `Group.label` |
| `variant` | 已落地 | 主题颜料；resolve 只应用 group 适用字段 |
| `layout:` | **规划** | 组内布局 hint；dsl-spec 标 planned，引擎未读 |
| `style.*` / `meta.*` | 已落地 / 忽略 | 样式 vs 渲染器忽略元数据 |

### 1.4 `group_anchor` 与 `@group` 糖

```plotgram
@frontend -> @backend { from_side: east, to_side: west }
// parse 展开为两个隐形 group_anchor node + 一条普通边
```

| 字段 | 含义 |
|------|------|
| `role: group_anchor` | 结构角色（封闭集：`entity` / `group_anchor`） |
| `host_group` | 所属 group id（必填） |
| `side` | `north` / `south` / `east` / `west`（必填） |
| `slot` | 可选；同侧多锚点序 |

设计意图（ADR-004）：

- 边永远只连 node；不引入 `Endpoint::Group`
- render **不绘制**锚点形体
- 几何：组框定稿后，由 `host_group` + side + slot **唯一派生**
- 布局：锚点**不应**单独占 Sugiyama 叶层/序（收缩进 host）

**现行缺口**：`group_anchor` 经 `all_node_ids()` 仍与普通 node 同等进入 rank/order/metric——ADR-004 的「不参与叶分层」**未闭合**。

### 1.5 与 PartitionGrid 的关系

泳道 / 矩阵是 **PartitionGrid**（ADR-008：`partition` / `cell_col` / `cell_row`），与 group **正交**。机制上可与「层内连续块」同构，但 Hier **尚未消费** partition；不要用 group「演」泳道。

---

## 2. 模型层

### 2.1 `Group`（`plotgram-model`）

```rust
pub struct Group {
    pub id: String,
    pub label: Option<String>,
    pub attrs: AttrMap,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,      // 仅组内边
    pub groups: Vec<Group>,    // 递归嵌套
}
```

顶层 `Graph`：`nodes` / `edges` / `groups` + 可选 `PartitionGrid`。

### 2.2 布局输出中的组框

```rust
pub struct GroupPlacement {
    pub id: String,
    pub frame: Rect,  // 成员包围盒 + padding
}
```

**现行**：`GroupPlacement` 由 `plotgram-engine::finalize` **后验**计算，**不是** Hier Metric 求解变量。

---

## 3. 概念词典

| 概念 | 是什么 | 写者 / 阶段 |
|------|--------|-------------|
| **Group** | 嵌套容器；成员 = nodes + 子 groups + 组内边 | 模型；布局只读结构 |
| **group_path** | 每个 node 的 root→leaf 组链 | Compose `graph_index` |
| **边界 dummy**（`GroupBoundary`） | 每层每组 Left/Right 零宽夹点，夹住同组成员 | Compose `boundary` → Order |
| **组框 (frame)** | 视觉矩形（含 padding） | **现行** finalize 后验 bbox；**目标** Metric VPSC 变量 |
| **Substrate** | rank × order 离散走廊骨架 | Channel derive |
| **Segment** | track 被组边界 `cut_line` 切开后的段 | Channel |
| **Gate** | 组边界上「合法进出」口：外侧 segment ↔ 内侧 segment 配对 | Channel derive 注册 |
| **ScopeMask** | 某条边允许走的 scope 集合（两端祖先链 ∪ 公共祖先） | Channel 搜索硬过滤 |
| **channel-group-fallback** | 组矩形不纯 / 路由失败 → 退回 root Substrate，Gate 全无 | Channel；现有 relaxation |
| **group_policy** | `weak`（默认，已落地）/ `strong-macro`（硬 `Unsupported`） | `params` |

### 3.1 Gate 是什么（重点）

Gate **不是** UI 控件，而是 **组边界上的合法穿行口**。

边要从组外进组内（或反过来），在离散 Substrate 上必须经 Gate；否则：

1. **构建期**：跨非法 scope 的 link 被拒绝  
2. **搜索期**：ScopeMask 硬过滤  

`GateCapacity::Fixed` 已删除；当前 Gate **无容量上限**。容量作为 Metric 缝宽预算（相 I 累计 demand → 相 II 撑开段长）**尚未落地**。

fallback 后 Gate 消失 ≈「布局当这图没有组」——边可任意穿组，交叉容易爆炸。

### 3.2 穿组三道防线

真源：[`architecture.md`](../architecture.md) §6.3。

1. **构建期**：跨非法 scope 的 link 拒绝（H2）  
2. **搜索期**：ScopeMask 硬过滤  
3. **检查期**：`verify_no_group_penetration` / Ink 层穿组 FAIL —— **尚未落地**（D₂ 遗留）

`verify_route_scope`（路径是否只走允许 scope）**已落地**（`channel/verify.rs`）。

### 3.3 三条不变量

真源：[`atlas-reference/group-invariants.md`](../atlas-reference/group-invariants.md)。

| 不变量 | 含义 | 现行 |
|--------|------|------|
| **Containment** | 成员 ⊂ 组框 | finalize bbox（弱；非求解保证） |
| **Sibling separation** | 兄弟组框不重叠 | **未做**（兄弟框可重叠） |
| **No penetration** | 边不穿组内，只经 Gate | H2 + ScopeMask 部分；L6 / Ink 穿透 verifier **未做** |

Atlas 曾在 post-Ink 用刚体平移推开兄弟组——新架构禁止下游推翻上游，应在 Metric 保证分离。

---

## 4. 布局管线：group 出现在哪里

```text
Parse / Expand (@group → group_anchor)
  → Compose
      graph_index     算 group_path
      rank            （尚无「组占连续层」硬约束）
      boundary        插 GroupBoundary Left/Right + 跨层对齐
      order           median / transpose + restore_group_clamps
      ports
  → Channel
      derive          组 (rank, order) 矩形 → cut_line → 注册 Gate
                      失败 → channel-group-fallback
      search          ScopeMask + 加权 Dijkstra
  → Metric            只写节点 / track；组框不进 VPSC
  → Ink               展开 ChannelPath + EscapePlan（零组决策）
  → finalize          成员 bbox 并集 → GroupPlacement.frame
```

### 4.1 分相位表

| 相位 | 子步骤 | group 相关行为 | 写者 |
|------|--------|----------------|------|
| Compose | `graph_index` | 为每个 node 算 `group_path` | — |
| Compose | `rank` | 无组连续层 / rank 区间硬约束 | LayerWriter |
| Compose | `boundary::insert_group_boundaries` | 每层每 group 插 Left/Right 零宽夹点；跨 rank 高权段对齐 order 区间 | OrderWriter（边界结构） |
| Compose | `order::order_layers` | `restore_group_clamps`；`group_boundary_weight` 软拉齐 | OrderWriter |
| Compose | `ports` | `group_anchor` 可走 `FixedOrder` | PortWriter |
| Channel | `derive_substrate` | 切 segment + 注册 Gate；失败 fallback | Channel（Gate **非**独立 Plan 字段） |
| Channel | `search` / `route_all` | ScopeMask；不可行则二次 fallback | Channel |
| Metric | BK + VPSC + track | **组框不进 VPSC** | CoordWriter |
| finalize | `group_frames` | 成员 + 嵌套组 frame 的 bbox 并集 + pad | **后验**（非 Metric） |
| Ink | `route` | 只展开拓扑；无组相关新决策 | InkWriter |

### 4.2 现行策略 = Weak

- 全图一套 Sugiyama  
- 组靠「层内被边界夹住 + 跨层软拉齐」保持连续块  
- 跨组路由靠 Gate  

`group_policy: strong-macro`（组内分层 → macro → 回填）绑定即 **Unsupported**。

### 4.3 边界 dummy 的用意

[`architecture.md`](../architecture.md) §8.3：

- **不改**交叉最小化内核  
- 用夹点 + 高权跨层段，让同组在相邻层的 order 区间对齐  
- 对齐失败 → `(rank, order)` 包围盒互咬 → `ForeignNodeInGroupRect` → **fallback**

这正是 [`2026-08-08-hier-review.md`](2026-08-08-hier-review.md) §2.6 因果链的核心：层内连续不够，必须跨层区间对齐，组通道才能启用。

### 4.4 `channel-group-fallback` 何时触发

| 场景 | 行为 | 可观测性 |
|------|------|----------|
| 组矩形不纯（`EmptyGroup` / `OverlappingGroups` / `ForeignNodeInGroupRect`） | 退回 `derive_root_substrate`；`used_gates=false` | `diagnostics.relaxations` |
| group-gate 路由不可行 | 同上二次 fallback | `route_all` |
| Ink 穿真实节点 + 曾用 gate | `CHANNEL_FORCE_ROOT` 重跑 | `mod.rs` |

fallback 后：`ScopeMask::unrestricted()`，Gate 边消失。

启用率（2026-08-09 对照表）：组 fixture **约 32/40** 启用 gate；**约 8/40** 仍 fallback（典型：多 namespace 跨层区间交叉换位）。

---

## 5. 与 yFiles / 期望的差距（组相关）

| 能力 | yFiles | 现行 | 差距 |
|------|--------|------|------|
| 组布局 | 递归子图 + 组框进求解 | Weak：全局 Sugiyama + 边界 dummy | 无递归子布局 |
| 组框几何 | 求解变量（padding / title demand） | finalize 后验 bbox | 非 VPSC；`group_sizing` / `group_align` bind 即拒 |
| 跨组路由 | 走廊 + gate 闭合 | Gate + ScopeMask（多数图）；部分 fallback | fallback 时等同无组；无穿透 verifier |
| StrongMacro | 有 | `Unsupported` | 未实现 |
| 组间分离 | 求解保证 | 无 sibling separation | 兄弟框可重叠 |
| `group_anchor` | 贴框、不参与分层 | 与普通 node 同等分层 | ADR-004 几何写权未闭合 |
| PartitionGrid | 泳道/矩阵 | 模型有，Hier 未消费 | 与 group 正交，均后置 |

路线图归属：组框写权 / 穿透 verifier → **D₂**；Channel Gate 基础 → **D1.2 已大部分落地**。见 [`roadmap.md`](../roadmap.md)。

---

## 6. 落地 vs 规划总表

| 领域 | 已落地 | 规划 / 缺口 |
|------|--------|-------------|
| DSL + 模型 | 嵌套 group、`label`/`variant`、`group_anchor` 字段、`@group` 展开 | 组级 `layout:`；anchor 不参与叶分层 |
| Compose 连续性 | 边界 dummy + `restore_group_clamps` + 跨层软对齐 | rank 连续层硬约束；`GatePlan` 进 Plan IR |
| Channel | cut_line + Gate + ScopeMask + `verify_route_scope` | `verify_no_group_penetration`；Gate 容量 → Metric |
| 组框 | finalize 后验 bbox | **D₂**：VPSC 组框变量、sibling separation |
| Policy | `weak` | `strong-macro`；`group_sizing` / `group_align` |
| 可观测性 | `channel-group-fallback` relaxation；`channel_used_gates` | hier_eval 对 fallback 率设硬门禁（评审建议） |

---

## 7. 关键代码与文档入口

### 7.1 文档

| 路径 | 职责 |
|------|------|
| `docs/specs/dsl-spec.md` | group DSL §6；anchor §5.7；`@group` §7.6 |
| `docs/design/adr/004-group-anchor-nodes.md` | 组间边经 anchor |
| `docs/design/model-boundary.md` | group 不作边端点 |
| `docs/design/layout/hierarchical/architecture.md` | §6.3 三道防线；§8 组与 partition |
| `docs/design/layout/hierarchical/atlas-reference/group-invariants.md` | containment / separation / penetration |
| `docs/design/layout/hierarchical/phases/ports-and-channel.md` | GatePlan 目标语义 |
| `docs/design/layout/hierarchical/phases/channel-d1.md` | D1.0–D1.2；Gate 归属 D1.2 |
| `docs/design/layout/hierarchical/notes/2026-08-08-hier-review.md` | fallback 因果链、启用率 |
| `docs/design/layout/hierarchical/roadmap.md` | D₂ 组框写权 |

### 7.2 模型 / 解析

| 路径 | 职责 |
|------|------|
| `crates/plotgram-model/src/graph.rs` | `Group` / `Node.role` / `host_group` |
| `crates/plotgram-model/src/result.rs` | `GroupPlacement` |
| `crates/plotgram-model/src/diagnostics.rs` | `channel_used_gates` |
| `crates/plotgram-parse/src/lower.rs` | group 降级；跨组边校验 |
| `crates/plotgram-parse/src/expand.rs` | `@group` → `group_anchor` |

### 7.3 布局

| 路径 | 职责 |
|------|------|
| `hierarchical/mod.rs` | 主管线；`CHANNEL_FORCE_ROOT`；`group_policy` |
| `hierarchical/compose/graph_index.rs` | `group_path` |
| `hierarchical/compose/boundary.rs` | 边界 dummy 插入与跨层对齐 |
| `hierarchical/compose/order.rs` | 定序 + `restore_group_clamps` |
| `hierarchical/model.rs` | `ElemKey::GroupBoundary` |
| `hierarchical/params.rs` | `group_policy` / `group_boundary_weight` |
| `hierarchical/channel/derive.rs` | 组切 Substrate；Gate；fallback |
| `hierarchical/channel/substrate.rs` | Segment / Gate IR |
| `hierarchical/channel/search.rs` | ScopeMask |
| `hierarchical/channel/route_all.rs` | 全图路由与 fallback |
| `hierarchical/channel/verify.rs` | `verify_route_scope` |
| `crates/plotgram-engine/src/finalize.rs` | **后验** `group_frames()` |

v1 Atlas（只读参考）：`crates/v1/plotgram-core/src/layout/atlas/channel/`。

---

## 8. 「基本可用」vs「当优化目标」（决策备忘）

集中做 group 时，建议拆开两档，避免和全局走廊优化缠在一起：

| | **基本可用**（建议先做） | **当优化目标**（可后置） |
|--|--------------------------|---------------------------|
| 要什么 | 组通道能开、少静默 fallback；包围盒/Scope 基本纯；穿透可证 | 组框进 VPSC、递归子布局、组内美学 J |
| 验收 | fallback 率、`channel_used_gates`、穿组 verifier | 框对齐、组内紧致、视觉打磨 |
| 和全局路由关系 | **前置基板** | 与 Channel 全局分配器正交、可后做 |

建议顺序（备忘，非契约）：

```text
1. 结构：跨层边界对齐，压掉 fallback（Gate 真生效）
2. 可证：verify_no_group_penetration + fallback 门禁可见
3. 写权：组框是否进 Metric（D₂）——框才有单写者
4. anchor：按 ADR-004 从叶分层摘掉，框定稿后再挂
── 之后再开全局走廊分配 / 交叉后处理 ──
```

全局走廊分配器可以等「基本可用」至少到第 1（最好到第 2）之后；否则密集组图上的 crossings 仍会被 fallback 淹没，路由层收益验不出来。

---

## 9. 与边路由缺口的关系（上下文）

[`2026-08-08-hier-review.md`](2026-08-08-hier-review.md) §3 边路由行：加权 `J` + Occupancy 交叉项 + bounded rip-up **已闭合**；仍缺 yFiles 级全局走廊分配器与边交叉后处理。

若 **group 尚未作为优化目标**，路由主线是「准全局分配 + 廊内 CM + VPSC nudge」，组通道失效图上的高交叉记为 residual。

若 **先集中把 group 基本可用做好**，则上述 residual 会明显缩小，再做全局路由更有验收意义。

本文只做回顾，不裁定二选一；顺序见 §8。
