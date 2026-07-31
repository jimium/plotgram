# plotgram-algo · 优先零件清单

> 目的：在本 crate 里先落地一批**重要、相对独立、API 易稳、可单独测与优化**的图绘制零件。  
> 不负责：Hier/Tree 管线编排、Channel 搜索策略、`LayoutContract`、profile / 图种。  
> 证据与算法细节：[`docs/reference/yfiles/14`](../../docs/reference/yfiles/14-图论与优化工具箱.md)、[`13`](../../docs/reference/yfiles/13-实现路线图与选型.md)、[`15`](../../docs/reference/yfiles/15-几何与落笔层.md)。  
> 写权：[`docs/design/layout/write-authority.md`](../../docs/design/layout/write-authority.md)。

---

## 0. 挑选原则

入选须同时满足：

| 原则 | 含义 |
|------|------|
| **多消费者** | Hier 坐标 / nudging / 组框 / 路由 track / Ink 等至少两处会用，或度量门禁必用 |
| **输入可合成** | 不依赖完整 `Graph` / DSL；用数组、约束列表、点列即可测 |
| **输出可观测** | 断言坐标满足约束、交叉数、反向边集、变换可逆等——不断言内部计数器 |
| **API 面窄** | 函数少、结构体字段稳定；策略用枚举参数，不塞管线状态 |
| **确定性** | 平局显式 tie-break；禁止 `HashMap` 迭代序影响结果 |

**刻意不放进本 crate（或后置）的：**

- Network Simplex 分层、median 定序全文 —— 与 proper hierarchy / dummy 耦合紧，先跟 Hier 模块长，稳定后再考虑上提  
- LexA* / Channel 选路 —— 策略与 Plan IR 绑在一起  
- 真 MCF —— 非 MVP；有界 rip-up 留在路由实现  
- DemandBoard —— 先当设计契约；数据结构可很薄，不挡下面六件  

---

## 1. 首批六件（按落地顺序）

### P0-1 · `vpsc` — 一维分隔约束二次求解

| | |
|--|--|
| **为何重要** | 一物多用：层内间距、消重叠、正交 nudging、组框边界、紧致化（reference 13 ROI #3） |
| **独立性** | 只吃变量与不等式约束，不看图拓扑 |
| **稳定 API 直觉** | `solve(vars, constraints) -> Result<Positions, Infeasible>`；约束形如 `x_j - x_i ≥ d` |
| **验收** | 随机/手工约束：全部满足；小实例目标不差于枚举可行解；同一输入双跑 bit-identical |
| **参考** | 14 §VPSC；Dwyer–Marriott–Stuckey；与 v1 关系见 [设计笔记](../../docs/design/notes/v1-atlas-solver-vs-vpsc.md) |
| **模块** | [`src/vpsc.rs`](src/vpsc.rs) |

### P0-2 · `crossing` — 分层邻接两层交叉计数

| | |
|--|--|
| **为何重要** | P3 内环与 CI 度量；优化定序必须有正确、快的 oracle |
| **独立性** | 输入：两层序 + 跨层边列表；输出：交叉数 `u64` |
| **稳定 API 直觉** | `count_bipartite_crossings(order_a, order_b, edges) -> u64`；可选 Fenwick 实现 |
| **验收** | 手工小图与朴素 $O(E^2)$ 对拍；稳定 tie 无关（纯计数） |
| **参考** | 01 P3；Barth–Jünger–Mutzel；$O(E\log V)$ |
| **模块** | [`src/crossing.rs`](src/crossing.rs) |

### P1-1 · `fas` — Greedy Feedback Arc Set

| | |
|--|--|
| **为何重要** | Hier P1；重建 stub 缺显式反向位，必须补 |
| **独立性** | 有向邻接 + 稳定节点序 → 反向边集合 |
| **稳定 API 直觉** | `greedy_fas(nodes_sorted, successors) -> BTreeSet<EdgeId>`（或 `(u,v)` 对）；**保留反向标记语义，不删边** |
| **验收** | 结果图无环；平局按 id；同输入双跑一致；若干环图快照 |
| **参考** | 01 §P1；Eades–Lin–Smyth 1993 |
| **模块** | [`src/fas.rs`](src/fas.rs) |

### P1-2 · `orientation` — 布局方向变换

