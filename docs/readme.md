# Drawify 文档中心

项目级文档索引。各子目录的详细说明见对应 `README.md`。

## 目录结构

```
docs/
├── readme.md                 ← 本文件
├── specs/                    语言规范、AST、样式系统、视觉语言
├── guides/                   使用指南（CLI、LayoutLint 等实操文档）
├── product/                  产品愿景、功能、竞品与路线图
├── architecture/             系统架构、布局算法、边路由、WASM
├── enterprise/               企业场景需求与能力规划
├── animation/                SVG 动画能力研究
├── TRAE大赛/                 活动相关临时资料（非正式文档）
└── 重构方案/                 占位目录（暂无文档）
```

## 快速导航

| 我想了解… | 从这里开始 |
|-----------|------------|
| 怎么写 `.dfy` 文件 | [specs/dsl/dsl-writing-manual.md](specs/dsl/dsl-writing-manual.md) |
| 语法与语义完整定义 | [specs/dsl/language-spec.md](specs/dsl/language-spec.md) |
| 六种图表怎么选、怎么画 | [specs/visual-language/](specs/visual-language/README.md) |
| 系统整体架构 | [architecture/overview.md](architecture/overview.md) |
| 产品定位与差异化 | [product/vision.md](product/vision.md) |
| 企业落地路径 | [enterprise/scale-diagram-strategy.md](enterprise/scale-diagram-strategy.md) |
| HTTP 服务 API | [architecture/drawify-server-api.md](architecture/drawify-server-api.md) |
| 布局质量检查 LayoutLint | [guides/layout-lint.md](guides/layout-lint.md) |
| CLI 与工具链 | [guides/drawify-cli.md](guides/drawify-cli.md) |
| 渲染管线 | [guides/render-pipeline.md](guides/render-pipeline.md) |
| Studio 前端文档 | [../studio/docs/README.md](../studio/docs/README.md) |

---

## guides/ — 使用指南

> 详细索引：[guides/README.md](guides/README.md)

### 工具与管线

| 文档 | 内容 |
|------|------|
| [drawify-cli.md](guides/drawify-cli.md) | CLI 全命令 |
| [render-pipeline.md](guides/render-pipeline.md) | 渲染管线与 Rust API |
| [diff-and-patch.md](guides/diff-and-patch.md) | 语义 diff / patch |
| [drawify-eval.md](guides/drawify-eval.md) | 布局算法评估 |
| [showcase-workflow.md](guides/showcase-workflow.md) | 样例集回归工作流 |

### 布局与渲染

| 文档 | 内容 |
|------|------|
| [layout-lint.md](guides/layout-lint.md) | LayoutLint 布局质量检查 |
| [layout-intent.md](guides/layout-intent.md) | Layout Intent 快速入门 |
| [theme-and-style.md](guides/theme-and-style.md) | Theme 与 Graphic Style |
| [svg-debug.md](guides/svg-debug.md) | SVG 调试元数据 |

---

## specs/ — 语言与技术规范

> 详细索引：[specs/README.md](specs/README.md)

### 核心规范

| 文档 | 内容 |
|------|------|
| [language-spec.md](specs/dsl/language-spec.md) | 语言语法与语义 — BNF、标识符、entity/relation/group 约束 |
| [dsl-writing-manual.md](specs/dsl/dsl-writing-manual.md) | DSL 写作手册 — 场景化实践指南 |
| [dsl-attribute-redesign.md](specs/dsl-attribute-redesign.md) | 属性设计优化方案 — AttributeValue、命名空间、parser 策略 |
| [ast-spec.md](specs/ast-spec.md) | AST 数据结构 — Rust 结构体、JSON 序列化、Diff/Patch |
| [export-scene-spec.md](specs/export-scene-spec.md) | Exporter Scene JSON — 对外导出契约与 schema |
| [export-scene-spec.md](specs/export-scene-spec.md) | ExportScene 输入契约 |
| [draw.io 导出说明](../crates/drawify-core/src/render/encode/drawio/README.md) | draw.io 格式编码、映射与降级策略 |
| [error-model.md](specs/error-model.md) | 错误模型 — 错误码、Fix Action、LSP 映射与实现参考 |

### 视觉语言

> 详细索引：[specs/visual-language/README.md](specs/visual-language/README.md)

