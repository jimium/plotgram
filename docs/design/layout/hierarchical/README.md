# HierarchicalLayout

> 状态：现行设计档（重建中；v1 Atlas 为功能参考真源）  
> 引擎注册名：`hierarchical`  
> 代码：`crates/plotgram-layout/src/layout/hierarchical/`  
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
- 二者契约见 [../../routing/](../../routing/)。

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
| **[architecture.md](architecture.md)** | **目标架构真源**：IR、算法表、Channel、组/分区、参数、里程碑 |
| **[roadmap.md](roadmap.md)** | MVP 之后的**阶段路线与方向**（A 拉直 → B 端口 → C 诊断 → D Channel/组框） |
| [debug-profile.md](debug-profile.md) | Hier 的 DebugTrace **扩展剖面**（rank/dummy/…） |
| [../debug-inspector.md](../debug-inspector.md) | **跨核**调试检视器信封 + UI 壳 |
| [phases/](phases/README.md) | 跨 crate 契约与各相可执行细节 |
| [from-yfiles-reference.md](notes/from-yfiles-reference.md) | yFiles 参考文库启发纪要 |
| [edge-parameters.md](edge-parameters.md) | 边参数支持研究（对照 yFiles Edges 分组）+ 分批实施路线 |
| [01 Sugiyama](../../../reference/yfiles/01-sugiyama分层布局.md) | P1–P5 算法证据 |
| [08 分组·泳道·端口](../../../reference/yfiles/08-分组泳道与端口约束.md) | group / partition / port |
| [archive/atlas 21](../../../archive/atlas/21-Hierarchical统一内核与泳道语义-可行性与演进建议-2026-07.md) | 统一内核立场（只读） |
| [archive/atlas 22](../../../archive/atlas/22-Atlas下一代布局与路由架构-总纲-2026-07.md) | 三相总纲（只读） |
| [shared/](../shared/) | group / port / label 共享语义 |

## 相级设计（phases）

细节从 architecture 下沉，不另造写权：

- [contracts-and-ir](phases/contracts-and-ir.md) — LayoutOutput/RouteScene、稳定 key、Stage、Diagnostics
- [composition](phases/composition.md) — FAS、ranking、properify、ordering、Plan freeze
- [ports-and-channel](phases/ports-and-channel.md) — 五档端口、gate/scope、Channel、track、bundle
- [channel-d1](phases/channel-d1.md) — D₁ 分阶段契约（D1.0–D1.2）
- [channel-corridor-allocator](phases/channel-corridor-allocator.md) — D1.3 Corridor Allocator 执行方案（①–⑤）
- [coordinate-and-demand](phases/coordinate-and-demand.md) — Demand epoch、BK/VPSC、组框、PartitionGrid、Orientation
- [ink-and-verification](phases/ink-and-verification.md) — Ink 纯展开与分相 verifier
