# Plotgram 文档中心

项目级文档索引。

## 目录结构

```
docs/
├── readme.md                 ← 本文件
├── design/                   架构设计（当前主战场）
├── specs/                    语言规范、AST、样式系统、视觉语言
├── guides/                   使用指南（实操文档）
├── reference/                外部参考与调研（yFiles、Graphviz、Cytoscape）
└── archive/                  历史设计文档（只读参考，不再维护）
    └── atlas/                Atlas 三相架构 21–31 号文档
```

---

## design/ — 架构设计

> 重新设计阶段的产出物。设计约束与方案文档放这里。

| 文档 | 内容 |
|------|------|
| [yFiles第一性原理与写权纪律-2026-07.md](design/yFiles第一性原理与写权纪律-2026-07.md) | 设计约束 — 单写者、落笔零新决策、判断句 |
| [model-boundary.md](design/model-boundary.md) | plotgram-model 边界 — Graph / 端口 / 边组 / 时序边序 |
| [adr/001…](design/adr/001-diagram-type-not-in-engine.md) | 图类型不进引擎 |
| [adr/002…](design/adr/002-no-config-block-freeform-options.md) | 废除 config 块 |
| [adr/003…](design/adr/003-edge-structural-fields.md) | Edge 结构一等字段与时序边序 |
| [adr/004…](design/adr/004-group-anchor-nodes.md) | 组间边经由 group_anchor 隐形节点 |
| [adr/005…](design/adr/005-content-measure-params.md) | 内容块、启发式度量与布局前 MeasureParams |

---

## specs/ — 语言与技术规范

> 详细索引：[specs/README.md](specs/README.md)

| 文档 | 内容 |
|------|------|
| [language-spec.md](specs/dsl/language-spec.md) | 语言语法与语义 — BNF、标识符、entity/relation/group 约束 |
| [dsl-writing-manual.md](specs/dsl/dsl-writing-manual.md) | DSL 写作手册 — 场景化实践指南 |
| [ast-spec.md](specs/ast-spec.md) | AST 数据结构 — Rust 结构体、JSON 序列化、Diff/Patch |
| [export-scene-spec.md](specs/export-scene-spec.md) | Exporter Scene JSON — 对外导出契约与 schema |
| [error-model.md](specs/error-model.md) | 错误模型 — 错误码、Fix Action、LSP 映射 |
| [style-sheet-spec.md](specs/style-sheet-spec.md) | Theme / StyleSheet V2 — 扁平 kind_styles cascade |

### 视觉语言

> 详细索引：[specs/visual-language/README.md](specs/visual-language/README.md)

| 文档 | 内容 |
|------|------|
| [entity-types.md](specs/visual-language/entity-types.md) | 实体 type 标准 — 跨图表语义、别名、适用矩阵 |
| [flowchart.md](specs/visual-language/diagrams/flowchart.md) | 流程图 |
| [sequence.md](specs/visual-language/diagrams/sequence.md) | 时序图 |
| [architecture.md](specs/visual-language/diagrams/architecture.md) | 架构图 |
| [state.md](specs/visual-language/diagrams/state.md) | 状态图 |
| [er.md](specs/visual-language/diagrams/er.md) | ER 图 |
| [mindmap.md](specs/visual-language/diagrams/mindmap.md) | 思维导图 |

---

## guides/ — 使用指南

> 详细索引：[guides/README.md](guides/README.md)

| 文档 | 内容 |
|------|------|
| [layout-lint.md](guides/layout-lint.md) | LayoutLint — 布局静态质量检查 |
| [group-layout-and-frame.md](guides/group-layout-and-frame.md) | Group layout + Frame |
| [theme-and-style.md](guides/theme-and-style.md) | Theme 与 Graphic Style |
| [diff-and-patch.md](guides/diff-and-patch.md) | diff2 语义差异与 Agent 增量改图 |

---

## reference/ — 外部参考与调研

| 文档 | 内容 |
|------|------|
| [layouts-and-routing.md](reference/layouts-and-routing.md) | yFiles 布局算法与路由风格（[HTML 版](reference/layouts-and-routing.html)） |
| [graphviz-algorithms-research.md](reference/graphviz-algorithms-research.md) | Graphviz 核心算法研究与 Rust 实现路线 |
| [cytoscape-js-research.md](reference/cytoscape-js-research.md) | Cytoscape.js 能力研究与 Plotgram 取舍 |

---

## archive/ — 历史设计（只读）

> Atlas 三相架构（组合→度量→Ink）的完整设计过程。不再驱动开发，仅供回溯。

| 文档 | 内容 |
|------|------|
| [archive/atlas/README.md](archive/atlas/README.md) | Atlas 文档索引与阶段记债 |
| 21–31 号文档 | 从立场、总纲、执行、channel、探针到收口的完整链路 |

---

## 其他文档位置

| 位置 | 范围 |
|------|------|
| [studio/docs/](../studio/docs/README.md) | Plotgram Studio 前端 |
| [showcase/README.md](../showcase/README.md) | 示例图集 |
| [playground/README.md](../playground/README.md) | Playground 编辑器 |

## 阅读建议

1. **写图表**：`specs/dsl/dsl-writing-manual.md` → `specs/visual-language/` → [showcase/](../showcase/)
2. **布局/路由设计**：`design/` → `reference/layouts-and-routing.md`
3. **语言实现**：`specs/language-spec.md` → `specs/ast-spec.md` → `specs/error-model.md`
