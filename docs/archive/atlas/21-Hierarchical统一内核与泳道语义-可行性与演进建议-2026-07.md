# 21 - Hierarchical 统一内核与泳道语义：可行性分析与演进建议

> 日期：2026-07-25（2026-07-26 修订：剔除 DSL swimlane/table 第一需求）  
> 状态：讨论稿（待评审修订）  
> 范围：是否按 yFiles 思路做「巨大重构」——图种选 Scheme/Profile、布局内核集合、`group` 一等公民、内建路由与独立路由双模式；**核心是布局与路由体系**  
> 必保图种（已明确）：**architecture、flowchart、sequence、state、mindmap**  
> 目标：先冻结**可行性 / 必要性 / 推荐演进路径**，供后续讨论修改；**本文不是实施任务书**，也不授权立刻推倒现有 Recipe。  
> **非本期范围**：DSL 新引入 `swimlane` / `table` 等分区语法——可作远期产品能力，**不是**布局/路由收敛的前置或第一需求。  
> **后继**：本文的立场已由 [`22-Atlas 总纲`](22-Atlas下一代布局与路由架构-总纲-2026-07.md)（设计）与 [`23-Atlas 分阶段推进方案`](23-Atlas分阶段推进方案-2026-07.md)（执行）承接。  
> 前置阅读：  
> - [`12-多图类型布局共享内核与独立配方架构-2026-07.md`](../优化重构/12-多图类型布局共享内核与独立配方架构-2026-07.md)  
> - [`14-布局Recipe生命周期收敛方案-2026-07.md`](../优化重构/14-布局Recipe生命周期收敛方案-2026-07.md)  
> - [`16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md`](../优化重构/16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md)  
> - [`18-DiagramScheme-布局与路由总配方方案-2026-07.md`](../优化重构/18-DiagramScheme-布局与路由总配方方案-2026-07.md)  
> - [`19-DSL布局Hint与GroupConfig盘点与归零重规划-2026-07.md`](../优化重构/19-DSL布局Hint与GroupConfig盘点与归零重规划-2026-07.md)  
> - [`20-路由Recipe-Kernel实施审查与后续收敛计划-2026-07.md`](../优化重构/20-路由Recipe-Kernel实施审查与后续收敛计划-2026-07.md)  
> - [`布局与路由核心手册-2026-07.md`](../总结经验/布局与路由核心手册-2026-07.md)  
> - 现行结构可视化：[`layout-routing-architecture.html`](../architecture/layout-routing-architecture.html)

---

## 0. 一句话结论

> **设想里「group 一等公民 + 图种选 Scheme/Profile + 共享布局内核 + 内建/独立双路由」与现有 Kernel/Recipe/doc18 同向，应当做；「巨大重写 / 推倒重来」在写权未收口时不可取。**  
> **必保五图种下，布局内核至少应有：Hierarchical、Tree、Sequence、Circular。**  
> **`group` 一等公民；废弃用户侧 `group_frame`。**  
> **`flowchart` 与 `architecture` 保留为两个图类型，但共享同一套 Hierarchical 布局与同一套路由系统（差异只在 Scheme/Profile）。**  
> **核心战场是布局内核收敛与路由双模式契约；不是先扩 DSL 表面积。**

正确姿态：**收敛式重构（演进）**，不是推倒重来。

---

## 1. 背景：提出的设想（待评估原文归纳）

讨论中提出的目标形态大致为：

1. **不再按图形类型去设计布局算法**；Hierarchical 的典型应用靠 **profile**（方向、路由风格、间距、group policy）调成「流程图样子」；DSL 仍可声明流程图 / 架构图，据此 **自动选择预置 profile**。  
2. **主要提供** `HierarchicalLayout`、`TreeLayout`（初稿表述；下文按必保图种**扩充**为完整内核集合）。  
3. 与 yFiles 一样，提供 **内置路由** 或 **独立路由**。  
4. **（明确补充）`group` 成为一等公民**：进入图模型与 Hierarchical（及路由）原生语义，对齐 yFiles grouped graph。

对照 yFiles：流程图默认走 Hierarchical；架构感分层图可走 Hierarchical（recursive group）或 RecursiveGroup 式两阶段；独立 `EdgeRouter` 在节点冻结后补丁/增量路由；**group 是层次结构节点，布局算位置与尺寸，跨组边有 recursive/inter-edge 策略**。