| | |
|--|--|
| **为何重要** | 核心算法只实现 TB；LR/RL/BT 靠变换（09 / 13）；避免四套代码 |
| **独立性** | 点、矩形、端口 side 的纯几何/枚举映射；可逆 |
| **稳定 API 直觉** | `Orientation::{Tb,Bt,Lr,Rl}`；`to_tb` / `from_tb` 作用于 `Point`/`Size`/`Side` |
| **验收** | 四向往返恒等（在量化误差内）；宽高互换规则有单测 |
| **参考** | 09 §2.3 |
| **模块** | [`src/orientation.rs`](src/orientation.rs) |

### P2-1 · `interval_color` — 通道内 track / 区间着色

| | |
|--|--|
| **为何重要** | 正交路由 L3：平行段占不同 track；与 Channel 搜索解耦后可单测 |
| **独立性** | 输入：一维区间列表（或段在通道上的占用）；输出：track 下标 |
| **稳定 API 直觉** | `color_intervals(intervals) -> Vec<TrackId>`；贪心按左端点 + 稳定 id |
| **验收** | 无同 track 重叠；track 数 ≤ 贪心上界；与朴素扫描对拍 |
| **参考** | 03 §通道内定序；14 区间着色 |
| **模块** | [`src/interval_color.rs`](src/interval_color.rs) |

### P2-2 · `path_ortho` — 正交折线规范化

| | |
|--|--|
| **为何重要** | Ink 观感一半在此；与「发明端口」严格分离（15） |
| **独立性** | 点列 in → 点列 out；只读几何，不改拓扑意图以外的东西（去共线/吸附） |
| **稳定 API 直觉** | `normalize_orthogonal(points, opts) -> Vec<Point>`；opts：`eps`、`min_segment`、是否 snap |
| **验收** | 表驱动：重复点、共线三点、近正交吸附、二次共线合并；规范化后仍正交 |
| **参考** | 15 §3 六步流水线 |
| **模块** | [`src/path_ortho.rs`](src/path_ortho.rs) |

---

## 2. 建议落地节奏

```text
Week 焦点
──── ──────────────────────────────────
 1   P0-1 vpsc + P0-2 crossing（度量与约束地基）
 2   P1-1 fas + P1-2 orientation（Hier M1 可接线）
 3   P2-1 interval_color + P2-2 path_ortho（路由/Ink 下游）
```

每件定义：

1. 模块内 **表驱动** `#[test]`（多 case 一个测试函数循环）；  
2. `cargo test -p plotgram-algo` 单独绿；  
3. 再在 `plotgram-engine` 的 hierarchical / route 里接线（可先只接一件）。

---

## 3. API / 依赖边界

| 允许 | 禁止 |
|------|------|
| `f64`、`usize` 索引、本 crate 内小型 `Point`/`Rect` 或日后薄依赖 `plotgram-model::geometry` | 依赖 `plotgram-engine` / `engine-api` 的 run 类型 |
| `thiserror` | `if diagram_type` / `profile` |
| 稳定序：`BTreeMap` / 显式 sort | `HashMap` 迭代驱动结果 |
| 迭代算法显式上限 + 超限错误 | 静默死循环 |

当前 **不**依赖 `plotgram-model`，减少编译耦合；若 `orientation` 需要与 model 的 `Side` 对齐，再增加 **仅 geometry/port 枚举** 的薄依赖，并在本文件注明。

---

## 4. 与 Hier / Atlas 迁移的关系

| 零件 | 迁 v1 / 造 Hier 时的角色 |
|------|--------------------------|
| `crossing` | 定序优化与回归门禁的 oracle |
| `fas` | 补齐重建反向边；Ink 按原方向画箭头 |
| `vpsc` | 逐步替换「BK 上打补丁」与散落 gap 启发式 |
| `orientation` | 取代多方向复制；LTR 不再是第二套坐标核 |
| `interval_color` | Channel 出拓扑后的 track 真源 |
| `path_ortho` | Ink 只展开：规范化 + 降半径，不发明端口 |

更多叙事见 [`docs/design/layout/hierarchical/from-yfiles-reference.md`](../../docs/design/layout/hierarchical/from-yfiles-reference.md)。

---

## 5. 完成定义（本清单）

- [ ] 六件均有可调用 API + 表驱动测试  
- [ ] `cargo test -p plotgram-algo` 稳定绿、确定性双跑（关键用例）  
- [ ] engine 至少接线：`fas` 或 `crossing` 之一进入 hierarchical 路径  
- [ ] 本文与模块 `//!` 状态从 stub 改为简述 API；不在此写进度日记
