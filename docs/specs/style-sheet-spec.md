# Drawify StyleSheet 规范

> 版本：0.2.0-draft | 状态：设计中
>
> 取代 [style-sheet-spec.md](../style-sheet-spec.md) v0.1 的结构方向。v0.1 保留作历史参考，新实现以本文档为准。

本文档定义 Drawify 样式方案（`StyleSheet`）的 JSON 结构与解析语义。核心原则：

- **所有视觉默认值以数据形式存在于 StyleSheet**，不在 Renderer 中硬编码
- **三层 cascade**：全局兜底 → 图表命名空间 → entity type
- **物化在 `prepare()` 完成**，Renderer 只读 `attributes.style`

---

## 1. 背景与动机

### 1.1 设计目标


| 目标               | 说明                                        |
| ---------------- | ----------------------------------------- |
| 单一数据源            | per-type 默认样式全部进入 StyleSheet JSON         |
| 三层 cascade       | 全局兜底 → 图表级 → type 级，语义清晰                  |
| 主题可替换            | 换 `builtin.blueprint` 即换全套视觉，无需改 Renderer |
| 与 Expand Pass 对齐 | 样式在 `prepare()` 物化，布局与渲染消费同一 AST          |
| 职责分离             | StyleSheet 管视觉；DiagramProfile 管语义与主题引用    |


### 1.3 非目标

- 图形风格（手绘、草图）— 见 `GraphicStyle`，不写入 StyleSheet
- 布局意图 — 见 [layout-intent-refinement.md](../../architecture/layout-intent-refinement.md)
- CSS 兼容语法、动画、主题编辑器 UI

---

## 2. 核心概念

### 2.1 术语


| 术语               | 含义                        | 示例                                        |
| ---------------- | ------------------------- | ----------------------------------------- |
| **StyleSheet**   | 一份完整的视觉主题 JSON            | `builtin.blueprint`                       |
| **tokens**       | 可复用的设计 token（颜色、字号等）      | `colors.primary`                          |
| **defaults**     | 第一层：跨图表全局兜底               | `defaults.node.fill`                      |
| **diagrams**     | 第二层：按 `DiagramType` 的命名空间 | `diagrams.flowchart`                      |
| **entity_types** | 第三层：图表内按 `entity.type` 覆盖 | `diagrams.flowchart.entity_types.service` |
| **shape**        | 节点几何形态，是样式属性而非独立层级        | `rounded_rect`, `cylinder`                |


> 「图形」在本规范中指 **DiagramType**（flowchart、sequence 等），不是 `NodeShape`。

### 2.2 职责边界


| 模块                 | 负责                                                             | 不负责               |
| ------------------ | -------------------------------------------------------------- | ----------------- |
| **StyleSheet**     | tokens、三层 cascade 全部视觉数据                                       | 语义 `type` 默认值     |
| **DiagramProfile** | 支持的 entity type 列表、`default_entity_type`、**引用哪套 StyleSheet**   | 具体 fill/stroke 颜色 |
| **prepare()**      | 按优先级将 cascade 物化到 `attributes.style`                           | —                 |
| **Renderer**       | `attributes.style` → `NodeStyle`；canvas/title；graphic_style 装饰 | `match type` 猜颜色  |


### 2.3 与图形风格的边界


| 概念           | 内容                | 例子                |
| ------------ | ----------------- | ----------------- |
| StyleSheet   | 颜色、字体、线宽、圆角、shape | `fill: "#E3F2FD"` |
| GraphicStyle | 绘制方式、笔触           | 标准、手绘、草图          |


StyleSheet JSON 中**不得**出现 `hand_drawn`、`roughness`、`wobble` 等字段。

---

## 3. 顶层结构

### 3.1 JSON 骨架

