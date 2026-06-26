# VS Code 插件需求：Drawify DSL 集成

> 版本：0.1.0-draft | 状态：规划中

## 目标

在 VS Code 中为 Drawify DSL（`.dfy`）提供**语言级编辑体验**和**图形化预览能力**，并支持在 Markdown 文档中**内嵌并渲染** DSL 图表，让「写代码 → 看图形 → 写文档」在同一工作流里完成。

---

## 功能模块

### 1. 语言支持：语法高亮 + 语法/语义检查

**目标：** 编辑 `.dfy` 文件时有 IDE 级别的语言反馈。

| 能力 | 说明 |
|------|------|
| 语法高亮 | 关键字、标识符、字符串、注释、关系符号（`->`、`-->` 等）分色显示 |
| 语法检查 | 解析失败时给出精确位置的错误提示（行/列） |
| 语义检查 | 未定义引用、重复声明、图表类型约束等（基于 Validator） |
| 诊断体验 | 错误下划线、Problems 面板、可选 Quick Fix |

**典型场景：**

- 打开 `example.dfy`，立刻看到语法着色
- 写错 `entity` 或引用不存在的节点时，编辑器实时提示
- 保存或输入时自动校验，无需手动跑 CLI

**技术方向：**

- TextMate 语法（`syntaxes/`）负责高亮
- Language Server（`crates/drawify-lsp`，待建）负责诊断，复用 `drawify-core` 的 Parser / Validator
- 错误模型见仓库根目录 `docs/specs/error-model.md`（已按 LSP Diagnostic 设计）

---

### 2. 图形预览：`.dfy` 文件侧栏/面板预览

**目标：** 编辑 DSL 时可同步查看渲染后的图表。

| 能力 | 说明 |
|------|------|
| 预览入口 | 命令面板 / 编辑器标题栏按钮 / 快捷键打开 Preview |
| 实时更新 | 编辑 `.dfy` 后预览自动刷新（可 debounce） |
| 渲染输出 | SVG（首选），在 Webview 中展示 |
| 错误态 | DSL 有错误时预览区显示诊断信息，而非空白或崩溃 |

**典型场景：**

- 分屏：左侧编辑 DSL，右侧实时看流程图/架构图
- 修改 `entity` 或 `relation` 后，图形立即反映变化

**技术方向：**

- Webview Panel（`src/preview/`）
- 渲染链路复用 `drawify-wasm`，产物放入 `media/`（构建时从 `crates/drawify-wasm` 复制）
- 可参考 `playground/src/lib/wasm.ts` 的加载与调用方式

---

### 3. Markdown 集成：内嵌 DSL 在预览中渲染

**目标：** 在 Markdown 文档里写 Drawify 代码块，预览时显示为图形而非纯文本。

| 能力 | 说明 |
|------|------|
| 代码块识别 | 支持 ` ```drawify ` 或 ` ```dfy ` 等 fenced code block |
| 预览注入 | 扩展 Markdown 预览渲染器，将 DSL 块替换为 SVG/图片 |
| 错误降级 | 某段 DSL 无效时，显示错误信息或保留代码块，不影响整篇文档预览 |
| 可选同步 | 编辑 Markdown 时，内嵌图表随 DSL 内容更新（与 VS Code Markdown Preview 机制一致） |

**典型场景：**

在 `README.md` 或设计文档中写：

````markdown
```drawify
diagram flowchart {
    a -> b
}
```
````

打开 Markdown Preview 后，该代码块显示为流程图。

**技术方向：**

- 注册 `markdown.markdownItPlugins`，在扩展宿主侧用 Node WASM 渲染 `drawify` 代码块并注入 SVG（避免 Markdown Preview Webview 的 CSP 限制）
- 样式见 `media/markdown-preview.css`

---

## 用户价值

| 场景 | 价值 |
|------|------|
| 日常编辑 `.dfy` | 像写代码一样写图表，有高亮、报错 |
| 调试图表结构 | 边改边看，缩短「写 DSL → 跑 CLI → 看结果」的循环 |
| 文档与代码一体 | 设计文档、README 里直接嵌入可渲染的图表 |

---

## 仓库内位置与依赖

本扩展位于 monorepo 的 `editors/vscode/`，与以下模块协作：

```
flowml/
├── crates/
│   ├── drawify-core/     # Parser、Validator、Renderer
│   ├── drawify-wasm/     # WASM 绑定（预览 & Markdown 渲染）
│   └── drawify-lsp/      # 待建：LSP 语言服务
├── editors/
│   └── vscode/           # 本扩展
└── playground/           # 可参考的 WASM + 编辑器联动实现
```

**原则：** 解析与校验逻辑不重复实现，扩展层只做 IDE 集成（激活、Webview、Markdown 钩子、LSP 客户端）。

---

## 分期计划

| 阶段 | 范围 |
|------|------|
| **MVP** | `.dfy` 语法高亮 + 基础语法错误 + 手动触发的图形预览 |
| **V1** | LSP 语义校验 + 预览实时同步 + Markdown `drawify` 代码块渲染 |
| **V2+** | Quick Fix、跳转定义、格式化、导出 PNG/SVG 等 |

---

## 已确认决策

| 项 | 决策 |
|----|------|
| 文件扩展名 | 仅 `.dfy` |
| Markdown 代码块语言标识 | `drawify` |
| 预览渲染方式 | 本地 WASM（`drawify-wasm`） |
| 离线能力 | 是，WASM 打包进扩展，不依赖网络 |
| WASM 产物 | 在 `editors/vscode` 内独立构建，不复用 `playground/drawify-wasm` |

WASM 构建命令：`npm run build:wasm`（输出至 `media/wasm/` 与 `media/node/`）。

---

## MVP 实现状态

| 能力 | 状态 |
|------|------|
| `.dfy` 语法高亮 | ✅ |
| 语法/语义诊断（WASM validate） | ✅ |
| `.dfy` 图形预览（WASM render） | ✅ |
| Markdown `drawify` 代码块渲染 | ✅ |
| LSP（`drawify-lsp`） | 待建（V1） |
| Quick Fix / 格式化 | 待建（V2+） |
