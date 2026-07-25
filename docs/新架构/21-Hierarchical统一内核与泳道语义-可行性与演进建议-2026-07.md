# 21 - Hierarchical 统一内核与泳道语义：可行性分析与演进建议

> 日期：2026-07-25  
> 状态：讨论稿（待评审修订）  
> 范围：是否按 yFiles 思路做「巨大重构」——DSL 一等泳道/table、图种选 Scheme/Profile、布局内核集合、内建路由与独立路由双模式  
> 必保图种（已明确）：**architecture、flowchart、sequence、state、mindmap**  
> 目标：先冻结**可行性 / 必要性 / 推荐演进路径**，供后续讨论修改；**本文不是实施任务书**，也不授权立刻推倒现有 Recipe。  
> 前置阅读：  
> - [`12-多图类型布局共享内核与独立配方架构-2026-07.md`](../优化重构/12-多图类型布局共享内核与独立配方架构-2026-07.md)  
> - [`14-布局Recipe生命周期收敛方案-2026-07.md`](../优化重构/14-布局Recipe生命周期收敛方案-2026-07.md)  
> - [`16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md`](../优化重构/16-路由Recipe-Kernel与求解器架构改造方案-2026-07.md)  
> - [`18-DiagramScheme-布局与路由总配方方案-2026-07.md`](../优化重构/18-DiagramScheme-布局与路由总配方方案-2026-07.md)  
> - [`19-DSL布局Hint与GroupConfig盘点与归零重规划-2026-07.md`](../优化重构/19-DSL布局Hint与GroupConfig盘点与归零重规划-2026-07.md)  
> - [`20-路由Recipe-Kernel实施审查与后续收敛计划-2026-07.md`](../优化重构/20-路由Recipe-Kernel实施审查与后续收敛计划-2026-07.md)  
> - [`布局与路由核心手册-2026-07.md`](../总结经验/布局与路由核心手册-2026-07.md)

---

## 0. 一句话结论

> **设想里「泳道一等语义 + group 一等公民 + 图种选 Scheme/Profile + 共享内核 + 内建/独立双路由」与现有 Kernel/Recipe/doc18 同向，应当做；「巨大重写 / 推倒重来」在写权未收口时不可取。**  
> **必保五图种下，布局内核至少应有：Hierarchical、Tree、Sequence、Circular。**  
> **`group` 一等公民；废弃用户侧 `group_frame`。**  
> **`flowchart` 与 `architecture` 保留为两个图类型，但共享同一套 Hierarchical 布局与同一套路由系统（差异只在 Scheme/Profile）。**

正确姿态：**收敛式重构（演进）**，不是推倒重来。

---

## 1. 背景：提出的设想（待评估原文归纳）

讨论中提出的目标形态大致为：

1. **DSL** 引入 `swimlane` / `table` 等直接语义，支撑泳道类需求。  
2. **不再按图形类型去设计布局算法**；Hierarchical 的典型应用靠 **profile**（方向、路由风格、间距、泳道）调成「流程图样子」；DSL 仍可声明流程图 / 架构图，据此 **自动选择预置 profile**。  
3. **主要提供** `HierarchicalLayout`、`TreeLayout`（初稿表述；下文按必保图种**扩充**为完整内核集合）。  
4. 与 yFiles 一样，提供 **内置路由** 或 **独立路由**。  
5. **（明确补充）`group` 成为一等公民**：进入图模型与 Hierarchical（及路由）原生语义，对齐 yFiles grouped graph。

对照 yFiles：流程图默认走 Hierarchical；架构感分层图可走 Hierarchical（recursive group）或 RecursiveGroup 式两阶段；独立 `EdgeRouter` 在节点冻结后补丁/增量路由；**group 是层次结构节点，布局算位置与尺寸，跨组边有 recursive/inter-edge 策略**。

---

## 1.1 必保图种与最低布局内核集合（修订）

