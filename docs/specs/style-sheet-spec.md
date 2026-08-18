# Tautcore Theme（StyleSheet）规范

> 版本：2.4 | 状态：现行（规范已定；实现仍漂移，见 §6.4）
>
> 取代 v0.2-draft 的「三层 cascade + `diagrams.*`」草案。图种差异不进主题（ADR-001）。  
> 2.2：`kind` / `kind_styles` → `**variant` / `variants**`（与 `shape` / `icon` 正交）  
> 2.4：统一 compile / resolve 叙述；**node / group / edge** 共用 `variants`；对齐 dsl-spec 2.4

本文档定义两件事，且**只**定义这两件事：

**视觉属性词表**（§5）—— 名字、值类型、适用元素、是否可内联。视觉属性的**单一真源**。

1. **主题 JSON 结构与解析语义** —— tokens / defaults / variants / extends / cascade。

DSL 语法与语义属性登记见 [dsl-spec.md](dsl-spec.md)（§5–§7 / §14）；archetype 见 [archetype-spec.md](archetype-spec.md)。DSL 只引用 §5 词表，不另立键名清单。

实现入口：`crates/tautcore-render/src/theme/`（编译）、`resolve.rs`（cascade）。

---

## 1. 设计原则


| 原则                  | 说明                                                                |
| ------------------- | ----------------------------------------------------------------- |
| 扁平主题                | 顶层只有 `tokens` / `defaults` / `variants`；**无** `diagrams` 段        |
| Theme = 颜料          | 颜色、字体、线宽、圆角、**全局**默认 shape                                        |
| `render_style` = 笔触 | `standard` / `sketch` 等；写在 diagram 属性，**不**进主题 JSON               |
| variant 驱动视觉差       | node / group / edge 共用 `variants` 表区分语义色；**不含** per-variant shape |
| 三轴正交（node）          | `shape` × `variant` × `icon`；见 [dsl-spec.md](dsl-spec.md) §14.3   |
| 内联最高优               | DSL `style.*` 覆盖主题与 variant 颜料                                    |


### 非目标

- 主题 JSON 中描述手绘抖动 / hatch（属 `render_style`）
- 按 `DiagramType` 分支的主题段（ADR-001）
- structural / context palette（待 `LayoutResult` 索引后再设计）
- 领域实体名（`database`…）作 variant 键
- 主题中定义 archetype（CSV → 二进制，见 archetype-spec）

---

## 2. 术语