| 文档 | 内容 |
|------|------|
| [entity-types.md](specs/visual-language/entity-types.md) | 实体 type 标准 — 跨图表语义、别名、适用矩阵 |
| [diagrams/flowchart.md](specs/visual-language/diagrams/flowchart.md) | 流程图 |
| [diagrams/sequence.md](specs/visual-language/diagrams/sequence.md) | 时序图 |
| [diagrams/architecture.md](specs/visual-language/diagrams/architecture.md) | 架构图 |
| [diagrams/state.md](specs/visual-language/diagrams/state.md) | 状态图 |
| [diagrams/er.md](specs/visual-language/diagrams/er.md) | ER 图 |
| [diagrams/mindmap.md](specs/visual-language/diagrams/mindmap.md) | 思维导图 |
| [diagrams/c4/index.html](specs/visual-language/diagrams/c4/index.html) | C4 模型可视化页面 |

### 样式系统

> 详细索引：[specs/style-system/README.md](specs/style-system/README.md)

| 文档 | 内容 |
|------|------|
| [style-sheet-spec.md](specs/style-system/style-sheet-spec.md) | StyleSheet v0.2 — 三层 cascade、物化优先级、校验规则 |
| [style-system/README.md](specs/style-system/README.md) | 22 套内置主题 JSON 索引与生成脚本说明 |

---

## product/ — 产品设计

> 详细索引：[product/README.md](product/README.md)

| 文档 | 内容 |
|------|------|
| [vision.md](product/vision.md) | 项目愿景与定位 |
| [features.md](product/features.md) | 功能特性设计 |
| [use-cases.md](product/use-cases.md) | 使用场景与案例 |
| [comparison.md](product/comparison.md) | 与 Mermaid / PlantUML / Graphviz 对比 |
| [competitive-strategy.md](product/competitive-strategy.md) | 竞争策略 — 语义微调 vs 图形美观 vs 布局合理 |
| [success-roadmap.md](product/success-roadmap.md) | 成功路线图 — 必须做对的五件事 |
| [agent-mcp-skills-strategy.md](product/agent-mcp-skills-strategy.md) | Agent 生态：MCP、Skills、商业化与 draw.io |
| [connector-ast-scenarios.md](product/connector-ast-scenarios.md) | Connector → AST → Web 渲染的高价值场景 |
| [github-app-workflow-visualizer.md](product/github-app-workflow-visualizer.md) | GitHub App：Actions Workflow 可视化 |
| [d2-vs-drawify-code-review.md](product/d2-vs-drawify-code-review.md) | D2 源码对比与借鉴报告 |

---

## architecture/ — 架构与算法

> 详细索引：[architecture/README.md](architecture/README.md)（部分链接待更新，以本文件为准）

### 系统架构

| 文档 | 内容 |
|------|------|
| [overview.md](architecture/overview.md) | 整体架构 — 应用层、绑定层、核心引擎分层 |
| [wasm-module.md](architecture/wasm-module.md) | WASM 模块设计 — drawify-wasm 绑定与浏览器集成 |
| [drawify-server-api.md](architecture/drawify-server-api.md) | Server API 使用说明 — HTTP 端点与调用示例 |
| [drawify-core-pipeline.html](architecture/drawify-core-pipeline.html) | Core 渲染管线可视化 |
| [graphic-style-and-theme.html](architecture/graphic-style-and-theme.html) | Graphic Style 与 Theme 架构分离 |
| [layout-algorithms-classification.html](architecture/layout-algorithms-classification.html) | 布局算法分类可视化 |

### 布局意图（intent/）

| 文档 | 内容 |
|------|------|
| [layout-intent-optimized.md](architecture/intent/layout-intent-optimized.md) | Layout Intent 优化设计 v2.1 |
| [layout-intent-usage.md](architecture/intent/layout-intent-usage.md) | Layout Intent 使用指南 |
| [layout-intent-usage.html](architecture/intent/layout-intent-usage.html) | 使用指南可视化页面 |
| [layout-refinement-todo.md](architecture/intent/layout-refinement-todo.md) | Grid Snap / Layout Intent 实施进度跟踪 |

### 布局优化（布局优化/）

| 文档 | 内容 |
|------|------|
| [edge-routing-optimization-plan.md](architecture/布局优化/edge-routing-optimization-plan.md) | 边路由与标签布局优化方案 |
| [group-subgraph-layout.md](architecture/布局优化/group-subgraph-layout.md) | Group 子图独立布局方案 |
| [layout-routing-friendliness-evaluation.md](architecture/布局优化/layout-routing-friendliness-evaluation.md) | 拓扑路由友好性评估 — 布局↔路由反馈闭环 |
| [group-frame-spec.md](architecture/布局优化/group-frame-spec.md) | Group Frame 统一规范 — 组间/组内/节点三层框格模型 |
| [hint-vs-intent-research.html](architecture/布局优化/hint-vs-intent-research.html) | Hint vs Intent 对比研究可视化 |