```json
{
  "version": "0.2",
  "id": "builtin.blueprint",
  "name": "Blueprint",
  "meta": {
    "author": "Drawify",
    "description": "工程制图风格，适合架构图与技术文档",
    "tags": ["builtin", "blueprint", "technical"]
  },
  "tokens": {
    "colors": {},
    "typography": {},
    "strokes": {},
    "radius": {},
    "spacing": {},
    "effects": {}
  },
  "defaults": {
    "canvas": {},
    "title": {},
    "node": {},
    "edge": {},
    "group": {}
  },
  "diagrams": {
    "flowchart": {
      "node": {},
      "edge": {},
      "group": {},
      "entity_types": {},
      "edge_kinds": {}
    }
  }
}
```

### 3.2 顶层字段


| 字段         | 类型       | 必填    | 说明                             |
| ---------- | -------- | ----- | ------------------------------ |
| `version`  | `string` | 是     | 规范版本，当前为 `"0.2"`               |
| `id`       | `string` | 是     | 唯一标识，如 `builtin.blueprint`     |
| `name`     | `string` | 是     | 显示名                            |
| `extends`  | `string` | 否     | 单层继承：指向基座 theme_id（基座不得有 `extends`） |
| `meta`     | `object` | 否     | 作者、描述、标签                       |
| `tokens`   | `object` | 是     | 设计 token 集合                    |
| `defaults` | `object` | 是     | 第一层全局兜底                        |
| `diagrams` | `object` | 否     | 第二层图表命名空间；键为 `DiagramType` 小写名 |

#### `extends` 继承机制

- **单层**：子主题 `extends` 一个无 `extends` 的基座；禁止链式（A extends B, B extends C）。
- **merge 语义**：对象 deep merge；**数组整段替换**；merge **不展开** token。
- `context_palettes` 按 palette `id` deep merge（`entries` 整段替换）。
- merge 后 `sheet.id = overlay.id`（子主题保留自己的 ID）。


### 3.3 v0.1 → v0.2 字段映射


| v0.1                                  | v0.2                                       |
| ------------------------------------- | ------------------------------------------ |
| `diagram_defaults`                    | `defaults`                                 |
| `diagram_overrides.{type}`            | `diagrams.{type}`                          |
| `diagram_overrides.{type}.node_kinds` | `diagrams.{type}.entity_types`             |
| `diagram_overrides.{type}.edge_kinds` | `diagrams.{type}.edge_kinds`               |
| （无）                                   | `diagrams.{type}.node/edge/group` 作为图表级第二层 |


---

## 4. 三层 Cascade

### 4.1 结构示意

```text
tokens（设计原料，不直接绑定节点）
    ↓ 引用解析
defaults（第一层：全局兜底）
    ↓ 合并
diagrams.{diagram_type}.node / edge / group（第二层：图表级）
    ↓ 合并
diagrams.{diagram_type}.entity_types.{type}（第三层：type 级）
    ↓ prepare() 物化
entity.attributes.style
```

### 4.2 各层职责

#### 第一层：`defaults`

所有图表共享的兜底视觉。任何字段在更深层未定义时，回退到此层。

典型内容：

- `defaults.canvas.background`
- `defaults.node.fill`、`stroke`、`shape`、`radius`
- `defaults.edge.stroke`、`stroke_width`
- `defaults.group.fill`、`stroke`
- `defaults.title` 文本样式

#### 第二层：`diagrams.{type}`

按 `DiagramType` 的命名空间。覆盖该图表下所有节点的图表级默认。

典型差异：

- `sequence`：参与者框更窄、字号更小
- `architecture`：组框边框更粗、强调结构感
- `er`：表格式节点，无 `entity_types` 第三层
- `mindmap`：根节点与分支的全局倾向

第二层可包含 `node`、`edge`、`group`、`title` 子对象，语义与 `defaults` 同构。

#### 第三层：`diagrams.{type}.entity_types`

按 `entity.type`（语义类型）覆盖。可定义 `fill`、`stroke`、`shape`、`stroke_width`、`stroke_dasharray`、`label_weight` 等。

```json
"entity_types": {
  "service": {
    "fill": "#E8F4FD",
    "stroke": "#1565C0",
    "shape": "rounded_rect"
  },
  "database": {
    "fill": "#E8EEF4",
    "stroke": "#0D47A1",
    "shape": "cylinder"
  }
}
```

