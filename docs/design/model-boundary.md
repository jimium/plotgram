# plotgram-model 边界

> 状态：已落地 | 对应 crate：`crates/plotgram-model`

## 职责

model 是**纯数据层**：定义所有 crate 共享的类型，不含布局/渲染算法。  
解析器可将 DSL attrs **提升**为一等字段（`Edge::lift_structural_attrs`），之后引擎只读字段。

## 模块

| 模块 | 核心类型 | 消费者 |
|------|----------|--------|
| `geometry` | `Point`, `Rect` | 全部 |
| `attr` | `AttrValue`, `AttrMap` | 全部（样式/meta/开放扩展） |
| `graph` | `Node`（含 `role`/`host_group`/`anchor`/`partition_cell`）, `Edge`, `Group`, `Graph`（含 `partition`）, `Arrow` | engine, render |
| `partition` | `PartitionGrid`, `PartitionAxis`, `PartitionCell`, `validate_graph_partition` | parse / engine（消费 planned） |
| `port` | `Side`, `PortConstraint`（作者钉死）, `PortRef`（已决议） | engine |
| `contract` | `AlgorithmRef`, `LayoutContract`（含 `node_sizes`） | **engine 入口** |
| `sizes` | `NodeSizes`, `Size`（geometry） | 编排度量 → engine |
| `result` | `LayoutResult`, placements（含决议后的 `PortRef`） | engine 产出 |
| `render` | `RenderMeta`, `RenderInput` | **render 入口** |
| `profile` | `DiagramType`, `Profile` | **仅** DSL / parse / 编排（engine 禁止 import） |

编排入口：[`plotgram-pipeline`](../../crates/plotgram-pipeline)（parse → measure → engine → render）。  
CLI 仅做参数与文件 I/O，调用 pipeline，不写编排逻辑。

## `Edge` 一等结构字段（写权）

| 字段 | 含义 | 写者 |
|------|------|------|
| `from_port` / `to_port` | `Option<PortConstraint>`：作者侧/槽钉死；`None` = 算法决定 | DSL→parse 提升；布局组合相读约束并写出 **`EdgePlacement.from_port/to_port: PortRef`** |
| `edge_group` | `Option<String>`：同 id 可合流/总线 | DSL→parse；路由/Ink 读 |
| `id` / `source` / `target` / `arrow` / labels | 拓扑与标签 | parse |
| `attrs` | `variant` / `style.*` / `meta.*` 等 | **不得**再承载 `from_side` / `edge_group`（提升后剥除） |

**不**为时序增加 `Edge::seq`：见下节。

## 时序：边声明序 = 时间轴

当 `LayoutContract.layout.name == "sequence"`：

- 消息时间序 = [`Graph::edges_in_declaration_order`](../../crates/plotgram-model/src/graph.rs)（顶层 `edges` 向量序，再按 `groups` 声明序深度优先）。
- **无** `seq` 字段；重排时间 = 重排 DSL 中边的书写顺序。
- 产品上消息写在顶层；组内消息不是一等时序能力。

生命线 / 激活条是 layout/render **派生几何**，不进入 `Graph`。

## 组间边（ADR-004）

- **不**把 group 当作边端点。
- 区到区连线经由 **`group_anchor` 隐形节点**：`Node.role = GroupAnchor`，必填 `host_group` + `anchor`（`side`/`slot`）。
- DSL：手写见 dsl-spec §5.7；推荐糖 `@group -> @group`（§7.6）由 parse 展开。
- 几何由组框派生；禁止 Ink 特判挪点。详见 [`adr/004-group-anchor-nodes.md`](adr/004-group-anchor-nodes.md)。

### `Node` 结构字段（写权）

| 字段 | 含义 | 写者 |
|------|------|------|
| `role` | `Entity`（默认）/ `GroupAnchor` | DSL→parse 提升 |
| `host_group` | 锚点所属 group id | 同上；仅 GroupAnchor |
| `anchor` | `Option<PortConstraint>`：贴框侧/槽 | 同上；仅 GroupAnchor |
| `partition_cell` | `Option<PartitionCell>`：正交分区格 | DSL→parse 提升（`cell_col`/`cell_row`）；引擎只读 |
| `attrs` | 样式/meta 等 | **不得**再承载 `role` / `host_group` / `side` / `slot` / `cell_col` / `cell_row`（提升后剥除） |

### PartitionGrid（ADR-008）

| 字段 | 含义 | 写者 |
|------|------|------|
| `Graph.partition` | 有序 columns/rows 轴 | DSL `partition { … }`（parse planned）/ 手写 IR |
| 轴声明序 | 几何轴序（稳定） | 作者 |
| 校验 | `Graph::validate_partition` | parse / 编排在 lift 后调用 |

与 group **正交**：group 不演泳道。详见 [`adr/008-partition-grid.md`](adr/008-partition-grid.md)、[`layout/shared/partition.md`](layout/shared/partition.md)。

## 硬约束

1. **`DiagramType` / profile 名不得出现在 engine**（ADR-001）。入口只认 `LayoutContract`。DSL 表面键为 `profile:`。
2. **`AttrMap` 用 `BTreeMap`**（确定性）。
3. **model 不依赖其它 workspace crate**。
4. **边用稳定 `Edge.id`**；placement / label 用同一 id。
5. **端口**：作者约束在 `Edge`；决议 `PortRef` 在 `EdgePlacement`；Ink 不得发明。
6. **结构字段一等**：端口 / 边组 / `group_anchor` / **partition cell** 不靠引擎读自由 attrs。

## 内容块与度量（ADR-005）

- 框内**通用富文本**（说明、列表等）：瘦 MD → Content AST → **`ContentLayout`（度量相唯一写者）**；主路径为 SVG。
- **ASCII 只支持 label**，不做内容块。
- **不**用 MD 解决 ER 字段表（ER 另案）。
- **不读字体文件**；码点分档启发式 + padding 预算。
- 布局前须有 **MeasureParams**（字号/行高/padding…）；engine **不**依赖完整 render 主题。
- 详见 [`adr/005-content-measure-params.md`](adr/005-content-measure-params.md)。

## Engine 入口（ADR-006）

- [`LayoutContract`](../../crates/plotgram-model/src/contract.rs) = `layout` + `edge_routing?` + `graph` + **`node_sizes`**。
- `edge_routing: None` → Layout 内建写边；`Some` → `EdgeRouter` 写边（端口仍由 Layout 决议）。
- Crate：`engine-api`（Trait）→ `plotgram-engine`（`run` + in-tree `layout/` / `route/`；长大再拆）。详见 [`adr/006-engine-io-and-crates.md`](adr/006-engine-io-and-crates.md)。

## 管线位置

```
.pgm
  → parse（attrs 含 from_side / edge_group / role …；可选 @group 端点）
  → 展开 @group 糖（→ group_anchor nodes）
  → lift_all_node_structural_attrs / lift_all_edge_structural_attrs
  → Graph::validate_partition（若有 cell / grid）
  → profile expand
  → compile MeasureParams（主题中的度量字段；ADR-005）
  → 度量：preferred size → NodeSizes（+ ContentLayout）
  → LayoutContract { layout, edge_routing?, graph, node_sizes }
  → plotgram_engine::run（Layout / 可选 EdgeRouter；group 包络）
  → LayoutResult（EdgePlacement 含 PortRef）
  → RenderInput { graph, layout, meta }
  → SVG（跳过 group_anchor 形体；内容块展开 ContentLayout）
```

## `edge_routing` 语义

- **`None`**：布局内建边几何（Hier 正交、sequence 消息等）。
- **`Some`**：节点冻结后的独立路由器。