产品明确需要支持的图类型：

| 图种 | 英文 / DSL | 布局需求本质 |
|------|------------|--------------|
| 架构图 | `architecture` | 强分组、macro 分层、条带/组框、跨组边 |
| 流程图 | `flowchart` | 主方向流、分层减交叉、可选泳道 |
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
| Swimlane / Table | DSL 语义 → Hierarchical 分区约束 | flowchart / 部分 architecture；**与 group 并列，不互相替代** |
| group_sizing / align | LayoutProfile（原 strips 等宽等） | architecture 默认 equal；flowchart 默认 fit |

**明确不作为必保主核（可降级或后置）：**

| 项 | 说明 |
|----|------|
| ER 专用布局 | 若不在必保五图种内，可不单独保核；若保留产品入口，可挂 Hierarchical + ER profile |
| force-directed | 非必保；可不对外主推 |
| 独立「ArchitectureLayout」巨石 | **禁止**长期并存；语义进 Hierarchical StrongGroup |

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
    swimlane / table ──► 分区语义（可与 group 组合）
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

### Group vs Swimlane vs Compose

| 概念 | 一等吗 | 关系 |
|------|--------|------|
| **group** | **是**（容器层次） | 子系统 / 域 / 嵌套边界 |
| **swimlane / table** | **是**（分区归属） | 责任道 / 表格格；成员可同时属于 lane，也可再包在 group 内 |
| **group_frame / Compose** | 策略层 | 组间几何（strips 等宽等）；不替代 group 模型 |

### 必要性

**高必要。** 没有 group 一等公民，architecture 无法从「专用两阶段巨石」收敛到 Hierarchical profile，flowchart 的 group 也永远是二等；与 yFiles 体验和 agent「声明边界」都不对齐。

---

## 2. 现状摘要（评估基线）

### 2.1 已有资产（应保留）

| 资产 | 位置 / 状态 | 与设想关系 |
|------|-------------|------------|
| Coordinate Kernel | `layout/kernel/coordinate/` | Hierarchical Drawing 的坐标写者 |
| LayoutRecipe 外形 | `layout/kernel/recipe.rs` + `recipes/*` | 应收成 Hierarchical/Tree Recipe，而非删除 |
| Layered 引擎 | `layout/engines/layered/` | Hierarchical 内核雏形 |
| RoutingRecipe 骨架 | `layout/routing/recipe/`（6 家族） | 独立路由模式 |
| group_frame | `layout/group/frame/` | lanes/stages/strips/tiles **几何预设**，非泳道语义 |
| architecture two_phase | `recipes/architecture/two_phase/` | 强分组两阶段；语义应保留，实现可收敛 |
| DiagramScheme 方案 | doc18 | **尚未落地**；正是「图种 → 总配方」 |

### 2.2 关键差距（阻碍「立刻巨大重写」）

1. LayoutRecipe 生命周期仍部分空心（`execute()` 绕过 compile/solve）。  
2. flowchart `group_divide` 与 architecture `two_phase` **双轨**，未共享 IntraGroup 契约。  
3. 节点写权未单一：GroupFrame / SpaceBudget / route feedback / refine 仍可能动点。  
4. 正交路由仍大量 lift 旧几何；`PreparedRoutingInput` 未成真实消费 API（见 doc20）。  
5. 无 `FrozenNodeProduct` 类型级冻结边界。  
6. 无 DiagramScheme；`layout` 与 `edge_routing` 仍分选。  
7. 无独立 TreeLayout；mindmap 内嵌树形逻辑。  
8. **无内建路由**：默认布局后独立 route（sequence 例外）。  
9. **group 非横切一等公民**：强支持绑在 architecture two_phase；flowchart 另一套；框与跨组边未统一进 Hierarchical IR。

在以上未收口前做「推倒式重写」，会导致回归无法归因、与 AGENTS.md 写权红线冲突。

---

