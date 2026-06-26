# 渲染管线使用指南

Drawify 核心把「DSL 文本 → 像素/矢量输出」拆成多个纯函数阶段，由 `drawify_core::pipeline` 编排。

> 实现：`crates/drawify-core/src/pipeline/`

---

## 总览

```text
DSL 源码
  ↓ parse()                    RawDiagram
  ↓ prepare()                  PreparedDiagram（样式物化、layout_plan）
  ↓ validate()                 语义校验（可选）
  ↓ compute_layout()           LayoutResult
  ↓ build_scene()              ExportScene
  ↓ encode                     RenderOutput（SVG / PNG / …）
```

**原则**：各阶段只依赖上一阶段的类型；不在 parser 里做布局，不在 layout 里做编码。

---

## 阶段与类型

| 阶段 | 函数 | 输入 → 输出 | 说明 |
|------|------|-------------|------|
| 解析 | `pipeline::parse` | `&str` → `RawDiagram` | Parser 直出；可有缺省 `entity.type` |
| 预处理 | `pipeline::prepare` | `RawDiagram` → `PreparedDiagram` | theme 物化、layout_plan 解析 |
| 校验 | `validation::validate` | `&PreparedDiagram` → `ValidationResult` | 语义规则 |
| 布局 | `layout::compute_layout_with_plan` | `&Diagram` + plan → `LayoutResult` | 节点/分组/边几何 |
| 场景 | `render::scene::build_scene` | `RenderRequest` + layout → `ExportScene` | 视觉属性物化 |
| 编码 | `render::encode_*` | `ExportScene` → `RenderOutput` | 格式相关 |

### 关键类型

| 类型 | 层级 | 典型用途 |
|------|------|----------|
| `RawDiagram` | 作者意图 | diff2、patch、format |
| `PreparedDiagram` | 可渲染态 | validate、layout、render |
| `LayoutResult` | 几何 | LayoutLint、drawify-eval |
| `ExportScene` | 渲染中间层 | 多格式共享 |
| `RenderOutput` | `Text` / `Binary` | 最终产物 |

---

## 编排入口

### 一站式：`pipeline::run`

```rust
use drawify_core::pipeline::{self, PipelineResult};
use drawify_core::prepare::StyleRequest;
use drawify_core::render::RenderFormat;

match pipeline::run(source, &StyleRequest::default(), RenderFormat::Svg) {
    PipelineResult::Ok(output) => { /* RenderOutput */ }
    PipelineResult::Errors { errors, warnings } => { /* 诊断 */ }
}
```

适合 CLI / Server / WASM 的「给源码要图」场景。

### 分步：`parse_prepare_validate`

```rust
use drawify_core::pipeline::{parse_prepare_validate, parse_prepare};
use drawify_core::prepare::StyleRequest;

// 标准：校验 + 渲染
let output = parse_prepare_validate(source, &StyleRequest::default());
if output.is_valid() {
    let prepared = output.diagram.unwrap();
}

// 不校验：diff / export / patch
let output = parse_prepare(source, &StyleRequest::default());
```

### 渲染：`render_*`

```rust
use drawify_core::pipeline::{render_text, render_bytes, render_json, render_output};
use drawify_core::render::{RenderFormat, RenderRequest};

let request = RenderRequest::new(&prepared, RenderFormat::Svg);
let svg = render_text(&request)?;
```

| 函数 | 返回 |
|------|------|
| `render_text` | `String`（SVG、ASCII、JSON 文本等） |
| `render_bytes` | `Vec<u8>`（PNG、WebP） |
| `render_json` | PreparedDiagram JSON 字符串 |
| `render_output` | `RenderOutput` 枚举 |

### 仅布局

```rust
use drawify_core::layout::compute_layout_with_plan;

let layout = compute_layout_with_plan(prepared.inner(), prepared.layout_plan())?;
```

用于 LayoutLint、drawify-eval、自定义分析。

---

## `StyleRequest`（prepare 阶段）

```rust
pub struct StyleRequest {
    pub theme_id: Option<String>,  // 显式 theme，最高优先级
    pub dark_mode: bool,
}
```

Theme 解析优先级（简）：

1. `StyleRequest.theme_id`
2. diagram 属性 `theme: "..."`
3. profile 的 `dark_theme_id`（`dark_mode=true`）
4. profile 的 `default_theme_id`

---

## `RenderRequest`（render 阶段）

除 `diagram` + `format` 外，常用字段：

| 字段 | 说明 |
|------|------|
| `explicit_theme_id` | 覆盖 scene theme |
| `explicit_style_json` | 内联 StyleSheet JSON |
| `explicit_graphic_style` | Excalidraw、SpatialClarity 等 |
| `dark_mode` | 暗色 theme |
| `attribution` | SVG 右下角署名（默认 true） |
| `transparent_background` | 省略画布背景 |
| `layout_overlay` | Layout Intent 叠加层 |
| `ascii_options` | ASCII 导出选项 |

构建示例：

```rust
let mut req = RenderRequest::new(&prepared, RenderFormat::Svg);
req.explicit_graphic_style = Some(GraphicStyleId::Excalidraw);
req.transparent_background = true;
```

Theme / graphic style 细节见 [theme-and-style.md](theme-and-style.md)。

---

## 各入口用哪条路径

| 入口 | 典型调用链 |
|------|------------|
| CLI `render` | `parse_prepare_validate` → `RenderRequest::new` → `render_bytes/text` |
| CLI `validate` | `parse_prepare_validate` |
| CLI `export` | `parse_prepare` → `render_json` |
| CLI `diff` | `parse` ×2 → `diff2::diff` |
| CLI `lint` | `parse_prepare_validate` → `compute_layout` → `LayoutLinter` |
| Markdown 导入 | `import_interchange` → `import_prepare_validate` → render |
| Server `/render` | 同 CLI render |
| diff2 patch | `parse` → `patch` → `prepare` |

---

## Layout Intent 透传

`RenderRequest.layout_overlay` 传入 `LayoutIntentOverlay`，在 `compute_layout_with_plan_and_overlay` 中消费。

Quick Start 见 [layout-intent.md](layout-intent.md)。

---

## 错误处理

- **parse / prepare / validate**：`PipelineOutput` 含 `errors`、`warnings`、`truncated`
- **layout**：`Result<LayoutResult, DiagnosticError>`
- **render**：`Result<RenderOutput, DrawifyError>`

诊断类型为 `DiagnosticError`，含行列号与 fix suggestion，与 [error-model.md](../specs/error-model.md) 一致。

---

## 相关文档

- [drawify-cli.md](drawify-cli.md) — 命令行封装
- [theme-and-style.md](theme-and-style.md) — 主题与视觉风格
- [layout-lint.md](layout-lint.md) — 布局质量检查
- [drawify-eval.md](drawify-eval.md) — 算法评分
