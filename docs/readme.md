# Plotgram 文档中心

项目级文档索引。各子目录的详细说明见对应 `README.md`。

## 目录结构

```
docs/
├── readme.md                 ← 本文件
├── specs/                    语言规范、AST、样式系统、视觉语言
├── guides/                   使用指南（LayoutLint、diff/patch 等实操文档）
├── product/                  产品愿景、功能、竞品与路线图
├── architecture/             系统架构总览（子文档待迁移整理）
├── enterprise/               企业场景需求与能力规划
├── 优化重构/                 布局与路由全局优化专案（当前主线）
├── 总结经验/                 踩坑复盘与核心手册
├── 方案计划/                 各方向方案设计与规划
├── 问题整理/                 问题清单与跟踪
└── notes/                    研究笔记与外部参考资料
```

## 快速导航

| 我想了解… | 从这里开始 |
|-----------|------------|
| 怎么写 `.pgm` 文件 | [specs/dsl/dsl-writing-manual.md](specs/dsl/dsl-writing-manual.md) |
| 语法与语义完整定义 | [specs/dsl/language-spec.md](specs/dsl/language-spec.md) |
| 六种图表怎么选、怎么画 | [specs/visual-language/](specs/visual-language/README.md) |
| 产品定位与差异化 | [product/vision.md](product/vision.md) |
| 企业落地路径 | [enterprise/scale-diagram-strategy.md](enterprise/scale-diagram-strategy.md) |
| 布局质量检查 LayoutLint | [guides/layout-lint.md](guides/layout-lint.md) |
| 布局路由全局优化 | [优化重构/00-研究总览.html](优化重构/00-研究总览.html) |
| 布局路由核心手册 | [总结经验/布局与路由核心手册-2026-07.md](总结经验/布局与路由核心手册-2026-07.md) |
| Studio 前端文档 | [../studio/docs/README.md](../studio/docs/README.md) |

---

## 优化重构/ — 布局与路由全局优化专案

> 当前主线工作，Phase A/B/C 推进中。详见各阶段文档。

| 文档 | 内容 |
|------|------|
| [00-研究总览.html](优化重构/00-研究总览.html) | 专案总览与导航页面 |
| [01-布局路由算法审查-现状诊断与问题根因分析.md](优化重构/01-布局路由算法审查-现状诊断与问题根因分析.md) | 现状诊断与根因分析 |
| [02-正交路由与布局优化方案.md](优化重构/02-正交路由与布局优化方案.md) | 整体优化方案设计 |
| [03-同类算法调研-工业实践与学术方法.md](优化重构/03-同类算法调研-工业实践与学术方法.md) | 工业与学术方法调研 |
| [04-Benchmarks美学指标扩展方案.md](优化重构/04-Benchmarks美学指标扩展方案.md) | Benchmark 美学指标扩展 |
| [05-PhaseA实施细节-代码级修复指南.md](优化重构/05-PhaseA实施细节-代码级修复指南.md) | Phase A 实施指南 |
| [06-PhaseA实施笔记-2026-07.md](优化重构/06-PhaseA实施笔记-2026-07.md) | Phase A 实施记录 |
| [07-PhaseB实施笔记-2026-07.md](优化重构/07-PhaseB实施笔记-2026-07.md) | Phase B 实施记录 |
| [08-PhaseC与美学指标实施笔记-2026-07.md](优化重构/08-PhaseC与美学指标实施笔记-2026-07.md) | Phase C 与美学指标实施记录 |
| [09-布局路由算法全面技术审查与重构建议-2026-07.md](优化重构/09-布局路由算法全面技术审查与重构建议-2026-07.md) | 全面技术审查与重构建议 |

---

## 总结经验/ — 复盘与核心手册

| 文档 | 内容 |
|------|------|
| [布局与路由核心手册-2026-07.md](总结经验/布局与路由核心手册-2026-07.md) | 布局与路由核心手册 — 踩坑复盘、缺陷审计、几何契约 |
| [简化重构经验-哪些不可精简-2026-07.md](总结经验/简化重构经验-哪些不可精简-2026-07.md) | 简化重构经验 — 哪些模块不可精简 |

---

## specs/ — 语言与技术规范

> 详细索引：[specs/README.md](specs/README.md)

### 核心规范

