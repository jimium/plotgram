# ADR-005: 内容块、启发式度量与布局前 MeasureParams

> 状态：accepted  
> 日期：2026-07-30  
> 关联：ADR-001、ADR-004、写权纪律、[`model-boundary.md`](../model-boundary.md)、主题 / dsl-spec（内容嵌入待跟进）

## 背景

节点内需要非拓扑「内容」：多行说明、职责列表、轻量富文本等。这要求：

1. 作者面可读的轻量写法  
2. 布局前已知的 **preferred size**（宽高）  
3. 渲染时框内 SVG  

同时存在几处易踩坑：

- 主题（含 `font_size`）目前住在 `plotgram-render`，若布局后再加载主题，度量与出图**双真源**  
- 真读字体文件可对齐字形，但与 headless / WASM /「不想读字体」产品偏好冲突  
- 若在 Ink/render 因「字挤了」再改节点高，违反落笔零新决策  

yFiles（HTML）的纪律是：样式侧 `measureText` → preferred size → 布局消费；量与画共用策略。其「真字宽」来自**浏览器环境**，不是布局内核解析 TTF。plotgram 无 DOM，不能照搬机制，但应照搬职责切分。

**ER 图**：框内字段表 / 结构化属性 **不**由本 ADR 的 MD 内容块解决；ER 另案（结构化字段或专用语法）。MD 不承担「画出正规 ER」的期望。

## 决策

### 1. 内容块 = 瘦 Markdown 子集 → Content AST

- 作者面：节点上的多行内容（DSL 嵌入形态另开 dsl-spec；倾向 `content` 多行 string）。  
- 解析为 **Content AST**（只结构，不定宽高）。  

**对 MD 的期望（要做好）**：

| 能力 | 用途 |
|------|------|
| 段落 / 硬换行 | 流程·架构节点的说明文案 |
| `**粗**` / `*斜*` / `` `code` `` | 强调、标识符 |
| 单层无序/有序列表 | 职责点、步骤摘要、注意项 |
| `---` 分隔线 | 框内简单分区（例如标题区与正文） |
| 与度量/SVG 闭环 | 上述子集能量宽高、能稳定出 SVG 片段 |

**白名单（v1）**即上表；**不做**完整 Markdown 表格引擎、嵌套列表、图片、链接跳转、HTML、foreignObject、CommonMark 全量。

**明确不期望 MD 解决**：

- ER 实体字段表、PK/FK 列对齐、schema 校验  
- 任何「必须结构化、可被引擎当数据读」的框内模型（那类另建模，MD 至多将来作可选糖，非本 ADR 范围）

### 2. 度量写 `ContentLayout`；渲染只展开

| 产物 | 写者 | 读者 |
|------|------|------|
| Content AST | parse | 度量 |
| **`ContentLayout`**（行盒 / run 几何 + `size`） | **度量相（唯一）** | 布局（preferred size）、render（SVG） |
| SVG `<g>` 片段 | render | — |

- 节点 preferred size = content size + chrome（标题栏、边框、icon 等，规则在度量相定稿）。  
- 放不下：wrap / 截断 / 省略号——策略在度量相定死；**禁止** Ink 因溢出改节点框。  
- 与 `group_anchor` **正交**：锚点无内容块。  
- **ASCII 后端**：不要求渲染内容块；**只支持 `label`**（有 content 时可忽略或仅显示 label）。内容块的度量/SVG 闭环只约束 SVG 主路径。

### 3. 不读字体文件（主路径硬约束）

- 宽：码点分档启发式（CJK ≈ 1em、拉丁 ≈ 0.55–0.6em 等）× `font_size`。  
- 高：固定 `line_height = font_size * k` + padding / gap 常数。  
- 误差靠 **padding / 最小框宽预算** 吸收；SVG `font-family` 在主题中声明为「估宽假设」，不保证与任意系统字体像素级一致。  
- **禁止**主路径打开 TTF/OTF；精确字体度量若将来需要，仅作可选旁路（如 raster），**不**阻塞内容块与 Hier。

### 4. 布局前只要 MeasureParams，不要整包 Theme

影响尺寸的量必须在 **engine 之前**定稿；颜色等仍可只在 render。

**MeasureParams**（名称以实现为准）至少含：

- typography：`font_size`、行高系数、（假设用）`font_family` 名  
- 框：node / label / content 的 padding、最小宽高  
- 内容块：列表缩进、分隔线厚度、max 宽高策略  

管线：

```
parse / profile
  → 按 DSL theme id compile 出 MeasureParams（及可选完整 CompiledTheme）
  → 度量：Graph + MeasureParams → 各 node preferred size / ContentLayout
  → LayoutContract（图 + 已定尺寸；仍无 diagram_type）
  → engine（只消费尺寸）
  → render（同一套字号画字；颜料用 Theme）
```

- **禁止** `plotgram-engine` 依赖 `plotgram-render` 整包主题。  
- MeasureParams 可与主题 JSON **同源字段**（编排层先抽），或抽到共享小模块；render 继续拥有色板 / variant。

### 5. crate / 模块边界（方向）

- Content AST + 启发式 measure + SVG emit：宜独立小模块（或 model 旁路），供编排层 / engine（只要 size）/ render（要片段）共用。  
- engine **不** import 完整 theme compile；只收 `MeasureParams` 或已算好的 size 表。

## 含义

| 层 | 影响 |
|----|------|
| **dsl-spec** | 增补 content 嵌入与 MD 子集；**不**把 ER 字段语法绑进 MD |
| **theme** | 区分「度量字段」与「颜料字段」；布局前必须能抽出前者 |
| **engine** | 度量相消费 preferred size；不猜字号、不读字体 |
| **render（SVG）** | 内容块按 `ContentLayout` 出片段；单行 label 估宽逐步与启发式对齐 |
| **render（ASCII）** | **只画 label**；不做内容块 / MD |
| **ER** | 另案；本 ADR 不定义字段表 IR |
| **测试** | 同 AST + 同 MeasureParams → 尺寸快照稳定（SVG 路径）；换 theme 字号必须改变 preferred size |

## 备选方案（未采用）

| 方案 | 放弃原因 |
|------|----------|
| 布局后 / render 内再量字改框 | 双真源；落笔发明尺寸 |
| engine 依赖完整 render theme | 依赖倒置；把颜料拖进布局 |
| 主路径读字体文件 | 与产品偏好、WASM/打包冲突；非纪律所需 |
| 全量 Markdown / HTML foreignObject | 面过大；尺寸与安全不稳 |
| 用 MD 扛 ER 字段表 | 产品明确不期望；结构化 ER 另案 |
| 固定所有节点同一尺寸、内容只裁切 | 可作降级，不能作为多行说明的主方案 |

## 非目标

- **ER 框内字段表与 schema 语义**（另案）  
- **ASCII 内容块**（ASCII 只支持 label）  
- 浏览器级排版、双向文、复杂 shaping  
- 主题切换保证「几何完全不变」（字号变则尺寸变；色变应尽量几何不变）