**shape 是第三层的属性**，不单独设层级。`database` 配 `cylinder`、`decision` 配 `diamond` 即可。

### 4.3 Fallback 链

对任意节点样式字段 `field`，解析顺序：

```text
1. entity.attributes.style[field]     （用户内联，最高，在 prepare 前/后均不覆盖）
2. diagrams[type].entity_types[t][field]
3. diagrams[type].node[field]
4. defaults.node[field]
5. tokens 引用解析后的字面量
6. 引擎最小安全值（仅防崩溃，不承载设计语义）
```

边（edge）同理：

```text
relation.attributes.style[field]
  → diagrams[type].edge_kinds[kind][field]   （若有）
  → diagrams[type].edge[field]
  → defaults.edge[field]
  → tokens
  → 引擎最小安全值
```

### 4.4 无 entity type 的图表


| DiagramType    | 第三层                | 说明                                     |
| -------------- | ------------------ | -------------------------------------- |
| `er`           | 不使用 `entity_types` | ER 实体无语义 type，仅 defaults + diagrams.er |
| `flowchart`    | 完整 entity_types    | 见 visual-language                      |
| `sequence`     | 完整 entity_types    | 见 visual-language                      |
| `state`        | 完整 entity_types    | initial / state / final / choice       |
| `architecture` | 完整 entity_types    | 含 frontend / backend 等                 |
| `mindmap`      | 完整 entity_types    | root / main / branch / leaf            |


### 4.5 Context Palettes（实例级样式）

三层 cascade 解决 **类型级** 配色（同一 `entity.type` 在所有实例上配色一致）。但有些场景需要 **实例级** 配色：同一 type 的不同实例因图结构而不同色（mindmap 分支色、architecture group 嵌套色等）。

`context_palettes` 挂在 `diagrams.<diagram_type>` 下（与 `entity_types` 同级），通过 **palette 数组 + 下标 + 绑定规则** 实现实例级样式。

```json
"diagrams": {
  "mindmap": {
    "entity_types": { "..." },
    "context_palettes": {
      "branch": {
        "entries": [
          { "fill": "#E8DAEF", "stroke": "#8E44AD", "edge_stroke": "#8E44AD" },
          { "fill": "#D5F5E3", "stroke": "#27AE60", "edge_stroke": "#27AE60" }
        ],
        "index": { "from": "branch_slot", "wrap": true },
        "bindings": [
          { "target": "entity", "types": ["main", "branch"], "fields": { "fill": "fill", "stroke": "stroke" } },
          { "target": "edge", "fields": { "stroke": "edge_stroke" } }
        ]
      }
    }
  }
}
```

| 字段 | 说明 |
|------|------|
| `entries[]` | 调色板条目；值可含 sheet 级 token（`{colors.*}` / `{role.*}`）及 compile 期颜色函数表达式（`{lighten(...)}` / `{darken(...)}`），compile 时展开为字面量 |
| `index.from` | 下标来源：`branch_slot` / `tree_depth` / `group_depth` |
| `index.wrap` | `true` 时 `index % entries.len()`（分支并列区分） |
| `index.cap` | 上限值；`min(index, cap)`。省略时默认 `entries.len() - 1` |
| `bindings[]` | 该 palette 作用于哪类图元、覆盖哪些字段 |
| `bindings[].target` | `entity` / `edge` / `group` |
| `bindings[].types` | （可选）仅作用于这些 entity type / edge kind |
| `bindings[].fields` | 目标 StyleBlock 键 → entry 键的映射 |

**下标计算**：

```text
raw = ctx.{from}.unwrap_or(0)
cap = index.cap.unwrap_or(entries.len() - 1)
index = if wrap { raw % entries.len() } else { min(raw, cap) }
entry = entries[index]
```

**物化语义**：`materialize_*` 时，对每个 matching binding，从 `entry` 中按 `fields` 映射取出字段，用 **`insert`（强制覆盖）** 语义 overlay 到类型级块上。最终写入 `attributes.style` 时用 **`or_insert`** 语义，保护内联 `style.*` 不被覆盖。

