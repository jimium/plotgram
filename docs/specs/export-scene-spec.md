# Drawify Scene JSON 规范

> 版本：0.1.0-draft | 状态：已实现

本文档定义 Drawify `Exporter` 层的对外 JSON 契约。`Scene JSON` 是 `PreparedDiagram` 经过样式解析、布局计算和导出标准化后的结构化结果，面向多渲染器、外部服务和第三方消费方。

核心原则：

- `Scene JSON` 是 **导出层契约**，不是 AST 存储格式
- 它表达的是 **已准备好渲染的场景**，而不是“待解释的源码语义”
- `SVG / PNG / WebP / ASCII / JSON` 共享同一份 `ExportScene`
- 协议演进通过 `schema_version` 管理，破坏性变更必须升级版本

相关规范：

- AST 结构见 [ast-spec.md](ast-spec.md)
- Prepare / Layout / Render 管线见 [pipeline-spec.md](pipeline-spec.md)
- StyleSheet 见 [style-system/style-sheet-spec.md](style-system/style-sheet-spec.md)

---

## 1. 背景与定位

### 1.1 为什么需要 Scene JSON

在仅有 `PreparedDiagram` 的阶段，下游格式仍需要各自重复完成：

- 主题解析
- graphic style 解析
- layout 计算
- 节点 / 边 / 分组基础视觉样式物化

这会导致多格式导出重复实现同一套逻辑，也不利于对外暴露统一协议。

`Scene JSON` 解决的问题是：

- 让外部调用方直接拿到“可渲染场景”
- 让多格式导出共享一份中间表示
- 为远程渲染、缓存、前端预览、第三方 renderer 提供稳定输入

### 1.2 它与 AST 的区别

| 维度 | AST / PreparedDiagram | Scene JSON |
|------|------------------------|------------|
| 作用 | 语义存储与下游统一输入 | 导出层标准化结果 |
| 是否包含 layout | 否 | 是 |
| 是否包含最终基础样式 | 否，需运行期解释 | 是 |
| 是否适合直接渲染 | 不直接适合 | 适合 |
| 是否保留全部源码语义 | 是 | 保留渲染必需语义与源对象 |

### 1.3 当前导出链路

```text
DSL / JSON AST
    ↓ parse / deserialize
RawDiagram
    ↓ prepare()
PreparedDiagram
    ↓ export_scene()
ExportScene
    ↓
Scene JSON / SVG / PNG / WebP / ASCII
```

---

## 2. 顶层结构

### 2.1 JSON 骨架

```json
{
  "schema_version": "0.1",
  "format": "drawify.export_scene",
  "diagram_type": "flowchart",
  "theme_id": "builtin.clean-light",
  "theme_name": "Clean Light",
  "graphic_style": "standard",
  "canvas": {},
  "nodes": [],
  "edges": [],
  "groups": [],
  "source_info": {}
}
```

### 2.2 顶层字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `schema_version` | `string` | 是 | Scene JSON 协议版本，当前为 `"0.1"` |
| `format` | `string` | 是 | 固定值 `"drawify.export_scene"` |
| `diagram_type` | `string` | 是 | 图表类型，小写枚举值，如 `flowchart` |
| `theme_id` | `string` | 是 | 实际解析后的 StyleSheet ID |
| `theme_name` | `string` | 是 | 实际解析后的 StyleSheet 显示名 |
| `graphic_style` | `string` | 是 | 实际生效的 graphic style，如 `standard` |
| `canvas` | `object` | 是 | 画布级导出信息 |
| `nodes` | `array` | 是 | 已布局节点列表 |
| `edges` | `array` | 是 | 已布局边列表 |
| `groups` | `array` | 是 | 已布局分组列表 |
| `source_info` | `object` | 是 | 源信息，来自 `Diagram.source_info` |

### 2.3 顶层不变量

- `nodes` 中只包含成功参与布局的实体
- `edges.len()` 与 `relations.len()` 对齐；若某条边未生成路径，也必须保留条目
- `groups` 中只包含成功生成分组包围框的分组
- `theme_id` 与 `graphic_style` 是 **实际生效值**，不是用户原始输入