| 文档 | 内容 |
|------|------|
| [language-spec.md](specs/dsl/language-spec.md) | 语言语法与语义 — BNF、标识符、entity/relation/group 约束 |
| [dsl-writing-manual.md](specs/dsl/dsl-writing-manual.md) | DSL 写作手册 — 场景化实践指南 |
| [ast-spec.md](specs/ast-spec.md) | AST 数据结构 — Rust 结构体、JSON 序列化、Diff/Patch |
| [export-scene-spec.md](specs/export-scene-spec.md) | Exporter Scene JSON — 对外导出契约与 schema |
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

| 文档 | 内容 |
|------|------|
| [style-sheet-spec.md](specs/style-sheet-spec.md) | StyleSheet v0.2 — 三层 cascade、物化优先级、校验规则 |

---

## guides/ — 使用指南

> 详细索引：[guides/README.md](guides/README.md)

| 文档 | 内容 |
|------|------|
| [layout-lint.md](guides/layout-lint.md) | LayoutLint — 布局静态质量检查 |
| [group-layout-and-frame.md](guides/group-layout-and-frame.md) | Group `layout` + Group Frame；含 architecture macro rank 与场景短名 |
| [theme-and-style.md](guides/theme-and-style.md) | Theme 与 Graphic Style |
| [diff-and-patch.md](guides/diff-and-patch.md) | diff2 语义差异与 Agent 增量改图 |

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
| [d2-vs-plotgram-code-review.md](product/d2-vs-plotgram-code-review.md) | D2 源码对比与借鉴报告 |

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

## 方案计划/ — 各方向方案设计

### 顶层方案

| 文档 | 内容 |
|------|------|
| [架构图领域通用性反思-2026-07.md](方案计划/架构图领域通用性反思-2026-07.md) | 架构图领域通用性反思 |
| [甘特图设计方案-2026-07.md](方案计划/甘特图设计方案-2026-07.md) | 甘特图设计方案 |

### animation/ — SVG 动画能力

| 文档 | 内容 |
|------|------|
| [animation-capability-research.md](方案计划/animation/animation-capability-research.md) | SVG 动画能力需求分析与技术实现评估 |
| [animation-implementation-plan.md](方案计划/animation/animation-implementation-plan.md) | 动画能力分阶段落地方案 |
| [export-format-guide.md](方案计划/animation/export-format-guide.md) | 三类用户的导出格式选型与 Playground 提示文案 |
| [svg-embedding-design-impact.md](方案计划/animation/svg-embedding-design-impact.md) | SVG 嵌入方式对设计的影响 |

---

## 问题整理/

| 文档 | 内容 |
|------|------|
| [问题清单.md](问题整理/问题清单.md) | 问题清单与跟踪 |

---

## notes/ — 研究笔记与参考资料

### 参考资料/

| 文档 | 内容 |
|------|------|
| [graphviz-algorithms-research.md](notes/参考资料/graphviz-algorithms-research.md) | Graphviz 核心算法研究与 Rust 实现路线 |
| [cytoscape-js-research.md](notes/参考资料/cytoscape-js-research.md) | Cytoscape.js 能力研究与 Plotgram 取舍 |

---

## architecture/ — 系统架构

> 待整理迁移，详见 [architecture/README.md](architecture/README.md)（内容可能滞后）

---

## 其他文档位置

| 位置 | 范围 |
|------|------|
| [studio/docs/](../studio/docs/README.md) | Plotgram Studio 前端 — Agent、API、部署 |
| [showcase/README.md](../showcase/README.md) | 示例图集 — 按类型与复杂度组织的 `.pgm` 用例 |
| [crates/plotgram-core/src/layout/readme.md](../crates/plotgram-core/src/layout/readme.md) | 布局模块实现说明（代码旁文档） |
| [crates/plotgram-server/README.md](../crates/plotgram-server/README.md) | Server crate 快速入门 |
| [playground/README.md](../playground/README.md) | Playground 编辑器 |

## 阅读建议

1. **新贡献者**：`specs/language-spec.md` → `specs/dsl-writing-manual.md` → `guides/layout-lint.md`
2. **写图表**：`specs/visual-language/` → [showcase/](../showcase/)
3. **布局/路由开发**：[总结经验/布局与路由核心手册-2026-07.md](总结经验/布局与路由核心手册-2026-07.md) → [优化重构/](优化重构/00-研究总览.html) → `guides/layout-lint.md`
4. **产品/战略**：`product/vision.md` → `product/competitive-strategy.md` → `enterprise/`
