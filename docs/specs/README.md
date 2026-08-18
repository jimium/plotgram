# Tautcore 规范目录

> 本目录存放 Tautcore 的**现行**语言与契约规范。重建期（v2）以标「现行」的文档为准。

## 现行

| 文档 | 内容 | 真源范围 |
|------|------|----------|
| [dsl-spec.md](dsl-spec.md) | DSL 2.4 — 语法形态（node / group / edge `{}` + 糖）+ **§14 属性注册表** | **语法 + 语义属性** |
| [archetype-spec.md](archetype-spec.md) | Archetype 展开糖 — CSV 真源、只填空、编译进二进制 | **archetype 目录 / 展开** |
| [style-sheet-spec.md](style-sheet-spec.md) | Theme 2.4 — 视觉属性词表 + 主题 JSON（`variants`）+ cascade | **视觉属性 / 主题** |
| [content-md-spec.md](content-md-spec.md) | 内容块 MD 瘦子集 — 行级 / 行内语法、降级规则、Content AST（ADR-005） | **内容块语法 / Content AST** |

四者互不重叠：一个属性只在一处被定义，跨文档只引用、不复制表格（见 dsl-spec §14.10）。

CSV 真源：`crates/tautcore-model/assets/archetypes.csv`。

## v1 遗留（待重写，不驱动 v2 实现）

| 文档 | 状态 |
|------|------|
| [ast-spec.md](ast-spec.md) | 0.1.0-draft；描述 v1 AST，与 `tautcore-model` 的 `Graph` / `LayoutContract` 不对齐 |
| [export-scene-spec.md](export-scene-spec.md) | 0.1.0-draft；基于 v1 `PreparedDiagram` / Exporter 分层 |
| [error-model.md](error-model.md) | 0.1.0；实现引用已不存在的 `crates/tautcore-core` |

## 相关

- [../design/](../design/) — 现行设计与 ADR（写权纪律、model boundary、图名不进引擎）
- [../reference/](../reference/) — yFiles / graphviz / cytoscape 能力参考
- [../archive/](../archive/) — 历史设计（只读）
