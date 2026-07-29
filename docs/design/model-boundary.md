# plotgram-model 边界

> 状态：已落地 | 对应 crate：`crates/plotgram-model`

## 职责

model 是**纯数据层**：定义所有 crate 共享的类型，不含逻辑（无解析、无布局、无渲染算法）。

## 模块

| 模块 | 核心类型 | 消费者 |
|------|----------|--------|
| `geometry` | `Point`, `Rect` | 全部 |
| `attr` | `AttrValue`, `AttrMap`（`BTreeMap` 别名） | 全部 |
| `graph` | `Node`, `Edge`（含稳定 `id`）, `Group`, `Graph`, `Arrow` | engine, render |
| `port` | `Side`, `PortRef`（已解析端口）, `PortConstraint`（作者约束） | engine |
| `contract` | `AlgorithmRef`, `LayoutContract` | **engine 入口** |
| `result` | `LayoutResult`, `NodePlacement`, `EdgePlacement`, `LabelSlot`, `LabelOwner` | engine 产出 |
| `render` | `RenderMeta`, `RenderInput` | **render 入口** |
| `profile` | `DiagramType`, `Profile` | **仅** DSL / profile 展开（engine 禁止 import） |

## 硬约束

1. **`DiagramType` 不得出现在 engine 代码中**（ADR-001）。engine 入口只认 `LayoutContract`。
2. **`AttrMap` 用 `BTreeMap`**（AGENTS.md 确定性迭代）。
3. **model 不依赖其它 workspace crate**（依赖链叶子）。
4. **边用稳定 `Edge.id`**；`EdgePlacement` / `LabelOwner::Edge` 引用同一 id（支持平行边）。
5. **`AttrValue` 无嵌套 map**；diagram 级算法配置用 `AlgorithmRef`，不塞进 attrs。

## 管线位置

```
.pgm
  → parse
  → profile expand          (DiagramType / Profile；不进 engine)
  → LayoutContract          (layout + edge_routing? + Graph)
  → engine
  → LayoutResult            (纯几何)
  → RenderInput { graph, layout, meta }   (meta = title/theme/render_style)
  → render → SVG
```

| 阶段产物 | 含有 | 不含 |
|----------|------|------|
| `LayoutContract` | 算法名/参数、`Graph` | `diagram_type`、theme、title |
| `LayoutResult` | 框、折线、label 槽位 | shape / kind / style / theme |
| `RenderInput` | `Graph` + `LayoutResult` + `RenderMeta` | 布局算法决策 |

## `edge_routing` 语义

- **`None`**：布局内建边几何（Hier 正交主路径、sequence 消息等）。Hier 类 profile 默认如此。
- **`Some(AlgorithmRef)`**：节点冻结后的**独立**路由器。作者可显式写 `edge_routing: orthogonal` 打开。