## 3. 必要性分析

### 3.1 高必要（建议纳入路线图）

| 设想点 | 必要性理由 |
|--------|------------|
| DSL 一等 `swimlane` / `table` | 现 `group_frame: lanes` 是组间整形，不是「节点属于第几道」；BPMN/职责分区、agent 意图都需要语义级归属 |
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

### 3.3 低必要 / 时机不当

| 设想点 | 判断 |
|--------|------|
| 「巨大重写 / 推倒重来」 | **时机错误**。与进行中的写权收口抢带宽，且浪费已投入的 Kernel/Recipe |

### 3.4 必要性一句话

> 必要的是 **产品与架构收敛**（**group 一等公民**、**flowchart/arch 去双轨**、泳道语义、Scheme、四类布局内核、双路由）；  
> 不必要的是 **在写权未收口时做推倒式重写**。

---

## 4. 可行性分析（对照当前代码）

### 4.1 DSL：`swimlane` / `table`

| 项 | 评估 |
|----|------|
| 可行性 | **高**（增量） |
| 做法 | 解析为 Partition / Swimlane Intent → 编译进 Hierarchical 的 layer/sequence 约束或 Compose 格子；与现有 `group` 一期并存 |
| 依赖 | Intent 编译链（doc19）、Compose 层清晰写权 |
| 风险 | 与 `group` / `group_frame` 语义重叠；需迁移表与 deprecate 计划 |

### 4.2 图种只选 Profile，统一算法

| 项 | 评估 |
|----|------|
| 可行性 | **中高**（演进） |
| 正确表述 | **不再按图种复制算法实现；图种只选择 DiagramScheme（含 Layout/Routing/Compose Profile）** |
| 做法 | 落地 doc18；`HierarchicalRecipe` 内用 profile 区分 flowchart / architecture-strong-group / swimlane 等 path |
| 依赖 | doc14 Recipe 真拆分；two_phase 语义迁入 Hierarchical 的 StrongGroup path |
| 风险 | architecture 质量若无 macro/strips policy 会断崖 |

### 4.3 布局内核集合（扩充后）

| 项 | 评估 |
|----|------|
| 可行性 | **高**（多数已有雏形） |
| 必保四核 | Hierarchical、Tree、Sequence、Circular（§1.1） |
| Hierarchical | 吸收 flowchart + architecture StrongGroup + state 分层路径 |
| Tree | 从 `recipes/mindmap.rs` 抽核；mindmap = Tree profile |
| Sequence | 保留并收口为正式 SequenceLayout / Recipe；内建边 |
| Circular | 现有 circular 引擎一等化；state 环形 scheme 挂接 |
| 不可做 | 用 Hierarchical「模拟」sequence 消息轴；删除 Circular 后 state 只剩分层一档 |

### 4.4 内建路由 + 独立路由

| 项 | 评估 |
|----|------|
| 可行性 | **中**（分阶段） |
| Standalone | 继续 doc16/20；`FrozenNodeProduct` → RoutingRecipe（当前主路径） |
| Integrated | Hierarchical Drawing 输出边骨架 + 风格（ortho/polyline/…）；先 MVP 于 **无复杂 group 的 flowchart** |
| 切换 | Scheme.`routing_mode = Integrated \| Standalone` |
| 风险 | 两模式视觉不一致；Integrated 与现有正交质量对拍成本高 |

### 4.5 与最新改造的兼容性

| 改造 | 对设想的作用 |
|------|----------------|
| CoordinateKernel | **保留**为 Hierarchical 坐标唯一写者 |
| LayoutRecipe | **演进**为 Hierarchical / Tree / Sequence / Circular Recipe |
| RoutingRecipe | **直接服务** Standalone |
| DiagramScheme (doc18) | **加速落地**，不要另起第三套产品层 |
| group_frame | **迁移**进 swimlane 语义或 Compose，勿先砸光 |
| two_phase | **语义保留**，实现并入 Hierarchical StrongGroup profile |

