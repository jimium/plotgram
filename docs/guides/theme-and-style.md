# 主题与视觉风格使用指南

Drawify 的视觉外观由 **Theme（StyleSheet）** 和 **Graphic Style（手绘/蓝图等渲染风格）** 两层控制。本文说明在 DSL、CLI 和 Rust API 中如何指定它们。

> 规范：[style-sheet-spec.md](../specs/style-sheet-spec.md)

---

## 两层模型

```text
StyleSheet (theme)     → 颜色、字号、边宽、分组填充等「语义样式」
GraphicStyle           → 几何绘制方式（标准 / Excalidraw / Spatial Clarity …）
```

- **prepare** 阶段：theme cascade 物化到 `entity.attributes.style`
- **render** 阶段：graphic style 影响 SVG 路径、滤镜、marker

---

## 在 DSL 中指定

### Theme

```drawify
diagram flowchart {
    theme: "common.clean-light"
    // ...
}
```

### Graphic Style（diagram 级）

```drawify
diagram architecture {
    graphic_style: excalidraw
    // ...
}
```

具体可用 id 见 `GraphicStyleId` 与 profile 默认值。

### 实体 / 边局部样式

在 `entity` / `relation` 的 `style { }` 或 `standard { }` 块中覆盖（物化前写入 RawDiagram，prepare 时与 theme 合并）。

---

## CLI

当前 `drawify render` **未暴露** `--theme` / `--graphic-style` 参数，使用 diagram 内声明 + profile 默认 theme。

自定义 theme JSON 需在 Rust `RenderRequest.explicit_style_json` 中传入（见下文）。

---

## Rust API

### prepare：`StyleRequest`

```rust
use drawify_core::prepare::StyleRequest;

let req = StyleRequest {
    theme_id: Some("common.clean-light".into()),
    dark_mode: false,
};
let output = parse_prepare_validate(source, &req)?;
```

Theme 优先级：`StyleRequest.theme_id` > diagram `theme` 属性 > dark 默认 > profile 默认。

### render：`RenderRequest`

```rust
use drawify_core::render::{RenderRequest, RenderFormat};
use drawify_core::types::GraphicStyleId;

let mut req = RenderRequest::new(&prepared, RenderFormat::Svg);
req.explicit_theme_id = Some("common.clean-light");
req.dark_mode = false;
req.explicit_graphic_style = Some(GraphicStyleId::Excalidraw);
req.transparent_background = true;
req.attribution = false;
```

### 内联 StyleSheet JSON

```rust
req.explicit_style_json = Some(r##"{
    "version": "0.2",
    "id": "custom.demo",
    "name": "Demo",
    "tokens": { "colors": { "canvas": "#101820" } },
    "defaults": {
        "canvas": { "background": "#101820" },
        "node": { "fill": "#243447", "stroke": "#67B7D1" },
        "edge": { "stroke": "#67B7D1" }
    },
    "diagrams": {}
}"##);
```

`explicit_style_json` 优先级高于 builtin theme。

### Graphic Style 解析顺序

1. `RenderRequest.explicit_graphic_style`
2. diagram 属性 `graphic_style`
3. profile `default_graphic_style`

---

## 内置 Theme

主题 JSON 位于仓库 theme 资源目录；内置主题说明见 specs 目录下样式相关文档。

注意：**不能**直接使用 internal base theme（如仅用于 `extends` 的基座），`prepare` 会报错。

---

## 调试技巧

```bash
# 查看物化后的 AST（含 attributes.style）
drawify export diagram.dfy | jq '.entities[0].attributes'

# 对比不同 theme 的渲染
# （需在 Rust 测试或 playground 中切换 RenderRequest）
```

---

## 相关文档

- [render-pipeline.md](render-pipeline.md) — prepare / render 阶段
- [graphic-style-and-theme.html](../architecture/graphic-style-and-theme.html) — 架构说明
- [crates/drawify-core/src/graphic_style/README.md](../../crates/drawify-core/src/graphic_style/README.md) — 各 graphic style 实现
