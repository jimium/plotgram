# 节点文本与 Text Node 设计方案

> 状态：草案（讨论沉淀）  
> 日期：2026-07-22  
> 背景：当前多数图类型的节点内容仅为单行 `label` + 可选图标，难以满足文档配图与未来工程视图的信息密度需求。

---

## 1. 问题与目标

### 1.1 现状

- **数据模型**：`Entity { id, label, attributes, ... }`，`label` 为单行字符串。
- **渲染**：大多数图类型在节点内画「图标 + 单行居中文字」；ER 图例外，有结构化的表名 + 字段列表。
- **已有但未进渲染**：`status`、`owner`、`description` 等在 DSL 手册中有描述；`meta.*` 明确不参与渲染。
- **尺寸估算**：标准节点按单行 label 估宽并 clamp（`estimate_standard_node_width`），不适合多行正文。

### 1.2 产品目标（双阶段）

| 阶段 | 定位 | 对节点内容的要求 |
|------|------|------------------|
| **近期** | AI / 文档配图 | 结构清晰；允许图内说明、图例、段落注释；AI 易生成 |
| **远期** | 工程交付 / 运维视图 | 节点可承载 status、badge、指标等；不推翻现有布局路由契约 |

### 1.3 核心判断

- 仅给业务节点加 subtitle **不够**；需要独立的 **text node**（无边框、不参与拓扑）承载文档性内容。
- text node 与节点内富文本 **应共用同一套文本引擎**，但 **布局契约必须分离**。
- 不应为了说明文字去膨胀普通 `entity` 的拓扑语义。

---

## 2. 方案概览

### 2.1 两类载体，一套引擎

```
┌─────────────────────────────────────┐
│  TextContent（共享层）               │
│  - 解析：受限标记子集               │
│  - 测量：给定 width → 计算 height   │
│  - 渲染：SVG <text> / <tspan>       │
└─────────────────────────────────────┘
           ▲                ▲
           │                │
    ┌──────┴──────┐  ┌──────┴──────┐
    │ TextNode    │  │ EntityBody  │
    │ 无边框独立   │  │ 填入节点形状 │
    │ 不参与拓扑   │  │ 参与拓扑     │
    └─────────────┘  └─────────────┘
```

### 2.2 职责对比

| 维度 | 业务节点 `entity` | 独立 `text` node | 节点内 `body` |
|------|-------------------|------------------|---------------|
| 参与拓扑 / 布局层级 | 是 | 否 | 是（随父节点） |
| 可作为边的端点 | 是 | 否（v1 禁止） | 是（随父节点） |
| 可视边框 | 有 | 无 | 有（父节点形状） |
| 典型内容 | 短 `label` + icon | 段落、图例、章节说明 | 2–4 行补充说明 |
| 尺寸驱动 | label / 结构化字段 | `width` + 换行估高 | `width` + 换行估高 |

### 2.3 与远期工程视图的关系

```
现在：  entity = 拓扑节点（单行 label）
        text   = 说明 / 图例 / 段落

以后：  entity = 拓扑节点 + 可选结构化槽位（status / badge / metric）
        text   = 纯文档性说明
        card?  = 重内容节点（若需堆叠指标，另立类型）
```

text node **不替代**运维卡片，但可为 entity 的结构化扩展留出空间，避免把「段落说明」和「服务节点」混在同一语义里。

---

## 3. Text Node 设计

### 3.1 语义

- 无可视化边框（`fill` / `stroke` 透明或 none）。
- 内容为受限富文本（非 HTML）。
- DSL 可限定宽度（必填或有默认值）；高度由换行自动计算。
- **不参与** sugiyama / 力导向等主图层排序；**不可**作为边的端点。

### 3.2 DSL 草案

**推荐：顶层 `text` 关键字**（语义清晰，利于 AI 生成）：

