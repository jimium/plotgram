# Mindmap 大纲类 Interchange 方案（导出 + 导入）

> 版本：0.1.1-draft | 状态：设计中
>
> 目标：为 `diagram mindmap` 增加 **Markdown 大纲**、**OPML**、**FreeMind (.mm)** 三种语义型 interchange 的 **导出与导入**，使 Drawify 与文档工具、大纲编辑器、桌面思维导图生态双向互通；并支持 **Markdown 大纲文本直接出图**（不经 Drawify DSL 手写）。

相关文档：

- [Mindmap 视觉语言规范](../specs/visual-language/diagrams/mindmap.md)
- [Scene JSON 导出规范](../specs/export-scene-spec.md)
- [Draw.io 导出实现说明](../../crates/drawify-core/src/render/encode/drawio/README.md)（降级报告模式参考）
- [Mindmap 结构展开](../../crates/drawify-core/src/prepare/structure/mindmap.rs)（`branch_slot` / `tree_depth`）

---

## 1. 背景与定位

### 1.1 现状

Drawify mindmap 已支持 **SVG / PNG / WebP / ASCII / Scene JSON / Draw.io** 导出。这些格式面向 **渲染与视觉编辑**，不面向：

- Obsidian、Notion、GitHub 等 **Markdown 文档** 工作流
- OmniOutliner 等 **大纲工具**
- FreeMind、XMind、MindMaster 等 **桌面思维导图**（Draw.io 可编辑但非原生导图 UX）

### 1.2 与 Scene JSON 的分工

| 维度 | Scene JSON | 本方案三种格式 |
|------|------------|----------------|
| 消费者 | Agent、自定义前端、CI | 人类作者、第三方桌面/文档工具 |
| 是否含 layout 坐标 | 是 | **否**（交给目标工具排版） |
| 是否含完整样式 | 是（物化后 fill/stroke） | 部分（FreeMind L1 可映射颜色） |
| 是否可读可 diff | 中 | Markdown / OPML **高** |

三种 interchange 格式与 Scene JSON **并列**，不替代 Scene JSON。

### 1.3 设计原则

1. **语义优先**：从 `PreparedDiagram` 的 relation 树导出，**不依赖** `ExportScene` 布局坐标。
2. **跳过视觉物化**：与 ASCII 类似，编码路径为 `PreparedDiagram → MindmapTree → 格式编码`，不调用 `build_scene()`。
3. **确定性**：子节点顺序按 **relation 插入序**，禁止依赖 `HashMap` 迭代序（见 [AGENTS.md](../../AGENTS.md)）。
4. **降级可观测**：FreeMind 采用与 draw.io 相同的 `ExportReport` + `DegradeTier` 模式（仅 L0 结构+文本，不导出样式）；Markdown / OPML 结构丢失时直接报错。
5. **仅 mindmap**：编码器在 `diagram_type != Mindmap` 时拒绝导出（`export_unsupported`）。

### 1.4 双入口定位

Drawify 主路径仍是 **DSL → 图**。本方案为 mindmap 增加 **第二输入通道**：

| 入口 | 典型用户 | 表达能力 |
|------|----------|----------|
| Drawify DSL | Agent、进阶用户 | 完整：type、theme、layout、显式 id |
| Interchange 导入 | 笔记用户、粘贴大纲、Obsidian/Notion 工作流 | **结构-only**：层级 + 文本 label |

