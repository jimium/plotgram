# 架构与设计文档

欢迎阅读 Drawify 的架构与设计文档！

## 文档列表

- [整体架构](./overview.md) - Drawify 系统的整体架构
- [布局算法](./layout-algorithms.md) - 各图表类型的默认布局算法
- [布局意图与增量约束](./layout-intent-refinement.md) - 自动布局之上的意图约束与微调设计
- [Graphviz 算法研究](./graphviz-algorithms-research.md) - Graphviz 核心算法分析与 Rust 实现路线
- [布局算法优化计划](./layout-algorithm-optimization-plan.md) - 布局/边路由优化的优先级与分阶段落地计划
- [布局与边路由复盘分析](./布局优化/layout-routing-refactoring-analysis.md) - 最新实现复盘与 P0–P3 重构路线图
- [布局网格吸附设计](./layout-grid-snap-refinement.md) - 类绘图工具网格 snap 的后处理量化方案
- [布局 Refinement 实施备忘（TODO）](./layout-refinement-todo.md) - Grid Snap / Layout Intent 进度与依赖跟踪
- [Cytoscape.js 能力研究](./cytoscape-js-research.md) - Cytoscape.js 布局与图论算法分析及 Drawify 取舍
- [WASM 模块设计](./wasm-module.md) - WebAssembly 模块的实现细节
- [设计哲学](./design-philosophy.md) - 核心设计原则和思路
- [Graphic Style 与 Theme](./graphic-style-and-theme.html) - 笔触皮肤与样式方案的架构分离（含 SVG 图解）
- [Mindmap 统一主题方案](./mindmap-unified-theming-design.md) - 消除 mindmap 分支主题特例，纳入 StyleSheet cascade
- [Mindmap 大纲类 Interchange 方案](./mindmap-interchange-export-design.md) - Markdown / OPML / FreeMind 导出与导入（含大纲直接出图）
- [Core 重构方案](./drawify-core-refactor-plan.md) - `drawify-core` 模块与架构的完整改造计划
- [Core 实施路线图](./drawify-core-implementation-roadmap.md) - `drawify-core` 重构的分阶段落地计划

## 快速导航

想了解其他方面？
- [产品文档](../product/) - 产品愿景、功能、使用案例
- [技术规范](../specs/) - 语言规范、AST 定义、错误模型

## 贡献者指南

如果您想为 Drawify 贡献代码，请先阅读：
1. [整体架构](./overview.md) - 理解项目结构
2. [设计哲学](./design-philosophy.md) - 遵循设计原则
3. [Core 重构方案](./drawify-core-refactor-plan.md) - 了解核心模块的演进方向
4. [Core 实施路线图](./drawify-core-implementation-roadmap.md) - 了解分阶段改造步骤

## 相关代码

- [drawify-core](../../crates/drawify-core/src/lib.rs)
- [drawify-wasm](../../crates/drawify-wasm/src/lib.rs)
- [Playground 编辑器](../../playground/src/App.jsx)