> **已剔除（非第一需求）**：原先把「DSL 引入 `swimlane` / `table`」列为设想第 1 条。泳道类观感若需要，应先由 **group 层次 + Hierarchical 分区约束 / profile** 承接；新 DSL 分区语法后置，不阻塞布局/路由体系收敛。

---

## 1.1 必保图种与最低布局内核集合（修订）

产品明确需要支持的图类型：

| 图种 | 英文 / DSL | 布局需求本质 |
|------|------------|--------------|
| 架构图 | `architecture` | 强分组、macro 分层、条带/组框、跨组边 |
| 流程图 | `flowchart` | 主方向流、分层减交叉、可选分组分区 |
| 序列图 | `sequence` | 生命线 + 消息时间轴；**布局阶段产边** |
| 状态图 | `state` | 分层状态机 **或** 环形/分量布局 |
| 思维导图 | `mindmap` | 树形（径向 / 方向性） |

据此，**至少应存在的布局类型（内核）** 如下——不是「对外宣传只提两个」，而是 **工程上必须保有的核**：

| 布局内核 | 职责 | 默认服务图种 | 备注 |
|----------|------|--------------|------|
| **HierarchicalLayout** | 分层：layering → sequencing → drawing（± 内建边） | flowchart、architecture、state（Sugiyama 路径） | **主核**；architecture = StrongGroup / recursive-macro **profile**，不是第三套复制实现 |
| **TreeLayout** | 树形递归放置（含 radial / LTR / TTB 等 profile） | mindmap | 从现有 mindmap 抽出；一等公民 |
| **SequenceLayout** | 参与者轴 + 消息几何 | sequence | **不可**并入 Hierarchical；`produces_edge_geometry` |
| **CircularLayout** | 单圆 / 多连通分量圆环 | state（环形路径） | state 双路径之一；可薄封装 |

**不是独立「布局内核」、但是 Hierarchical/Compose 必须具备的能力：**

| 能力 | 归属 | 服务 |
|------|------|------|
| **Group 层次模型（一等公民）** | 图 IR + Hierarchical **原生** | 全图种凡声明 group 者；architecture 默认强策略 |
| StrongGroup / 两阶段组语义 | Hierarchical **group policy**（recursive / macro） | architecture；flowchart 可选 |
| 分区 / 条带约束 | Hierarchical **partition / track policy**（由 group 成员与 profile 驱动） | flowchart / 部分 architecture；**不依赖新 DSL 关键字** |
| group_sizing / align | LayoutProfile（原 strips 等宽等） | architecture 默认 equal；flowchart 默认 fit |

**明确不作为必保主核（可降级或后置）：**

| 项 | 说明 |
|----|------|
| ER 专用布局 | 若不在必保五图种内，可不单独保核；若保留产品入口，可挂 Hierarchical + ER profile |
| force-directed | 非必保；可不对外主推 |
| 独立「ArchitectureLayout」巨石 | **禁止**长期并存；语义进 Hierarchical StrongGroup |
| DSL `swimlane` / `table` | **非本期**；见文首「非本期范围」 |

```text
必保图种 5
    │
    ├─ flowchart ──────────► Hierarchical（flow profile；group 可选弱/强）
    ├─ architecture ───────► Hierarchical（StrongGroup + equal-track profile）
    ├─ state ──────────────► Hierarchical 或 Circular（scheme 二选一/可切换）
    ├─ mindmap ────────────► Tree
    └─ sequence ───────────► Sequence（内建边；group 需求弱）

横切一等公民：
    group hierarchy ──► Hierarchical / Router 原生消费（非后验画框）
```

---

## 1.2 Group 一等公民（对齐 yFiles）

### 目标语义

与 yFiles grouped graph 对齐，**`group` 不是装饰**：

| 维度 | 一等公民要求 |
|------|----------------|
| **图模型** | Group 进入稳定层次树（可嵌套）；成员、父子、跨组边可查询 |
| **布局** | Hierarchical **原生**处理：组内 layering policy、组框 **position + size**、组与普通节点同层对齐策略 |
| **跨组边** | 显式 Inter-edge / recursive-edge 策略（进出组侧、走廊），非事后猜 |
| **路由** | Standalone router 消费冻结的 group 障碍 / 门廊；Integrated 时 Drawing 已知 group 边界 |
| **图种差** | 差异只在 **group policy / profile**（architecture 强、flowchart 默认可弱），**不是**「只有 architecture 才认识 group」 |

### 与现状差距

