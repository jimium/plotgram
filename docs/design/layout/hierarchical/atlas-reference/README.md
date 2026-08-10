# Atlas 参考索引

> 状态：**参考文档**，不是代码真源  
> 日期：2026-08-01  
> 父页：[hierarchical/architecture.md](../architecture.md) · [write-authority.md](../../write-authority.md)

## 这是什么

本目录把 v1 Atlas（`crates/v1/plotgram-core/src/layout/atlas/`）里**值得新 Hier 借鉴的设计、不变量与算法形状**抽出来，配精简 Rust 签名与典型测试场景，**避免日后为参考细节回去读 v1 代码**。

v1 Atlas 是 [architecture.md §12](../architecture.md) 定义的「功能与坑真源」。它**已经跑通端到端**，但其写权纪律、图种分支、三路径壳正是新架构要拆掉的东西。所以：

- **可以借**：数据结构形状、构建期不变量、verifier 违规分类、Plan IR 与稳定指纹、phase 序与 verify gate 位置、StrongMacro 展开序。
- **不能借**：写权归属（Atlas 是反例）、图种分支、post-Metric/post-solve 修几何、三路径壳、全局可变 override。

## 使用纪律

1. **遇到「Atlas 怎么做的」时，先查本目录**；只在本文未覆盖时才去读 v1 源码，且读完应回填到这里。
2. **本目录的 Rust 签名是精简版**，去掉了 v1 特有类型耦合（标 `[v1-coupled]`），字段名保留原文。**不要**把这里的代码当可编译的实现，它是设计模板。
3. **每篇文档末尾有「不该照搬」一节**——这是从 Atlas 反例里提炼的写权警告，新实现必须避开。
4. **本目录不记录 v1 进度**；只记录「设计决定 + 不变量 + 反模式」。

## 文档索引

| 文档 | 主题 | 对应新架构章节 |
|------|------|---------------|
| [channel-substrate.md](channel-substrate.md) | Substrate / Segment / Gate 数据结构 + 奇偶坐标编码 + B1–B8 切割 + 构建期防线 | §6.2 空间零件 |
| [channel-search.md](channel-search.md) | 词典序 Dijkstra + ScopeMask 硬过滤 + LexCost + 有界 rip-up | §6.1 L2/L3 写权 |
| [ink-verifier.md](ink-verifier.md) | InkPlanViolation 七类 + hard/soft 分割 + 双命中规则 | §11 InkVerifier |
| [strong-macro-expansion.md](strong-macro-expansion.md) | intra-rank → super-graph → macro-block → phase-D 四步展开（**只读参考**；现行方案见 [phases/strong-macro.md](../phases/strong-macro.md)） | §8.2 Group policy |
| [plan-ir-diff-fingerprint.md](plan-ir-diff-fingerprint.md) | Plan 字段 + Change 三态 diff + FNV-1a 稳定指纹 | §3.1 Plan IR |
| [pipeline-phase-order.md](pipeline-phase-order.md) | solve.rs 三路径 + Main LP L0–L3 阶梯 + verify gate 位置 | §4 / §12 落地顺序 |
| [group-invariants.md](group-invariants.md) | Containment / Sibling separation / verify_no_group_penetration | §6.3 三道防线 |
| [relaxation-provenance.md](relaxation-provenance.md) | RelaxationLadder L0–L4 + Sourced\<T\> 几何溯源 | §3.4 失败语义 |

## 三条核心原则

### 借鉴：奇偶坐标编码 + 构建期防线

Atlas 用 `ext: (usize, usize)` 奇偶坐标（`2j = gap`，`2j+1 = 节点体`）表达段在走廊上的区间，让 B1–B8 切割规则**由编码自然产出**，构建期拒绝穿组/斜线/不相交 link，而非事后检测。这是 Atlas 最有价值的设计创新，新实现应原样保留。详见 [channel-substrate.md](channel-substrate.md)。

### 借鉴：构造保证 + 独立证明器双层

`Substrate` 构建期保证无穿组，`verify_route_scope` 在搜索层独立复证（**显式不复用 ScopeMask 代码路径**）。这种「构造 + 证明」双层范式应推广到所有不变量。详见 [channel-search.md](channel-search.md) 与 [group-invariants.md](group-invariants.md)。

### 避免：post-Metric 修几何 + 图种分支 + 三路径壳

Atlas 最大的三个反模式：

1. **post-Metric 修几何**：`assign_port_along_offsets` / `assign_lane_indices` 在度量相入口被调；`skirt_root_main_x` 在 Ink 阶段平移根走廊；`nudge_intra_nodes_toward_cross_group_edges` 在组框定后改节点；`rigid_shift_group` 在 post-Ink 平移组——每层都不信任上游，留了「事后修」口子。
2. **图种分支**：`preset_for(diagram)` 按 `DiagramType` 选 preset；`intra_builder` 把 hub 居中 + client 对齐（architecture 语义）硬编码进 P1 objective——违反 [AGENTS.md §2](../../../../AGENTS.md) 禁止图名特判。
3. **三路径壳**：`solve_atlas_flat/weak/strong` 三个独立函数，metric tail 逻辑在 `finalize_metric_tail_on_nodes` / `metric_from_slots_publish_tracks` / `run_metric_tail_after_channel` 间重复；`run_metric_tail_after_channel` 注释「weak/strong 未跑 BK → 清空 LP track_coords」靠清空兜底。

新架构 [architecture.md §6 H6](../architecture.md) 要求「一个组合相」，[§2.4](../architecture.md) 要求类型化 Writer——正是拆掉这三件。

## v1-coupled 标注说明

文档里的 Rust 签名保留 Atlas 原字段名，但与 v1 上层耦合的类型（`Diagram` / `LayoutResult` / `SugiyamaLayoutConfig` / `DiagramType` / `GroupTable` / `LayoutSession` / `recipes::architecture::*` 等）标注 `[v1-coupled]`。新实现须用 `Graph` / `LayoutContract` / `HierarchicalParams` / typed Writer 替换。

## 与其他文档的关系

| 文档 | 角色 |
|------|------|
| **本文及子页** | Atlas 设计与坑的参考索引 |
| [architecture.md](../architecture.md) | Hier 目标架构真源（写权、IR、phase 序） |
| [write-authority.md](../../write-authority.md) | 全布局写权尺子 |
| [from-yfiles-reference.md](../nodes/from-yfiles-reference.md) | yFiles 阅读启发纪要 |
| `crates/v1/plotgram-core/src/layout/atlas/` | v1 源码（只在本文未覆盖时才读） |
