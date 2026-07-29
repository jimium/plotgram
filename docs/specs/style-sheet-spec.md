# Plotgram Theme（StyleSheet）规范

> 版本：2.0 | 状态：现行（以 `plotgram-render` 实现为准）
>
> 取代此前 v0.2-draft 的「三层 cascade + `diagrams.*`」草案。图种差异不进主题（ADR-001）。

本文档定义 Plotgram 主题 JSON 的结构与解析语义。实现入口：

- 主题加载 / 编译：`crates/plotgram-render/src/theme/`
- 样式 cascade：`crates/plotgram-render/src/resolve.rs`
- DSL 内联样式：见 [dsl-spec.md](dsl/dsl-spec.md) `style.*`

---

## 1. 设计原则

| 原则 | 说明 |
|------|------|
| 扁平主题 | 顶层只有 `tokens` / `defaults` / `kind_styles`；**无** `diagrams` 段 |
| Theme = 颜料 | 颜色、字体、线宽、圆角、默认 shape |
| `render_style` = 笔触 | `standard` / `sketch` 等，写在 diagram 属性，**不**写入主题 JSON |
| kind 驱动视觉差 | 同一主题下用 `kind_styles[kind]` 区分节点外观；不用图种命名空间 |
| 内联最高优 | DSL `style.*` 覆盖主题默认与 kind 样式 |

### 非目标

- 在主题 JSON 中描述手绘抖动 / hatch（属 `render_style`）
- 按 `DiagramType` 分支的主题段（违反 ADR-001）
- 实例级 structural / context palettes（`group_nest` / `branch`）— **暂缓**，待 `LayoutResult` 提供 depth / slot 索引后再设计

---

## 2. 术语

| 术语 | 含义 | 示例 |
|------|------|------|
| Theme / StyleSheet | 一份扁平视觉主题 JSON | `common.clean-light` |
| tokens | 可复用设计原料 | `colors.canvas` |
| defaults | 全局兜底（canvas / title / node / edge / group） | `defaults.node.fill` |
| kind_styles | 按节点 `kind` 覆盖的样式块 | `kind_styles.database` |
| cascade | resolve 时的优先级链 | 见 §6 |

---

## 3. 顶层结构

### 3.1 JSON 骨架

```json
{
  "id": "common.clean-light",
  "name": "Clean Light",
  "extends": null,
  "tokens": {
    "colors": {},
    "palette": {},
    "typography": {},
    "strokes": {},
    "radius": {},
    "spacing": {}
  },
  "defaults": {
    "canvas": {},
    "title": {},
    "node": {},
    "edge": {},
    "group": {}
  },
  "kind_styles": {
    "service": {},
    "database": {}
  }
}
```

### 3.2 顶层字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | `string` | 是 | 唯一标识，如 `common.clean-light` |
| `name` | `string` | 是 | 显示名 |
| `extends` | `string \| null` | 否 | 父主题 id；支持**链式**继承 |
| `tokens` | `object` | 是 | 设计 token |
| `defaults` | `object` | 否 | 全局兜底（缺省为空对象） |
| `kind_styles` | `object` | 否 | kind → 样式块 |

**禁止**字段：`diagrams`、`structural_palettes`、`context_palettes`、`version`（版本由本规范文档管理，不写进 JSON）。

### 3.3 `extends` 继承

- 子主题 `extends` 父主题 id；父主题可再 `extends`（链式）。
- Merge：对象字段 deep merge（子覆盖父）；`kind_styles` 按 kind 名 deep merge 属性。
- Merge **不**展开 token 引用；token 在 compile 阶段统一解析。
- 合并后 `id` / `name` 取子主题。

实现：`theme/compile.rs` 的 `resolve_extends` + `merge_theme_files`。

---

## 4. Tokens

`tokens` 通过 `{category.key}`（及 palette 的 `{palette.role.key}` / 兼容别名 `{role.role.key}`）引用。

| 组 | 用途 | 示例键 |
|----|------|--------|
| `colors` | 画布、文字、边、组等全局色 | `canvas`, `text`, `edge_stroke` |
| `palette` | 按角色的 fill/stroke/text_fill | `blue.fill`, `start_green.text_fill` |
| `typography` | 字体族、字号、字重 | `font_family`, `label_size` |
| `strokes` | 线宽与虚线数组 | `normal`, `dashed` → `"4,3"` |
| `radius` | 圆角 | `sm`, `md`, `lg`, `pill` |
| `spacing` | 间距（布局可参考；render 侧主要消费视觉 token） | `node_padding_x` |

### 4.1 Token 引用解析

- 字符串值中的 `{token.path}` 在 compile 时替换为字面量。
- 不可解析的 `{...}` **原样保留**，解析器保证前进（不死循环）。
- `palette.<role>.*` 同时注册为 `role.<role>.*`，便于主题稿写 `{role.blue.fill}`。

### 4.2 值类型

JSON 中样式值可以是：