| 现状 | 目标 |
|------|------|
| architecture：two_phase 强支持；flowchart：弱容器 / 分治另一套 | **统一 Group IR**；policy 分强弱 |
| group 框常后验包围或 Phase D / pipeline 整形 | 布局产物即含 group bounds；Compose 只做等宽/对齐等策略 |
| 跨组边散落在 corridor / escape / 穿组检测 | typed InterEdge + 统一进出组契约 |
| `group.layout` 仅 architecture 读 | 组内 arrange 进 Hierarchical/组内子核，图种共用 |

### Group vs 尺寸策略（Compose）

| 概念 | 一等吗 | 关系 |
|------|--------|------|
| **group** | **是**（容器层次） | 子系统 / 域 / 嵌套边界；布局与路由的原生输入 |
| **group_sizing / align / density** | 策略层（profile） | 组间几何（等宽、对齐、疏密）；不替代 group 模型 |
| **DSL swimlane / table** | **本期不做** | 远期产品语法；本期用 group + partition/track policy 覆盖同类几何需求 |

### 必要性

**高必要。** 没有 group 一等公民，architecture 无法从「专用两阶段巨石」收敛到 Hierarchical profile，flowchart 的 group 也永远是二等；与 yFiles 体验和布局/路由统一写权都不对齐。

---

## 2. 现状摘要（评估基线）

> 注：自 2026-07 末 Phase 0–6 / Group 收敛 / doc 34 目录重构后，部分「缺口」已部分闭合（如 `FrozenNodeProduct`、group 写权棘轮、语义隔离门禁）。下表保留讨论稿视角；实施时以手册与现行代码为准。

### 2.1 已有资产（应保留）

| 资产 | 位置 / 状态 | 与设想关系 |
|------|-------------|------------|
| Coordinate Kernel | `layout/kernel/coordinate/` | Hierarchical Drawing 的坐标写者 |
| LayoutRecipe 外形 | `layout/kernel/recipe.rs` + `recipes/*` | 应收成 Hierarchical/Tree Recipe，而非删除 |
| Layered 引擎 | `layout/kernel/layered/`（原 `engines/layered`） | Hierarchical 内核雏形 |
| RoutingRecipe 骨架 | `layout/routing/recipe/`（6 家族） | 独立路由模式 |
| group / frame_spec | `recipes/frame_spec`；几何 Frame pass 已删 | 组间尺寸策略应进 profile，勿复活 Frame 生产链 |
| architecture two_phase | `recipes/architecture/two_phase/` | 强分组两阶段；语义应保留，实现可收敛 |
| DiagramScheme 方案 | doc18 | **尚未落地**；正是「图种 → 总配方」 |
| RoutingCoordinator | `layout/routing/coordinator.rs` | Standalone 编排入口；冻结契约已落地 |

### 2.2 关键差距（阻碍「立刻巨大重写」）

1. LayoutRecipe 生命周期仍可能部分空心（部分路径 `execute()` 绕过 compile/solve）。  
2. flowchart `group_divide` 与 architecture `two_phase` **双轨**，未共享 IntraGroup 契约。  
3. 节点/组写权已大幅收口，但仍需盯 route feedback / refine 边界，禁止破约推点。  
4. 正交路由体量大；`kernel/route` IR 与生产 `routing` 仍双轨（生产未切 LexA* 内核）。  
5. 无完整 DiagramScheme 产品层；`layout` 与 `edge_routing` 仍分选为主。  
6. 无独立 TreeLayout；mindmap 内嵌树形逻辑。  
7. **无内建路由**：默认布局后独立 route（sequence 例外）。  
8. **group 横切一等公民未完成**：强支持仍偏 architecture；flowchart 另一套；InterEdge 契约未统一进 Hierarchical IR。

在写权与双轨未进一步收口前做「推倒式重写」，会导致回归无法归因、与 AGENTS.md 写权红线冲突。

---

## 3. 必要性分析

### 3.1 高必要（建议纳入路线图）

| 设想点 | 必要性理由 |
|--------|------------|
| **`group` 一等公民（对齐 yFiles）** | 层次树 + 组框尺寸 + 跨组边进内核；消灭「仅 architecture 真懂 group」；Hierarchical 才能吃掉 two_phase |
| **flowchart / architecture 去双轨** | 两图种、**一套** Hierarchical + **一套** RoutingRecipe；差异只在 Scheme/Profile（见 §5.0） |
| 图种 → 预置 Scheme/Profile，算法不按图种复制 | 对接 doc12/18；降低 agent 认知负担 |
| 共享 Hierarchical 内核 | flowchart / arch / state-Sugiyama 同族同实现 |
| 内建路由 + 独立路由双模式 | 对齐 yFiles；独立模式已在建；缺内建 Drawing 风格路径 |