---

## 3. 画布对象

### 3.1 结构定义

```json
{
  "width": 640.0,
  "height": 360.0,
  "title": "用户认证流程",
  "background": "#ffffff",
  "title_color": "#222222",
  "attribution": true
}
```

### 3.2 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `width` | `number` | 是 | 内容画布宽度，不含外部消费方额外边距 |
| `height` | `number` | 是 | 内容画布高度，不含外部消费方额外边距 |
| `title` | `string \| null` | 是 | 图表标题；无标题时为 `null` |
| `background` | `string` | 是 | 画布背景色 |
| `title_color` | `string` | 是 | 标题文字颜色 |
| `attribution` | `boolean` | 是 | 是否启用署名区域 |

### 3.3 语义说明

- `width` / `height` 来自 layout 结果，不因具体输出格式而变化
- `attribution` 只表示开关，不表示最终像素占位；具体格式可以决定如何展示署名

---

## 4. 节点对象

### 4.1 结构定义

每个节点由三部分组成：

- `entity`：原始 AST 实体对象
- `layout`：节点布局矩形
- `style`：导出层物化后的节点视觉样式

```json
{
  "entity": {
    "id": "api",
    "label": "API",
    "attributes": { "standard": {}, "style": {}, "meta": {} },
    "group_id": null,
    "span": {
      "start": { "line": 3, "column": 1 },
      "end": { "line": 3, "column": 10 }
    }
  },
  "layout": {
    "x": 120.0,
    "y": 80.0,
    "width": 140.0,
    "height": 56.0
  },
  "style": {
    "fill": "#E8F4FD",
    "stroke": "#1565C0",
    "shape": "rounded_rect",
    "stroke_width": 1.5,
    "stroke_dasharray": null,
    "stroke_linecap": null,
    "stroke_linejoin": null,
    "transform": null,
    "radius": 8.0,
    "label_weight": "600",
    "hand_drawn": false
  }
}
```

### 4.2 `layout` 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `x` | `number` | 是 | 左上角 X |
| `y` | `number` | 是 | 左上角 Y |
| `width` | `number` | 是 | 节点宽度 |
| `height` | `number` | 是 | 节点高度 |

### 4.3 `style` 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `fill` | `string` | 是 | 填充色 |
| `stroke` | `string` | 是 | 描边色 |
| `shape` | `string` | 是 | 节点形状 |
| `stroke_width` | `number` | 是 | 描边宽度 |
| `stroke_dasharray` | `string \| null` | 是 | SVG 语义虚线模式 |
| `stroke_linecap` | `string \| null` | 是 | SVG 语义端点样式 |
| `stroke_linejoin` | `string \| null` | 是 | SVG 语义连接样式 |
| `transform` | `string \| null` | 是 | 节点变换描述 |
| `radius` | `number \| null` | 是 | 圆角半径 |
| `label_weight` | `string \| null` | 是 | 标签字重 |
| `hand_drawn` | `boolean` | 是 | 是否为手绘/草图风格 |

### 4.4 `shape` 可选值

当前支持：

- `rect`
- `rounded_rect`
- `circle`
- `diamond`
- `cylinder`
- `hexagon`
- `person`
- `stadium`

---

## 5. 边对象

### 5.1 结构定义

每条边由三部分组成：

- `index`：与 `Diagram.relations[index]` 对齐的稳定索引
- `relation`：原始 AST 边对象
- `layout`：边路径与端口信息
- `style`：导出层物化后的边视觉样式