```plotgram
diagram architecture {
    group backend "后端层" {
        text note "本层通过 API 网关统一接入，所有写操作走主库。" {
            width: 280
            font_size: 12
            align: left
        }

        entity[gateway] gateway "API 网关"
        entity[service] order_svc "订单服务"
    }

    text legend {
        body: """
        图例：
        - 实线：同步调用
        - 虚线：异步消息
        """
        width: 240
    }
}
```

**备选**：`entity[text] note "..."` —— 复用 entity 管线，但 type 特殊化；语义不如顶层 `text` 干净。

### 3.3 v1 硬规则

1. 禁止入边 / 出边（validate 报错）。
2. 不参与主图布局层级（非图顶点）。
3. 默认在**父 group 内**排版（如顶部 caption 区）。
4. `width` 必填或默认（如 240）；`height` 由测量得出；可选 `max_height` 截断或缩小字号。

### 3.4 与现有能力的关系

- 可复用：`style.width` / `style.height` 作为布局 hint 的既有机制；`style.font_size`、`style.text_fill` 等文字样式。
- 待补齐：**无 x/y 绝对定位** —— 当前布局全自动，text node 的定位策略需单独设计（见 §5）。

---

## 4. 受限富文本格式

### 4.1 原则

- **不用完整 Markdown**，用**确定性子集**：同样文本 + 同样 `width` → 高度可预估、渲染可复现。
- 完整 MD 的链接、图片、嵌套列表、代码块等对布局不友好，AI 配图子集已够用。

### 4.2 v1 白名单

支持：

- `**粗体**`、`*斜体*`
- 换行（`\n`）
- `-` 无序列表（**最多一层**）

或等价标签：`<b>`、`<i>`、`<br/>`、`<ul><li>`。

**v1 明确不做**：链接、图片、表格、代码块、嵌套列表。

字号 / 颜色走 `style.font_size`、`style.text_fill`，不必写进内联语法。

### 4.3 共享字段：`body`

独立 text node 与 entity 内正文共用 `body` 字段与同一解析 / 测量 / 渲染管线：

```plotgram
text note {
    body: "图例：实线=同步调用"
    width: 240
}
```

---

## 5. 布局策略

### 5.1 定位（最大设计点）

当前 DSL **没有** `x` / `y`，布局全自动。text node 需分阶段解决「放哪儿」：

| 阶段 | 策略 | 适用 |
|------|------|------|
| **Phase 1** | 父 group 内 caption（顶部 / 底部）；参与 group bounds；边路由可选软障碍或忽略 | AI / 文档配图 |
| **Phase 2** | 相对锚定：`anchor: top-left` 等，相对 group 或 canvas | 图内注释微调 |
| **Phase 3** | `layout.x` / `layout.y` 绝对定位 | 运维大屏、手工微调 |

近期目标：**Phase 1 即可**，不必一上来支持自由拖拽。

### 5.2 边路由

- text node 可作为**软障碍**（lint WARN）或完全不参与障碍，避免打乱正交路由主干。
- 与业务节点分图层处理，降低对现有 port / OVG 管线的影响。

---

## 6. 节点内填充（Entity Body）

### 6.1 能否复用 text 能力？

**能。** 共享 `TextContent` 引擎；**布局模式**与独立 text node 不同。

### 6.2 DSL：`label` 与 `body` 分离（推荐）

```plotgram
entity[service] api "API 服务" {
    body: """
    统一入口，负责：
    - JWT 校验
    - 限流
    """
    width: 160
}
```

- `label`：短名，用于拓扑识别、图例、diff。
- `body`：多行说明，驱动换行与节点增高。

**不推荐**把多行内容直接写进 `label` 字符串：语义模糊，AI 与 diff 都难处理。

### 6.3 按图类型 / 形状开放

