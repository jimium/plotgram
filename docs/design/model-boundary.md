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
| `graph` | `Node`, `Edge`, `Group`, `Graph`, `Arrow` | engine, render |
| `port` | `Side`, `PortConstraint`（作者钉死）, `PortRef`（已决议） | engine |
| `contract` | `AlgorithmRef`, `LayoutContract` | **engine 入口** |
| `result` | `LayoutResult`, placements（含决议后的 `PortRef`） | engine 产出 |
| `render` | `RenderMeta`, `RenderInput` | **render 入口** |
| `profile` | `DiagramType`, `Profile` | **仅** DSL / profile（engine 禁止 import） |

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

## 硬约束

1. **`DiagramType` 不得出现在 engine**（ADR-001）。入口只认 `LayoutContract`。
2. **`AttrMap` 用 `BTreeMap`**（确定性）。
3. **model 不依赖其它 workspace crate**。
4. **边用稳定 `Edge.id`**；placement / label 用同一 id。
5. **端口**：作者约束在 `Edge`；决议 `PortRef` 在 `EdgePlacement`；Ink 不得发明。
6. **结构字段一等**：端口 / 边组不靠引擎读自由 attrs。

## 管线位置

```
.pgm
  → parse（attrs 含 from_side / edge_group …）
  → lift_structural_attrs（升到 Edge 字段并剥 attrs）
  → profile expand
  → LayoutContract { layout, edge_routing?, graph }
  → engine（组合相：端口决议 → … → 落笔）
  → LayoutResult（EdgePlacement 含 PortRef）
  → RenderInput { graph, layout, meta }
  → SVG
```

## `edge_routing` 语义

- **`None`**：布局内建边几何（Hier 正交、sequence 消息等）。
- **`Some`**：节点冻结后的独立路由器。