- string（颜色、token 引用、shape 名、dasharray）
- number（字号、线宽、圆角）
- boolean
- number 数组（如 `[4, 3]` → SVG `"4,3"`）

---

## 5. Defaults

| 块 | 常见字段 |
|----|----------|
| `canvas` | `background` |
| `title` | `fill`, `font_size`, `font_weight` |
| `node` | `fill`, `stroke`, `stroke_width`, `text_fill`, `font_size`, `font_weight`, `radius`, `shape`, `stroke_linecap`, `stroke_linejoin`, opacities, `stroke_dasharray` |
| `edge` | `stroke`, `stroke_width`, `text_fill`, `font_size`, `arrow_style`（`normal` / `hollow` / `none`）, `arrow_fill`, `response_dasharray`（`-->` 回程线虚线模式，默认 `6,4`）, linecap/linejoin/opacity, `label_bg`, `label_bg_opacity` |
| `group` | `fill`, `stroke`, `stroke_width`, `text_fill`, `radius`, `stroke_dasharray` |

`label_bg: "canvas"` 在 render 时解析为画布背景色，用于边标签衬底。

---

## 6. Cascade（resolve）

Render 入口收到 `RenderInput { graph, layout, meta }` 后：

```text
theme = load(meta.theme)
resolved = resolve_graph(graph, theme)
```

### 6.1 节点

```text
defaults.node
  → kind_styles[node.kind]   （若有；编译期已相对 defaults 物化）
  → DSL `: shape`            （显式形状覆盖）
  → node.attrs["style.*"]    （最高）
  → icon 解析（icon: / kind 推断 + shape 兼容性）
```

### 6.2 边

```text
defaults.edge
  → edge.attrs["style.*"]
  → Arrow::Response 且尚无 dash → stroke_dasharray = "6,4"
```

支持的内联键：`style.stroke`, `style.stroke_width`, `style.dashed`, `style.stroke_dasharray`, `style.stroke_linecap`, `style.stroke_linejoin`, `style.stroke_opacity`。

### 6.3 组

```text
defaults.group
  → group.attrs["style.*"]
```

支持：`style.fill`, `style.stroke`, `style.stroke_width`, `style.text_fill`, `style.radius`, `style.dashed`, `style.stroke_dasharray`。

### 6.4 与 DSL 的边界

- 批量主题：diagram `theme:` atom → `RenderMeta.theme`
- 笔触：diagram `render_style:` → `RenderMeta.render_style`（非主题字段）
- 单元素覆盖：仅内联 `style.*`（无顶层 `node_style` / `edge_style` 声明）

---

## 7. `kind_styles`

键为节点 `kind`（atom）。值是与 `defaults.node` 同形的部分样式块；compile 时相对 `defaults.node` 填充缺省字段。

常见 kind：`service`, `database`, `person`, `gateway`, `cache`, `queue`, `start`, `end`, `decision`, `root`, `leaf`, …（以各主题 JSON 为准）。

主题**不**按 flowchart / mindmap 分命名空间；mindmap 主题通过不同 `kind_styles` 与默认值表达差异（如 `mindmap.base`）。

---

## 8. 内置主题 id

嵌入于 `plotgram-render`：

| id | 说明 |
|----|------|
| `common.clean-light` | 默认浅色 |
| `common.clean-dark` | 深色 |
| `common.blueprint` | 蓝图 / hollow 箭头 |
| `common.paper-ink` | 纸墨 |
| `common.github-light` / `common.github-dark` | GitHub 风 |
| `common.presentation` | 演示 |
| `common.floating-cards` | 卡片 |
| `common.dual-channel` | 双通道 |
| `common.okabe-ito` | 色觉友好 |
| `mindmap.base` | 思维导图基座（常 `arrow_style: none`） |
| `mindmap.ink-dark` / `mindmap.vivid-branches` | 导图变体（可 `extends`） |

未知 id 回退到 `common.clean-light`。

---

## 9. 管线位置

```text
.pgm
  → parse / profile expand
  → LayoutContract → engine → LayoutResult
  → RenderInput { graph, layout, meta: { title, theme, render_style } }
  → theme::load + resolve_graph
  → SVG（或 ASCII）
```

| 产物 | 含有 | 不含 |
|------|------|------|
| `LayoutResult` | 几何框、折线、label 槽 | shape / kind / theme |
| Theme JSON | tokens、defaults、kind_styles | render_style、layout 参数 |
| resolve 输出 | 每元素已解析颜色/线宽/shape | 布局坐标 |

---

## 10. 相对旧草案的变更

| v0.2-draft | V2 现行 |
|------------|---------|
| `diagrams.<type>.entity_types` | `kind_styles` |
| 三层 cascade + prepare 物化到 AST | render 侧 `resolve_graph` |
| `context_palettes` / `structural_palettes` | **暂缓删除** |
| 单层 `extends` | 链式 `extends` |
| `GraphicStyle` 写入 StyleSheet | 禁止；用 `render_style` |

旧 v1 / prepare 物化路径仅存在于 `crates/v1/`，不驱动本规范。
