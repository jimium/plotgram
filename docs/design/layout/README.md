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
| Hierarchical | `hierarchical` | 有向分层；主核 | [hierarchical/](hierarchical/) | `plotgram-engine` → `layout/hierarchical` |
| Tree | `tree` | 树形递归放置 | [tree/](tree/) | 待建 |
| Sequence | `sequence` | 参与者轴 + 消息时间序 | [sequence/](sequence/) | 待建 |
| Circular | `circular` | 圆环 / 分量环 | [circular/](circular/) | 待建 |

横切（按需生长，避免在每个内核里重复）：

| 夹 | 用途 |
|----|------|
| [shared/](shared/) | group / port / label 等共享语义如何被各核消费 |
| [routing/](routing/) | 内建边 vs 独立 `EdgeRouter`、正交原语 |
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
2. 做 Hier：本目录 [hierarchical/](hierarchical/) → **目标架构** [architecture](hierarchical/architecture.md) → [相级契约](hierarchical/phases/README.md) → 启发纪要 [from-yfiles-reference](hierarchical/nodes/from-yfiles-reference.md) → reference [01](../../reference/yfiles/01-sugiyama分层布局.md) / [13](../../reference/yfiles/13-实现路线图与选型.md)。
3. 回溯 Atlas 决策动机时再翻 `docs/archive/atlas/`（21 立场 → 22 总纲）。
