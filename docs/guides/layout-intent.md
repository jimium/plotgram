# Layout Intent 快速入门

Layout Intent 允许在**不修改** diagram 源码 `relations` 的前提下，向布局算法注入额外拓扑/几何约束，并返回每条意图的满足度报告。

> **完整指南**（数据结构、API、示例）：[architecture/intent/layout-intent-usage.md](../architecture/intent/layout-intent-usage.md)  
> 设计依据：[layout-intent-optimized.md](../architecture/intent/layout-intent-optimized.md)

---

## 何时使用

- 渲染期临时约束：「A 必须在 B 下方」但不画幽灵边
- 对齐需求：多节点垂直/水平对齐
- 固定位置：`Pin` 锁定某节点坐标

Intent 存在 **overlay** 参数中，不是图结构的一部分。

---

## 两类意图

| 类型 | 消费阶段 | 示例 |
|------|----------|------|
| `TopologyIntent` | 布局算法内（如 Sugiyama rank） | `Below` / `Above` |
| `GeometricIntent` | `apply_geometric_refinement` | `Pin`、`AlignVertical`、`AlignHorizontal` |

满足状态：`Satisfied` / `Partial` / `Conflicted` / `NotFound`（见 `RefinementReport`）。

---

## Rust 最小示例

```rust
use drawify_core::layout::intent::{
    GeometricIntent, LayoutIntentOverlay, TopologyIntent,
};
use drawify_core::layout::compute_layout_with_plan_and_overlay;
use drawify_core::render::{RenderRequest, RenderFormat};
use drawify_core::pipeline::render_text;

let overlay = LayoutIntentOverlay {
    topology: vec![TopologyIntent::Below {
        upper: "api".into(),
        lower: "db".into(),
    }],
    geometric: vec![GeometricIntent::AlignVertical {
        ids: vec!["a".into(), "b".into(), "c".into()],
    }],
};

let (layout, report) = compute_layout_with_plan_and_overlay(
    diagram,
    prepared.layout_plan(),
    Some(&overlay),
)?;

let mut req = RenderRequest::new(prepared, RenderFormat::Svg);
req.layout_overlay = Some(&overlay);
let svg = render_text(&req)?;
```

`RefinementReport` 记录每条意图是否满足、是否冲突。

---

## 与管线的关系

```text
compute_layout_with_plan_and_overlay
  ├─ 拓扑意图校验（环、矛盾）
  ├─ strategy.compute_with_overlay
  ├─ 几何微调（Pin / Align）
  └─ grid_snap / group_frame / routing / refine
```

详见 [render-pipeline.md](render-pipeline.md)。

---

## 约束

- 嵌套 `steps` 块内不能再嵌套 steps（多帧演示另议）
- 拓扑意图不能引入环；冲突意图会被跳过并记入 report
- Intent 不改变 `relations.len()`，不破坏 `relations[i] ↔ edges[i]` 索引

---

## 进一步阅读

- [layout-intent-usage.md](../architecture/intent/layout-intent-usage.md) — 完整 API 与用例
- [layout-refinement-todo.md](../architecture/intent/layout-refinement-todo.md) — 实施进度