**内置 palette id 约定**：

| palette id | diagram | 说明 |
|------------|---------|------|
| `branch` | `mindmap` | 并列分支色带 |
| `group_nest` | `architecture` / `flowchart` | 嵌套 group 背景递进（替代 render 算法派生） |
| `edge_depth` | `mindmap` | 边线随深度变细 |

> `group_nest` 的 `entries[1+]` 使用 `{lighten(...)}` / `{darken(...)}` 表达式，子主题仅改 `tokens.colors.group_fill` 即可让所有 depth 档位自动重算。


---

## 5. Tokens

`tokens` 定义可复用设计原料，通过 `{category.key}` 引用。

### 5.1 结构

```json
{
  "tokens": {
    "colors": {
      "canvas": "#F6FBFF",
      "text": "#154360",
      "muted_text": "#5D7A8A",
      "border": "#90A4BE",
      "primary": "#1565C0",
      "success": "#2E7D32",
      "warning": "#E65100",
      "danger": "#C62828"
    },
    "typography": {
      "font_family": "'Noto Sans CJK SC', 'Segoe UI', sans-serif",
      "title_size": 16,
      "label_size": 13,
      "small_size": 11,
      "font_weight_regular": 400,
      "font_weight_medium": 500,
      "font_weight_bold": 700
    },
    "strokes": {
      "thin": 1.0,
      "normal": 1.5,
      "thick": 2.0,
      "dashed": [6, 3],
      "dotted": [2, 4]
    },
    "radius": {
      "sm": 4,
      "md": 6,
      "lg": 10,
      "pill": 999
    },
    "spacing": {
      "node_padding_x": 12,
      "node_padding_y": 8,
      "group_padding": 16,
      "label_gap": 6
    },
    "effects": {
      "shadow": false
    }
  }
}
```

### 5.2 引用语法

```json
"stroke_width": "{strokes.normal}",
"fill": "{colors.primary}"
```

支持以下 token 命名空间：

| 命名空间 | 示例 | 说明 |
|---------|------|------|
| `{colors.*}` | `{colors.canvas}` | 画布、默认 node/edge/group 配色 |
| `{typography.*}` | `{typography.label_size}` | 字号、字重、字体族 |
| `{strokes.*}` | `{strokes.normal}` | 线宽、虚线 pattern |
| `{radius.*}` | `{radius.md}` | 圆角 |
| `{spacing.*}` | `{spacing.node_padding_x}` | 间距 |
| `{effects.*}` | `{effects.shadow}` | 效果开关 |
| `{role.*}` | `{role.blue.fill}` | **按 entity type 的语义配色**（见 §5.4） |

#### compile 期颜色函数表达式

token 引用之外，`context_palettes.entries` 的值还支持 **compile 期颜色函数表达式**，在 compile 阶段求值为字面量：

```
{lighten(<color_expr>, <amount>)}
{darken(<color_expr>, <amount>)}
```

- `<color_expr>`：hex 字面量（`#F1F2F6`）或 sheet 级 token（`{colors.group_fill}`），先展开为 hex
- `<amount>`：浮点数字面量（`0.35`），表示混合比例
- 整个 `{lighten(...)}` / `{darken(...)}` 在 compile 期求值，替换为结果 hex 字符串

`lighten` 向白色混合（提亮），`darken` 向黑色混合（加深），算法对 R/G/B 三通道独立计算。

### 5.3 颜色格式

支持：`#RRGGBB`、`#RRGGBBAA`。第一版不建议 HSL、命名色、CSS 变量。

数值单位默认为逻辑像素（JSON number，不带 `px` 后缀）。

### 5.4 `tokens.palette` 与 `{role.*}`

`tokens.palette` 是按 entity type 的语义配色命名空间。基座 `entity_types` 中使用 `{role.<角色>.<fill|stroke|text_fill|edge_stroke>}` 引用，compile 时展开为字面量。

