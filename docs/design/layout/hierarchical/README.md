# HierarchicalLayout

> 状态：现行设计档（重建中；v1 Atlas 为功能参考真源）  
> 引擎注册名：`hierarchical`  
> 代码：`crates/plotgram-engine/src/layout/hierarchical/`（stub）  
> 参考实现：`crates/v1/plotgram-core/src/layout/atlas/` + `layout/kernel/layered/`  
> **目标架构真源**：[architecture.md](architecture.md)

## 签名

有向流、节点分层（rank）、层内定序减交叉；主方向多数边同向。plotgram 的**主布局核**。

## 基本逻辑

Sugiyama 骨架 + Atlas 三相写权（组合 → 度量 → Ink）。  
完整管线、算法选型、Channel、参数与里程碑见 **[architecture.md](architecture.md)**。

```text
① 拓扑    rank / order / 反转边
② 端口    side + along（显式决策，不可借住 Ink）
③ 度量    节点框、缝宽、track
④ 落笔    Plan+Metric → 折线（零新决策）
⑤ 标注    至少进 Demand；完整 integrated labeling 可渐进
```

## 能力范围 · 非目标 · 典型域

见 [scope.md](scope.md)。摘要：

| | |
|--|--|
| **做** | 分层、定序、坐标、组层次（Weak / StrongMacro profile）、端口与通道式内建正交边 |
| **不做** | 时序消息轴（→ Sequence）；纯树径向（→ Tree）；专用 ArchitectureLayout 巨石 |
| **典型域** | flowchart、architecture、state（分层路径）；ER 可挂本核 + profile |

## 边几何

- **默认**：内建正交 Ink（`EdgeGeometryMode::Builtin`），组合相决定端口/通道拓扑；`routing_style` 可选 polyline 等。  
- **可选**：节点冻结后交给独立 `EdgeRouter`（`DeferToRouter`）。  
- 二者契约见 [../routing/](../routing/)。

## 写权（本核）

| 自由度 | 写者 |
|--------|------|
| 边反向、layer、层内 order | 组合相 |
| 端口 side / along、gate / channel / bundle | 组合相 |
| 节点坐标、组框、track / 缝宽 | 度量相 |
| 折点像素路径 | Ink（只展开） |
| label 落位 | Demand + 度量（目标）；事后贴为债 |

纪律全文：[写权纪律](../write-authority.md)。

## 相关阅读

| 文档 | 用途 |
|------|------|
| **[architecture.md](architecture.md)** | **最终架构**：IR、算法表、Channel、组/分区、参数、里程碑 |
| [from-yfiles-reference.md](from-yfiles-reference.md) | yFiles 参考文库启发纪要 |
| [01 Sugiyama](../../../reference/yfiles/01-sugiyama分层布局.md) | P1–P5 算法证据 |
| [08 分组·泳道·端口](../../../reference/yfiles/08-分组泳道与端口约束.md) | group / partition / port |
| [archive/atlas 21](../../../archive/atlas/21-Hierarchical统一内核与泳道语义-可行性与演进建议-2026-07.md) | 统一内核立场（只读） |
| [archive/atlas 22](../../../archive/atlas/22-Atlas下一代布局与路由架构-总纲-2026-07.md) | 三相总纲（只读） |
| [shared/](../shared/) | group / port / label 共享语义 |

## 待展开（phases）

按需增加，不提前空文件（细节从 architecture 下沉）：

- `phases/ranking.md` — layering 策略与约束  
- `phases/ordering.md` — 交叉最小化与 group 连续性  
- `phases/ports-and-channel.md` — 端口、gate、channel、bundle  
- `phases/coordinate.md` — BK / Main·Cross track  
- `phases/ink.md` — 落笔与自反证门禁