| 场景 | 是否适合 body |
|------|----------------|
| 架构图 service 矩形 | 适合，建议 title + 少量 body（2–4 行） |
| 流程图 process 矩形 | 适合 |
| 状态图小圆 / initial | 不适合，保持单行 label |
| ER 表 | 继续用 `meta.fields` 结构化字段，非富文本 body |

**v1 建议仅对** `rect` / `rounded_rect` / `stadium` 等规整形状开放 `body`；`circle` / `diamond` / `initial` 等保持单行。

### 6.4 版式（icon + 多行）

固定一种版式，避免自由组合：

```
┌──────────────────┐
│  [icon]  Title    │  ← 第一行：icon + 短 label（单行）
│  body 多行…       │  ← 下方：body（可含列表）
└──────────────────┘
```

### 6.5 尺寸逻辑变化

当前：单行 label 估宽 → clamp。

有 `body` 后：

1. 先定内容区宽度（`width` hint 或默认）。
2. 按 width 换行，计算行数。
3. `height = padding + icon 区（如有）+ 文本区`。
4. `width = max(label 行宽, body 行宽) + padding`，再 clamp。

与 text node 共用测量函数；节点额外扣除形状内边距、icon 占位、min/max 策略。

### 6.6 内容放哪里的决策

| 内容 | 放置 |
|------|------|
| 「订单服务」 | `label` |
| 「负责创建、取消、查询订单」 | 节点 `body` |
| 「本图展示下单主流程，不含退款」 | 独立 `text` node |

---

## 7. 实施路径

### 7.1 推荐顺序

1. **TextContent 内核**：解析子集、换行测量、SVG 渲染。
2. **独立 `text` node**：AST / validate、group caption 布局、无边框绘制。
3. **Entity `body`**：复用内核；限定形状；`max_lines` 默认上限（如 4）。
4. **远期**：entity 结构化槽位（status、badge、metric），与 `body` 正交。

### 7.2 预计改动面

| 模块 | 内容 |
|------|------|
| AST / validate | 新类型 `text`；禁止 text 连边；`body` 字段 |
| sizing | width 驱动换行估高（新模块，可参考 ER 行高或边 label 估高） |
| layout | text 从主图层剥离，annotation 布局 pass |
| routing | text 可选 obstacle 或 skip |
| render | 多行 `<tspan>`；text 无 shape path |
| group bounds | 纳入 text 节点 |

**预期**：不改动正交路由核心逻辑，仅增加「非图顶点」实体类型与共享文本层。

### 7.3 开放问题

- [ ] `text` 用顶层关键字还是 `entity[text]`？
- [ ] Phase 1 caption 默认在 group 顶部还是底部？
- [ ] text 是否参与边路由障碍（默认软 / 硬 / 忽略）？
- [ ] `body` 与 ER `meta.fields` 的长期边界是否需要在 language-spec 中写死？

---

## 8. 结论

1. **仅单行 name/label 对实际应用偏薄**；ER 以外图类型尤其明显。
2. **独立 text node** 适合文档配图阶段的图例与段落，且不污染拓扑。
3. **同一套文本引擎**应同时服务 text node 与 entity `body`，但布局契约分离。
4. **`label`（短名）+ `body`（富文本）** 优于把多行内容塞进 `label`。
5. **先 text node，后 entity body**；定位从 group caption 做起，再考虑锚定与绝对坐标。
6. **受限标记子集**优于完整 Markdown，以保证布局确定性与 AI 生成稳定性。

---

## 附录：与现有代码的对应关系

| 现有能力 | 本方案中的角色 |
|----------|----------------|
| `Entity.label` | 保持短名；节点识别与拓扑 |
| `style.width` / `style.height` | text / body 的内容区尺寸 hint |
| `icons::render_entity_content` | 单行 + icon；body 阶段需扩展或并行新渲染路径 |
| `kinds/er/semantics` 字段列表 | ER 专用结构化内容，非富文本 body |
| `estimate_standard_node_width` | 仅适用于无 body 的标准节点；有 body 时走新测量 |
