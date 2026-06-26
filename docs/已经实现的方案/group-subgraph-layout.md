# Group 子图独立布局方案

## 一、问题背景

### 1.1 现状

当前流程图布局中，group **不参与布局**，只是事后画框。这导致：

- 不同 group 的节点在分层时交错排列
- group 包围框在 y 方向严重重叠
- 视觉上无法区分 group 的边界

### 1.2 已有的优化

`apply_group_rank_constraints`（已实现）在分层后按 group 分配不重叠的 rank 窗口，消除了 group 包围框重叠。但存在以下局限：

- group 内布局仍受全局影响（排序、坐标分配都是全局的）
- group 无法独立指定布局方向（如泳道图横向、阶段划分纵向）
- 图高度增加约 1.9 倍（每个 group 占用独立 rank 范围）

### 1.3 需求

- **泳道图**：group 应横向排列（垂直泳道，流程从上到下）
- **阶段划分**：group 应纵向排列（水平阶段块，流程从上到下）
- group 内部应能独立指定布局方向
- group 内部布局质量应优于全局布局

## 二、方案概述

### 2.1 核心思想

**自底向上的分治布局**：把每个 group 当做独立子图，独立布局后再合并。

```
1. 提取每个 group 的子图（内部节点 + 内部边）
2. 每个 group 独立调用 Sugiyama 布局（可指定方向）
3. 按 group 依赖关系排列子图（拓扑排序 + 垂直/水平）
4. 合并子图布局结果（坐标对齐）
5. 补充跨 group 边和无 group 节点
```

### 2.2 与 two_phase 的关系：复用思路，简化组间排列