| 术语                 | 含义                                            | 示例                   |
| ------------------ | --------------------------------------------- | -------------------- |
| Theme / StyleSheet | 扁平视觉主题 JSON                                   | `common.clean-light` |
| tokens             | 可复用设计原料                                       | `colors.canvas`      |
| defaults           | 全局兜底（canvas / title / node / edge / group）    | `defaults.node.fill` |
| variants           | 按 `variant` 属性的**颜料**覆盖块（compile 期物化）         | `variants.primary`   |
| compiled_variants  | compile 后 `defaults.node ⊕ variants[v]` 的颜料快照 | 供 resolve 读取         |
| cascade            | compile + resolve 优先级链                        | §6                   |


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
  "variants": {
    "default": {},
    "primary": {},
    "secondary": {},
    "muted": {},
    "info": {}
  }
}
```

### 3.2 顶层字段


| 字段         | 类型              | 必填  | 说明                      |
| ---------- | --------------- | --- | ----------------------- |
| `id`       | `string`        | 是   | 唯一标识                    |
| `name`     | `string`        | 是   | 显示名                     |
| `extends`  | `string | null` | 否   | 父主题 id；链式继承             |
| `tokens`   | `object`        | 是   | 设计 token                |
| `defaults` | `object`        | 否   | 全局兜底                    |
| `variants` | `object`        | 否   | variant → 颜料块；键属封闭集（§7） |


**禁止**：`diagrams`、`structural_palettes`、`context_palettes`、`version`、`kind_styles`、`archetypes`。

### 3.3 `extends` 继承

- 子 `extends` 父；可链式。
- Merge：对象 deep merge（子覆盖父）；`variants` 按 variant 名 deep merge 属性。
- Merge **不**展开 token；token 在 compile 统一解析。
- 合并后 `id` / `name` 取子主题。

---

## 4. Tokens

`tokens` 通过 `{category.key}`（及 `{palette.role.key}` / 别名 `{role.role.key}`）引用。


| 组            | 用途                         | 示例键                             |
| ------------ | -------------------------- | ------------------------------- |
| `colors`     | 画布、文字、边、组等                 | `canvas`, `text`, `edge_stroke` |
| `palette`    | 按角色的 fill/stroke/text_fill | `blue.fill`                     |
| `typography` | 字体族、字号                     | `font_family`, `label_size`     |
| `strokes`    | 线宽与虚线                      | `normal`, `dashed`              |
| `radius`     | 圆角                         | `sm`, `md`, `pill`              |
| `spacing`    | 间距（布局可参考）                  | `node_padding_x`                |


### 4.1 引用规则

- `{token.path}` 在 compile 时替换为字面量；不可解析则**原样保留**。
- `palette.<role>.*` 同时注册为 `role.<role>.*`。
- token 路径里的 `role`（如 `role.blue`）是**色板角色**，与 DSL 属性 `variant` **无关**。

### 4.2 值类型

string · number · boolean · number 数组（如 `[4, 3]` → SVG `"4,3"`）。

---

## 5. 属性词表（视觉属性单一真源）

「主题块」= 可写在 `defaults.<块>`（`variants.*` 仅颜料，见 §7）；「可内联」= DSL `style.<prop>`。**两列都空则静默忽略。**


| 属性                   | 值类型                          | 主题块                           | 可内联                                    |
| -------------------- | ---------------------------- | ----------------------------- | -------------------------------------- |
| `background`         | color                        | `canvas`                      | —                                      |
| `fill`               | color                        | `node` `group`                | node、group                             |
| `stroke`             | color                        | `node` `edge` `group`         | node、edge、group                        |
| `stroke_width`       | number                       | `node` `edge` `group`         | node、edge、group                        |
| `text_fill`          | color                        | `title` `node` `edge` `group` | node、edge、group                        |
| `font_size`          | number                       | `title` `node` `edge`         | node、edge                              |
| `font_weight`        | string                       | `node`                        | node                                   |
| `radius`             | number                       | `node` `group`                | node、group                             |
| `shape`              | atom（封闭集）                    | `node`（**仅** `defaults.node`） | **—**（DSL 用 `shape:`，见 dsl-spec §14.6） |
| `stroke_dasharray`   | string | number[]            | `node` `group`                | node、edge、group                        |
| `stroke_linecap`     | string                       | `node` `edge`                 | node、edge                              |
| `stroke_linejoin`    | string                       | `node` `edge`                 | node、edge                              |
| `fill_opacity`       | number                       | `node` `group`                | node、group                             |
| `stroke_opacity`     | number                       | `node` `edge`                 | node、edge                              |
| `arrow_style`        | `normal` / `hollow` / `none` | `edge`                        | edge                                   |
| `arrow_fill`         | color                        | `edge`                        | **—**（内联 `style.stroke` 连带，§6.3）       |
| `response_dasharray` | string | number[]            | `edge`                        | —                                      |
| `label_bg`           | color | `"canvas"`           | `edge`                        | —                                      |
| `label_bg_opacity`   | number                       | `edge`                        | —                                      |
| `dashed`             | bool                         | **—**                         | node、edge、group                        |


### 5.1 注解

- `shape` 封闭集与缺省链：dsl-spec §14.6 / §14.3.2；**不得**写入 `variants.`*
- `dashed`（仅 DSL）：`true` → `stroke_dasharray = "4,3"`，`false` → 清除 dash
- edge 通用虚线**只能内联** `stroke_dasharray`；主题 edge 块用 `response_dasharray` 管 `-->` 默认虚线
- `label_bg: "canvas"` 解析为画布背景色
- `group` 无 `stroke_linecap` / `stroke_linejoin` / `stroke_opacity` / `font_size` 主题字段
- `title.font_weight` 当前未实现

---

## 6. Cascade

```text
theme = load(meta.theme)          // compile：tokens + defaults + compiled_variants
resolved = resolve_graph(graph, theme)
```

### 6.1 Compile 期

```text
defaults.*     ← tokens 解析后的各块兜底
compiled_variants[v] = defaults.node ⊕ variants[v]   // 颜料字段；禁止 shape
```

- `variants.default` 可为 `{}`；物化后 `compiled_variants.default` 等同 `defaults.node` 颜料。
- 主题合并后 `variants` **必须**含 §7 五个键；缺键 → compile 报错。

### 6.2 Resolve 期（目标行为）


| 元素        | 几何 / 结构链                                                      | 颜料链                                                                | 后处理                                              |
| --------- | ------------------------------------------------------------- | ------------------------------------------------------------------ | ------------------------------------------------ |
| **node**  | `defaults.node.shape` → profile → archetype 填空 → DSL `shape:` | `compiled_variants[v]` → `style.`*                                 | `icon:` 显式解析 + shape 兼容（**不从 variant 推断**）       |
| **edge**  | `Edge::arrow`（语法）                                             | `defaults.edge` ⊕ `pick(compiled_variants[v], edge)` → `style.`*   | `Arrow::Response` 且无 dash → `response_dasharray` |
| **group** | —                                                             | `defaults.group` ⊕ `pick(compiled_variants[v], group)` → `style.`* | —                                                |


记 `v = element.variant ?? default`。`pick(…, element)` = 仅保留 §5 词表中该元素「主题块」或「可内联」列出现的属性名。

**作者须知**：`variants.`* 以 **node 颜料超集**书写。edge / group 只拾取同名兼容键；若某 variant 只写了 `fill`，边设 `variant:` 可能几乎不改变 stroke（仍走 `defaults.edge`）。

**边的不对称点**（§5 词表外行为）：

- 内联 `style.stroke` **同时**写 `arrow_fill`
- `label_bg` / `label_bg_opacity` 仅主题 `defaults.edge` 可设

内联键构造：`style.<prop>`，`<prop>` 取自 §5「可内联」列。

### 6.3 与 DSL 的边界

- diagram `theme:` → `RenderMeta.theme`；`render_style:` → 笔触（非主题 JSON）
- 语义属性（`label` / `shape` / `variant` / 端口 / archetype …）登记在 **dsl-spec §14**；本文只定义视觉 cascade
- `archetype` 展开后才进入 node 几何/颜料链；主题**不**查 archetype 名

### 6.4 实现漂移（相对本规范）


| 规范（§6）                          | 当前 `tautcore-render`       |
| ------------------------------- | -------------------------- |
| `variant` + `compiled_variants` | 仍 `kind` + `kind_styles`   |
| edge / group `variant` cascade  | 仅 `defaults.*` + `style.*` |
| node shape 链含 archetype         | 仍 `Node::kind()`           |


迁移跟踪：[dsl-spec.md](dsl-spec.md) §14.10.2。**规范优先于旧实现。**

---

## 7. `variants`

键为 **node / group / edge** 的 DSL 属性 `variant`，**必须**属于 [dsl-spec.md](dsl-spec.md) §14.7 封闭集：

```
default | primary | secondary | muted | info
```

值为**部分颜料**样式块（与 `defaults.node` 同形字段集）；compile 物化为 `compiled_variants`（§6.1）。


| 规则         | 说明                                                      |
| ---------- | ------------------------------------------------------- |
| 禁止 `shape` | 几何与颜料拆开                                                 |
| 缺省         | DSL 未知 variant → resolve 用 `default`（不报错）               |
| 命名空间       | 不按 flowchart / mindmap 分表；导图差异用不同主题文件（如 `mindmap.base`） |
| 三元素共用      | 同一 `variants.primary` 可同时影响节点填充、组框描边、边描边——取决于块内写了哪些键    |


---

## 8. 内置主题 id

嵌入于 `tautcore-render`（信息索引，非封闭承诺）：


| id                                            | 说明                |
| --------------------------------------------- | ----------------- |
| `common.clean-light`                          | 默认浅色              |
| `common.clean-dark`                           | 深色                |
| `common.blueprint`                            | 蓝图 / hollow 箭头    |
| `common.paper-ink`                            | 纸墨                |
| `common.github-light` / `common.github-dark`  | GitHub 风          |
| `common.presentation`                         | 演示                |
| `common.floating-cards`                       | 卡片                |
| `common.dual-channel`                         | 双通道               |
| `common.okabe-ito`                            | 色觉友好              |
| `mindmap.base`                                | 思维导图基座            |
| `mindmap.ink-dark` / `mindmap.vivid-branches` | 导图变体（可 `extends`） |


未知 id 回退 `common.clean-light`。

---

## 9. Render 特例


| 后端        | 行为                                                                   |
| --------- | -------------------------------------------------------------------- |
| **SVG**   | `theme::load` + `resolve_graph`；**暂不画** diagram title（layout 未预留标题带） |
| **ASCII** | 可画 title；跳过 group、形状方框化；**不走** theme resolve                         |


完整 parse → layout 管线见 dsl-spec §8。

---

## 附录 A：变更记录

### A.1 相对 v0.2-draft


| 旧                                          | 新                        |
| ------------------------------------------ | ------------------------ |
| `diagrams.<type>.*`                        | 扁平 `variants`            |
| 三层 cascade + AST 物化                        | render 侧 `resolve_graph` |
| `context_palettes` / `structural_palettes` | 已移除                      |
| `GraphicStyle` 进主题                         | `render_style` 在 diagram |


### A.2 2.2 → 2.4


| 项            | 2.2              | 2.4                                                 |
| ------------ | ---------------- | --------------------------------------------------- |
| variant 适用范围 | 主要写 node         | **node / group / edge** 共用表 + `pick`                |
| cascade 文档   | §6 三节重复          | compile / resolve 分述 + 一表                           |
| DSL 对齐       | node 2.2         | 对齐 dsl-spec 2.4（group/edge `variant`、edge 标签进 `{}`） |
| 实现状态         | 仅提 `kind_styles` | §6.4 漂移表                                            |


更早版本（2.0→2.1→2.2）与 `kind` 迁出细节见 [dsl-spec.md](dsl-spec.md) §14.10。