### 3.2 中必要（要做，但不能只靠旋钮）

| 设想点 | 说明 |
|--------|------|
| Profile 调成「流程图 / 架构图样子」 | **默认外观**可用 Profile；architecture 的 **macro rank、等宽 track、组当超节点** 必须是显式 **group policy**，不能假装「只调间距+正交」 |
| TreeLayout 一等公民 | 从 mindmap 抽出共享 Tree 核；mindmap = Tree + radial 等 profile |
| SequenceLayout / CircularLayout 保核 | 覆盖 sequence 与 state 环形路径；与 Hierarchical 并列，非「薄到可删」 |
| 分区 / 条带类观感 | 由 group + Hierarchical partition/track policy 表达；**不**先做新 DSL |

### 3.3 低必要 / 时机不当

| 设想点 | 判断 |
|--------|------|
| 「巨大重写 / 推倒重来」 | **时机错误**。与进行中的写权收口抢带宽，且浪费已投入的 Kernel/Recipe |
| DSL `swimlane` / `table` | **非第一需求**。不阻塞布局/路由收敛；远期再评估产品语法 |

### 3.4 必要性一句话

> 必要的是 **布局与路由体系收敛**（**group 一等公民**、**flowchart/arch 去双轨**、Scheme、四类布局内核、双路由）；  
> 不必要的是 **在写权未收口时做推倒式重写**，以及 **先扩 DSL 泳道/table 表面积**。

---

## 4. 可行性分析（对照当前代码）

### 4.1 图种只选 Profile，统一算法

| 项 | 评估 |
|----|------|
| 可行性 | **中高**（演进） |
| 正确表述 | **不再按图种复制算法实现；图种只选择 DiagramScheme（含 Layout/Routing/Group Profile）** |
| 做法 | 落地 doc18；`HierarchicalRecipe` 内用 profile 区分 flowchart / architecture-strong-group 等 path |
| 依赖 | doc14 Recipe 真拆分；two_phase 语义迁入 Hierarchical 的 StrongGroup path |
| 风险 | architecture 质量若无 macro/strips policy 会断崖 |

### 4.2 布局内核集合（扩充后）

| 项 | 评估 |
|----|------|
| 可行性 | **高**（多数已有雏形） |
| 必保四核 | Hierarchical、Tree、Sequence、Circular（§1.1） |
| Hierarchical | 吸收 flowchart + architecture StrongGroup + state 分层路径 |
| Tree | 从 `recipes/mindmap.rs` 抽核；mindmap = Tree profile |
| Sequence | 保留并收口为正式 SequenceLayout / Recipe；内建边 |
| Circular | 现有 circular 引擎一等化；state 环形 scheme 挂接 |
| 不可做 | 用 Hierarchical「模拟」sequence 消息轴；删除 Circular 后 state 只剩分层一档 |

### 4.3 内建路由 + 独立路由

| 项 | 评估 |
|----|------|
| 可行性 | **中**（分阶段） |
| Standalone | 继续 doc16/20；`FrozenNodeProduct` → RoutingRecipe（当前主路径） |
| Integrated | Hierarchical Drawing 输出边骨架 + 风格（ortho/polyline/…）；先 MVP 于 **无复杂 group 的 flowchart** |
| 切换 | Scheme.`routing_mode = Integrated \| Standalone` |
| 风险 | 两模式视觉不一致；Integrated 与现有正交质量对拍成本高 |

### 4.4 Group IR 与写权

| 项 | 评估 |
|----|------|
| 可行性 | **中高**（已部分落地） |
| 已有 | Group bounds / hierarchy、写权计数、FrozenNodeProduct、语义隔离门禁 |
| 仍缺 | flowchart/arch 统一 IntraGroup 契约；InterEdge 类型化；StrongGroup 吃掉 two_phase 实现 |
| 风险 | 合并时若丢掉 macro/equal-track，architecture 观感断崖 |

### 4.5 与最新改造的兼容性

