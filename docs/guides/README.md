# 使用指南（guides）

面向开发者与贡献者的**实操文档**：怎么用某个能力、CLI 怎么调、Rust API 怎么接。

与以下目录区分：

| 目录 | 定位 |
|------|------|
| [specs/](../specs/README.md) | 语言、AST、导出格式等**规范契约** |
| [architecture/](../architecture/README.md) | 系统设计、算法原理、研究笔记 |
| [已经实现的方案/](../已经实现的方案/) | 历史方案存档（可能与当前代码有偏差） |
| **guides/**（本目录） | **当前代码**对应的使用说明，随实现更新 |

---

## 文档索引

### 工具与管线

| 文档 | 内容 |
|------|------|
| [drawify-cli.md](drawify-cli.md) | CLI 全命令：render / validate / lint / diff / patch |
| [render-pipeline.md](render-pipeline.md) | parse → prepare → layout → render 阶段与 Rust API |
| [diff-and-patch.md](diff-and-patch.md) | diff2 语义差异与 Agent 增量改图 |
| [drawify-eval.md](drawify-eval.md) | 布局与路由算法评分、对比、回归 |
| [showcase-workflow.md](showcase-workflow.md) | 样例集批量渲染与 CI 回归 |

### 布局与渲染

| 文档 | 内容 |
|------|------|
| [layout-lint.md](layout-lint.md) | LayoutLint — 布局静态质量检查 |
| [layout-intent.md](layout-intent.md) | Layout Intent 快速入门 |
| [group-frame.md](group-frame.md) | Group Frame — 组间等宽、对齐与矩阵排列 |
| [theme-and-style.md](theme-and-style.md) | Theme 与 Graphic Style |
| [svg-debug.md](svg-debug.md) | SVG `data-dfy-*` 调试元数据 |

---

## 推荐阅读顺序

**新贡献者（跑通工具链）**

1. [drawify-cli.md](drawify-cli.md)
2. [render-pipeline.md](render-pipeline.md)
3. [showcase-workflow.md](showcase-workflow.md)

**布局 / 路由开发**

1. [layout-lint.md](layout-lint.md)
2. [group-frame.md](group-frame.md)
3. [drawify-eval.md](drawify-eval.md)
4. [layout-intent.md](layout-intent.md) → [完整 Intent 文档](../architecture/intent/layout-intent-usage.md)

**Agent / 自动化改图**

1. [diff-and-patch.md](diff-and-patch.md)
2. [render-pipeline.md](render-pipeline.md)

---

## 相关代码位置

| 能力 | 路径 |
|------|------|
| LayoutLint | `crates/drawify-core/src/layout/lint/` |
| pipeline | `crates/drawify-core/src/pipeline/` |
| diff2 | `crates/drawify-core/src/diff2/` |
| drawify-eval | `crates/drawify-eval/` |
| CLI | `crates/drawify-cli/` |
| SVG debug | `crates/drawify-core/src/render/paint/svg_debug.rs` |