```json
{
  "index": 0,
  "relation": {
    "from": "api",
    "to": "db",
    "arrow": "active",
    "label": "query",
    "attributes": { "standard": {}, "style": {}, "meta": {} },
    "span": {
      "start": { "line": 8, "column": 1 },
      "end": { "line": 8, "column": 20 }
    }
  },
  "layout": {
    "path": [[220.0, 108.0], [340.0, 108.0]],
    "path_kind": "straight",
    "control_points": null,
    "label_pos": [280.0, 100.0],
    "from_port": "right",
    "to_port": "left"
  },
  "style": {
    "stroke": "#555555",
    "dashed": false,
    "stroke_width": 1.5,
    "stroke_dasharray": null,
    "stroke_linecap": null,
    "stroke_linejoin": null,
    "hand_drawn": false,
    "arrow": "normal",
    "label_pos": "middle"
  }
}
```

### 5.2 `layout` 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | `array<[number, number]>` | 是 | 边路径点集合 |
| `path_kind` | `string` | 是 | 路径类型 |
| `control_points` | `array<[number, number]> \| null` | 是 | 三次贝塞尔控制点，仅 `bezier` 使用 |
| `label_pos` | `[number, number]` | 是 | 边标签建议位置 |
| `from_port` | `string` | 是 | 起点端口方向 |
| `to_port` | `string` | 是 | 终点端口方向 |

### 5.3 `style` 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `stroke` | `string` | 是 | 线条颜色 |
| `dashed` | `boolean` | 是 | 是否虚线 |
| `stroke_width` | `number` | 是 | 描边宽度 |
| `stroke_dasharray` | `string \| null` | 是 | SVG 语义虚线模式 |
| `stroke_linecap` | `string \| null` | 是 | SVG 语义端点样式 |
| `stroke_linejoin` | `string \| null` | 是 | SVG 语义连接样式 |
| `hand_drawn` | `boolean` | 是 | 是否为手绘/草图风格 |
| `arrow` | `string` | 是 | 渲染箭头样式 |
| `label_pos` | `string` | 是 | 标签定位语义 |

### 5.4 枚举值

`path_kind`：

- `straight`
- `polyline`
- `bezier`

`from_port` / `to_port`：

- `top`
- `bottom`
- `left`
- `right`

`arrow`：

- `normal`
- `hollow`
- `none`

`label_pos`：

- `middle`
- `start`
- `end`

### 5.5 空路径边

若某条关系未成功生成边路径：

- 该边仍必须出现在 `edges` 中
- `index` 与 `relation` 仍保留
- `layout.path` 可能为空数组

这是为了保证 Scene JSON 与 AST relation 序列保持稳定映射。

---

## 6. 分组对象

### 6.1 结构定义

```json
{
  "group": {
    "id": "backend",
    "label": "Backend",
    "attributes": { "standard": {}, "style": {}, "meta": {} },
    "parent_id": null,
    "depth": 0,
    "entity_ids": ["api", "db"],
    "child_group_ids": [],
    "span": {
      "start": { "line": 2, "column": 1 },
      "end": { "line": 10, "column": 1 }
    }
  },
  "layout": {
    "x": 80.0,
    "y": 40.0,
    "width": 360.0,
    "height": 240.0
  },
  "fill": "#f5f5f5",
  "stroke": "#cccccc",
  "label_color": "#666666",
  "stroke_width": 1.5
}
```

### 6.2 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `group` | `object` | 是 | 原始 AST 分组对象 |
| `layout` | `object` | 是 | 分组包围框 |
| `fill` | `string` | 是 | 分组背景色 |
| `stroke` | `string` | 是 | 分组描边色 |
| `label_color` | `string` | 是 | 分组标签色 |
| `stroke_width` | `number` | 是 | 分组描边宽度 |

### 6.3 分组导出规则

- 只有 layout 成功生成包围框的 group 才会进入 `groups`
- 若 group 含有子 group，导出层可以使用不同的默认视觉
- `group` 对象中的 `entity_ids` / `child_group_ids` 是语义关系，不等于布局顺序

---

## 7. SourceInfo

### 7.1 结构定义

```json
{
  "file": "diagram.dfy",
  "line_count": 32
}
```

### 7.2 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `file` | `string \| null` | 是 | 源文件路径；无来源时为 `null` |
| `line_count` | `integer` | 是 | 源文本总行数 |

---

## 8. 版本与兼容性