| 改造 | 对设想的作用 |
|------|----------------|
| CoordinateKernel | **保留**为 Hierarchical 坐标唯一写者 |
| LayoutRecipe | **演进**为 Hierarchical / Tree / Sequence / Circular Recipe |
| RoutingRecipe + Coordinator | **直接服务** Standalone |
| DiagramScheme (doc18) | **加速落地**，不要另起第三套产品层 |
| group_frame / Frame 几何 | **废弃用户侧 frame**；能力进 scheme / group_sizing；勿复活 Frame 生产链 |
| two_phase | **语义保留**，实现并入 Hierarchical StrongGroup profile |

**兼容结论：** 设想与 Kernel/Recipe **同向**；冲突只来自「推倒重写」或「先扩 DSL」的执行方式。

---

## 5. 推荐目标架构（讨论用）

```text
DSL（现有图种 + group；不新增 swimlane/table 为前置）
  diagram_type + 可选 scheme 覆盖
  + group hierarchy（一等）
        │
        ▼
 PreparedGraph（含 GroupHierarchy IR）
        │
        ▼
 DiagramScheme（doc18）
  ├── layout_kernel: Hierarchical | Tree | Sequence | Circular
  ├── layout_profile: 方向、间距、**group policy**（recursive / strong-macro / sizing / align / partition）
  ├── routing_mode: Integrated | Standalone | BuiltinEdges（sequence）
  ├── routing_profile: 风格、通道、label band、**inter-edge 策略**
  └── （无独立 Compose 产品层；无 flowchart/arch 第二套布局或路由实现）
        │
        ├─ HierarchicalRecipe   ← flowchart 与 architecture **共用此核**
        │     原生消费 GroupHierarchy
        │     Layering → Sequencing → Drawing（含组框尺寸/对齐）
        │     差异仅 profile：flow vs strong-macro + group_sizing …
        │
        ├─ TreeRecipe / SequenceRecipe / CircularRecipe
        │
        └─ 同一套 RoutingRecipe 家族（Standalone / Integrated）
              freeze(nodes+groups) → route → label
              sequence: BuiltinEdges
              Finalize: 可选像素 snap
```

### 5.0 正式立场：flowchart / architecture 去双轨

**两个图类型，一套布局系统，一套路由系统。**

| 保留 | 取消 |
|------|------|
| `diagram flowchart` / `diagram architecture` 两个图种 | `recipes/flowchart` 与 `recipes/architecture` **两套互不相通的布局实现**长期并存 |
| 各自默认 Scheme / Profile（观感与 group 强弱不同） | architecture 专用正交后处理链、flowchart 另一套路由例外（除 scheme 声明的差异外） |
| 同一 `HierarchicalRecipe` + 同一 `RoutingRecipe` 注册表 | `group_divide` vs `two_phase` 双分治内核 |

```text
diagram_type
  flowchart ──scheme──► Scheme(hierarchical-flow-…)
  architecture ─scheme─► Scheme(hierarchical-arch-…)
                              │
                              ▼
                    HierarchicalRecipe（唯一分层实现）
                              │
                              ▼
                    RoutingRecipe（同一家族；mode/profile 可不同默认）
```

**允许的差异（只在 Scheme/Profile/Intent）：**

- group policy：flow 默认弱/可选；arch 默认 StrongGroup + macro  
- group_sizing / group_align / density / partition-track  
- 默认 `direction`、默认 routing_mode / orthogonal 参数  
- 结构分析器不同（spine / hub 等）→ 编译进同一 Coordinate / Layered IR  

**不允许的差异：**

- 两套 layered engine、两套坐标写权、两套正交主链  
- 「只有 architecture 认识 group」  
- 图种名分支散落在 Kernel 内（`if diagram_type == Architecture`）

迁移：two_phase / group_divide **语义**迁入 Hierarchical group policy；**实现**合并后删除双轨目录。

### 5.1 必保图种与 Scheme / 内核（必须写清）

| 图种（必保） | 默认 Scheme（示意） | 布局内核 |
|--------------|---------------------|----------|
| flowchart | hierarchical-flow-ortho | **Hierarchical**（与 arch **同实现**） |
| architecture | hierarchical-arch-equal-track-ortho | **Hierarchical**（StrongGroup + group sizing） |
| state | hierarchical-state **或** circular-state | **Hierarchical** 或 **Circular** |
| mindmap | tree-radial-organic | **Tree** |
| sequence | sequence-builtin-edges | **Sequence** |

可选 / 非必保（若产品保留入口）：

| 图种 | 默认挂接 |
|------|----------|
| er | Hierarchical + ER profile（非第五必保核） |
| custom | 显式 scheme 或继承 flowchart |