```json
"tokens": {
  "palette": {
    "blue": { "fill": "#E3F2FD", "stroke": "#1976D2", "text_fill": "#0D47A1" },
    "green": { "fill": "#E8F5E9", "stroke": "#2E7D32", "text_fill": "#1B5E20" }
  }
}
```

```json
"entity_types": {
  "service": {
    "shape": "rounded_rect",
    "fill": "{role.blue.fill}",
    "stroke": "{role.blue.stroke}"
  }
}
```

子主题只改 `tokens.palette` 即可换全套 entity 配色，无需逐 type 重写 hex。

> **`{branch.*}` 已废弃**：旧版 contextual token `{branch.fill}` / `{branch.stroke}` / `{branch.edge_stroke}` 已删除，统一走 `context_palettes`（见 §4.5）。

---

## 6. 视觉属性 Schema

### 6.1 节点（node / entity_types）


| 字段                 | 类型       | 说明                                                 |
| ------------------ | -------- | -------------------------------------------------- |
| `fill`             | `string` | 填充色                                                |
| `stroke`           | `string` | 边框色                                                |
| `stroke_width`     | `number` | 线宽                                                 |
| `stroke_dasharray` | `string` | 虚线 pattern，如 `"5,3"`                               |
| `shape`            | `string` | 见 §6.3                                             |
| `radius`           | `number` | 圆角（对 rounded_rect 等生效）                             |
| `text_fill`        | `string` | 标签文字颜色                                             |
| `font_size`        | `number` | 标签字号                                               |
| `label_weight`     | `string` | 字重：`regular` / `medium` / `bold`                   |
| `width`            | `number` | 布局 hint（可选）                                        |
| `height`           | `number` | 布局 hint（可选）                                        |
| `transform`        | `string` | SVG transform（如 `skewX(-10)`，architecture queue 用） |


### 6.2 边（edge / edge_kinds）


| 字段                 | 类型        | 说明     |
| ------------------ | --------- | ------ |
| `stroke`           | `string`  | 线条颜色   |
| `stroke_width`     | `number`  | 线宽     |
| `stroke_dasharray` | `string`  | 虚线     |
| `dashed`           | `boolean` | 简写虚线开关 |
| `text_fill`        | `string`  | 边标签颜色  |
| `font_size`        | `number`  | 边标签字号  |


> 箭头形态（`ArrowType` → `ArrowStyle`）属于**渲染语义**，由 Renderer 根据 relation.arrow 决定，不写入 StyleSheet。

### 6.3 shape 枚举


| 值              | 说明         |
| -------------- | ---------- |
| `rect`         | 矩形         |
| `rounded_rect` | 圆角矩形（默认）   |
| `circle`       | 圆形         |
| `diamond`      | 菱形         |
| `cylinder`     | 圆柱（数据库）    |
| `hexagon`      | 六边形        |
| `person`       | 人形         |
| `stadium`      | 胶囊形（开始/结束） |


### 6.4 画布、标题、分组

与 v0.1 保持一致：

- `canvas.background`
- `title.fill`、`font_size`、`font_weight`
- `group.fill`、`stroke`、`stroke_width`、`radius`、`text_fill`

---

## 7. 物化优先级（与 Expand Pass 对齐）

样式默认值的合并**只在 `prepare()` / `materialize_styles()` 发生一次**。与 [pipeline-spec.md](../pipeline-spec.md) §4.2 对齐：

```text
优先级从低到高（后者通过 or_insert 填入前者未占用的键）：

1. 引擎最小安全值
2. defaults.*（第一层）
3. diagrams[type].node / edge / group（第二层）
4. diagrams[type].entity_types[type]（第三层）
5. DSL node_style / edge_style 声明
6. 内联 style.*（最高，全程不覆盖）
```

**Renderer 不再**：

- 执行 `default_node_style` 中的 `match entity_type`
- 在渲染时调用 `apply_node_style` 覆盖 per-node 属性

**Renderer 仍负责**：

- `attributes.style` → `NodeStyle` 字段映射
- `graphic_style` 装饰
- diagram 级 canvas / title（读 ResolvedStyleSheet，不写入 entity AST）
- `ArrowType` 相关箭头几何