**兼容结论：** 设想与 Kernel/Recipe **同向**；冲突只来自「推倒重写」的执行方式。

---

## 5. 推荐目标架构（讨论用）

```text
DSL
  diagram_type + 可选 scheme 覆盖
  + group hierarchy（一等）
  + swimlane/table/intent …
        │
        ▼
 PreparedGraph（含 GroupHierarchy IR）
        │
        ▼
 DiagramScheme（doc18）
  ├── layout_kernel: Hierarchical | Tree | Sequence | Circular
  ├── layout_profile: 方向、间距、**group policy**（recursive / strong-macro / sizing / align）
  ├── routing_mode: Integrated | Standalone | BuiltinEdges（sequence）
  ├── routing_profile: 风格、通道、label band、**inter-edge 策略**
  └── （无独立 Compose；无 flowchart/arch 第二套布局或路由实现）
        │
        ├─ HierarchicalRecipe   ← flowchart 与 architecture **共用此核**
        │     原生消费 GroupHierarchy
        │     Layering → Sequencing → Drawing（含组框尺寸/对齐）
        │     差异仅 profile：flow vs strong-macro + group_sizing …
        │     Swimlane/Table → 分区约束
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
- group_sizing / group_align / density  
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
今日 `group_frame` 解决的真需求保留，但用 **更干净的三处表达** 承接，避免「第三套控制面」。

#### 5.2.1 今天 group_frame 实际在干什么

| 能力 | 例子 | 本质 |
|------|------|------|
| 场景短名 | strips / lanes / stages / tiles | 预设打包 |
| 同级等宽/等高 | `track: equal` | **组框尺寸策略** |
| 交叉轴对齐 | `cross: start/center` | **组框对齐策略** |
| 组间间距 | `gap` | **间距 profile** |
| 边框共线 | `border: shared` | **装饰/框几何微调** |
| 矩阵摆放 | `matrix` / tiles | **分区/表格结构** |
| 泳道感 | lanes | **分区语义**（更像 swimlane） |
| snap | 组框量化 | **finalize** |

问题：把 **结构分区、尺寸策略、装饰、收尾量化** 揉进一个 DSL 入口，和即将一等化的 `group` / `swimlane` 叠床架屋。

#### 5.2.2 可行替代方案对比

| 方案 | 做法 | 优点 | 缺点 | 评价 |
|------|------|------|------|------|
| **A. 保留 group_frame** | 继续唯一组间入口 | 迁移成本低 | 与 group/swimlane 三套并存；agent 难学 | 不推荐作终态 |
| **B. 独立 Compose 层（内部+DSL）** | 改名 Compose，仍对外暴露 | 概念清晰一点 | 仍是第三控制面 | 过渡可以，终态偏冗余 |
| **C. 拆入三处（推荐）** | 见下表 | 对齐 yFiles；控制面变少 | 要迁移 showcase | **推荐终态** |
| **D. 全部变 intent** | 只有 `intent { equal_width: … }` | 极灵活 | 日常太啰嗦；缺默认 scheme | 作高级补充，不作唯一入口 |

#### 5.2.3 推荐方案 C：拆掉 frame DSL，能力三处安家

```text
原 group_frame
  ├─ lanes / tiles / 责任分区     →  DSL swimlane / table（结构一等）
  ├─ stages 纵向堆               →  Hierarchical 流向 + group policy（或 scheme 默认）
  ├─ strips 等宽条带             →  LayoutProfile.group_sizing = equal_siblings
  │                                + group_align（≈ yFiles groupAlignmentPolicy）
  ├─ gap / 疏密                  →  LayoutProfile.spacing / density
  ├─ border: shared              →  渲染/theme 或极轻的 group_chrome 策略（非布局主链）
  └─ snap                        →  FinalizePolicy（全局，不挂在 frame 上）