**图种不消失；flowchart 与 architecture 共享 Hierarchical + 路由家族；另保留 Tree / Sequence / Circular 服务其它必保图种。**

**路由搭配（示意）：**

| 图种 | 默认路由模式 |
|------|----------------|
| flowchart / architecture | **同一** orthogonal RoutingRecipe；默认 profile 可不同 |
| state + Hierarchical | Standalone orthogonal 或 circular edges |
| state + Circular | Standalone circular |
| mindmap | Standalone organic |
| sequence | BuiltinEdges（无独立 Router） |

### 5.2 还要不要 `group_frame` / Compose？

**产品层结论（建议）：用户可见的 `group_frame` DSL 可以不要；独立「Compose 产品概念」也可以不要。**  
今日 `group_frame` 解决的真需求保留，但用 **layout/routing 控制面内的 profile** 承接，避免「第三套控制面」，也**不**引入新的 `swimlane`/`table` DSL 作为替代前提。

#### 5.2.1 今天 group_frame 实际在干什么

| 能力 | 例子 | 本质 |
|------|------|------|
| 场景短名 | strips / lanes / stages / tiles | 预设打包 |
| 同级等宽/等高 | `track: equal` | **组框尺寸策略** |
| 交叉轴对齐 | `cross: start/center` | **组框对齐策略** |
| 组间间距 | `gap` | **间距 profile** |
| 边框共线 | `border: shared` | **装饰/框几何微调** |
| 矩阵摆放 | `matrix` / tiles | **分区/表格结构**（布局约束，不必新 DSL） |
| 泳道感 | lanes | **分区约束**（group 成员 + track policy） |
| snap | 组框量化 | **finalize** |

问题：把 **结构分区、尺寸策略、装饰、收尾量化** 揉进一个 DSL 入口，和即将一等化的 `group` 叠床架屋。

#### 5.2.2 可行替代方案对比

| 方案 | 做法 | 优点 | 缺点 | 评价 |
|------|------|------|------|------|
| **A. 保留 group_frame** | 继续唯一组间入口 | 迁移成本低 | 与 group 双轨；agent 难学 | 不推荐作终态 |
| **B. 独立 Compose 产品层** | 改名 Compose，仍对外暴露 | 概念清晰一点 | 仍是第三控制面 | 过渡可以，终态偏冗余 |
| **C. 拆入 layout profile（推荐）** | 见下表 | 对齐 yFiles；控制面变少；不扩 DSL | 要迁移 showcase | **推荐终态** |
| **D. 全部变 intent** | 只有 `intent { equal_width: … }` | 极灵活 | 日常太啰嗦；缺默认 scheme | 作高级补充，不作唯一入口 |
| **E. 新 DSL swimlane/table** | 一等分区语法 | 语义直白 | **扩表面积，非第一需求** | **本期不做** |

#### 5.2.3 推荐方案 C：拆掉 frame DSL，能力进布局体系

```text
原 group_frame
  ├─ lanes / tiles / 责任分区     →  group 成员归属 + Hierarchical partition/track policy
  ├─ stages 纵向堆               →  Hierarchical 流向 + group policy（或 scheme 默认）
  ├─ strips 等宽条带             →  LayoutProfile.group_sizing = equal_siblings
  │                                + group_align（≈ yFiles groupAlignmentPolicy）
  ├─ gap / 疏密                  →  LayoutProfile.spacing / density
  ├─ border: shared              →  渲染/theme 或极轻的 group_chrome 策略（非布局主链）
  └─ snap                        →  FinalizePolicy（全局，不挂在 frame 上）
```

示意（DSL，非冻结语法；**无** swimlane 关键字）：

```plotgram
diagram architecture {
    // 不再写 group_frame: strips
    // scheme 默认已含：strong-group + equal sibling track + shared chrome
    config { scheme: architecture-default }

    group backend "后端" { … }
    group data "数据" { … }
}

diagram flowchart {
    config { scheme: flowchart-default }
    // 分区感：用 group（或未来 intent）表达成员归属；由 Hierarchical policy 排布
    group lane_approval "审批" { entity a; entity b }
}
```

高级覆盖（仍不必叫 frame）：

```plotgram
config {
    scheme: architecture-default
    group_sizing: equal      // 或 fit
    group_align: center      // start | center | end
    density: spacious        // 映射 gap 等
}
```