---

## 8. 样式选择与 DiagramProfile

### 8.1 解析顺序

决定**使用哪份 StyleSheet**（不是 cascade 内部合并）：

```text
用户显式指定样式
  → 外层场景策略（深色、租户 preset）
  → DiagramProfile.dark_theme_id（深色场景）
  → DiagramProfile.default_theme_id
  → 全局 fallback: builtin.clean-light
```

### 8.2 DiagramProfile 字段

```rust
pub struct DiagramProfile {
    pub kind: DiagramType,
    pub default_entity_type: Option<&'static str>,  // 语义默认，非视觉
    pub default_theme_id: &'static str,              // 引用 StyleSheet ID
    pub dark_theme_id: Option<&'static str>,
    pub default_graphic_style: GraphicStyleId,
    pub entity_types: &'static [&'static str],       // entity_types 键的合法集合
}
```

推荐默认映射（主题选择，非 per-type 配色）：


| DiagramType    | `default_theme_id`    |
| -------------- | --------------------- |
| `flowchart`    | `builtin.clean-light` |
| `sequence`     | `builtin.clean-light` |
| `state`        | `builtin.clean-light` |
| `architecture` | `builtin.blueprint`   |
| `er`           | `builtin.blueprint`   |
| `mindmap`      | `builtin.clean-light` |


per-type 配色在该 StyleSheet 的 `diagrams.{type}.entity_types` 中定义，不在 Profile 中。

---

## 9. 校验规则

### 9.1 解析阶段

- 必填字段：`version`、`id`、`name`、`tokens`、`defaults`
- `version` 须为支持的 `"0.2"`（或兼容列表）
- 颜色、数值类型合法
- `diagrams` 的键须为已知 `DiagramType`

### 9.2 resolve 阶段

- token 引用可解析
- `entity_types` 的键须属于对应 `DiagramProfile.entity_types`
- 未知 `entity_types` 键：warning，不 hard error（便于主题先行、profile 后补）

### 9.3 未知字段

默认 warning + 忽略，利于向前兼容。

---

## 10. Rust 数据模型（草案）

```rust
pub struct StyleSheet {
    pub version: String,
    pub id: String,
    pub name: String,
    pub meta: StyleMeta,
    pub tokens: StyleTokens,
    pub defaults: ElementStyles,                       // canvas/title/node/edge/group
    pub diagrams: HashMap<DiagramType, DiagramStyles>,
}

pub struct DiagramStyles {
    pub node: StyleBlock,
    pub edge: StyleBlock,
    pub group: StyleBlock,
    pub title: StyleBlock,
    pub entity_types: HashMap<String, StyleBlock>,
    pub edge_kinds: HashMap<String, StyleBlock>,
}

pub struct ResolvedStyleSheet {
    pub id: String,
    pub defaults: ResolvedElementStyles,
    pub diagrams: HashMap<DiagramType, ResolvedDiagramStyles>,
    // tokens 已展开，供 canvas/title 等 diagram 级渲染使用
}
```

---

## 11. 与 pipeline-spec 的术语对照

[pipeline-spec.md](../pipeline-spec.md) 早期草案使用 `type_palettes` 命名，与本文档第三层为同一概念：


| pipeline-spec（早期草案）               | style-system v0.2（本文档）                    |
| --------------------------------- | ----------------------------------------- |
| `type_palettes.flowchart.service` | `diagrams.flowchart.entity_types.service` |
| `diagram 级 node token 兜底`         | `diagrams.{type}.node` + `defaults.node`  |
| `edge_defaults(diagram_type)`     | `diagrams.{type}.edge` + `defaults.edge`  |


实现落地时以本文档 JSON 结构为准；prepare 伪代码中的查表路径应改为 `diagrams[type].entity_types[type]`。

---

## 12. 完整示例

内置主题 JSON 位于 [`crates/drawify-core/src/theme/themes/`](../../../crates/drawify-core/src/theme/themes/)（运行时真源）：