### 8.1 版本策略

| 场景 | 是否需要升级 `schema_version` |
|------|-------------------------------|
| 新增可选字段 | 否 |
| 新增数组元素上的可选字段 | 否 |
| 新增枚举值 | 视消费方约束而定，默认建议升级 minor 文档版本 |
| 删除字段 | 是 |
| 重命名字段 | 是 |
| 修改字段类型 | 是 |
| 改变字段语义 | 是 |

### 8.2 稳定承诺

`Scene JSON` 当前对外承诺以下字段为稳定契约：

- 顶层：`schema_version`、`format`、`diagram_type`、`theme_id`、`theme_name`、`graphic_style`
- 容器：`canvas`、`nodes`、`edges`、`groups`、`source_info`
- 元素内部的字段名与枚举值

### 8.3 不承诺的内容

以下内容在不破坏 schema 的前提下允许调整：

- 数组内元素顺序之外的具体布局数值
- 默认颜色、线宽、圆角等主题数值
- 不同 `graphic_style` 下 `hand_drawn` 的具体判定逻辑

---

## 9. 面向消费方的建议

### 9.1 推荐使用方式

- 优先使用 `schema_version` + `format` 判断协议类型
- 以 `diagram_type` 决定高层语义处理
- 以 `nodes[*].style` / `edges[*].style` 驱动渲染，而不是重新解释 AST
- 保留 `entity` / `relation` / `group` 原始对象，用于跳转、检查和诊断映射

### 9.2 不推荐使用方式

- 不要假设所有边都有非空 `path`
- 不要把 `theme_id` 当成用户原始输入；它是解析后的最终值
- 不要依赖某个内置主题的具体颜色值作为业务逻辑条件

---

## 10. 完整示例

```json
{
  "schema_version": "0.1",
  "format": "drawify.export_scene",
  "diagram_type": "flowchart",
  "theme_id": "builtin.clean-light",
  "theme_name": "Clean Light",
  "graphic_style": "standard",
  "canvas": {
    "width": 420.0,
    "height": 180.0,
    "title": "用户认证流程",
    "background": "#ffffff",
    "title_color": "#222222",
    "attribution": true
  },
  "nodes": [
    {
      "entity": {
        "id": "login",
        "label": "Login",
        "attributes": { "standard": {}, "style": {}, "meta": {} },
        "group_id": null,
        "span": {
          "start": { "line": 3, "column": 1 },
          "end": { "line": 3, "column": 14 }
        }
      },
      "layout": {
        "x": 40.0,
        "y": 72.0,
        "width": 120.0,
        "height": 48.0
      },
      "style": {
        "fill": "#E8F4FD",
        "stroke": "#1565C0",
        "shape": "rounded_rect",
        "stroke_width": 1.5,
        "stroke_dasharray": null,
        "stroke_linecap": null,
        "stroke_linejoin": null,
        "transform": null,
        "radius": 8.0,
        "label_weight": null,
        "hand_drawn": false
      }
    }
  ],
  "edges": [
    {
      "index": 0,
      "relation": {
        "from": "login",
        "to": "token",
        "arrow": "active",
        "label": "issue",
        "attributes": { "standard": {}, "style": {}, "meta": {} },
        "span": {
          "start": { "line": 5, "column": 1 },
          "end": { "line": 5, "column": 22 }
        }
      },
      "layout": {
        "path": [[160.0, 96.0], [260.0, 96.0]],
        "path_kind": "straight",
        "control_points": null,
        "label_pos": [210.0, 88.0],
        "from_port": "right",
        "to_port": "left"
      },
      "style": {
        "stroke": "#555555",
        "dashed": false,
        "stroke_width": 1.5,
        "stroke_dasharray": null,
        "stroke_linecap": null,
        "stroke_linejoin": null,
        "hand_drawn": false,
        "arrow": "normal",
        "label_pos": "middle"
      }
    }
  ],
  "groups": [],
  "source_info": {
    "file": "auth-flow.dfy",
    "line_count": 12
  }
}
```
