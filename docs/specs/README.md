# Drawify 语言规范

> 此目录存放 Drawify 语言的设计规范文档。

## 文档索引

| 文档 | 内容 |
|------|------|
| [language-spec.md](language-spec.md) | 语言语法与语义规范 — 完整的 BNF 语法、标识符规则、entity/relation/group 语义约束 |
| [visual-language/](visual-language/README.md) | 视觉语言标准 — 六种图表类型的定位、适用场景、实体 type 与视觉约定 |
| [visual-language/entity-types.md](visual-language/entity-types.md) | 实体 type 标准 — 跨图表语义、别名归一化、适用矩阵与选型速查 |
| [ast-spec.md](ast-spec.md) | AST 数据结构定义 — Rust 结构体、JSON 序列化格式、Diff/Patch 操作规范 |
| [export-scene-spec.md](export-scene-spec.md) | Exporter Scene JSON 规范 — 对外导出契约、字段 schema、兼容性与完整示例 |
| [error-model.md](error-model.md) | 错误模型与反馈机制设计 — 结构化错误码体系、Fix Action、LSP 兼容映射 |
| [style-sheet-spec.md](style-sheet-spec.md) | 样式方案 JSON 结构草案（v0.1，历史参考） |
| [style-system/](style-system/README.md) | 样式系统规范 v0.2 — 三层 cascade、Expand Pass 物化、完整 Blueprint 样式稿示例 |