### 算法参考（algorithms/）

| 文档 | 内容 |
|------|------|
| [layout-algorithms-index.html](architecture/algorithms/layout-algorithms-index.html) | 布局算法总览 |
| [layout-sugiyama.html](architecture/algorithms/layout-sugiyama.html) | Sugiyama 层次布局 |
| [layout-radial.html](architecture/algorithms/layout-radial.html) | 径向布局 |
| [layout-node-algorithms.html](architecture/algorithms/layout-node-algorithms.html) | 节点级布局算法 |
| [edge-routing-algorithms.html](architecture/algorithms/edge-routing-algorithms.html) | 边路由算法 |

### 外部研究（参考资料/）

| 文档 | 内容 |
|------|------|
| [graphviz-algorithms-research.md](architecture/参考资料/graphviz-algorithms-research.md) | Graphviz 核心算法研究与 Rust 实现路线 |
| [cytoscape-js-research.md](architecture/参考资料/cytoscape-js-research.md) | Cytoscape.js 能力研究与 Drawify 取舍 |

### 归档（backup/）

| 文档 | 内容 |
|------|------|
| [architecture-layout-improvement-plan.md](architecture/backup/architecture-layout-improvement-plan.md) | 布局改进计划（历史） |
| [flowchart-layout-analysis.md](architecture/backup/flowchart-layout-analysis.md) | 流程图布局分析（历史） |

---

## enterprise/ — 企业场景

> 详细索引：[enterprise/README.md](enterprise/README.md)

| 文档 | 内容 | 状态 |
|------|------|------|
| [scale-diagram-strategy.md](enterprise/scale-diagram-strategy.md) | 规模化架构图战略 — 场景矩阵、能力清单、落地路径 | draft |
| [capability-roadmap.md](enterprise/capability-roadmap.md) | 企业能力路线图 — DSL / 解析 / 渲染 P0–P2 排期 | draft |
| [international-market-opportunities.md](enterprise/international-market-opportunities.md) | 国际市场企业服务机会 | draft |
| [k8s-visualization-landscape.md](enterprise/k8s-visualization-landscape.md) | K8s 可视化行业现状与竞品对比 | draft |

---

## animation/ — 动画能力

| 文档 | 内容 |
|------|------|
| [animation-capability-research.md](animation/animation-capability-research.md) | SVG 动画能力需求分析与技术实现评估 |
| [animation-capability-research.html](animation/animation-capability-research.html) | 同上（可视化页面） |
| [animation-implementation-plan.md](animation/animation-implementation-plan.md) | 动画能力分阶段落地方案 |
| [export-format-guide.md](animation/export-format-guide.md) | 三类用户的导出格式选型与 Playground 提示文案 |
| [svg-embed-comparison.html](animation/svg-embed-comparison.html) | SVG 嵌入方式对比（`<img>` 与 CSS 动画实测） |
| [svg-css-animation-tutorial.html](animation/svg-css-animation-tutorial.html) | SVG 内嵌 CSS 动画简明教程（说明 + 效果展示） |

---

## 其他文档位置

| 位置 | 范围 |
|------|------|
| [studio/docs/](../studio/docs/README.md) | Drawify Studio 前端 — Agent、API、部署 |
| [showcase/README.md](../showcase/README.md) | 示例图集 — 按类型与复杂度组织的 `.dfy` 用例 |
| [crates/drawify-core/src/layout/readme.md](../crates/drawify-core/src/layout/readme.md) | 布局模块实现说明（代码旁文档） |
| [crates/drawify-server/README.md](../crates/drawify-server/README.md) | Server crate 快速入门 |
| [playground/README.md](../playground/README.md) | Playground 编辑器 |

## 阅读建议

1. **新贡献者**：`architecture/overview.md` → `specs/language-spec.md` → `specs/dsl-writing-manual.md`
2. **写图表**：`specs/visual-language/` → [showcase/](../showcase/)
3. **布局/路由开发**：`architecture/参考资料/` → `architecture/布局优化/` → [guides/layout-lint.md](guides/layout-lint.md)
4. **产品/战略**：`product/vision.md` → `product/competitive-strategy.md` → `enterprise/`
