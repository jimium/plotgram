# Draw.io 导出

将已布局、已物化的 [`ExportScene`](../../scene.rs) 编码为 diagrams.net / draw.io 原生 `.drawio`（mxGraphModel XML）。

**输入**：只读 `ExportScene`，不在编码器内重新 layout 或物化样式。  
**输出**：可编辑的 mxCell vertex / edge（非扁平 SVG）。

相关契约：Scene 字段见 [`docs/specs/export-scene-spec.md`](../../../../../../docs/specs/export-scene-spec.md)。

---

## 模块

| 文件 | 职责 |
|------|------|
| `mod.rs` | `DrawioRenderer`（`FormatEncoder`）、`encode_scene_inner` |
| `encoder.rs` | mxfile / mxGraphModel XML 组装与图层顺序 |
| `style.rs` | 形状、颜色、箭头、XML 转义 |
| `routing.rs` | 边路由、端口 exit/entry、拐点 |
| `icon.rs` | 节点图标 → `shape=image` 嵌入 |
| `report.rs` | `DrawioExportOptions`、`ExportReport`、降级级别 |
| `compress.rs` | 可选 compressed 格式（`compressed-drawio` feature） |

---

## 管线位置

```text
PreparedDiagram → layout → ExportScene → DrawioEncoder → .drawio XML
```

注册为 `RenderFormat::Drawio`，扩展名 `drawio`。

---

## 输出格式

- UTF-8 XML，根元素 `<mxfile host="Drawify/{version}">`
- 单页 `<diagram>` → `<mxGraphModel>` → `<root>`
- 坐标：Scene 左上角原点，1 px = 1 draw.io unit；页面尺寸 = canvas + `page_padding`
- 默认**不压缩**；`DrawioExportOptions.compressed = true` 且启用 `compressed-drawio` feature 时 deflate + base64

### 图层顺序（先声明的在底层）

```text
背景 → groups → edges → nodes → title
```

与 SVG 一致：边在节点下，标题在最上。

### 元数据（可选）

`include_export_metadata = true` 时在 style 字符串写入：

- `drawifyEntityId` — 节点
- `drawifyRelationIndex` — 边
- `drawifyGroupId` — 分组
- `drawifyDegrade` — 该元素降级级别

draw.io UI 不展示这些字段，仅供调试与报告关联。

---

## 图表类型

| 类型 | 行为 |
|------|------|
| `flowchart` / `state` / `architecture` / `mindmap` / `custom` | 正常导出 |
| `sequence` / `er` | **默认拒绝**（无等价 mxGraph 模型） |

拒绝时错误信息含 `export_unsupported`；设置 `allow_unsupported_diagram_types = true` 后，`fallback = Error` 仍报错，`EmbeddedSvg` **尚未实现**。

---

## 映射要点

### 节点

| Scene | draw.io |
|-------|---------|
| `layout x,y,w,h` | `mxGeometry`；在 group 内时坐标**相对父容器** |
| `entity.id` | `drawio-node-{sanitized_id}` |
| `entity.label` | `value`（`\n` → `&#xa;`） |
| `style.shape` | 见 `style.rs` 中 `shape_to_drawio_style` |
| fill / stroke / strokeWidth / dashed | `fillColor` / `strokeColor` / … |
| `hand_drawn` 或 excalidraw/cross-hatch 全局风格 | `sketch=1` |
| 图标（architecture 等） | `embed_icons=true` 时嵌入 SVG 为 `shape=image`（L1）；失败则仅标签（L2） |

`Subprocess`、`Person` 映射为 L1（形状近似）。

### 边

| Scene | draw.io |
|-------|---------|
| `relation.from/to` | **始终**写入 `source` / `target`（最高优先级，保证可拖拽重连） |
| `from_port` / `to_port` | `exitX/Y`、`entryX/Y`（沿边偏移，非固定 0.5） |
| `Straight` | `edgeStyle=none` |
| `Bezier` | `edgeStyle=none;curved=1`（L1，非精确采样） |
| `Polyline` | 0 拐点直线；1 拐点 `elbowEdgeStyle`；2+ `segmentEdgeStyle` + 最多 `max_edge_waypoints` 个拐点 |
| 路径无效 | 仍绑定 source/target，由 draw.io 自动路由（L2） |
| `relation.arrow` | Active / Passive(dashed) / Bidirectional |
| 标签 | 写入 edge cell 的 `value`；多标签 / head_label / tail_label 仅导出首个或丢弃（L2 警告） |

### 分组

- 有标签 → `swimlane`；无标签 → 虚线矩形容器（L1 警告）
- 嵌套 group 的 parent 与相对坐标已处理
- 导出 fill / stroke 颜色

### 不导出

- `canvas.attribution`
- `theme_id`（Scene 已是具体颜色）
- `node.style.transform`（忽略）

---

## 降级级别

| 级别 | 含义 |
|------|------|
| **L0** | 完整结构化映射 |
| **L1** | 形状/路由/曲线近似 |
| **L2** | 部分语义丢失（图标、多标签、无效路径等） |
| **L3** | 元素级跳过 |
| **F** | 整图 SVG 嵌入（未实现） |

每次导出生成 `ExportReport`（`encode_scene_with_report` / `encode_scene_inner`），含 `warnings` 与 `stats`（nodes/edges/groups/l0…l3）。`export_version` 固定 `"0.1"`。

---

## 配置

`DrawioExportOptions`（默认值见 `report.rs`）：

| 选项 | 默认 | 说明 |
|------|------|------|
| `allow_unsupported_diagram_types` | `false` | 允许 sequence/er 走 fallback |
| `fallback` | `Error` | `Error` / `EmbeddedSvg`（后者未实现） |
| `max_edge_waypoints` | `2` | 折线最多保留的中间拐点数 |
| `embed_icons` | `true` | 节点图标嵌入 image shape |
| `include_export_metadata` | `true` | drawify* 元数据 |
| `page_padding` | `20.0` | 画布外留白 |
| `compressed` | `false` | 需 `compressed-drawio` feature |

`DrawioRenderer::encode_scene` 使用默认选项且不返回 report；需要报告时调用 `encode_scene_inner` 或 `encode_scene_with_report`。

---

## 测试

集成测试：`crates/drawify-core/tests/drawio_export.rs`（全管线 + XML 结构、颜色、箭头、端口等断言）。