| ID                     | 文件                                                                                                   |
| ---------------------- | ---------------------------------------------------------------------------------------------------- |
| `builtin.clean-light`  | [themes/builtin.clean-light.json](../../../crates/drawify-core/src/theme/themes/builtin.clean-light.json)   |
| `builtin.clean-dark`   | [themes/builtin.clean-dark.json](../../../crates/drawify-core/src/theme/themes/builtin.clean-dark.json)     |
| `builtin.blueprint`    | [themes/builtin.blueprint.json](../../../crates/drawify-core/src/theme/themes/builtin.blueprint.json)       |
| `builtin.presentation` | [themes/builtin.presentation.json](../../../crates/drawify-core/src/theme/themes/builtin.presentation.json) |
| `builtin.minimal-gray` | [themes/builtin.minimal-gray.json](../../../crates/drawify-core/src/theme/themes/builtin.minimal-gray.json) |
| `builtin.brand-vivid`  | [themes/builtin.brand-vivid.json](../../../crates/drawify-core/src/theme/themes/builtin.brand-vivid.json)   |


每份样式稿均涵盖 `flowchart`、`sequence`、`state`、`architecture`、`er`、`mindmap` 的 `defaults`、`diagrams` 第二层及 `entity_types` 第三层。`builtin.clean-light` 的 per-type 配色与当前 Renderer 硬编码（`common::type_style` 等）对齐，作为迁移基准。

另提供社区高口碑灵感配色（`inspired.*` 前缀，见 [README.md](README.md)），含 Dracula、Nord、Tokyo Night、Catppuccin Mocha/Latte、GitHub Light/Dark、Monokai、Solarized Light、Gruvbox Dark、One Dark、Rosé Pine 等，文件位于 `crates/drawify-core/src/theme/themes/inspired.*.json`。

色盲友好配色（`accessible.*` 前缀）：Okabe-Ito、Paul Tol Bright、Paul Tol High Contrast、IBM Carbon Accessible。设计原则：避免红绿单维度区分、提高笔画对比度、与 `entity.type` 的 shape 语义配合。

---

## 13. 迁移策略

### 13.1 从 Renderer 硬编码迁移

1. 将各 Renderer `match entity_type` 块迁入对应 `builtin.*` 的 `entity_types`
2. `materialize_styles()` 按 §7 物化到 `attributes.style`
3. Renderer 删除 `default_node_style` 中的 type match，改为 `node_style_from_attributes`
4. 删除 `common::type_style()` 等 Rust 调色板

### 13.2 行为等价要求

首期迁移须保证默认主题下视觉与迁移前一致（或仅接受 Blueprint 等主题的 intentional 差异）。每个 `builtin.*` 主题须包含完整的 `entity_types`，不能依赖 Renderer fallback。

### 13.3 实现模块


| 模块                    | 职责                       |
| --------------------- | ------------------------ |
| `style/schema.rs`     | v0.2 JSON 模型             |
| `style/resolve.rs`    | token 引用展开              |
| `style/builtin.rs`    | 内置主题加载（含完整 entity_types） |
| `prepare/styles.rs`   | cascade 物化               |
| `render/diagram/*.rs` | 只读 `attributes.style`    |


---

## 14. 命名约定

内置主题：

```text
builtin.<series-name>
```

社区灵感配色（`themes/inspired.*.json`）：

```text
inspired.<palette-name>
```

色盲友好配色（`themes/accessible.*.json`）：

```text
accessible.<palette-name>
```

自定义：

```text
custom.<name>
org.<team>.<name>
```

---

## 15. 结论

StyleSheet v0.2 的核心价值：

1. **取消 Renderer 内置默认样式**，全部数据化
2. **三层 cascade** 语义清晰：defaults → diagrams → entity_types
3. **与 Expand Pass 单点物化**，布局与渲染一致
4. **DiagramProfile 只管引用与语义**，不管具体颜色
5. **完整主题稿可独立校验**（见 `crates/drawify-core/src/theme/themes/builtin.blueprint.json`）

规范稳定后，按 §13 迁移顺序落地实现，不与 layout intent refinement 冲突。