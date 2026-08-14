# Hierarchical · 能力范围与典型域

> 父页：[README.md](README.md)

## 1. 能力范围（做）

| 能力 | 说明 |
|------|------|
| 有向分层 | 主方向流；反向边显式标记并保留到绘制 |
| 层内定序 | 降低相邻层交叉；支持 group 成员连续性约束 |
| Proper hierarchy | 跨层边经虚节点切段（组合/度量需要时） |
| 坐标与间距 | 节点不重叠；主/次轴与 track/缝宽可发布给 Ink |
| Group 一等公民 | 组框尺寸与位置由布局写出；跨组边经 gate/锚点策略，非纯后验描边 |
| Group policy | **Weak** / **StrongMacro** 均为本核 **profile 参数**，不是第二套布局器。Strong 方案：[phases/strong-macro.md](phases/strong-macro.md) |
| **PartitionGrid** | 正交列/行（ADR-008）；model/DSL 已定；组合/度量 **已消费**（PG-0–PG-4）→ [phases/partition-grid.md](phases/partition-grid.md)；render 泳道底色后置 |
| 端口 | 作者约束（`PortConstraint`）+ 算法决议（`PortRef`）；along 在组合相 |
| 内建正交边 | Channel/走廊拓扑 + Ink 展开；可选 bus/bundle（`edge_group`） |
| 独立路由衔接 | `DeferToRouter`：本核写节点与端口决议，边路径交给 router |

## 2. 非目标（故意不做）

| 非目标 | 归属 / 说明 |
|--------|-------------|
| 消息时间轴 + 生命线 | [Sequence](../sequence/) |
| 纯树 / 径向思维导图 | [Tree](../tree/) |
| 状态机默认「只圆环」 | [Circular](../circular/) 为 state 另一路径；本核保留分层路径 |
| 独立 `ArchitectureLayout` | 禁止；architecture = Hierarchical + StrongMacro 等 profile |
| DSL `swimlane` / `table` 关键字糖 | 非本期；正交分区用 **PartitionGrid**（[ADR-008](../../adr/008-partition-grid.md)、[shared/partition](../shared/partition.md)）；`group` 不演泳道 |
| 力导向 / 全图 Ortho-TSM 主路径 | 非本核；正交紧凑见 reference TSM，不冒充 Hier |
| 真全局 MCF 作为 MVP 阻塞 | 可渐进；有界返工优先于「先上真 MCF」 |
| 图种名分支 | 引擎内禁止；见 ADR-001 |

## 3. 典型域

| 域 | 为何适合 Hier | 常用 profile 直觉 |
|----|---------------|-------------------|
| 流程图 | 主方向、决策分支、可选分组 | flow + 正交内建；group Weak 可选 |
| 架构图 | 分层子系统、等宽条带、跨组边 | StrongMacro、equal-track、hub 对齐等 |
| 状态图（分层） | 状态迁移有向、层次嵌套 | flow 或收紧间距的分层 profile |
| 调用图 / 依赖图 | 有向无（少）环或可去环 | 同 flowchart 族 |
| ER（可选） | 可挂本核 + ER 向 profile；非必保专用核 | 正交边、端口约束更强 |

编排层：`profile: flowchart | architecture | …` 展开为 `layout: hierarchical` + 参数；用户可显式覆盖 `layout` / `edge_routing`。

## 4. 与产品能力的差距（设计记债，非进度日记）

相对 yFiles HierarchicalLayout 产品面，本核设计上承认、并单独跟踪的差距：

1. **Integrated labeling** — 完整联合求解可渐进；至少 label 需求进度量。  
2. **组合相单一产出类型** — Weak/StrongMacro 仅为收缩策略；输出字段路径间一致。  
3. **PartitionGrid 引擎消费** — 已落地（PG-0–PG-4：列连续块 + 行区间 + 四向 + Weak/Strong 共址）；render 泳道底色/标题后置。切片方案：[phases/partition-grid.md](phases/partition-grid.md)。  
4. **增量 / from-sketch** — 非 MVP 阻塞。

细节实现债见 `docs/archive/atlas/30`（只读）；本表只约束「设计上要收敛到哪」。