```

示意（DSL，非冻结语法）：

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
    swimlane "审批" { entity a; entity b }   // 原 lanes 场景
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

这与 yFiles 更接近：**分组进布局内核；等宽/对齐是 layouter 选项；泳道是格子/分区模型；没有单独的 group_frame 产品名。**

#### 5.2.4 实现上还要不要叫 Compose？

| 层次 | 建议 |
|------|------|
| **对外 DSL / 文档** | **不出现** `group_frame` / `Compose` 作为用户概念 |
| **对内代码** | 允许短暂保留 `group/frame` 模块作 **GroupSizingPass**（由 profile 驱动），迁完改名或并进 Hierarchical Drawing |
| **Scheme** | architecture 默认 `group_sizing=equal`；flowchart 默认 `fit`——**取代 strips/stages 短名心智** |

#### 5.2.5 风险

| 风险 | 缓解 |
|------|------|
| showcase 大量 `group_frame:` | 适配器：旧语法 → profile 字段；P6 再删 |
| 「少了一个旋钮」恐惧 | 能力都在；只是入口换到 scheme + 少量 group_* |
| border shared 没处放 | 先放渲染侧或 `group_chrome: shared`；不阻塞布局收敛 |

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

| 阶段 | 内容 | 退出判据（草案） |
|------|------|------------------|
| **P0** | 继续写权收口：节点 freeze、RoutingRecipe 真 IR、禁 route 后推点 | doc20 关键项可测关闭 |
| **P1** | 落地 DiagramScheme + **GroupHierarchy IR**（两图种共用读取） | scheme 可跑；group IR 单测/确定性 |
| **P2** | DSL `swimlane`/`table` → Intent；与 group 并存 | 至少 1～2 张 showcase；agent 可枚举 |
| **P3** | Hierarchical 原生 group policy；two_phase / group_divide **语义迁入、实现删双轨** | architecture/flowchart 含 group 样例可解释对齐 |
| **P4** | TreeKernel / Circular 一等化；SequenceRecipe 契约钉死；**InterEdge 契约**进路由 | 五图种均有明确内核；跨组边策略可测 |
| **P5** | Integrated routing MVP（简单 flowchart 正交） | 与 Standalone 对拍清单；模式可切换 |
| **P6** | 文档与 DSL：废弃用户侧 `group_frame`；能力迁入 scheme / group_sizing / swimlane；旧语法适配器 | language-spec 更新；迁移指南 |

**明确不做（本讨论稿立场）：**

- 现在删除 `recipes/architecture` / `recipes/flowchart` 另起仓库级重写  
- 删除 Sequence / Circular，或强迫 mindmap / sequence 走 Hierarchical  
- 以「去水印 / 本地 render 配额」等与本主题无关的商业手段绑定布局重构  

---

## 7. 风险与缓解

| 风险 | 缓解 |
|------|------|
| architecture 质量断崖 | StrongGroup profile 必须显式编码 macro + equal-track；禁止「只改默认间距」冒充合并完成 |
| 合并后偷偷留 diagram_type 分支 | 代码审查红线：Kernel/Router 无 DiagramType；差异只在 Recipe compile / Scheme |
| Integrated / Standalone 观感分裂 | 同一 Scheme 下两种 mode 的对拍集；文档写清适用边界 |
| DSL 双轨过长 | P2 起迁移表 + showcase 配额；到期删旧 lanes 写法 |
| 与 doc14/16 抢进度 | **P0 硬门槛**；Scheme/泳道可设计并行，破坏性合并等 P0 |
| 范围膨胀成「第二个 yFiles」 | **四核封顶主路径**（Hierarchical/Tree/Sequence/Circular）；不追力导向等表面积 |

---

## 8. 决策建议（供评审勾选）

请在后续讨论中明确：

- [ ] **采纳**：group 一等公民 + 泳道/table + DiagramScheme + **四布局内核**（§1.1–§1.2）+ 双路由（§5–§6）  
- [x] **正式立场：废弃用户侧 `group_frame` / Compose 产品概念**（§5.2）  
  - 能力拆入：`swimlane`/`table` + `group_sizing`/`group_align` + density/spacing + finalize snap  
  - 迁移期可保留旧语法 → profile 适配器；终态 DSL/文档不再出现 `group_frame`  
  - 对内代码可暂留 GroupSizingPass，由 scheme/profile 驱动，不对外暴露 Compose  
- [x] **正式立场：flowchart / architecture 去双轨**（§5.0）  
  - **保留**两个图类型（语义与默认 Scheme 不同）  
  - **取消**两套布局实现、两套路由主链；共用 HierarchicalRecipe + 同一 RoutingRecipe 家族  
  - 差异只在 Scheme/Profile/Intent；禁止 Kernel 内 `diagram_type` 分叉  
  - 迁移：`two_phase` / `group_divide` 语义并入后删除双轨目录  
- [ ] **拒绝**：立刻巨大重写 / 推倒重来（写权未收口时）  
- [ ] **确认必保图种**：architecture / flowchart / sequence / state / mindmap（已写入 §1.1）  
- [ ] **修订**：……（讨论后填）  
- [ ] **下一步文档**：是否需要单独开「HierarchicalRecipe Profile 表（flowchart vs architecture vs swimlane）」规格页  

---

## 9. 与既有文档的边界

| 文档 | 关系 |
|------|------|
| doc12 / 14 | Hierarchical 共享内核与 Recipe 生命周期；本文要求 **演进接入**，不替代其阶段任务 |
| doc16 / 20 | Standalone 路由收口；本文 Integrated 为后续增量 |
| doc18 | Scheme 产品层；本文视为 **必须落地的前置** |
| doc19 | Hint/group 归零；**本文已定：废弃 group_frame**，泳道/group sizing 承接原能力 |

本文 **不** 重复正交 Solver 内部阶段设计，也 **不** 展开具体 DSL BNF（待 Profile 表评审后再写）。

---

## 10. 附录：设想条目速查

| # | 设想 | 必要性 | 可行性 | 建议 |
|---|------|--------|--------|------|
| 1 | DSL swimlane/table | 高 | 高 | 做（增量） |
| 1b | **group 一等公民** | **高** | 中高 | **做**；统一 IR + Hierarchical 原生 policy |
| 1c | **flowchart/arch 去双轨** | **高** | 中 | **做**；两图种一布局一路由（§5.0） |
| 2 | 图种 → profile，不按图种复制算法 | 高 | 中高 | 做（Scheme）；保留图种语义 |
| 3 | 布局内核集合 | 高 | 高 | **四核**：Hierarchical / Tree / Sequence / Circular |
| 4 | 内建 + 独立路由 | 高 | 中 | 做（Standalone 先，Integrated MVP） |
| — | 巨大重写 | 低 | 低（时机） | 不做 |

---

## 11. 修订记录

| 日期 | 说明 |
|------|------|
| 2026-07-25 | 初稿：可行性/必要性分析与演进建议，供讨论修改 |
| 2026-07-25 | 修订：明确必保五图种；布局内核扩充为 Hierarchical / Tree / Sequence / Circular（§1.1） |
| 2026-07-25 | 修订：明确 **group 一等公民**（对齐 yFiles）；新增 §1.2；更新 Scheme/路线图 |
| 2026-07-25 | 修订：建议 **取消用户侧 group_frame/Compose**；能力拆入 swimlane/table + group sizing/align + density/finalize（§5.2） |
| 2026-07-25 | **决策勾选**：正式立场「废弃用户侧 group_frame / Compose」（§8 已勾选） |
| 2026-07-25 | **决策勾选**：正式立场「flowchart / architecture 去双轨」（§5.0 / §8）；并修复 §5 重复残片 |