这与 yFiles 更接近：**分组进布局内核；等宽/对齐是 layouter 选项；没有单独的 group_frame 产品名；也不先发明第二套分区 DSL。**

#### 5.2.4 实现上还要不要叫 Compose？

| 层次 | 建议 |
|------|------|
| **对外 DSL / 文档** | **不出现** `group_frame` / `Compose` 作为用户概念 |
| **对内代码** | 允许短暂保留 sizing pass（由 profile 驱动），迁完并进 Hierarchical Drawing |
| **Scheme** | architecture 默认 `group_sizing=equal`；flowchart 默认 `fit`——**取代 strips/stages 短名心智** |

#### 5.2.5 风险

| 风险 | 缓解 |
|------|------|
| showcase 大量 `group_frame:` | 适配器：旧语法 → profile 字段；收口后再删 |
| 「少了一个旋钮」恐惧 | 能力都在布局 profile；入口换到 scheme + 少量 group_* |
| border shared 没处放 | 先放渲染侧或 `group_chrome: shared`；不阻塞布局收敛 |
| 「没有 swimlane DSL 怎么做泳道」 | 本期用 group + partition policy；产品语法后置评估 |

### 5.3 双路由模式契约

| 模式 | 节点写权 | 边写权 | 适用 |
|------|----------|--------|------|
| Integrated | Drawing 内可写（freeze 前） | Drawing 主写 | 简单～中等流程图、风格与层强绑定 |
| Standalone | freeze 后 **零** | RoutingRecipe | 复杂正交、增量、architecture 走廊 |

禁止：Standalone 后再推节点（与手册 ★、doc16 一致）。  
Integrated 不足空间：仍走 SpacingDemand → 有界 re-solve（doc18），而非无限互推。

---

## 6. 推荐演进路线（非巨大重写）

> 退出判据未满足前，不开启「删除旧 Recipe 目录」类破坏性步骤。  
> **路线图只服务布局/路由体系**；不插入「先做 DSL swimlane」阶段。

| 阶段 | 内容 | 退出判据（草案） |
|------|------|------------------|
| **P0** | 继续写权收口：节点/组 freeze、RoutingRecipe 真 IR、禁 route 后破约推点 | doc20 / 手册写权项可测关闭 |
| **P1** | 落地 DiagramScheme + **GroupHierarchy IR**（两图种共用读取） | scheme 可跑；group IR 单测/确定性 |
| **P2** | Hierarchical 原生 group policy；two_phase / group_divide **语义迁入、实现删双轨** | architecture/flowchart 含 group 样例可解释对齐 |
| **P3** | TreeKernel / Circular 一等化；SequenceRecipe 契约钉死；**InterEdge 契约**进路由 | 五图种均有明确内核；跨组边策略可测 |
| **P4** | Integrated routing MVP（简单 flowchart 正交） | 与 Standalone 对拍清单；模式可切换 |
| **P5** | 文档与 DSL：废弃用户侧 `group_frame`；能力迁入 scheme / group_sizing / partition policy；旧语法适配器 | language-spec 更新；迁移指南 |

**明确不做（本讨论稿立场）：**

- 现在删除 `recipes/architecture` / `recipes/flowchart` 另起仓库级重写  
- 删除 Sequence / Circular，或强迫 mindmap / sequence 走 Hierarchical  
- **以 DSL `swimlane` / `table` 作为布局收敛前置**  
- 以「去水印 / 本地 render 配额」等与本主题无关的商业手段绑定布局重构  

---

## 7. 风险与缓解

| 风险 | 缓解 |
|------|------|
| architecture 质量断崖 | StrongGroup profile 必须显式编码 macro + equal-track；禁止「只改默认间距」冒充合并完成 |
| 合并后偷偷留 diagram_type 分支 | 代码审查红线：Kernel/Router 无 DiagramType；差异只在 Recipe compile / Scheme |
| Integrated / Standalone 观感分裂 | 同一 Scheme 下两种 mode 的对拍集；文档写清适用边界 |
| 范围膨胀成「第二个 yFiles」 | **四核封顶主路径**（Hierarchical/Tree/Sequence/Circular）；不追力导向；**不先扩 DSL** |
| 与 doc14/16 抢进度 | **P0 硬门槛**；Scheme 可设计并行，破坏性合并等 P0 |
| 泳道观感无人承接 | group + partition/track policy；不把缺口当成必须立刻加 DSL 的理由 |

---

## 8. 决策建议（供评审勾选）

请在后续讨论中明确：

