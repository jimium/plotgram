# Drawify 视觉语言标准

> 版本：0.1.0-draft | 状态：设计中

本文档定义 Drawify **六种内置图表类型**的语义、适用场景、视觉约定与写作规范。它是设计语言层面的**设计标准**，与语法规范、布局实现、样式方案分工如下：

| 文档 | 职责 |
|------|------|
| [language-spec.md](../language-spec.md) | 语法与语义约束（怎么写） |
| **visual-language/**（本文档集） | 图表选型与视觉约定（画什么、为什么） |
| [layout-algorithms.md](../../architecture/layout-algorithms.md) | 布局算法与边路由（怎么排） |
| [style-sheet-spec.md](../style-sheet-spec.md) | 样式 JSON 结构与 token（怎么配色） |

---

## 全局设计原则

1. **语义驱动形状** — 用户只声明 `entity` 和 `relation`，形状由 `type` 与图表类型共同决定，不手写坐标或 Mermaid 形状语法。
2. **图表类型决定解读** — 同一 `type` 在不同图表中可能有不同视觉表现（如 `database` 在流程图与 ER 图中含义不同）。
3. **三种箭头，固定语义** — `->` 主动流向、`-->` 被动/响应、`<->` 双向；禁止引入更多箭头变体。
4. **显式优于隐式** — 关键节点应标注 `type`；流程起点/终点、状态初始态、思维导图根节点等不可依赖引擎猜测。
5. **约定优于配置** — 每种图表有默认布局、边路由与样式方案；仅在需要时覆盖。

---

## 图表类型总览

| 图表 | 关键字 | 核心问题 | 默认布局 | 默认边路由 | 实现状态 |
|------|--------|----------|----------|-----------|----------|
| [流程图](./diagrams/flowchart.md) | `flowchart` | 步骤怎么流转？ | `sugiyama-v2` | `orthogonal` | ✅ 已实现 |
| [时序图](./diagrams/sequence.md) | `sequence` | 谁按什么顺序交互？ | `sequence` | —（内置） | ✅ 已实现 |
| [架构图](./diagrams/architecture.md) | `architecture` | 系统由哪些组件构成？ | `architecture-v2` | `orthogonal` | ✅ 已实现 |
| [状态图](./diagrams/state.md) | `state` | 状态如何迁移？ | `circular` | `circular` | ⏳ 渲染器中，未标记稳定 |
| [ER 图](./diagrams/er.md) | `er` | 数据实体如何关联？ | `sugiyama-v2` | `straight` | ⏳ 渲染器中，未标记稳定 |
| [思维导图](./diagrams/mindmap.md) | `mindmap` | 知识如何分层展开？ | `mindmap` | `bezier` | ⏳ 渲染器中，未标记稳定 |

> **实现状态说明**：以 `diagram/registry.rs` 中 `DiagramProfile.implemented` 为准。未标记稳定的类型语法可写、有专属渲染器，但布局与视觉可能继续调整。

---

## 图表选型指南

| 你想表达… | 选 | 不要选 |
|-----------|-----|--------|
| 业务流程、审批、决策分支 | `flowchart` | `sequence`（交互时序）、`architecture`（组件拓扑） |
| HTTP/RPC 调用顺序、消息往返 | `sequence` | `flowchart`（会丢失时间轴语义） |
| 微服务拓扑、分层架构、组件依赖 | `architecture` | `flowchart`（流程语义会误导读者） |
| 订单/连接/支付等状态机 | `state` | `flowchart`（无初始/终止态约定） |
| 表结构、主外键、基数关系 | `er` | `architecture`（无 cardinality 约定） |
| 项目计划、知识树、头脑风暴 | `mindmap` | `flowchart`（树形辐射语义弱） |

---

## 通用关系语义

三种箭头在所有图表中语义一致，但**视觉样式**因图表类型略有差异：

| 箭头 | 语义 | 典型场景 |
|------|------|----------|
| `->` | 主动流向 | 调用、请求、流程推进、状态迁移触发 |
| `-->` | 被动/响应 | 返回结果、回调、异步响应 |
| `<->` | 双向关系 | 双向通信、数据同步、对等依赖 |

详见 [language-spec.md §6.2](../language-spec.md)。

---

## 文档索引

### 图表类型

- [流程图 (flowchart)](./diagrams/flowchart.md)
- [时序图 (sequence)](./diagrams/sequence.md)
- [架构图 (architecture)](./diagrams/architecture.md)
- [状态图 (state)](./diagrams/state.md)
- [ER 图 (er)](./diagrams/er.md)
- [思维导图 (mindmap)](./diagrams/mindmap.md)

### 实体类型

- [实体类型标准 (entity-types.md)](./entity-types.md) — 跨图表 type 语义、别名、适用矩阵与视觉形状

---

## 示例与对照

- [Showcase 示例集](../../../showcase/README.md) — 按类型组织、复杂度前缀命名的 `.dfy` 用例
- [Agent 编写指南](../../agent-guide.md) — 面向 AI 的语法速查，详细语义以本文档为准

---

## 相关代码

| 模块 | 路径 |
|------|------|
| 图表 Profile（默认布局、允许的 entity type） | `crates/drawify-core/src/diagram/registry.rs` |
| type 别名归一化 | `crates/drawify-core/src/diagram/profile.rs` |
| 各类型渲染器 | `crates/drawify-core/src/render/diagram/` |
| 各类型校验 | `crates/drawify-core/src/validation/` |
