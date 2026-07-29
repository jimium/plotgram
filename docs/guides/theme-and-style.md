# 主题与视觉风格使用指南

Plotgram 的视觉外观由 **Theme（StyleSheet）** 和 **render_style（笔触）** 两层控制。本文说明在 DSL 与重建后的 render crate 中如何指定它们。

> 规范：[style-sheet-spec.md](../specs/style-sheet-spec.md)

---

## 两层模型

```text
Theme (theme)          → 颜色、字号、边宽、kind 默认样式等「颜料」
render_style           → 几何绘制方式（standard / sketch）
```

- **resolve** 阶段（`plotgram-render`）：`defaults` → `kind_styles` → DSL `: shape` → 内联 `style.*`
- **render** 阶段：`render_style` 影响路径抖动、hatch 等笔触

主题 JSON **无** `diagrams` 段；图种差异不进主题（ADR-001）。

---

## 在 DSL 中指定

### Theme

```plotgram
diagram flowchart {
    theme: common.clean-light
    // ...
}
```

### 笔触（diagram 级）

```plotgram
diagram architecture {
    render_style: sketch
    // ...
}
```

### 节点 / 边 / 组内联样式

v2 **仅**支持内联 `style.*`（见 dsl-spec）：

```plotgram
node api "API" { kind: service, style.fill: "#E3F2FD" }
api -> db "查询" { style.stroke: "#C62828", style.dashed: true }
```

---

## Render 入口

重建管线：

```text
RenderInput { graph, layout, meta: { title, theme, render_style } }
  → plotgram_render::render_svg / render_ascii
```

```rust
use plotgram_model::render::{RenderInput, RenderMeta};
use plotgram_render::render_svg;

let svg = render_svg(&RenderInput {
    graph,
    layout,
    meta: RenderMeta {
        title: Some("示例".into()),
        theme: Some("common.blueprint".into()),
        render_style: Some("sketch".into()),
    },
});
```

未知 `theme` id 回退到 `common.clean-light`。

---

## 内置 Theme

嵌入于 `crates/plotgram-render/src/theme/themes/`。完整 id 列表见 [style-sheet-spec.md](../specs/style-sheet-spec.md) §8。

子主题可通过 `extends` 链式继承父主题（如 `mindmap.vivid-branches` → `mindmap.base`）。

---

## 相关文档

- [dsl-spec.md](../specs/dsl/dsl-spec.md) — `theme` / `render_style` / `style.*`
- [model-boundary.md](../design/model-boundary.md) — `RenderInput` 边界
- [ADR-001](../design/adr/001-diagram-type-not-in-engine.md) — 引擎 / 主题禁止按图种分支