- [ ] **采纳**：group 一等公民 + DiagramScheme + **四布局内核**（§1.1–§1.2）+ 双路由（§5–§6）——**布局与路由体系为主轴**  
- [x] **正式立场：废弃用户侧 `group_frame` / Compose 产品概念**（§5.2）  
  - 能力拆入：`group_sizing` / `group_align` + density/spacing + partition/track policy + finalize snap  
  - 迁移期可保留旧语法 → profile 适配器；终态 DSL/文档不再出现 `group_frame`  
  - 对内代码可暂留 GroupSizingPass，由 scheme/profile 驱动，不对外暴露 Compose  
- [x] **正式立场：flowchart / architecture 去双轨**（§5.0）  
  - **保留**两个图类型（语义与默认 Scheme 不同）  
  - **取消**两套布局实现、两套路由主链；共用 HierarchicalRecipe + 同一 RoutingRecipe 家族  
  - 差异只在 Scheme/Profile/Intent；禁止 Kernel 内 `diagram_type` 分叉  
  - 迁移：`two_phase` / `group_divide` 语义并入后删除双轨目录  
- [x] **正式立场：DSL `swimlane` / `table` 非第一需求**（文首 / §3.3）  
  - 不作为路线图前置；远期产品语法另议  
- [ ] **拒绝**：立刻巨大重写 / 推倒重来（写权未收口时）  
- [ ] **确认必保图种**：architecture / flowchart / sequence / state / mindmap（已写入 §1.1）  
- [ ] **修订**：……（讨论后填）  
- [ ] **下一步文档**：是否需要单独开「HierarchicalRecipe Profile 表（flowchart vs architecture）」规格页  

---

## 9. 与既有文档的边界

| 文档 | 关系 |
|------|------|
| doc12 / 14 | Hierarchical 共享内核与 Recipe 生命周期；本文要求 **演进接入**，不替代其阶段任务 |
| doc16 / 20 | Standalone 路由收口；本文 Integrated 为后续增量 |
| doc18 | Scheme 产品层；本文视为 **必须落地的前置** |
| doc19 | Hint/group 归零；**本文已定：废弃 group_frame**，能力由 scheme / group sizing / partition policy 承接 |
| doc 34 / 架构 HTML | 现行目录分层快照；本文谈目标演进，不替代现状地图 |

本文 **不** 重复正交 Solver 内部阶段设计，也 **不** 展开新 DSL BNF（泳道/table 语法后置）。

---

## 10. 附录：设想条目速查

| # | 设想 | 必要性 | 可行性 | 建议 |
|---|------|--------|--------|------|
| 1 | **group 一等公民** | **高** | 中高 | **做**；统一 IR + Hierarchical 原生 policy |
| 2 | **flowchart/arch 去双轨** | **高** | 中 | **做**；两图种一布局一路由（§5.0） |
| 3 | 图种 → profile，不按图种复制算法 | 高 | 中高 | 做（Scheme）；保留图种语义 |
| 4 | 布局内核集合 | 高 | 高 | **四核**：Hierarchical / Tree / Sequence / Circular |
| 5 | 内建 + 独立路由 | 高 | 中 | 做（Standalone 先，Integrated MVP） |
| — | DSL swimlane/table | 低（时机） | 高（增量） | **本期不做**；非布局/路由收敛前置 |
| — | 巨大重写 | 低 | 低（时机） | 不做 |

---

## 11. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-07-25 | 初稿：可行性/必要性分析与演进建议，供讨论修改 |
| 2026-07-25 | 修订：明确必保五图种；布局内核扩充为 Hierarchical / Tree / Sequence / Circular（§1.1） |
| 2026-07-25 | 修订：明确 **group 一等公民**（对齐 yFiles）；新增 §1.2；更新 Scheme/路线图 |
| 2026-07-25 | 修订：建议 **取消用户侧 group_frame/Compose**；能力拆入 profile / sizing（§5.2） |
| 2026-07-25 | **决策勾选**：正式立场「废弃用户侧 group_frame / Compose」（§8 已勾选） |
| 2026-07-25 | **决策勾选**：正式立场「flowchart / architecture 去双轨」（§5.0 / §8）；并修复 §5 重复残片 |
| 2026-07-26 | **修订**：删除「DSL 引入 swimlane/table 为第一需求」；路线图与 §5.2 改为布局/路由体系承接分区观感；§8 勾选「swimlane/table 非第一需求」 |