> **修正说明**：早期版本曾将本方案与 `architecture_v2/two_phase` 描述为对立方案（"自底向上 vs 自顶向下"）。经核对 [two_phase.rs:1-4](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs#L1-L4)，two_phase 实际也是自底向上（组内 Sugiyama → 组间宏观定位 → 全局坐标回填），与本方案思路同源。二者关系应定位为：**复用 two_phase 的组内布局思路，简化组间排列**。

#### 2.2.1 思路同源

two_phase 与本方案的核心流程一致：

```
组内独立布局（IntraLayout）  →  组间排列  →  全局坐标回填
```

本方案的 `SubGraphLayout` 与 two_phase 的 `IntraLayout`（[two_phase.rs:26-33](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs#L26-L33)）结构同构，可直接复用。

#### 2.2.2 差异：组间排列的轻重

| 方面 | 本方案（轻量组间排列） | two_phase（重组间排列） |
|------|---------------------|----------------------|
| 组间定位策略 | 拓扑排序 + 垂直/水平堆叠 | `assign_super_macro_ranks` + `order_layers_group_aware` |
| group 级别 Sugiyama | ✗ 不使用 | ✓ 使用（super macro rank） |
| 嵌套 group 处理 | 复用 `GroupTree` 递归 | `GroupTree` 递归（[two_phase.rs:39-91](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs#L39-L91)） |
| 适用场景 | 阶段划分/泳道图（group 间依赖简单） | 架构图（group 间依赖复杂，需全局优化） |
| group 独立方向 | ✓ 支持（Phase 3） | ✗ 不支持 |

#### 2.2.3 取舍

- **本方案适用于 flowchart 场景**：showcase 中的退款流程、电商履约、CI/CD 流水线都是阶段划分，group 间依赖呈线性/树形，无需 super macro rank 的全局优化
- **two_phase 保留用于 architecture 场景**：架构图 group 间常有复杂依赖（多对多、回环），需要 Sugiyama 引擎做组间排序
- **避免重复实现**：组内布局部分应提取为通用模块，两个场景共用（详见 2.4 节）

### 2.3 优势

1. **group 可独立指定布局方向**：group A 用 left-to-right（泳道），group B 用 top-to-bottom（阶段）
2. **group 内布局质量更高**：不受外部节点干扰，排序和坐标分配都在子图内部优化
3. **复用现有引擎**：每个子图直接调用 `compute_with_preset`，无需新布局算法
4. **组间排列轻量**：相比 two_phase 的 super macro rank，本方案用拓扑排序 + 堆叠即可满足 flowchart 场景

### 2.4 与 architecture_v2 的关系

#### 2.4.1 定位：复用而非对立

本方案与 `architecture_v2/two_phase` 同源（详见 2.2 节），不应视为对立替代。核心策略：**选项 A —— 把"分治布局"本身做成一个通用框架，flowchart 与 architecture_v2 都基于此框架特化**。

#### 2.4.2 核心决策：通用分治框架（而非 flowchart 特化）

早期方案设想是"flowchart 新增 group_subgraph.rs + 抽取 two_phase 部分共用"，但这会导致**两套分治调度逻辑并存**（flowchart 一套、architecture 一套），违背"不过于复杂"的原则。

修正后的架构：**一套分治调度逻辑 + 可注入策略**。

```
┌─────────────────────────────────────────────────────────────┐
│  通用分治框架 divide_and_conquer.rs                          │
│  ─────────────────────────────────────────────────────────  │
│  1. 子图提取（GroupTree + 跨 group 边收集）                  │
│  2. 组内布局：委托给注入的 LayoutStrategy                    │
│  3. 组间排列：委托给注入的 GroupArrangement 策略             │
│  4. 合并 + 全局坐标回填                                      │
│  5. 跨 group 边收集（交给现有正交路由引擎）                  │
├─────────────────────────────────────────────────────────────┤
│  可注入策略                                                  │
│  ─────────────────────────────────────────────────────────  │
│  · LayoutStrategy（组内）：Flowchart / ArchitectureV2 / ER   │
│  · GroupArrangement（组间）：Stacking / SuperMacroRank       │
│  · LayoutDirection（方向）：Vertical / Horizontal            │
└─────────────────────────────────────────────────────────────┘
        ▲                              ▲
        │                              │
   ┌────┴───────────┐           ┌──────┴──────────────┐
   │ flowchart      │           │ architecture_v2     │
   │ 有 group 时    │           │ 重构为框架的         │
   │ 委托给框架     │           │ 一个策略组合         │
   └────────────────┘           └─────────────────────┘
```

#### 2.4.3 抽取范围

从 `architecture_v2/two_phase.rs` 抽取到 `layout/node/common/divide_and_conquer.rs`：

| 组件 | 当前位置 | 抽取后位置 | 说明 |
|------|---------|-----------|------|
| `IntraLayout` | [two_phase.rs:26-33](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs#L26-L33) | `divide_and_conquer.rs` | 组内布局结果结构 |
| `GroupTree` | [two_phase.rs:39-91](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs#L39-L91) | `divide_and_conquer.rs` | 嵌套 group 递归树 |
| 子图提取逻辑 | two_phase.rs 内联 | `divide_and_conquer.rs` | 提取为独立函数 |
| 合并 + 坐标回填 | two_phase.rs 内联 | `divide_and_conquer.rs` | 提取为独立函数 |
| 跨 group 边收集 | two_phase.rs 内联 | `divide_and_conquer.rs` | 提取为独立函数 |

**保留差异**（不抽取，作为可注入策略）：

| 组件 | 保留位置 | 形态 |
|------|---------|------|
| `assign_super_macro_ranks` + `order_layers_group_aware` | architecture_v2 | `GroupArrangement::SuperMacroRank` 策略实现 |
| 拓扑排序 + 垂直/水平堆叠 | flowchart 新增 | `GroupArrangement::Stacking` 策略实现 |
| `align_client_nodes_to_hubs` | architecture_v2 | architecture 专用后处理钩子 |

#### 2.4.4 共用接口设计

> **关键修正**：早期版本接口写死 `preset: &Preset`（即 Sugiyama），这会导致 flowchart 和 architecture 的组内布局都被绑死成 Sugiyama。修正后改为接受 `&dyn LayoutStrategy`，由调用方决定组内布局算法。

```rust
// layout/node/common/divide_and_conquer.rs（新增）

/// 组间排列策略
pub trait GroupArrangement {
    /// 根据 group 依赖图和各 group 的 IntraLayout，计算每个 group 的全局偏移
    fn arrange(
        &self,
        group_tree: &GroupTree,
        intra_layouts: &HashMap<String, IntraLayout>,
        cross_group_edges: &[CrossGroupEdge],
        direction: LayoutDirection,
    ) -> HashMap<String, (f64, f64)>;  // group_id → (x_offset, y_offset)
}

/// 堆叠排列（flowchart 用）：拓扑排序 + 垂直/水平堆叠
pub struct StackingArrangement {
    pub gap: f64,
    pub align: AlignMode,  // Center / Left / Top / ...
}

/// Super Macro Rank 排列（architecture 用）：复用现有 assign_super_macro_ranks
pub struct SuperMacroRankArrangement { /* ... */ }

/// 通用分治布局入口
///
/// 一套调度逻辑，通过注入策略实现 flowchart / architecture 的差异化。
pub fn divide_and_conquer(
    diagram: &Diagram,
    intra_strategy: &dyn LayoutStrategy,   // ← 组内布局算法（由调用方决定）
    arrangement: &dyn GroupArrangement,    // ← 组间排列策略
    direction: LayoutDirection,            // ← 组内方向（vertical/horizontal）
) -> LayoutResult {
    // 1. 构建 GroupTree + 提取子图 + 收集跨 group 边
    // 2. 对每个 group：intra_strategy.compute(sub_diagram) → IntraLayout
    // 3. arrangement.arrange(...) → 各 group 全局偏移
    // 4. 合并：IntraLayout + offset → 全局坐标
    // 5. 处理无 group 节点
    // 6. 返回 LayoutResult（跨 group 边交给现有正交路由引擎）
}
```

**调用方**：

```rust
// flowchart/engine.rs（有 group 时）
let result = divide_and_conquer(
    diagram,
    &FlowchartLayout::new(...),           // 组内用 flowchart 布局
    &StackingArrangement { gap: 60.0, align: AlignMode::Center },
    LayoutDirection::Vertical,
);

// architecture_v2/two_phase.rs（重构后）
let result = divide_and_conquer(
    diagram,
    &ArchitectureV2Layout::new(...),      // 组内用 architecture 布局
    &SuperMacroRankArrangement::new(...),
    LayoutDirection::Vertical,
);
```

#### 2.4.5 不选其他选项的理由

- **选项 B（flowchart 独立实现一套）**：会导致两套分治调度逻辑并存，维护成本高，且容易在边界情况上出现不一致
- **选项 C（flowchart 有 group 时直接委托给 architecture_v2）**：architecture_v2 的 super macro rank 对 flowchart 场景过重，且不支持 group 独立方向（Phase 3 目标）
- **早期"抽取组内模块"方案（已被本节取代）**：只抽取组内布局、保留两套组间调度，仍属两套分治逻辑；本节升级为"通用分治框架 + 可注入策略"，彻底统一调度逻辑

### 2.5 关键设计决策（4 个关心点的解法）

本节记录方案评审中确认的 4 个关键设计约束及其解法，作为后续实施的硬性要求。

#### 2.5.1 约束 1：架构图和流程图共用 group 处理代码

**背景**：当前 `architecture_v2/two_phase` 已实现一套组内布局 + 组间排列，flowchart 若另起一套会重复。

**解法**：采用 2.4 节的通用分治框架。共用边界明确：

| 可共用（框架内） | 不可共用（作为可注入策略） |
|-----------------|-------------------------|
| `IntraLayout` 结构 | 组内布局算法（`LayoutStrategy`） |
| `GroupTree` 嵌套递归 | 组间排列策略（`GroupArrangement`） |
| 子图提取逻辑 | 场景特化后处理（如 hub-client 对齐） |
| 跨 group 边收集 | |
| 合并 + 坐标回填 | |

**验证标准**：flowchart 和 architecture_v2 的分治调度代码只有一份（`divide_and_conquer.rs`），差异仅体现在注入的策略参数上。

#### 2.5.2 约束 2：config block 可配置 group 排列

**背景**：用户在泳道图、阶段划分等场景需要对 group 排列有控制力。当前 [ast.rs:470](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/ast.rs#L470) 的 `Group` 结构已有 `attributes: AttributeMap`，DSL 层面具备扩展基础。

**解法**：扩展 group 的 attributes，支持以下配置维度：

```yaml
groups:
  - id: review_stage
    direction: vertical       # 组内布局方向（vertical/horizontal）
    arrangement: vertical     # 组间排列（vertical=阶段划分, horizontal=泳道）
    align: center             # 组间对齐（center/left/top...）
    gap: 60                   # 组间距
    layout: flowchart         # 组内布局算法（可选，默认继承外层）
```

**配置维度拆分到 Phase**：

| 维度 | 归属 Phase | 原因 |
|------|-----------|------|
| `arrangement`（组间排列） | Phase 1 | MVP 就需要决定怎么堆叠，且与 `GroupArrangement` 策略直接对应 |
| `align` / `gap` | Phase 1 | 堆叠策略的参数 |
| `direction`（组内方向） | Phase 3 | 需要子图布局算法支持方向参数 |
| `layout`（组内算法） | 暂不开放 | 默认继承外层算法，避免用户误配 |

**验证标准**：用户可通过 DSL 指定 group 排列为 `horizontal` 实现泳道图，指定为 `vertical` 实现阶段划分。

#### 2.5.3 约束 3：group 内部布局由外部算法决定 ⭐

**背景**：group 内部布局不应写死成 Sugiyama。flowchart 的 group 内部应该用 flowchart 布局，architecture 的 group 内部应该用 architecture 布局。早期方案 2.4.3 的接口写死 `preset: &Preset`（即 Sugiyama），违背此约束。

**解法**：通用分治框架接受 `&dyn LayoutStrategy`，由调用方决定组内布局算法（见 2.4.4 接口设计）。

```rust
// 错误（写死 Sugiyama）
pub fn layout_intra_group(diagram, group_id, preset: &Preset, direction) -> IntraLayout

// 正确（接受任意策略）
pub fn divide_and_conquer(
    diagram,
    intra_strategy: &dyn LayoutStrategy,  // ← 由调用方决定
    arrangement: &dyn GroupArrangement,
    direction: LayoutDirection,
) -> LayoutResult
```

**对 architecture_v2 的影响**：重构后 `two_phase.rs` 不再直接调用 Sugiyama，而是通过框架注入 `ArchitectureV2Layout`（或其组内变体）作为组内策略。

**验证标准**：
- flowchart 的 group 内部使用 `FlowchartLayout`，布局风格与无 group 的 flowchart 一致
- architecture 的 group 内部使用 `ArchitectureV2Layout`，布局风格与现有架构图一致
- 未来 ER/State 若要支持 group，只需传入对应 strategy，无需改框架

#### 2.5.4 约束 4：整体布局逻辑不过于复杂

**背景**：分治布局本身已增加复杂度，不能再叠加过多特化逻辑。早期方案"flowchart 新增分治 + 抽取 two_phase 部分共用"会导致两套调度逻辑，复杂度过高。

**解法**：采用 2.4 节的通用分治框架，复杂度控制原则：

1. **一套调度逻辑**：`divide_and_conquer.rs` 是唯一的分治调度入口，flowchart 和 architecture 都通过它
2. **策略可注入**：差异通过 `LayoutStrategy` 和 `GroupArrangement` 两个 trait 注入，不通过条件分支
3. **不引入新布局算法**：组内布局复用现有 `LayoutStrategy` 实现，组间排列只有 `Stacking` 和 `SuperMacroRank` 两种
4. **跨 group 边路由复用现有引擎**：不新增路由算法，仅验证现有正交路由在分治布局下的表现（Phase 2）
5. **无 group 时走原路径**：`engine.rs` 检测到无 group 时直接走原 Sugiyama 路径，不经过分治框架

**复杂度对比**：

| 方面 | 早期方案（两套调度） | 修正方案（通用框架） |
|------|-------------------|-------------------|
| 分治调度逻辑 | 2 份（flowchart + architecture） | 1 份（`divide_and_conquer.rs`） |
| 组间排列策略 | 各自硬编码 | 2 个 trait 实现（Stacking / SuperMacroRank） |
| 组内布局算法 | 写死 Sugiyama | 注入 `&dyn LayoutStrategy` |
| 新增代码量 | ~1200 行（两套） | ~800 行（一套框架 + 2 个策略） |

**验证标准**：分治框架核心代码（不含策略实现）不超过 400 行；策略实现各自不超过 200 行。


## 三、技术设计

### 3.1 整体流程

```
┌─────────────────────────────────────────────────────────┐
│  输入：Diagram（含 entities, relations, groups）          │
├─────────────────────────────────────────────────────────┤
│  Step 1: 子图提取                                        │
│    - 按 group_id 分组节点                                │
│    - 提取 group 内部边                                   │
│    - 识别跨 group 边                                     │
│    - 识别无 group 节点                                   │
├─────────────────────────────────────────────────────────┤
│  Step 2: 子图独立布局                                    │
│    - 对每个 group 子图调用 Sugiyama 布局                 │
│    - 可指定独立布局方向                                   │
│    - 输出：子图内节点的局部坐标                           │
├─────────────────────────────────────────────────────────┤
│  Step 3: group 级别排列                                  │
│    - 构建 group 依赖图                                   │
│    - 拓扑排序                                            │
│    - 按序排列（垂直/水平）                                │
│    - 计算每个 group 的全局偏移                           │
├─────────────────────────────────────────────────────────┤
│  Step 4: 合并                                            │
│    - 子图局部坐标 + group 偏移 = 全局坐标                 │
│    - 处理无 group 节点                                   │
│    - 计算 group 包围框                                   │
├─────────────────────────────────────────────────────────┤
│  Step 5: 跨 group 边路由                                 │
│    - 对跨 group 边做正交路由                             │
│    - group 矩形作为障碍物                                │
├─────────────────────────────────────────────────────────┤
│  输出：LayoutResult（全局坐标 + 边路由）                  │
└─────────────────────────────────────────────────────────┘
```

### 3.2 Step 1: 子图提取

**输入**：`Diagram`（entities, relations, groups）

**输出**：
- `group_subgraphs: HashMap<String, SubGraph>` — 每个 group 的子图
- `cross_group_edges: Vec<CrossGroupEdge>` — 跨 group 边
- `ungrouped_nodes: Vec<Entity>` — 无 group 节点

**数据结构**：

```rust
struct SubGraph {
    group_id: String,
    entities: Vec<Entity>,
    internal_edges: Vec<Relation>,  // from 和 to 都在此 group
    boundary_nodes: HashSet<String>,  // 有跨 group 边的节点
}

struct CrossGroupEdge {
    from: String,  // entity_id
    to: String,    // entity_id
    from_group: Option<String>,
    to_group: Option<String>,
    relation: Relation,
}
```

**算法**：

```
1. 遍历 entities，按 group_id 分组
2. 遍历 relations：
   - from 和 to 在同一 group → 内部边
   - from 和 to 在不同 group（或一方无 group）→ 跨 group 边
3. 识别边界节点：有跨 group 边的节点
```

### 3.3 Step 2: 子图独立布局

**输入**：`SubGraph`（group 内部节点和边）

**输出**：`SubGraphLayout`（子图内节点的局部坐标 + 子图尺寸）

**数据结构**：

```rust
struct SubGraphLayout {
    group_id: String,
    nodes: HashMap<String, NodeLayout>,  // 局部坐标
    width: f64,
    height: f64,
    direction: String,  // 此 group 的布局方向
}
```

**算法**：

```
1. 构建 SubDiagram（只含 group 内部节点和边）
2. 调用 compute_with_preset(sub_diagram, preset, config)
3. 记录子图尺寸和节点局部坐标
```

**关键点**：
- 子图布局完全独立，不受其他 group 影响
- 可以指定不同的布局方向（group A 用 left-to-right，group B 用 top-to-bottom）
- 边界节点在子图内部有固定位置，合并后坐标对齐即可

### 3.4 Step 3: group 级别排列

**输入**：所有 `SubGraphLayout` + group 依赖图

**输出**：每个 group 的全局偏移 `(x_offset, y_offset)`

**算法**：

```
1. 构建 group 依赖图：
   - group A → group B 如果存在跨 group 边 (u, v) 且 u ∈ A, v ∈ B
2. 对 group 依赖图去环（反转回边）
3. 拓扑排序
4. 按序排列：
   - 垂直排列（阶段划分）：group y 偏移递增
   - 水平排列（泳道图）：group x 偏移递增
5. 计算每个 group 的全局偏移
```

**排列策略**：

```
垂直排列（阶段划分，top-to-bottom 流程图）：
  group A: offset = (0, 0)
  group B: offset = (0, A.height + gap)
  group C: offset = (0, A.height + B.height + 2*gap)

水平排列（泳道图，top-to-bottom 流程图）：
  group A: offset = (0, 0)
  group B: offset = (A.width + gap, 0)
  group C: offset = (A.width + B.width + 2*gap, 0)
```

**确定性保证**：
- group 拓扑排序用 Kahn's algorithm，入度为 0 的 group 按 `(min_rank, group_id)` 排序
- 如果有环，剩余 group 按 `(min_rank, group_id)` 排序追加

### 3.5 Step 4: 合并

**输入**：所有 `SubGraphLayout` + group 偏移

**输出**：全局 `LayoutResult`

**算法**：

```
1. 合并 group 内节点坐标：
   global_node_layout[node] = sub_graph_layout[node] + group_offset

2. 处理无 group 节点：
   - 方案 A：把无 group 节点当作"虚拟 group"独立布局
   - 方案 B：在全局布局中为无 group 节点预留位置
   - 推荐方案 A，统一处理

3. 计算 group 包围框：
   - 复用现有 compute_group_bounds
   - group 偏移 + 子图尺寸 = group 全局包围框

4. 计算总尺寸：
   total_width = max(group.x + group.width) + padding
   total_height = max(group.y + group.height) + padding
```

### 3.6 Step 5: 跨 group 边路由

**输入**：全局节点坐标 + group 包围框 + 跨 group 边

**输出**：跨 group 边的正交路由路径

**算法**：

```
1. 对每条跨 group 边 (from, to)：
   - from_port = global_node_layout[from].port
   - to_port = global_node_layout[to].port
   - obstacles = [group_bounding_boxes...]
   - route = orthogonal_route(from_port, to_port, obstacles)

2. 正交路由策略：
   - 从 from_port 出发，沿水平/垂直方向走到 group 边界
   - 在 group 之间的通道中路由
   - 到达 to_port 所在 group 边界后进入
   - 连接到 to_port
```

**关键挑战**：
- group 矩形作为障碍物，路由需要绕行
- 多条跨 group 边可能共享通道，需要避免重叠
- 回环边（如 review → intake）需要绕到 group 外侧

## 四、实施路径

> **总原则**：按 2.4 节通用分治框架的思路实施。Phase 0 建框架，Phase 1 在框架上落地 flowchart，Phase 2/3 做验证与扩展。architecture_v2 的重构作为框架的"第二消费者"穿插进行，用于验证框架的通用性。

### Phase 0: 通用分治框架（前置）

**目标**：新建 `divide_and_conquer.rs` 通用框架，把 `architecture_v2/two_phase.rs` 的调度逻辑抽取为框架，architecture_v2 改为框架的第一个消费者。

**改动**：
- 新增 `layout/node/common/divide_and_conquer.rs`：
  - `IntraLayout`、`GroupTree`、`CrossGroupEdge` 数据结构
  - `GroupArrangement` trait
  - `divide_and_conquer()` 通用入口（接受 `&dyn LayoutStrategy` + `&dyn GroupArrangement`）
  - 子图提取、合并、坐标回填等通用函数
- 新增 `SuperMacroRankArrangement`：包装现有 `assign_super_macro_ranks` + `order_layers_group_aware`
- 修改 `architecture_v2/two_phase.rs`：改为调用 `divide_and_conquer()`，注入 `ArchitectureV2Layout` + `SuperMacroRankArrangement`

**验证**：
- architecture showcase 布局结果不退化（像素级对比或视觉对比）
- 框架核心代码 ≤ 400 行
- `GroupArrangement` trait 设计能同时容纳后续的 `StackingArrangement`

#### Phase 0 实际实施情况（已完成）

**已完成**：
- 新增 [divide_and_conquer.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/divide_and_conquer.rs)：`IntraLayout`、`GroupTree`、`CrossGroupEdge` 数据结构 + `IntraGroupLayouter`、`GroupArrangement` trait + 3 个单元测试
- 修改 [two_phase.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs)：删除本地 `IntraLayout`、`GroupTree` 定义，改为从通用模块导入
- 全部 666 个测试通过（含 27 个 architecture_v2 测试），行为零退化

**与原方案的差异：推迟 `ArchitectureV2IntraLayouter` + `SuperMacroRankArrangement` 的实现**

原方案 Phase 0 要求"实现 `ArchitectureV2IntraLayouter` + `SuperMacroRankArrangement`，让 architecture_v2 成为框架的第一个消费者"。**此步骤推迟到 Phase 1 验证 trait 设计之后**，原因：

1. **上下文耦合度高**：`layout_intra_group_recursive` 依赖大量上下文（`group_tree`、`graph`、`sizes`、`reversed`、`padding`），强行包装成 trait 实现会增加复杂度和风险
2. **trait 签名待验证**：在 flowchart 还没实现 `IntraGroupLayouter` 的情况下，trait 签名可能需要根据实际使用调整（如是否需要传入 `GroupTree`、是否需要返回层结构等）
3. **渐进式策略更稳妥**：Phase 1 先实现 flowchart 的 `IntraGroupLayouter`，验证 trait 设计，再回头让 architecture_v2 实现 trait（作为可选的后续清理）

**后续完成情况（Phase 1-3 后补完）**：

- ✅ `ArchitectureV2IntraLayouter` 已实现（[two_phase.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs)）：作为 `layout_intra_group_recursive` 的 thin wrapper，持有 6 个上下文引用，实现 `IntraGroupLayouter` trait。当前 `compute_two_phase_layout` 仍直接调用 `layout_intra_group_recursive`，未走 trait 调度——wrapper 仅供文档化关系和未来统一调度使用。
- ❌ `SuperMacroRankArrangement` 不实现：`GroupArrangement` trait 的 `arrange` 签名（`group_ids + intra_layouts + cross_edges → offsets`）无法容纳 architecture_v2 需要的 `macro_ranks` + `pair_edge_counts` + `blocks`（含 `IntraLayout` 但还有 width/height/x/y 等字段）。强行适配会丢失 `adaptive_group_gap` 等优化。architecture_v2 的组间排列保持直接函数调用。

**当前状态**：通用类型基础已就位，architecture_v2 已复用通用类型。`IntraGroupLayouter` trait 已被 `FlowchartIntraGroupLayouter` 和 `ArchitectureV2IntraLayouter` 实现；`GroupArrangement` trait 已被 `StackingArrangement` 实现。

### Phase 1: flowchart 分治布局（MVP）+ 组间排列配置

**目标**：基于 Phase 0 框架，实现 flowchart 的分治布局；同时开放组间排列的 DSL 配置（约束 2）。

**改动**：
- 新增 `StackingArrangement`：拓扑排序 + 垂直堆叠（实现 `GroupArrangement` trait）
  - 支持 `gap`、`align` 参数（`arrangement` 方向复用 diagram 级 `direction` 属性，Phase 3 实现 horizontal）
- 修改 [flowchart/mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/flowchart/mod.rs)：`compute` 和 `compute_with_overlay` 检测到有 group 时调用 `divide_flowchart_with_groups()`
- 扩展 DSL：diagram 级 `config` 块支持 `group_gap`（Number）/ `group_align`（Atom: center|left）
  - **设计决策**：放在 diagram 级而非 group 级，因为组间排列是 diagram 级关注点，避免"哪个 group 的值优先"的歧义（约束 4：整体布局逻辑不过于复杂）
- group 包围框：分治路径直接用 group 偏移 + IntraLayout 尺寸构造 `GroupLayout`，不调用 `compute_group_bounds`（子图布局已含 preset padding）

**验证**：
- ✅ 退款流程图 group 不重叠（`intake → review → finance` 按声明顺序）
- ✅ group 内布局质量不低于当前 `apply_group_rank_constraints` 方案
- ✅ 无 group 的流程图不受影响（走原 Sugiyama 路径）
- ✅ 用户可通过 DSL 指定 `group_gap` / `group_align` 控制组间排列
- ✅ architecture showcase 仍不退化（Phase 0 的框架未被破坏）
- ✅ 677 个单元测试全部通过

#### Phase 1 实际实施情况（已完成）

**已完成**：
- [group_divide.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/flowchart/group_divide.rs)：`FlowchartIntraGroupLayouter` + `StackingArrangement` + `divide_flowchart_with_groups` + 11 个单元测试
- [flowchart/mod.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/flowchart/mod.rs)：`compute` 和 `compute_with_overlay` 分治路径分发
- DSL 扩展：`group_gap`（Number）/ `group_align`（Atom: center|left）注册到 [diagram.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/types/standard_attr_keys/diagram.rs) / [attr_constants.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/types/attr_constants.rs) / [attr_schema.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/types/attr_schema.rs) / [expr.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/dsl/parser/expr.rs) / [common.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/validation/common.rs)
- 3 个有 group 的 showcase 流程图（customer-refund / ci-cd-security / e-commerce-order）全部无重叠
- 无 group 的流程图不受影响
- architecture showcase 未退化

**与原方案的差异**：
1. **`arrangement` 属性推迟到 Phase 3**：原方案 Phase 1 要求支持 `arrangement: horizontal`（泳道图）。实际 Phase 1 只实现 vertical 堆叠，horizontal 推迟到 Phase 3。原因：vertical 已满足当前需求，horizontal 需要重新设计水平堆叠逻辑 + 跨 group 边路由适配，工作量较大。
2. **DSL 属性放在 diagram 级而非 group 级**：原方案说"group 的 `attributes` 支持 `arrangement` / `align` / `gap`"。实际放在 diagram 级 `config` 块中（`group_gap` / `group_align`），因为组间排列是 diagram 级关注点，避免"哪个 group 的值优先"的歧义。
3. **`group_bounds.rs` 无需修改**：原方案说"修改 `group_bounds.rs` 适配分治布局"。实际分治路径直接用 IntraLayout 尺寸构造 GroupLayout，不调用 `compute_group_bounds`，无需修改。

### Phase 2: 跨 group 边路由验证与适配

> **修正说明**：早期版本将本阶段描述为"实现 group 障碍物"。经核对，正交路由引擎**已支持 group 作为障碍物**：[path.rs:111](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs#L111) 的 `RoutingContext` 已包含 `groups` 字段，[path.rs:159-160](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/path.rs#L159-L160) 的 `build_channel_detours` 已通过 `channel_outside` / `channel_outside_v` 考虑 group 边框位置做侧通道绕行。因此本阶段工作从"实现"降级为"验证与适配"。

**目标**：验证分治布局产出的全局坐标下，跨 group 边能被现有正交路由引擎正确路由；必要时做局部适配。

**改动**（预期较小，可能为 0）：

- **验证项**：
  - 跨 group 边不穿过 group 矩形（依赖现有 `build_channel_detours` 的 group 障碍物处理）
  - 回环边（如 review → intake）绕行合理
  - 端口选择（[slot.rs:53](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs#L53) `choose_pair_sides`）在分治布局下仍能选出合理边
  - 磁吸点分配（[mod.rs:209-340](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs#L209-L340)）对跨 group 边的并线策略是否合理
- **可能的适配**（仅在验证发现问题时做）：
  - 调整 `OrthoConfig.channel_margin` 以适应分治布局下 group 间通道宽度
  - 对跨 group 边的端口选择增加 group 边界感知（如优先选朝向目标 group 的边）

**验证用例**：

- 退款流程图（review → intake 回环边）
- 紧密耦合 group 场景（group 间边密集）
- 嵌套 group 场景（跨层级边）

**回退策略**：若验证发现现有路由质量不可接受，则升级为本阶段新增跨 group 边专用路由逻辑（原计划工作量）。

#### Phase 2 实际实施情况（已完成）

**验证结果**：✅ 全部通过，无需适配。

对 3 个有 group 的 showcase 流程图进行了跨 group 边路由验证：
- `c.customer-refund-process`：3 条跨 group 边（含 `escalate(review)→reject(intake)` 回环边），0 违规
- `c.ci-cd-security-pipeline`：4 条跨 group 边，0 违规
- `c.e-commerce-order-fulfillment`：2 条跨 group 边，0 违规

**验证方法**：检查每条跨 group 边的路径段是否与非起点/终点的 group 矩形相交（Liang-Barsky 线段-矩形相交检测）。回环边 `escalate→reject` 有 6 个路径点，x 跨度 198，说明正交路由引擎正确绕行到 group 右侧通道。

**结论**：现有正交路由引擎的 `build_channel_detours` + group 障碍物处理已能正确路由分治布局下的跨 group 边，无需新增逻辑。

### Phase 3: 水平排列（泳道图）支持

**目标**：支持 group 水平排列（泳道图场景），分离组间排列方向与组内布局方向。

**改动**：
- 新增 diagram 级 `group_arrangement` 属性（Atom: vertical/horizontal），控制组间排列方向
- `StackingArrangement` 增加 `ArrangementMode`（Vertical/Horizontal），水平模式下 group 从左到右排列
- 组内布局方向仍由 diagram 级 `direction` 属性控制（top-to-bottom → 组内垂直），与组间排列方向独立

**验证**：
- 泳道图 group 水平排列（`group_arrangement: horizontal`）+ 组内垂直布局（`direction: top-to-bottom`）
- 阶段划分 group 垂直排列（`group_arrangement: vertical`，默认）+ 组内垂直布局
- 新增泳道图 showcase：[c.swimlane-order-process.dfy](file:///Users/jimichan/zaprt-projects/flowml/showcase/flowchart/c.swimlane-order-process.dfy)

#### Phase 3 实际实施情况（已完成）

**已完成**：
- 新增 `ArrangementMode` 枚举（Vertical/Horizontal）和 `group_arrangement` diagram 级属性
- `StackingArrangement` 支持水平堆叠：group 从左到右排列，垂直方向按 `align` 对齐（Center 垂直居中 / Left 顶部对齐）
- 新增泳道图 showcase `c.swimlane-order-process.dfy`：4 个 group（客户/销售/仓库/物流）水平排列，组内节点垂直布局
- 12 个 group_divide 单元测试通过（含水平排列测试）
- 678 个全量测试通过
- 垂直排列 showcase 无退化

**与原方案的差异**：
1. **未实现"group 独立方向"**：原方案要求每个 group 可独立指定组内布局方向。实际实现中，组内布局方向仍由 diagram 级 `direction` 统一控制，未下放到 group 级。原因：组内方向独立的需求场景较少，且会增加 DSL 复杂度（约束 4）。如需此功能，可在后续迭代中扩展 group 级 `direction` 属性。
2. **`group_arrangement` 放在 diagram 级**：与 Phase 1 的 `group_gap`/`group_align` 一致，放在 diagram 级 `config` 块中，而非 group 级。

## 五、风险与挑战

### 5.1 跨 group 边路由质量

**风险**：合并后跨 group 边可能绕行较长路径，视觉上不如全局布局紧凑。

**缓解**：
- group 间预留足够通道
- 路由优化：多条跨 group 边共享通道
- 可选：对跨 group 边做全局优化（如最小化总路由长度）

### 5.2 group 间空白

**风险**：如果 group 宽度/高度差异大，排列后会有空白。

**缓解**：
- 垂直排列时，group 水平居中或左对齐
- 水平排列时，group 垂直居中或顶部对齐
- 可选：紧凑排列（填满空白区域）

### 5.3 回环边处理

**风险**：回环边（如 review → intake）需要绕到 group 外侧，可能影响美观。

**缓解**：
- 回环边走 group 外侧通道
- 可选：回环边用不同样式（虚线/浅色）区分

### 5.4 嵌套 group

**风险**：嵌套 group 需要递归处理，增加复杂度。

**缓解**：
- 递归提取子图：外层 group 的子图包含内层 group 的子图
- 内层 group 先布局，外层 group 再布局
- 或扁平化嵌套 group（当前 `build_node_to_top_group` 已实现）

#### 5.4.1 当前限制：flowchart 分治路径不支持嵌套 group

**现状**：flowchart 的 `FlowchartIntraGroupLayouter` 用 `tree.descendant_entities(gid)` 把顶层 group 的所有后代实体（含子 group 内的）扁平化为一个子图，**子 group 的边界和标签会丢失**。

**影响**：
- DSL 仍允许 flowchart 声明嵌套 group（parser 层不报错）
- 但分治路径下，子 group 不会被渲染为独立的框，其内部节点会被当作顶层 group 的直接成员一起布局
- architecture_v2 不受此限制（`layout_intra_group_recursive` 已递归处理嵌套）

**适用范围**：
- ✅ 单层 group（当前所有 flowchart showcase 均为单层）
- ❌ 嵌套 group（子 group 边界丢失）

**后续计划**：如需支持嵌套 group，可参考 architecture_v2 的 `layout_intra_group_recursive` 实现递归版 `FlowchartIntraGroupLayouter`。当前无 showcase 用到嵌套 group，暂不实现。

### 5.5 性能

**风险**：多个子图独立布局可能比全局布局慢。

**缓解**：
- 子图规模小，单个子图布局快
- 可并行布局多个子图
- 实测：N 个小子图的总布局时间通常 < 1 个大图

## 六、与其他方案对比

| 方案 | group 不重叠 | group 独立方向 | group 内布局质量 | 实现复杂度 | 图高度 |
|------|------------|---------------|-----------------|-----------|--------|
| 当前（apply_group_rank_constraints） | ✓ | ✗ | 受全局影响 | 低 | 高（1.9x） |
| 本方案（子图独立布局） | ✓ | ✓ | 独立优化 | 中 | 可控 |
| two_phase | ✓ | ✗ | 受 group 布局影响 | 高 | 中 |
| 全局布局 + group 偏置（P1s） | ✗ | ✗ | 受全局影响 | 低 | 原始 |

## 七、待讨论的问题

### 7.1 group 排列方向如何决定？

**选项 A**：用户在 DSL 中指定（group 的 `direction` 属性）
**选项 B**：自动检测（根据 group 依赖图形状）
**选项 C**：全局配置（diagram 级别的 `group_direction` 属性）

### 7.2 无 group 节点如何处理？

**选项 A**：当作"虚拟 group"独立布局
**选项 B**：在全局布局中预留位置
**选项 C**：依附到最近的 group

### 7.3 跨 group 边的路由策略？

**选项 A**：简单正交路由（group 作为障碍物）
**选项 B**：全局路由优化（最小化总路由长度）
**选项 C**：通道路由（group 间预留通道）

### 7.4 嵌套 group 如何处理？

**选项 A**：递归子图布局
**选项 B**：扁平化（用 `build_node_to_top_group`）
**选项 C**：只支持一层 group

### 7.5 子图布局参数如何继承？

**选项 A**：继承全局 preset
**选项 B**：group 可指定独立 preset
**选项 C**：混合（部分参数继承，部分可覆盖）

## 八、适用场景

### 8.1 适合本方案的场景

- **泳道图**：group 横向排列，流程从上到下
- **阶段划分流程图**：group 纵向排列，流程从上到下
- **复杂审批流程**：group 划分阶段，内部独立优化
- **微服务架构图**：group 划分服务边界

### 8.2 不适合本方案的场景

- **简单流程图**（<10 节点，无 group）：直接用全局布局
- **紧密耦合的图**：group 间边很多，独立布局后合并收益小
- **无 group 的图**：走原路径，不受影响

## 九、代码改动预估

> **修正说明**：早期版本预估 ~800 行，但未计入 Phase 0（框架抽取）和 architecture_v2 重构工作量，且未考虑通用分治框架的接口设计。修正后按 Phase 拆分预估。

| Phase | 模块 | 改动 | 预估行数 |
|-------|------|------|---------|
| Phase 0 | `divide_and_conquer.rs`（新增） | 通用框架：数据结构 + trait + 调度入口 + 子图提取 + 合并 | ~400 行 |
| Phase 0 | `SuperMacroRankArrangement`（新增） | 包装现有 `assign_super_macro_ranks` | ~100 行 |
| Phase 0 | `architecture_v2/two_phase.rs`（重构） | 改为调用框架 | -150 行（净减少） |
| Phase 1 | `StackingArrangement`（新增） | 拓扑排序 + 堆叠 | ~200 行 |
| Phase 1 | `flowchart/engine.rs`（修改） | 检测 group 走分治路径 | ~50 行 |
| Phase 1 | DSL 扩展 + `group_bounds.rs` 适配 | arrangement/align/gap 配置 + 包围框 | ~80 行 |
| Phase 2 | 路由验证 + 适配 | 预期 0~100 行（视验证结果） | ~50 行 |
| Phase 3 | direction 支持 | DSL + 框架 + FlowchartLayout | ~100 行 |
| 测试 | 单元测试 + 集成测试 | 跨 Phase | ~300 行 |
| **总计** | | | ~1130 行 |

**说明**：
- Phase 0 的 -150 行是 architecture_v2 重构后的净减少（调度逻辑移到框架）
- Phase 0 的 400 行框架代码中，约一半是从 two_phase.rs 抽取的（非全新编写）
- 实际新增代码量约 800~900 行，与早期预估一致，但覆盖范围更广（含 architecture_v2 重构）

## 十、结论

本方案的核心是**通用分治布局框架**：把"子图提取 → 组内布局 → 组间排列 → 合并"这套调度逻辑抽取为 `divide_and_conquer.rs`，通过注入 `LayoutStrategy`（组内算法）和 `GroupArrangement`（组间策略）两个 trait，让 flowchart 和 architecture_v2 共用同一套调度逻辑。

**4 个关键设计约束的落实**（详见 2.5 节）：

1. **共用 group 处理代码**：flowchart 和 architecture_v2 都通过 `divide_and_conquer()` 入口，差异仅 in 注入的策略
2. **DSL 可配置 group 排列**：group 的 `attributes` 支持 `arrangement` / `align` / `gap` / `direction`，用户可控制泳道图/阶段划分
3. **组内布局由外部算法决定**：框架接受 `&dyn LayoutStrategy`，flowchart 注入 `FlowchartLayout`，architecture 注入 `ArchitectureV2Layout`
4. **整体逻辑不过于复杂**：一套调度逻辑 + 两个 trait 注入点，框架核心 ≤ 400 行，策略实现各自 ≤ 200 行

建议分四个 Phase 实施：
0. **Phase 0**（前置）：建通用分治框架，architecture_v2 重构为框架的第一个消费者
1. **Phase 1**（MVP）：基于框架实现 flowchart 分治布局 + 组间排列 DSL 配置
2. **Phase 2**：跨 group 边路由验证与适配（正交路由已支持 group 障碍物，预期工作量小）
3. **Phase 3**：group 独立方向支持

当前 showcase 中三个带 group 的流程图（退款流程、电商履约、CI/CD 流水线）都是阶段划分，group 纵向排列，可作为 Phase 1 的验证用例。泳道图需要新增 showcase 用例验证 Phase 3。