两入口在 **`Diagram` AST 层汇合**，之后共用 `prepare → validate → layout → render`。Import **不替代** DSL，见 [§7 Import 通道](#7-import-通道大纲--图)。

### 1.5 非目标

- **XMind 原生 `.xmind`**（格式碎片化；导出侧经 FreeMind 中转；导入侧第一期不做）
- **完整 CommonMark 解析**（Import 仅支持大纲子集，见 §7.4）
- **在 `.dfy` 文件中自动猜测输入格式**（必须显式 `--input-format`，见 §7.2）
- **flowchart / sequence 等图表的 Markdown 导入**（mindmap-only）
- **Import 保留 radial 布局或 FreeMind 坐标**（导入后由 Drawify 重新布局）
- **跨分支 relation（非树边）** 的语义保留（导出第一期按树降级；Import 输入本身即为树）

---

## 2. 共享中间层：`MindmapTree`

三种格式共用同一棵规范化树，避免重复遍历 relation。

### 2.1 数据结构（Rust 草案）

```rust
/// 规范化后的 mindmap 树，供 outline / opml / freemind 编码器消费。
/// Export 与 Import 共用此结构（统一 owned 版本，避免生命周期泛型传染）。
pub struct MindmapTree {
    /// 图表标题（来自 diagram title 属性；可为 None）
    pub title: Option<String>,
    /// 树形根节点（见 §2.3 根节点策略）
    pub root: MindmapTreeNode,
    /// 无法挂入树的孤立节点（宽松模式下非空）
    pub orphans: Vec<MindmapTreeNode>,
}

pub struct MindmapTreeNode {
    pub entity_id: String,
    pub label: String,
    pub entity_type: Option<String>,   // root | main | branch | leaf
    pub branch_slot: Option<usize>,
    pub tree_depth: Option<usize>,
    /// 按 relation 插入序排列的子节点
    pub children: Vec<MindmapTreeNode>,
}
```

> **说明**：`MindmapTreeNode` 统一使用 `String`（owned），Export 侧从 `PreparedDiagram` clone 字符串，Import 侧直接 owned。mindmap 节点数量小，clone 开销可忽略。避免 `Cow<'a, str>` 或双套 `MindmapTreeOwned` 带来的泛型传染和维护负担。

### 2.2 构建算法

输入：`PreparedDiagram`（已通过 `prepare/structure/mindmap::expand`）。

1. 用与 [`prepare/structure/mindmap.rs`](../../crates/drawify-core/src/prepare/structure/mindmap.rs) 相同的 `build_children_map` + `find_root_id` 定位 root。
2. 对每个节点的子 id 列表，按 **该父节点下 relation 首次出现的顺序** 排序（与 `expand` 中 branch_slot 分配一致）。
3. DFS 递归构建 `MindmapTreeNode`。
4. 运行 **树合法性检查**（§2.4）。

模块建议路径（Export + Import 共用 `MindmapTree`）：

```text
crates/drawify-core/src/interchange/mindmap/
  tree.rs          # MindmapTree、树合法性检查
  diagram.rs       # build_mindmap_tree() / mindmap_tree_to_diagram()
  export/
    markdown.rs    # MarkdownOutlineEncoder
    opml.rs        # OpmlEncoder
    freemind.rs    # FreemindEncoder
    report.rs      # ExportReport（FreeMind 用）
  import/
    markdown.rs    # MarkdownOutlineParser
    report.rs      # ImportReport、ImportWarning
  mod.rs           # InputFormat / 统一入口
```

> **说明**：早期草案将 Export 放在 `render/encode/outline/`；引入 Import 后，双向逻辑统一到 `interchange/mindmap/`，Export 编码器仍 **不经过** `ExportScene`。

### 2.3 根节点与标题策略

mindmap DSL 中 **title**（图表级）与 **root entity label**（中心主题）可能不同：

```drawify
diagram mindmap {
    title: "2025 产品规划"
    entity root "产品规划" { type: root }
    ...
}
```

| 字段 | 来源 | 用途 |
|------|------|------|
| `MindmapTree.title` | `diagram` 的 `title` 属性 | Markdown H1 / OPML `<head><title>` / FreeMind 可选元数据 |
| `MindmapTree.root.label` | root entity 的 `label` | 树形内容根文本 |

**导出规则（默认 `root_title_mode: separate`）**：

| 格式 | title 存在且 ≠ root.label | title 缺失或与 root.label 相同 |
|------|---------------------------|--------------------------------|
| Markdown | 首行 `# {title}`，空行，再以 `## {root.label}` 起树 | 首行 `# {root.label}`，子节点从 `##` 起 |
| OPML | `<head><title>{title}</title></head>`，`<body>` 根 outline 为 root.label | `<title>` 省略或与 root.label 相同 |
| FreeMind | 根 `<node TEXT="{root.label}">`；title 写入可选属性或忽略 | 同左 |

可选 `root_title_mode: merge`：仅输出一个根文本 = `title.unwrap_or(root.label)`，不重复。

### 2.4 树合法性检查

当前 mindmap 校验仅保证 **最多 1 个 `type: root`**，不保证严格树。导出前需统一策略：

| 问题 | 检测 | `strict_tree: true`（默认） | `strict_tree: false` |
|------|------|----------------------------|----------------------|
| 节点有多个父 relation | 入度 > 1 | **拒绝导出**，`export_unsupported: multi_parent` | 取 **第一条** relation（插入序）为主父，其余写入 warning |
| 环 | DFS 回边 | **拒绝导出** | 截断环上边，warning |
| 不可达 root 的节点 | BFS/DFS | **拒绝导出** | 收入 `orphans`，Markdown 末尾以 `## 未连接节点` 段列出 |
| 无 root entity | 无 type:root | 回退入度 0 节点（与 layout 一致），warning L2 | 同左 |
| 孤立节点（零 relation） | 不在树中 | 同不可达 | 收入 `orphans` |

**第一期推荐**：CLI / Playground 默认 `strict_tree: true`；Agent 批量导出可通过选项放宽。

---

## 3. 管线位置

### 3.1 与现有 encode 层关系

```text
PreparedDiagram
    ↓
[diagram_type == Mindmap?] ──否──→ export_unsupported
    ↓ 是
build_mindmap_tree(options)
    ↓
┌─────────────┬─────────────┬──────────────┐
│ Markdown    │ OPML        │ FreeMind     │
│ Renderer    │ Renderer    │ Renderer     │
└─────────────┴─────────────┴──────────────┘
    ↓
RenderOutput::Text
```

**不经过** `compute_layout` / `build_scene` / `ExportScene`。

### 3.2 `FormatEncoder` 扩展：`EncodingPath`

现有 `FormatEncoder` trait 的核心方法 `encode_scene(&self, scene: &ExportScene)` 要求传入 `ExportScene`，但本方案的三种语义型格式不走 `ExportScene`。ASCII 已有同样问题，当前通过 `render_output_with_report` 中的 `if format == Ascii` 硬编码旁路解决。

为避免格式名硬编码累积，扩展 `FormatEncoder` 增加 `EncodingPath` 能力声明：

```rust
/// 编码路径：声明编码器需要哪条管线分支。
pub enum EncodingPath {
    /// 需要 layout → build_scene → encode_scene（SVG / PNG / JSON / Draw.io 等）
    Scene,
    /// 直接从 PreparedDiagram 编码，跳过 layout 和 scene（ASCII / Markdown / OPML / FreeMind 等）
    Diagram,
}

pub trait FormatEncoder {
    // 现有方法不变
    fn format(&self) -> RenderFormat;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn encode_scene(&self, scene: &ExportScene<'_>) -> Result<RenderOutput>;
    fn file_extension(&self) -> &str;

    /// 声明编码路径。默认 `Scene`。
    fn encoding_path(&self) -> EncodingPath {
        EncodingPath::Scene
    }

    /// 从 PreparedDiagram 直接编码（`EncodingPath::Diagram` 的编码器必须实现）。
    /// `EncodingPath::Scene` 的编码器无需覆写，默认返回 unsupported 错误。
    fn encode_from_diagram(&self, _diagram: &PreparedDiagram) -> Result<RenderOutput> {
        Err(DrawifyError::render_internal_msg(
            "format does not support direct diagram encoding",
        ))
    }
}
```

管线调度改为 `match` 分发，无格式名硬编码：

```rust
pub fn render_output_with_report(request: &RenderRequest<'_>) -> Result<RenderOutputWithReport> {
    let encoder = encoder_for(request.format)?;
    match encoder.encoding_path() {
        EncodingPath::Diagram => {
            let output = encoder.encode_from_diagram(request.diagram)?;
            Ok(RenderOutputWithReport {
                output,
                report: None,
                export_report: None,
            })
        }
        EncodingPath::Scene => {
            let scene = export_scene(request)?;
            let (output, export_report) = encoder.encode_scene_with_report(&scene)?;
            Ok(RenderOutputWithReport {
                output,
                report: scene.refinement_report,
                export_report,
            })
        }
    }
}
```

各编码器实现一行即可：

```rust
// 现有视觉格式
impl FormatEncoder for SvgRenderer {
    fn encoding_path(&self) -> EncodingPath { EncodingPath::Scene }
}

// 语义型格式
impl FormatEncoder for AsciiRenderer {
    fn encoding_path(&self) -> EncodingPath { EncodingPath::Diagram }
    fn encode_from_diagram(&self, diagram: &PreparedDiagram) -> Result<RenderOutput> { ... }
}

impl FormatEncoder for MdOutlineEncoder {
    fn encoding_path(&self) -> EncodingPath { EncodingPath::Diagram }
    fn encode_from_diagram(&self, diagram: &PreparedDiagram) -> Result<RenderOutput> { ... }
}
```

**迁移**：ASCII 的现有 `if` 硬编码旁路应迁移到 `EncodingPath::Diagram` + `encode_from_diagram`，作为本方案的 E0 前置清理。

### 3.3 `RenderFormat` 扩展

| 枚举值 | CLI / API 字符串 | 文件扩展名 | MIME（参考） |
|--------|------------------|------------|--------------|
| `RenderFormat::MdOutline` | `md-outline` | `.md` | `text/markdown` |
| `RenderFormat::Opml` | `opml` | `.opml` | `text/x-opml` |
| `RenderFormat::Freemind` | `freemind` | `.mm` | `application/x-freemind` |

```bash
drawify render showcase/mindmap/s.brainstorm.dfy -f md-outline -o brainstorm.md
drawify render showcase/mindmap/n.tech-stack.dfy -f opml -o tech-stack.opml
drawify render showcase/mindmap/c.product-roadmap.dfy -f freemind -o roadmap.mm
```

WASM / HTTP Server 第一期仅保证 **文本格式**（三种皆可）；与 draw.io 一样按需渲染即可。

---

## 4. Markdown 大纲导出

### 4.1 目标消费者

Obsidian、Notion（粘贴）、GitHub/GitLab README、飞书文档、任意 Markdown 编辑器、LLM 上下文。

### 4.2 语法模式

通过 `MarkdownOutlineOptions.syntax` 选择：

#### 模式 A：`atx_headings`（**默认**）

使用 ATX 标题，`#` 层级 = 树深度（含 root 层偏移，见下）。

**规则**：

- 有效标题级别：`min_level` ..= `max_level`（默认 1..=6）。
- 超过 `max_level` 的更深节点：**降级为同级列表**（见模式 B 嵌套列表段落）并 warning L2。
- 标题文本 = `entity.label`（不转义 `#`，若 label 含换行则替换为空格并 warning）。
- 标题行尾 **不** 追加 `{#entity-id}`（保持 Markdown 干净）；可选 `include_entity_ids: true` 时在 HTML 注释中保留 `<!-- drawify:entity-id -->`。

**深度映射**（`root_title_mode: separate` 且存在独立 title 时）：

| 树节点 | ATX 级别 |
|--------|----------|
| （文档 title） | `#` |
| root | `##` |
| root 子节点 | `###` |
| 再下一层 | `####` … |

**示例**（对应 `showcase/mindmap/s.brainstorm.dfy`）：

```markdown
# 头脑风暴

## 产品规划

### 功能需求

### 技术方案

### 市场调研
```

#### 模式 B：`nested_list`

使用无序列表 `-`，每深一层增加 **2 空格**缩进。

**导出**：固定 2 空格缩进（紧凑，CommonMark 合法）。

**导入**：兼容 2 空格和 4 空格缩进（常见 Markdown 编辑器两种都有）。

```markdown
# 头脑风暴

- 产品规划
  - 功能需求
  - 技术方案
  - 市场调研
```

`root_title_mode: separate` 时：title 仍为 `#`；root 作为列表第一项。

### 4.3 配置项

```rust
pub struct MarkdownOutlineOptions {
    pub syntax: MarkdownOutlineSyntax,  // AtxHeadings | NestedList
    pub root_title_mode: RootTitleMode,  // Separate | Merge
    pub min_level: u8,                   // 默认 1
    pub max_level: u8,                   // 默认 6
    pub include_entity_ids: bool,        // 默认 false
    pub strict_tree: bool,               // 默认 true
    pub newline: Newline,                // Lf | CrLf，默认 Lf
}
```

### 4.4 字段映射

| Drawify | Markdown |
|---------|----------|
| `diagram.title` | 可选 `# title` |
| `entity.label` | 标题文本或列表项 |
| `entity.type` | **不导出**（深度已隐含层级） |
| `branch_slot` / 颜色 / 形状 | **不导出** |
| `relation` 顺序 | 兄弟节点顺序 |
| `entity.id` | 仅 `include_entity_ids` 时 HTML 注释 |

### 4.5 降级与错误

| 级别 | 场景 |
|------|------|
| 错误 | 非 mindmap；strict_tree 下非树结构 |
| L2 warning | label 含换行/控制字符；深度超过 max_level |
| L3 | 孤立节点（宽松模式）跳过或附录输出 |

Markdown **无 L1 视觉近似**问题，实现最简单，**无 ExportReport 强制要求**（warnings 可选）。

Import 侧对称规则见 [§7.5 Markdown 导入](#75-markdown-大纲导入)。

---

## 5. OPML 导出

### 5.1 目标消费者

OmniOutliner、MindNode（导入 OPML）、RSS 阅读器类大纲工具、Workflowy 替代品。

### 5.2 文件结构

遵循 [OPML 2.0](http://opml.org/spec2.opml) 最小子集：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head>
    <title>头脑风暴</title>
    <dateCreated>Mon, 22 Jun 2026 12:00:00 GMT</dateCreated>
    <!-- 可选 -->
    <dateModified>...</dateModified>
  </head>
  <body>
    <outline text="产品规划" drawifyEntityId="root">
      <outline text="功能需求" drawifyEntityId="feature"/>
      <outline text="技术方案" drawifyEntityId="tech"/>
      <outline text="市场调研" drawifyEntityId="market"/>
    </outline>
  </body>
</opml>
```

### 5.3 映射规则

| Drawify | OPML |
|---------|------|
| `diagram.title` | `<head><title>` |
| `entity.label` | `outline/@text`（XML 转义） |
| 子 relation | 嵌套 `<outline>` |
| `entity.id` | 可选 `drawifyEntityId` 属性（`include_metadata: true`，默认 true） |
| `_created` | `dateCreated` = 导出 UTC 时间 |
| 样式 / layout | **不导出** |

**不使用的 OPML 特性**（第一期）：`_note`、`type`、`isComment`、`category`、`outline` 的 `url`/`xmlUrl`（除非 entity 未来有 link 属性）。

### 5.4 配置项

```rust
pub struct OpmlExportOptions {
    pub root_title_mode: RootTitleMode,
    pub include_metadata: bool,   // drawifyEntityId，默认 true
    pub strict_tree: bool,
    pub include_date: bool,       // head dateCreated，默认 true
}
```

### 5.5 孤立节点（宽松模式）

在 `<body>` 末尾追加平级 `<outline>`，并设 `drawifyOrphan="true"`：

```xml
<outline text="孤立节点" drawifyEntityId="orphan" drawifyOrphan="true"/>
```

Import 侧不在本方案范围内（Import 通道仅支持 Markdown 大纲，见 §7）。

---

## 6. FreeMind (.mm) 导出

### 6.1 目标消费者

FreeMind、XMind（导入 FreeMind）、MindMaster、MindManager（部分版本支持 .mm）。

### 6.2 文件结构

FreeMind 1.0.1 XML（最广泛兼容的子集）：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<map version="1.0.1">
  <node TEXT="产品规划" ID="FreemindRoot" FOLDED="false">
    <node TEXT="功能需求" ID="Freemind_1"/>
    <node TEXT="技术方案" ID="Freemind_2"/>
    <node TEXT="市场调研" ID="Freemind_3"/>
  </node>
</map>
```

根元素 `<map version="1.0.1">`；节点嵌套表示子树。**不导出颜色/样式**，由 FreeMind / XMind 打开后使用其默认自动样式。

### 6.3 映射规则

#### 6.3.1 结构（L0）

| Drawify | FreeMind |
|---------|----------|
| root entity | 顶层 `<node>`（单根） |
| 子 relation | 嵌套 `<node>` |
| 兄弟顺序 | relation 插入序 |
| `entity.label` | `TEXT` 属性（XML 转义） |
| `entity.id` | 生成唯一 `ID="Freemind_{sanitized}"`；可选 `drawifyEntityId` 自定义属性 |

**不导出** layout 坐标（`POSITION`、`VGAP` 等）：交给 FreeMind / XMind 打开后自动布局。

#### 6.3.2 视觉

FreeMind 导出 **不导出颜色/样式**（`COLOR`、`BACKGROUND_COLOR`、`STYLE`、`FONT_*` 等属性均不写入）。理由：

- Drawify 的样式物化依赖 theme cascade，与 layout 管线耦合，语义型导出路径不经过物化。
- FreeMind / XMind 打开 `.mm` 文件后会自动应用其默认主题样式，视觉效果由目标工具负责。
- 若未来需要样式导出，可作为独立增强在 `FreemindExportOptions` 中增加 `style_level` 选项，届时需解决物化路径的独立性问题。

#### 6.3.3 边

FreeMind 纯树格式 **无边对象**；父子关系仅由嵌套表达。Drawify 的 bezier 边 **不导出**（implicit L0）。

#### 6.3.4 不导出字段

| 字段 | 原因 |
|------|------|
| `canvas.title` | 可选写入 FreeMind `<!-- ... -->` 注释，非标准 |
| `graphic_style` / hand_drawn | 无等价 |
| 边 stroke / width | 无独立边 |
| `groups` | mindmap 不使用 |

### 6.4 配置项

```rust
pub struct FreemindExportOptions {
    pub root_title_mode: RootTitleMode,
    pub strict_tree: bool,
    pub include_entity_ids: bool,         // 自定义属性 drawifyEntityId
    pub id_prefix: &'static str,          // 默认 "Freemind_"
}
```

### 6.5 降级级别

| 级别 | 场景 |
|------|------|
| L0 | 结构 + TEXT 完整 |
| L2 | 多父 relation 丢弃的边；label 内特殊字符折叠 |
| L3 | 孤立节点跳过 |
| 错误 | 非 mindmap；strict_tree 下严重结构违规 |

每次导出返回 `ExportReport`（复用 draw.io 的 `DegradeTier` / `ExportWarning` 结构）。

### 6.6 与 XMind 的关系

不在 Drawify 内实现 XMind 原生格式。文档与 CLI help 中说明：

> 如需在 XMind 中编辑：导出 FreeMind (`.mm`) → XMind「导入」→ 选择 FreeMind 文件。

---

## 7. Import 通道（大纲 → 图）

### 7.1 定位

允许用户输入 **Markdown 大纲**，**不经手写 Drawify DSL** 直接渲染 mindmap。典型场景：

- 会议记录 / 培训大纲粘贴到 Playground 一键出图
- Obsidian、Notion 复制标题结构 → 可视化
- LLM 输出层级 Markdown（比生成带 `entity`/`relation` 的 DSL 更稳定）→ 引擎内部 import 再 render
- 与 Export 组成 round-trip：DSL → 图 → Markdown → 再编辑大纲 → 再出图

Import 产出 **`Diagram` AST**（`RawDiagram`），**不**经「拼 DSL 字符串 → 再 parse」的绕路。

### 7.2 设计原则

1. **显式输入格式**：调用方必须指定 `InputFormat`，**禁止**对 `.dfy` / 任意文本做启发式格式猜测。
2. **mindmap-only**：Import 固定 `diagram_type = Mindmap`；其他图表类型走 DSL。
3. **结构-only**：Import 只恢复树形结构与 label；theme、layout、graphic_style 使用 profile 默认（与新建 mindmap DSL 一致）。
4. **直接构造 AST**：`MindmapTree → Diagram`，然后走现有 `prepare()`；不修改 `dsl/parser`。
5. **与 Export 对称**：共用 `MindmapTree`；Export 为 `Diagram → Tree → 文本`，Import 为 `文本 → Tree → Diagram`。
6. **大纲子集**：不做完整 CommonMark；非大纲行（段落、代码块、表格等）**跳过并 warning**，或 strict 模式下报错。

### 7.3 管线位置

```text
Markdown 大纲文本（.md）
    ↓
parse_interchange(input, InputFormat::MdOutline, ImportOptions)
    ↓
MindmapTree（owned：含生成的 entity id）
    ↓
mindmap_tree_to_diagram(tree, DiagramBuildOptions)
    ↓
RawDiagram（Diagram { diagram_type: Mindmap, ... }）
    ↓
prepare() → validate() → layout → render    ← 现有管线，无改动
    ↓
SVG / PNG / …
```

与 DSL 路径对比：

```text
DSL 文本 ──parse()──→ RawDiagram ──prepare()──→ …
Interchange ──import()──→ RawDiagram ──prepare()──→ …
                              ↑ 汇合点
```

### 7.4 支持的输入子集

Import **不**承诺解析任意 Markdown 文档，仅支持 **大纲形态**：

| 语法 | 支持 | 说明 |
|------|------|------|
| ATX 标题 `#` … `######` | **是**（默认） | 层级 = 标题级别差 |
| 缩进无序列表 `-` / `*` | **是**（可选模式） | 2 或 4 空格一级，兼容常见编辑器；与 Export `nested_list` 对称 |
| YAML front matter | 可选 | `title:` 映射 diagram title；默认忽略其余字段 |
| Setext 标题 `===` | 否 | warning 跳过 |
| 段落、引用、代码块 | 否 | 跳过 + warning；`strict_content: true` 时报错 |
| 有序列表 `1.` | 第二期 | 与 `-` 二选一或统一为 list 模式 |
| 混用 ATX 与 list | 否 | 报错：ambiguous_syntax |

**自动检测语法**（在已选定 `InputFormat::MdOutline` 的前提下）：

1. 扫描非空行：若存在 ATX 标题行 → `atx_headings`
2. 否则若存在列表项 → `nested_list`
3. 否则 → 错误 `import_empty_outline`

> **注意**：自动检测基于 `#` 开头行的存在性，可能将非标题的 `#` 行（如 shell 注释）误识别为 ATX 标题。由于调用方必须显式指定 `--input-format md-outline`，用户已主动声明输入为大纲格式，此风险可接受。若输入内容明显不是大纲，解析器会在后续步骤中通过 `import_empty_outline` 或 `import_skipped_content` 报错/警告。

### 7.5 Markdown 大纲导入

#### 7.5.1 title 与 root 逆映射

与 Export §2.3 `root_title_mode: separate` **互逆**：

| 输入形态 | 生成结果 |
|----------|----------|
| 仅一个 `#` + 子树 | `#` → root.label；无 diagram title |
| `# title` + 空行 + `## root` + 子树 | `diagram.title = title`；root.label = 第二行标题文本 |
| `# title` + 空行 + 列表根 `- root` | 同上 |
| 多个 `#` 同级（多个 H1） | **strict**：报错 `multiple_roots`；**loose**：第一棵为 root，其余为 orphans appendix（不推荐） |

默认策略：**第一个大纲块的第一层节点为 root**；若全文只有一个 `#` 且无更深内容，则该 `#` 即 root。

#### 7.5.2 跳级标题

| 场景 | 行为 |
|------|------|
| `# A` 后直接 `### B`（跳过 `##`） | 视为 depth +2；warning L2 `heading_level_skip` |
| 层级深于 `max_level`（6） | 钳制到 depth 6 或报错（可配置） |

#### 7.5.3 label 与 inline 语法

| 输入 | 导入 label |
|------|------------|
| 纯文本 | 原样 trim |
| `[text](url)` | 默认取 `text`；可选 `preserve_links: true` 保留 Markdown |
| `**bold**`、`*italic*` | 第一期 strip 标记，仅保留 inner text；warning L2 |
| 行内 `` `code` `` | 保留反引号内文本 |

#### 7.5.4 entity id 生成

```rust
pub enum EntityIdStrategy {
    /// node_1, node_2, … 按 DFS 序（默认，确定性）
    Sequential,
    /// slugify(label)；冲突时追加 _2, _3
    Slugify,
    /// 解析 Export 写入的 <!-- drawify:entity-id -->
    FromMetadata,
}
```

- 默认 `Sequential`：与 Export 无 metadata 时 round-trip 时 id 会变，**结构不变**。
- 若 Export 时 `include_entity_ids: true`，Import 优先 `FromMetadata` 恢复 id。

#### 7.5.5 entity type 推导

Import 不写 DSL `type:`，在 `mindmap_tree_to_diagram` 中按树位置写入 `attributes.standard["type"]`：

| 树位置 | type |
|--------|------|
| 根节点 | `root` |
| root 的直接子节点 | `main` |
| 有子节点的更深层节点 | `branch` |
| 叶子节点 | `leaf` |

之后 `apply_profile_defaults` 仅补 **缺失** type，不会覆盖上述赋值。

#### 7.5.6 配置项

```rust
pub struct MarkdownImportOptions {
    pub syntax: MarkdownImportSyntax,  // Auto | AtxHeadings | NestedList
    pub entity_id_strategy: EntityIdStrategy,
    pub strict_content: bool,            // 非大纲行是否报错，默认 false
    pub max_level: u8,                   // 默认 6
    pub default_layout: Option<String>,   // 缺省 radial；可 override top-to-bottom
    pub default_theme: Option<String>,
}
```

#### 7.5.7 示例

输入（用户粘贴）：

```markdown
# 头脑风暴

## 产品规划

### 功能需求
### 技术方案
### 市场调研
```

等价于 `showcase/mindmap/s.brainstorm.dfy` 的语义（id 为生成值）：

```drawify
diagram mindmap {
    title: "头脑风暴"
    entity node_1 "产品规划" { type: root }
    entity node_2 "功能需求" { type: main }
    entity node_3 "技术方案" { type: main }
    entity node_4 "市场调研" { type: main }
    node_1 -> node_2
    node_1 -> node_3
    node_1 -> node_4
}
```

### 7.6 `mindmap_tree_to_diagram`

```rust
pub struct DiagramBuildOptions {
    pub diagram_type: DiagramType,  // 固定 Mindmap
    pub infer_entity_types: bool,   // 默认 true，见 §7.5.5
    pub layout: Option<String>,
    pub theme: Option<String>,
    pub graphic_style: Option<String>,
}

pub fn mindmap_tree_to_diagram(
    tree: &MindmapTree,
    opts: &DiagramBuildOptions,
) -> Diagram;
```

- 按 DFS 序创建 `Entity` 列表与 `Relation` 列表（父 → 子，`ArrowType::Active`，与 mindmap 惯例一致）。
- **不**在此阶段调用 `expand_structure`；由后续 `prepare()` 写入 `branch_slot` / `tree_depth`。
- `source_info.file` 可设为导入文件名；`span` 使用 `Span::dummy()` 或记录行号供 ImportReport。

### 7.7 入口 API 与 CLI

#### 7.7.1 `InputFormat`（Import）

```rust
pub enum InputFormat {
    Drawify,      // 默认：现有 DSL
    MdOutline,    // Markdown 大纲 → mindmap（Import 仅此一种 interchange）
}
```

> OPML / FreeMind 仅 **Export**；Import 通道不支持。

#### 7.7.2 核心函数

```rust
/// Interchange → RawDiagram（不跑 prepare）
pub fn import_interchange(
    source: &str,
    format: InputFormat,
    options: &ImportOptions,
) -> Result<ImportOutput>;

pub struct ImportOutput {
    pub diagram: RawDiagram,
    pub warnings: Vec<DiagnosticError>,
    pub report: ImportReport,
}

/// 一步到位：import + prepare + validate（CLI / WASM 常用）
pub fn import_prepare_validate(
    source: &str,
    format: InputFormat,
    style_request: &StyleRequest,
    options: &ImportOptions,
) -> PipelineOutput;
```

#### 7.7.3 CLI

```bash
# 大纲 → SVG（显式 input format）
drawify render notes.md --input-format md-outline -f svg -o mindmap.svg

# 大纲 → 打印等价的 Drawify DSL（调试 / Agent 转码）
drawify import notes.md --input-format md-outline --emit-dsl
```

**禁止**：`drawify render foo.dfy` 时根据内容自动切换 parser。

#### 7.7.4 WASM / Playground / Studio

|  surface | 行为 |
|----------|------|
| Playground | 编辑器模式切换：**DSL** \| **大纲（Markdown）**；后者走 `import_prepare_validate` |
| WASM | 新增 `import_render(source, input_format, output_format, options_json)` |
| Studio Agent | 检测用户消息是否为大纲子集时，内部 `InputFormat::MdOutline`，对用户可透明 |

可选：`--emit-dsl` 在大纲模式下显示等价的 Drawify 源码，供进阶用户继续 patch。

### 7.8 Round-trip 保证

| 路径 | 保证 |
|------|------|
| DSL → Export MD → Import → render | **结构等价**（节点数、层级、label、兄弟序） |
| 同上 | entity id **不保证**不变（除非 Export/Import 均启用 metadata） |
| 同上 | 颜色 / layout 方向 **不保证**（Import 用默认 theme/layout） |
| MD → Import → Export MD | 在固定 `syntax` + `root_title_mode` 下应 **规范化一致**（golden test） |

集成测试：`outline_roundtrip.rs` 对 showcase mindmap fixtures 断言结构同构。

### 7.9 Import 降级与错误

| 代码 | 级别 | 场景 |
|------|------|------|
| `import_unsupported_format` | 错误 | 未知 InputFormat |
| `import_empty_outline` | 错误 | 无有效标题/列表 |
| `import_ambiguous_syntax` | 错误 | ATX 与 list 混用 |
| `import_multiple_roots` | 错误 / warning | 多个 H1 |
| `import_skipped_content` | warning L2 | 段落、代码块等被跳过 |
| `import_heading_level_skip` | warning L2 | 跳级标题 |
| `import_duplicate_slug` | warning L2 | Slugify id 冲突已重命名 |

### 7.10 Import 非目标

- **OPML / FreeMind 导入**（仅 Export）
- 不解析 Mermaid `mindmap` 语法块
- 不从 Import 自动推断 `layout: left-to-right`（除非 options 指定）
- 不保证与 Obsidian Canvas / Excalidraw 互操作

---

## 8. 实现分期

### 8.1 Export

| 阶段 | 内容 | 估时 |
|------|------|------|
| **E0** | `interchange/mindmap/tree.rs` + `diagram.rs` + strict 校验 | 1–2 天 |
| **E1** | Markdown Export `atx_headings` + CLI `-f md-outline` | 1 天 |
| **E2** | OPML Export | 1 天 |
| **E3** | FreeMind L0 + L1 + `ExportReport` | 3–4 天 |
| **E4** | Playground 导出；WASM 注册 | 1 天 |
| **E5** | Markdown `nested_list` Export；FreeMind `Full` | 按需 |

### 8.2 Import

| 阶段 | 内容 | 估时 |
|------|------|------|
| **I1** | Markdown Import（ATX + Auto 检测）+ `import_prepare_validate` + CLI | 2–3 天 |
| **I2** | Playground「大纲模式」+ WASM `import_render` | 1–2 天 |
| **I3** | Markdown `nested_list` Import；`--emit-dsl` | 1 天 |
| **I4** | Studio Agent 大纲检测与内部 import | 按需 |

**推荐顺序**：E0 → E1 → **I1** → E2 → …（Export 与 Import 共用 E0 的 `MindmapTree` 后，**Markdown 导入可直接出图**，用户价值最大）。

---

## 9. 测试策略

### 9.1 单元测试

- `build_mindmap_tree` / `mindmap_tree_to_diagram`：互逆、多父、环、孤立节点、relation 序
- Export 编码器：XML/Markdown 快照测试
- Import 解析器：ATX / list / 跳级 / 混用语法 / 空输入
- `entity type` 推导：root / main / branch / leaf 与深度一致

### 9.2 集成测试

| 文件 | 覆盖 |
|------|------|
| `tests/outline_export.rs` | DSL → Markdown / OPML / FreeMind |
| `tests/outline_import.rs` | Markdown → Diagram → render SVG 非空 |
| `tests/outline_roundtrip.rs` | DSL → Export MD → Import → 结构同构（节点 label + 层级 + 兄弟序） |

| Fixture | 断言 |
|---------|------|
| `showcase/mindmap/s.brainstorm.dfy` | Export 三格式；Import MD 后 entity 数 = 4 |
| `showcase/mindmap/n.tech-stack.dfy` | 深度 > 3 缩进/层级正确 |
| `showcase/mindmap/c.product-roadmap.dfy` | FreeMind 节点数 = entity 数 |
| 手工非树 DSL | Export strict 报错含 `multi_parent` |
| 含段落/代码块的 .md | Import 跳过 + warning 或 strict 报错 |

### 9.3 人工验收

- **Export Markdown**：Obsidian 粘贴预览
- **Import Markdown**：Playground 大纲模式粘贴会议 notes → 辐射图
- **OPML**：OmniOutliner 往返
- **FreeMind**：FreeMind 1.0.x 或 XMind 导入/导出

---

## 10. 示例对照

源文件 `showcase/mindmap/s.brainstorm.dfy`：

```drawify
diagram mindmap {
    title: "头脑风暴"
    entity root "产品规划" { type: root }
    entity feature "功能需求" { type: main }
    entity tech "技术方案" { type: main }
    entity market "市场调研" { type: main }
    root -> feature
    root -> tech
    root -> market
}
```

### Markdown（`atx_headings`，`separate`）

见 §4.2 示例。

### OPML

见 §5.2 示例。

### FreeMind（`Basic`）

见 §6.2 示例（颜色随主题变化，测试时用 snapshot 宽松匹配或 mock theme）。

### Import 示例（Markdown → 等价 DSL）

见 §7.5.7。

---

## 11. 开放问题

| # | 问题 | 倾向 |
|---|------|------|
| 1 | FreeMind 导出是否跑 `materialize_styles`（无 layout）以取 fill 色？ | **已决定**：不导出样式，FreeMind 仅 L0（结构+文本），由目标工具应用默认自动样式，见 §6.3.2 |
| 2 | Markdown 默认 `atx` 还是 `nested_list`？ | **`atx_headings`**（文档感更强） |
| 3 | 是否在 mindmap validate 阶段加 strict tree 校验？ | 导出层校验即可；validate 加强列为后续独立 PR |
| 4 | `RenderFormat` 是否合并为单一 `Outline` + 子选项？ | **否**，三种独立格式，与 svg/json 并列 |
| 9 | 语义型格式如何接入 `FormatEncoder` 管线？ | **已决定**：扩展 `EncodingPath` 枚举 + `encode_from_diagram` 方法，见 §3.2 |
| 5 | orphans 在 strict 模式报错还是 warning？ | **报错**（strict）；宽松模式 appendix |
| 6 | Import 默认 entity id：`Sequential` 还是 `Slugify`？ | **`Sequential`**（确定性简单）；UI 展示 DSL 时用 slug 可读性更好，可 `--emit-dsl` 时可选 |
| 7 | Playground 大纲模式是否默认隐藏 DSL 编辑器？ | **分栏并存**；Import 后可「转为 DSL」切换 |
| 8 | Studio Agent 何时自动走 Import 而非生成 DSL？ | 消息 **仅含** ATX/list 大纲、无 `diagram` 关键字时 |

---

## 12. 参见

- [Mindmap 视觉语言](../specs/visual-language/diagrams/mindmap.md)
- [Draw.io 导出（降级报告参考）](../../crates/drawify-core/src/render/encode/drawio/README.md)
- [OPML 2.0 规范](http://opml.org/spec2.opml)
- [FreeMind XML 格式说明（社区文档）](https://freemind.sourceforge.io/wiki/index.php/File_format)
