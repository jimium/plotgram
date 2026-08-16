# 布局内核设计档

> 状态：现行
> 位置：`docs/design/layout/`
> 约束入口：[AGENTS.md](../../../AGENTS.md) §1、[写权纪律](write-authority.md)

本目录按**布局内核**立档（不按图种）。每个内核一个文件夹，描述 plotgram **要做什么**：基本逻辑、能力范围、典型域，以及可展开的相级设计。

图种差异只经 profile 展开进算法参数，见 [ADR-001](../adr/001-diagram-type-not-in-engine.md)。

---

## 与其它文档的分工

| 层 | 位置 | 回答什么 |
|----|------|----------|
| **本目录** | `docs/design/layout/` | 我们要建的布局内核：契约、边界、写权、相切分 |
| 写权尺子 | [`write-authority.md`](write-authority.md) | 单写者、落笔零新决策 |
| Engine I/O | [ADR-006](../adr/006-engine-io-and-crates.md)、[model-boundary](../model-boundary.md) | Contract / Result、crate 边界 |
| 算法证据 | [`docs/reference/yfiles/`](../../reference/yfiles/00-索引与阅读指南.md) | 领域算法怎么做、论文与工程坑 |
| 产品能力对照 | [`yFiles-layouts-and-routing`](../../reference/yFiles-layouts-and-routing.md) | yFiles **有什么** |
| 历史债单 | [`docs/archive/atlas/`](../../archive/atlas/README.md) | Atlas 21–31（只读，不驱动实现） |
| 设计笔记 | [`docs/design/notes/`](../notes/README.md) | 短纪要（如 [v1 求解器 vs VPSC](../notes/v1-atlas-solver-vs-vpsc.md)） |

**不**把实现进度、基线跑分、删改日记写进本目录；那些属于 archive / issue。

---

## 必保内核

| 内核 | 注册名（目标） | 一句话 | 文档 | 代码（重建） |
|------|----------------|--------|------|----------------|
| Hierarchical | `hierarchical` | 有向分层；主核 | [hierarchical/](hierarchical/) | `plotgram-layout` → `layout/hierarchical` |
| Tree | `tree` | 树形递归放置 | [tree/](tree/) | `plotgram-layout` → `layout/tree`（骨架：层式居中放置器） |
| Sequence | `sequence` | 参与者轴 + 消息时间序；Builtin 边 | [sequence/](sequence/) · [架构](sequence/architecture.md) | `plotgram-layout` → `layout/sequence`（M4） |
| Circular | `circular` | 圆环 / BCC 多环 | [circular/](circular/) · [架构](circular/architecture.md) · [后置](circular/deferred.md) | **M3** 节点 `circle` 自定义分区 |

横切（按需生长，避免在每个内核里重复）：

| 夹 | 用途 |
|----|------|
| [shared/](shared/) | group / port / label 等共享语义如何被各核消费 |
| [debug-inspector.md](debug-inspector.md) | **跨核**布局调试 Trace 信封 + 检视 UI 壳（各核写 extension profile） |
| [routing/](../routing/) | 独立 `EdgeRouter`（正交）与原语 · [架构](../routing/orthogonal/architecture.md) |
| 实现零件 | [`crates/plotgram-algo`](../../../crates/plotgram-algo/PARTS.md) — VPSC / FAS 等可单测组件 |

新建内核时照 [_template.md](_template.md) 起夹。

---

## 图种 → 内核（编排层，非引擎分支）

```text
flowchart / architecture / state(分层)  → hierarchical（不同 profile）
mindmap                                 → tree
sequence                                → sequence
state(环形)                             → circular
```

引擎只认 `LayoutContract.layout.name` + 参数；禁止 `DiagramType` 分支。

---

## 阅读建议

1. 先读 [写权纪律](write-authority.md)。
2. 做 Hier：本目录 [hierarchical/](hierarchical/) → **架构** [architecture](hierarchical/architecture.md) → [相级契约](hierarchical/phases/README.md) → [anti-patterns](hierarchical/notes/anti-patterns.md) → 启发纪要 [from-yfiles-reference](hierarchical/nodes/from-yfiles-reference.md) → reference [01](../../reference/yfiles/01-sugiyama分层布局.md) / [13](../../reference/yfiles/13-实现路线图与选型.md)。
3. 做独立正交路由： [routing/](../routing/) → [orthogonal/](../routing/orthogonal/) → [architecture](../routing/orthogonal/architecture.md) → reference [03](../../reference/yfiles/03-正交边路由.md)。可手写 `RouteScene` 夹具，不经布局核。
4. 做 Circular：本目录 [circular/](circular/) → **架构** [architecture](circular/architecture.md) → [分区/圈序](circular/phases/partition-and-order.md) · [骨架/Ink](circular/phases/backbone-and-ink.md) → [vs-reference](circular/vs-reference.md) → [后置](circular/deferred.md) → reference [05 §4](../../reference/yfiles/05-树与径向布局.md) / [14](../../reference/yfiles/14-图论与优化工具箱.md)。径向树仍是 Tree placer，不是第二注册名。
5. 回溯 Atlas 决策动机时再翻 `docs/archive/atlas/`（21 立场 → 22 总纲）。
