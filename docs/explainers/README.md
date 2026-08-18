# explainers/ — 设计解读

> 图文并茂地带你读懂代码里某个**具体设计**的来龙去脉。

## 这个文件夹是什么

`explainers/` 放"帮我读懂设计"的解读文档：针对代码中一个具体的接口、数据结构或
协作机制，用**图 + 文 + 代码**讲清楚它为什么这样设计、各组件如何配合。

每篇通常对应一个源码位置（如某个 trait、某个 struct），并配一个自包含的 HTML 页面
（内联 SVG 图，无外部依赖，浏览器直接打开即可）。

## 与其它文档目录的区别

| 目录 | 性质 | 作用 |
|------|------|------|
| `design/` | **规范性**设计档 | 驱动实现，是设计的"真源" |
| `specs/` | **契约性**规范 | 语言 / AST / 样式的硬性约定 |
| `explainers/`（本目录） | **解释性**读物 | 帮助理解已落地的设计，**不驱动实现** |

解读文档若与设计档冲突，以 `design/` 为准。解读文档过时时应及时修订或标注。

## 命名约定

```
NNN-主题.html      图文解读页面（自包含，可直接浏览器打开）
NNN-主题.md        （可选）对应的纯文本摘要
```

编号 `NNN` 按写作顺序递增，便于排序与引用。

## 索引

| 编号 | 主题 | 对应源码 | 页面 |
|------|------|----------|------|
| 001 | EdgeRouter trait 与 RouteScene 生态 | `crates/tautcore-engine-api/src/traits.rs` L54-62 | [edge-router-trait.html](001-edge-router-trait.html) |
| 002 | Hierarchical 布局模块：三相架构与执行流程 | `crates/tautcore-layout/src/layout/hierarchical/` | [hierarchical-layout.html](002-hierarchical-layout.html) |
