# 拓扑路由友好性评估：布局 ↔ 路由反馈闭环研究方案

> 版本：0.1.0-draft | 状态：研究方案
>
> 相关文档：[Graphviz 算法研究](./graphviz-algorithms-research.md) | [Cytoscape.js 研究](./cytoscape-js-research.md) | [边路由优化方案](./edge-routing-optimization-plan.md) | [Layout Intent 优化](./layout-intent-optimized.md)

---

## 0. 摘要

当前 Drawify 的布局与边路由是**严格的两阶段流水线**：节点布局（Sugiyama / force-directed / circular / architecture-v2 等）产出 `LayoutResult`，再交给边路由（orthogonal / spline / bezier 等）计算路径。节点布局阶段**不感知边路由的需求**，导致布局完成后路由阶段可能产生大量穿障、长绕行、交叉。现有的 [refine.rs](../../crates/drawify-core/src/layout/refine.rs) 反馈循环只在**路由完成后**检测折线穿障并局部推开节点，属于"事后补救"，且只覆盖 `Polyline` 路径。

**提案**：在节点布局完成后、边路由前，插入一个**基于拓扑的快速路由友好性评估**步骤，基于节点 / group 在布局中的位置和图的拓扑结构，快速预测当前布局对边路由是否友好；若不友好，反馈给布局阶段做局部调整（节点位移、group 重排、通道预留），再进入正式路由。

**研究结论**：

- **可行性：高**。VLSI/EDA 领域有 15+ 年成熟的"routability-driven placement"研究主线（SimPLR、RUDY、congestion-driven placement），技术要素可直接迁移。
- **创新性：明确**。图绘制领域**无显式先例**——现有工作要么是事后度量（GD 2025 metrics），要么是隐式耦合（Graphviz dot 虚拟节点链、OGDF TSM shape 前移），"路由友好性评估 → 布局反馈"的显式闭环是空白点。
- **有效性：有证据支持**。VLSI 文献证明 lookahead routing 与 RUDY 密度图能显著减少布线线长与 DRC 违规；图绘制领域的 Nachmanson 路由 cost function 提供了可借鉴的度量组合。
- **风险：可控**。简单拓扑度量可能与真实路由质量脱节（VLSI sub-14nm 教训），需用真实路由器输出校准；单度量可被"愚弄"（GD 2025 警示），必须多维组合。

---

## 1. 背景与提案动机

### 1.1 当前架构

`compute_layout_with_plan` 的核心流水线（[mod.rs L695-L756](../../crates/drawify-core/src/layout/mod.rs)）：

```
validate_layout_config
  → strategy.compute(diagram)            // 节点布局（不感知路由）
  → grid_snap（可选）
  → router.route(diagram, result)        // 边路由（不感知布局意图）
  → refine::run_refine（仅 Polyline）    // 事后穿障补救
  → grid_snap edge waypoints
```

**关键观察**：

1. 节点布局阶段（`LayoutStrategy::compute`）只产出 `nodes` / `groups` / `total_width` / `total_height` / `hints`，**不产出任何"路由友好性"信号**。
2. 边路由阶段（`EdgeRoutingStrategy::route`）拿到的是"既成事实"的布局，只能在给定布局下做局部最优路径选择。
3. `LayoutHints`（[mod.rs L531-L537](../../crates/drawify-core/src/layout/mod.rs)）目前只携带 `circular` / `sequence` / `edge_routing_style` 三类算法特定提示，**没有路由友好性反馈通道**。
4. `refine.rs` 的反馈循环（[refine.rs L179-L227](../../crates/drawify-core/src/layout/refine.rs)）在路由**之后**才检测穿障，且只对 `Polyline` 路径生效——bezier / circular / straight 路径不参与。

### 1.2 信息量缺失的具体表现

| 场景 | 布局阶段不知道 | 路由阶段无法修复 | 后果 |
|------|---------------|------------------|------|
| 长边跨越多层 | 中间层节点会阻挡边路径 | 只能绕行或穿障 | 长折线 / 穿障 |
| group 间密集边 | group 间距不足容纳边通道 | 边被迫贴 group 边框 | 视觉拥挤 |
| 力导向布局 | 节点散布无层级 | bezier 直线穿过中间节点 | 穿障（bezier 不避障） |
| 同侧多边汇入 | slot 分布是否合理 | 只能按固定 pitch 分布 | 箭头分散 |
| 跨 group 长边 | group 边界是否预留通道 | 边必须绕 group 外圈 | 长绕行 |

### 1.3 提案核心思想

借鉴 VLSI 的 **routability-driven placement** 范式：在布局后、路由前，用一个**轻量评估器**（远快于完整路由）预测当前布局的路由友好度，输出：

1. **全局友好度分数**（标量，用于多候选布局排序或作为布局目标惩罚项）
2. **局部热点定位**（哪些区域 / 哪些边 / 哪些 group 不友好）
3. **调整建议**（节点位移方向、group 间距增加、通道预留）

评估器**不替代正式路由**，而是作为布局 ↔ 路由之间的"前置反馈层"，把路由阶段才知道的问题前移到布局阶段解决。

---

## 2. 现状代码基线

### 2.1 已有的"准友好性"信号

代码库中已存在若干散落的、可被友好性评估复用的信号：

| 信号 | 位置 | 现状 | 复用价值 |
|------|------|------|---------|
| 边穿障检测 | [refine.rs L67-L110](../../crates/drawify-core/src/layout/refine.rs) `analyze_edge_node_crossings` | 路由后检测 Polyline 穿障 | ★★★ 算法可直接复用为"事后友好度"基准 |
| orthogonal 障碍惩罚 | [scoring.rs L47-L76](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs) `obstacle_penalty` | 路由时对穿障路径加惩罚 | ★★ 惩罚函数可前移为预测信号 |
| orthogonal 近距擦过 | [scoring.rs L159-L178](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs) `segment_near_misses_node` | 检测贴边擦过 | ★★ 可作为"通道狭窄"信号 |
| 可见性图 | [visibility.rs L103-L156](../../crates/drawify-core/src/layout/edge/visibility.rs) `ObstacleIndex` | spline 路由的障碍索引 | ★★★ 可复用为"通道可用性"评估 |
| 评估指标 | [drawify-eval/metrics.rs L20-L56](../../crates/drawify-eval/src/metrics.rs) `LayoutMetrics` | 事后计算交叉数 / 穿障 / 边长方差 | ★★★ 度量定义可直接复用 |
| group 边界 | [mod.rs L82-L109](../../crates/drawify-core/src/layout/mod.rs) `GroupLayout` | group 包围框 + margin | ★★ 可用于 group 通道宽度计算 |
| LayoutHints | [mod.rs L531-L537](../../crates/drawify-core/src/layout/mod.rs) | 算法特定提示 | ★ 可扩展为友好性反馈通道 |

### 2.2 现有反馈循环的局限

[refine.rs](../../crates/drawify-core/src/layout/refine.rs) 的 `run_refine` 是当前唯一的布局 ↔ 路由反馈，但存在以下局限：

1. **时机靠后**：在路由完成后才检测，此时布局已"定型"，只能局部推开节点（`push_problem_nodes`），无法做结构性调整（如 group 重排、层间距调整）。
2. **覆盖不全**：只检测 `PathGeometry::Polyline`（[refine.rs L77-L79](../../crates/drawify-core/src/layout/refine.rs)），bezier / circular / straight 路径不参与。
3. **无前瞻**：推开一个节点可能引入新的穿障（依赖回退机制兜底，[refine.rs L215-L223](../../crates/drawify-core/src/layout/refine.rs)），但没有"推开前预测"能力。
4. **无全局视角**：只针对单条边的穿障，不评估"这条边的绕行会不会挤压其他边的通道"。
5. **无 group 感知**：推开节点时不考虑 group 边界约束，可能把节点推出 group。

### 2.3 度量体系现状

[drawify-eval/metrics.rs](../../crates/drawify-eval/src/metrics.rs) 的 `LayoutMetrics` 已定义 11 个事后度量，但：

- 全部是**事后评估**（post-hoc），在路由完成后计算，无法用于布局阶段预测。
- 缺少**通道拥堵度**、**group 间距充裕度**、**长边跨层数**等与路由友好性强相关的度量。
- 未与布局阶段建立反馈通道。

---

## 3. 学术与工业界调研

### 3.1 VLSI/EDA 领域：routability-driven placement（最强支持）

VLSI 物理设计与 Drawify 的布局-路由问题**结构同构**：placement ↔ 节点布局，global routing ↔ 边路由。VLSI 领域有 15+ 年的"可布性驱动布局"研究主线，是提案最直接的前置工作。

#### 3.1.1 SimPLR — lookahead routing（ICCAD 2011）

**论文**：Kim, Hu, Lee, Markov — "SimPLR: A Fast and Simple Placement Algorithm with Routability-aware Mode"  
**来源**：https://web.eecs.umich.edu/~imarkov/pubs/conf/iccad11-simplr.pdf

**核心思想**：在布局过程中调用一个**快速 3D 全局路由器**（lookahead routing）估计可布性，用其结果反馈布局调整。

**关键洞见**：

> "由于快速路由器与最终路由器的相关性，产生的可布性驱动布局在传统目标（线长、拥堵）上也更好。"

> "中间布局可以被评估多次，允许全局布局器做出适当调整。"

> "HPWL（半周长线长）这类简化度量在先进节点不再捕捉质量关键方面。"

**与提案的对应**：

| SimPLR | Drawify 提案 |
|--------|-------------|
| 快速全局路由器 | 快速友好性评估器（可见性图 / RUDY） |
| 中间布局多次评估 | 布局后、路由前的评估点 |
| 可布性驱动布局调整 | 友好度反馈布局调整 |

#### 3.1.2 RUDY — 路由需求密度图（DATE 2007）

**论文**：Spindler, Johannes — "Fast and Accurate Routing Demand Estimation for Efficient Routability-driven Placement"  
**来源**：https://www.researchgate.net/publication/221339455

**核心思想**：每条网用**矩形均匀线密度**（Rectangular Uniform wire DensitY）建模路由需求，叠加成全图密度场，拥堵区即不友好区。

**关键特性**：

- **与路由器无关**：不依赖特定路由模型，纯几何计算。
- **O(|E|) 复杂度**：每条边贡献一个矩形密度，叠加即可。
- **可嵌入力导向布局器**：在拥堵区减少路由需求（推开节点），在非拥堵区增加路由供应。
- **效果**：比 mPL/ROOSTER/APlace 减少 9%/8%/5% 布线线长，速度快 8 倍。

**对 Drawify 的直接价值**：RUDY 密度图是**最轻量、最路由器无关**的友好性估计，可直接迁移到图布局。每条边贡献一个矩形（从 from 节点到 to 节点的包围框），叠加成密度场，密度高的区域即潜在拥堵区。

#### 3.1.3 Congestion-driven placement — cell inflation（DAC 2025）

**论文**：Li et al. — "DCGP: Differentiable Congestion-Driven Global Placement"  
**来源**：https://xingquan-li.github.io/docs/paper/25-DAC25-DCGP.pdf

**两类拥堵**：

- **局部路由拥堵**：某区域 cell 过多 → **cell inflation** 技术（膨胀 cell 占位，推开邻居）。
- **全局路由拥堵**：过多网穿过 G-cell → 不能靠移动 cell 解决，需**可微拥堵函数**。
- **Momentum-based cell inflation**：考虑历史膨胀比，避免 cell 回流到拥堵区。

**对 Drawify 的启发**：

- cell inflation ↔ 在拥堵区"膨胀"节点 margin，推开邻居——可直接迁移到力导向 / 约束求解布局。
- momentum 机制 ↔ 避免节点在多次反馈中回流到不友好位置。

#### 3.1.4 ML 预测 DRC 违规（ISPD 2017）

**论文**：Chan, Ho, Kahng, Saxena — "Machine Learning for Pre-Routing DRC Prediction"  
**来源**：https://vlsicad.ucsd.edu/Publications/Conferences/348/c348.pdf

**关键警示**：

> "在 sub-14nm，全局路由拥堵图与详细路由 DRC 违规**相关性崩塌**。"

> "用 ML 预测 DRC 热点位置（74% 召回，<0.2% 误报），自动减少 5× DRC。"

**教训**：简单拓扑度量可能与真实路由质量脱节，**需用真实路由器输出校准**。这对提案是重要风险提示——友好性评估器必须与目标路由器（orthogonal / spline）的真实输出做相关性验证。

#### 3.1.5 RoutePlacer — GNN 预测可布性（DAC 2024）

**论文**：RoutePlacer — "End-to-End Routability-aware Placer"  
**来源**：https://dl.acm.org/doi/pdf/10.1145/3637528.3671895

**核心**：训练 **RouteGNN**（定制 GNN）预测 placement 的可布性，融合几何 + 拓扑表示，作为**可微代理**支持端到端梯度优化。

**对 Drawify 的长期价值**：若启发式评估器精度不足，可考虑训练 GNN 代理。但需要先积累 (布局, 路由质量) 数据集——Drawify 的 [drawify-eval](../../crates/drawify-eval/) 框架天然适合生成此类数据。

### 3.2 图绘制领域：质量度量与隐式耦合

#### 3.2.1 GD 2025 — 通用质量度量

**论文**：Mooney, Hegemann, Wolff, Wybrow, Purchase — "Universal Quality Metrics for Graph Drawings"  
**来源**：https://drops.dagstuhl.de/storage/00lipics/lipics-vol357-gd2025/LIPIcs.GD.2025.30/LIPIcs.GD.2025.30.pdf

将 10 个经典度量推广到曲线 / 折线边，核心包括：

- **Edge Crossings (EC)**：`c = Σ|E(x)|²`，按 `cmax` 归一化
- **Edge Length Deviation (EL)**：边长相对均值的均方偏差
- **Edge Orthogonality (EO)**：边段与坐标轴的对齐度
- **Crossing Angle (CA)**：交叉处角度偏离 90° 的程度

**对提案的价值**：这些度量的数学定义可直接复用为"友好度"的组成项。但注意——它们都是**事后度量**，需改造为事前预测。

#### 3.2.2 GD 2025 — 单度量可被愚弄（重要警示）

**论文**：van Wageningen, Mchedlidze, Telea — "Same Quality Metrics, Different Graph Drawings"  
**来源**：https://arxiv.org/html/2508.15557v1/

**关键发现**：通过模拟退火可将图变形为任意目标形状（恐龙、网格等）而**保持单一度量几乎不变**。

**教训**：友好度评估**必须多维组合**，不能依赖单一指标。这与 VLSI 的教训（HPWL 在先进节点失效）一致。

#### 3.2.3 Nachmanson — 路由 cost function

**论文**：Bereg, Holroyd, Nachmanson, Pupyrev — "Edge Routing with Ordered Bundles"  
**来源**：https://www.arxiv-vanity.com/papers/1209.4227/

提出显式的边路由代价函数：

```
cost = f(ink, edge_length, width, separation, congestion_in_narrow_channels)
```

用约束 Delaunay 三角剖分惩罚"过多边穿过窄通道"。这是少有的把"通道拥堵度"写进路由代价函数的工作。

**对提案的价值**：`congestion_in_narrow_channels` 可直接作为友好度评估的核心项——布局阶段若能预测哪些通道窄、哪些通道拥堵，就能提前调整。

#### 3.2.4 Shape-first 范式（GD 2025）

**论文**："A Walk on the Wild Side: a Shape-First Methodology for Orthogonal Drawings"  
**来源**：https://arxiv.org/html/2508.19416v1/

**颠覆传统 TSM 流水线**：先优化 shape（折弯最小化，SAT 求解），再算坐标。说明图绘制社区正在反思"布局优先"的范式。

**对提案的启发**：把路由相关约束（折弯、通道）前移到布局阶段是学术趋势，提案方向与之一致。

### 3.3 主流图可视化库的反馈机制

| 库 | 布局-路由关系 | 反馈机制 | 对提案的启发 |
|---|---|---|---|
| **yFiles** | 模块化：HierarchicalLayout / OrthogonalLayout 与 EdgeRouter 分离 | `EdgeRouterCosts`（sketch 违规代价等）；`routingPolicy` 基于"现有路由质量"决定是否重路由（`PATH_AS_NEEDED` 检测"严格违规或与其他元素相交"的边） | ★★★ `routingPolicy` 是"基于质量决策"的工业先例 |
| **MSAGL** | 顺序式：Sugiyama/MDS/IPSepCola → Spline/SplineBundling/Rectilinear | 无显式反馈；sleeve routing（CDT 对偶图 + funnel）质量依赖布局产生的自由空间 | ★★ sleeve routing 的"自由空间"思想可借鉴 |
| **OGDF** | 高度模块化：SugiyamaLayout = Ranking + CrossMin + LayoutModule；EdgeRouter 独立 | OrthogonalLayout 用 **TSM（Topology-Shape-Metrics）**，shape 阶段决定折弯——布局内部考虑路由形状 | ★★ TSM 的"shape 前移"是耦合的一种形式 |
| **Tom Sawyer** | 13.6 版引入 **"Automatic Layout Quality" 设置**（Draft/Medium/Proof） | "通过减少通常用于精修布局的后处理，更快生成初始布局"——暗示有质量评估驱动后处理 | ★★ 质量分级可作为评估输出的离散化形式 |
| **Dagre.js** | Sugiyama 布局内含交叉计数（bilayer cross counting）+ barycenter 排序 | 交叉最小化在布局的 ordering 阶段，不涉及最终样条/正交路由 | ★ 仅层间直线交叉，不等于最终路由交叉 |

**来源**：
- yFiles EdgeRouter: https://docs.yfiles.com/yfiles-html/dguide/polyline_router/
- yFiles routing policy: https://www.yworks.com/products/yfiles-for-java-2.x/changelog
- MSAGL sleeve routing: https://arxiv.org/pdf/2605.17498
- OGDF: https://ogdf.github.io/doc/ogdf/ex-layout.html

**关键结论**：主流库普遍采用"布局 → 路由"顺序流水线，**没有"路由友好性评估反馈布局"的显式机制**。yFiles 的 `routingPolicy` 是最接近的工业先例，但它是"路由后决策是否重路由"，而非"路由前预测并反馈布局"。

### 3.4 Graphviz dot 的布局-路由耦合

**论文**：Dobkin, Gansner, Koutsofios, North — "Implementing a General-Purpose Edge Router"  
**来源**：https://graphviz.org/documentation/DGKN97.pdf

dot 的样条路由是布局后的独立阶段，采用两阶段启发式：

1. 计算"白空间"约束多边形（边可绘制且不触碰其他节点 / 不产生不必要交叉的区域）。
2. 在约束多边形内拟合 Bezier 样条；若越界且无法微调，递归细分多边形重试。

**关键局限**（Graphviz 文档自述）：

> "Curves are routed individually, not globally, so the edge router does not prevent them from crossing. An interesting improvement would be to introduce some notion of global planning to prevent unwanted edge crossings."

**dot 的隐式耦合**：在 Sugiyama 布局中为长边引入**虚拟节点链**（dummy vertices），样条路由沿虚拟节点链的"等距矩形序列"拟合——**布局阶段为路由预留了通道**。但这只是结构化预留，非质量评估反馈。

**对提案的价值**：

- dot 的虚拟节点链思想可借鉴：布局时为长边预留通道。
- Graphviz 自述的局限（无全局交叉规划）**正是提案要填补的空白**。
- 约束多边形 / 通道思想可用于友好性评估：布局后为每条边计算可用通道，通道狭窄或缺失即不友好。

### 3.5 调研小结

| 维度 | 先例情况 | 对提案的支持 |
|------|---------|-------------|
| VLSI routability-driven placement | ★★★ 15+ 年成熟研究 | 直接对应，技术可迁移 |
| 图绘制质量度量 | ★★ 事后度量成熟，事前预测空白 | 度量定义可复用，需改造为预测 |
| 主流图可视化库 | ★ 无显式闭环 | yFiles routingPolicy 是最近先例 |
| Graphviz dot | ★★ 隐式耦合（虚拟节点链） | 局限性即提案价值主张 |
| ML 预测路由质量 | ★★★ EDA 领域成熟 | 长期可考虑，需数据积累 |

**综合判断**：提案在 VLSI 领域有大量成熟先例，在图可视化领域是明确的创新空白点，技术要素齐备、可行。最值得借鉴的是 **RUDY 密度图**、**SimPLR lookahead routing**、**Nachmanson 路由 cost function**。

---

## 4. 提案设计：拓扑路由友好性评估

### 4.1 评估器设计目标

| 目标 | 说明 | 量化指标 |
|------|------|---------|
| **快速** | 评估开销远小于完整路由 | < 20% 完整路由时间 |
| **路由器无关** | 不依赖特定路由算法 | 同一评估器对 orthogonal/spline/bezier 都有效 |
| **可定位** | 不仅给分数，还能定位热点 | 输出热点区域 / 边 / group |
| **可反馈** | 输出可被布局阶段消费的调整建议 | 节点位移方向 / group 间距 / 通道预留 |
| **可校准** | 与真实路由质量相关性可测 | 与事后度量的 Pearson 相关系数 > 0.7 |

### 4.2 友好度度量体系

借鉴 GD 2025 多维度量 + Nachmanson cost + RUDY 密度，设计**五维友好度**：

#### 4.2.1 通道拥堵度（Channel Congestion）— RUDY 式

**定义**：每条边贡献一个矩形均匀密度（从 from 节点到 to 节点的包围框），叠加成全图密度场。密度高的区域即潜在拥堵区。

**计算**：

```
对每条边 e = (u, v):
  rect_e = bounding_box(u, v)  // 含 margin 膨胀
  density_field += uniform_density(rect_e, weight=1.0)

congestion_score = max(density_field) / median(density_field)
hotspots = regions where density_field > threshold
```

**复杂度**：O(|E| + 网格分辨率)。网格分辨率典型 50×50，即 2500 单元。

**与路由器相关性**：高密度区 → orthogonal 通道绕行困难 / spline 可见性图路径长 / bezier 穿障概率高。

**Drawify 复用点**：[GroupLayout](../../crates/drawify-core/src/layout/mod.rs) 的 margin 可作为 group 通道宽度；[NodeLayout](../../crates/drawify-core/src/layout/mod.rs) 的 margin 可作为节点膨胀间距。

#### 4.2.2 长边跨层度（Long-edge Span）— Sugiyama 专属

**定义**：在分层布局中，跨越多层的边需要绕行或拆分，路由友好度低。

**计算**：

```
对每条边 e = (u, v):
  span_e = |rank(u) - rank(v)|
  if span_e > 1:
    long_edge_score += span_e - 1  // 惩罚跨层
```

**复杂度**：O(|E|)，需布局阶段导出 rank 信息。

**与路由器相关性**：跨层边在 orthogonal 路由中必然产生折弯，在 spline 路由中可见性图路径长。

**Drawify 复用点**：[sugiyama_v2](../../crates/drawify-core/src/layout/node/sugiyama_v2/) 已有 rank 信息，可扩展 `LayoutHints` 携带 rank map。

#### 4.2.3 group 间距充裕度（Group Gap Adequacy）

**定义**：group 之间的间距是否足以容纳跨 group 边的通道。

**计算**：

```
对每对相邻 group (g1, g2):
  gap = distance(g1.bounds, g2.bounds)
  cross_edges = count(edges where from in g1 and to in g2)
  required_width = cross_edges * EDGE_CHANNEL_WIDTH  // 经验值 16px
  if gap < required_width:
    gap_adequacy_score += (required_width - gap)
```

**复杂度**：O(|Groups|² + |E|)。

**与路由器相关性**：group 间距不足 → 跨 group 边被迫绕 group 外圈，产生长绕行。

**Drawify 复用点**：[architecture_v2/two_phase.rs](../../crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs) 已有 group 间宏观定位，可在此处评估间距充裕度。

#### 4.2.4 穿障预测度（Obstacle Crossing Prediction）

**定义**：基于直线连接预测穿障概率，作为 bezier / straight 路由的友好度估计。

**计算**：

```
对每条边 e = (u, v):
  segment = (center(u), center(v))
  for each non-endpoint node n:
    if segment_intersects_aabb(segment, n.bounds + margin):
      predicted_crossings += 1
```

**复杂度**：O(|E| × |V|)，可用空间索引（网格 / R-tree）降到 O(|E| × k)。

**与路由器相关性**：直线穿障数高 → bezier 必穿障（bezier 不避障）、spline 需绕行、orthogonal 需通道绕行。

**Drawify 复用点**：直接复用 [refine.rs L262-L314](../../crates/drawify-core/src/layout/refine.rs) 的 `segment_intersects_aabb`，以及 [visibility.rs](../../crates/drawify-core/src/layout/edge/visibility.rs) 的 `ObstacleIndex`。

#### 4.2.5 端口冲突度（Port Conflict）

**定义**：同侧多边汇入同一节点时，slot 分布是否合理。

**计算**：

```
对每个节点 n:
  for each side (top/bottom/left/right):
    incoming = count(edges docking at n.side)
    if incoming > 1:
      required_span = incoming * SLOT_PITCH  // 40px
      available_span = side_length(n, side)
      if required_span > available_span:
        port_conflict_score += required_span - available_span
```

**复杂度**：O(|V| + |E|)。

**与路由器相关性**：端口冲突 → orthogonal slot 拥挤、箭头分散。

**Drawify 复用点**：[edge_routing_orthogonal/slot.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs) 的 `slot_fraction`（L93）与 [edge_routing_orthogonal/mod.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs) 的 `SLOT_PITCH`（L50）。

#### 4.2.6 复合友好度分数

```
friendliness_score = w1 * congestion_score
                   + w2 * long_edge_score
                   + w3 * gap_adequacy_score
                   + w4 * predicted_crossings
                   + w5 * port_conflict_score
```

权重 `w1..w5` 需通过校准确定（见 §4.4）。**注意 GD 2025 警示**：不能依赖单一指标，必须五维组合。

### 4.3 评估器架构

```
┌─────────────────────────────────────────────────────────────┐
│                  compute_layout_with_plan                    │
├─────────────────────────────────────────────────────────────┤
│  strategy.compute(diagram)  ──→  LayoutResult                │
│                                          │                   │
│                                          ▼                   │
│              ┌──────────────────────────────────────────┐   │
│              │  RoutingFriendlinessEvaluator (新增)      │   │
│              │  ──────────────────────────────────────   │   │
│              │  1. ChannelCongestion (RUDY 密度图)       │   │
│              │  2. LongEdgeSpan (rank 跨层)              │   │
│              │  3. GroupGapAdequacy (group 间距)         │   │
│              │  4. ObstacleCrossingPrediction (穿障预测) │   │
│              │  5. PortConflict (端口冲突)               │   │
│              │  ──────────────────────────────────────   │   │
│              │  输出: FriendlinessReport                  │   │
│              │    .score: f64                            │   │
│              │    .hotspots: Vec<Hotspot>                │   │
│              │    .suggestions: Vec<Adjustment>          │   │
│              └──────────────────────────────────────────┘   │
│                                          │                   │
│                          ┌───────────────┴───────────────┐  │
│                          ▼                               ▼  │
│               (V1) 仅诊断/日志            (V2) 反馈布局调整  │
│               router.route(...)          apply_suggestions  │
│                                          → strategy.compute  │
│                                          (或局部调整)        │
└─────────────────────────────────────────────────────────────┘
```

### 4.4 与现有架构的集成点

#### 4.4.1 V1：诊断模式（最小侵入）

在 `compute_layout_with_plan` 中、`router.route` 之前插入评估，**只输出报告，不反馈**：

```rust
// mod.rs compute_layout_with_plan
let mut result = strategy.compute(diagram);
// ... grid_snap ...

// 新增：路由友好性评估（V1 诊断模式）
let report = RoutingFriendlinessEvaluator::default()
    .evaluate(diagram, &result);
// 仅记录日志 / 写入 LayoutHints.friendliness_report
result.hints.friendliness_report = Some(report);

let router = /* ... */;
let mut result = router.route(diagram, result);
```

**价值**：

- 零风险，不改变现有行为。
- 可在 [drawify-eval](../../crates/drawify-eval/) 中对比 `friendliness_score` 与事后 `LayoutMetrics`（交叉数、穿障数）的相关性，校准权重。
- 为 V2 反馈模式积累数据。

#### 4.4.2 V2：反馈模式（局部调整）

评估后若 `friendliness_score < threshold`，对热点区域做**局部调整**：

```rust
let report = evaluator.evaluate(diagram, &result);
if report.score < FRIENDLINESS_THRESHOLD {
    let adjuster = FriendlinessAdjuster::new(config);
    result = adjuster.apply(diagram, result, &report);
    // 重新评估，确认改善
    let report2 = evaluator.evaluate(diagram, &result);
    if report2.score >= report.score {
        result = /* 接受调整 */;
    } else {
        // 回退（借鉴 refine.rs 的回退机制）
    }
}
```

**调整策略**（借鉴 VLSI cell inflation + Drawify refine）：

| 热点类型 | 调整策略 | 借鉴来源 |
|---------|---------|---------|
| 通道拥堵 | 膨胀拥堵区节点 margin，推开邻居 | VLSI cell inflation |
| group 间距不足 | 增加问题 group 对的间距 | architecture_v2 group gap |
| 长边跨层 | 在中间层插入虚拟通道节点 | Graphviz dot 虚拟节点链 |
| 穿障预测 | 沿穿障法线推开中间节点（复用 refine.rs `push_problem_nodes`） | Drawify refine |
| 端口冲突 | 增加节点侧边长度或分散到多侧 | orthogonal slot |

#### 4.4.3 V3：布局目标集成（长期）

将友好度作为布局算法的**目标函数惩罚项**：

- Sugiyama 的交叉最小化阶段：在 barycenter 评分中加入"长边跨层惩罚"。
- force-directed：在力计算中加入"拥堵区排斥力"（RUDY 密度梯度）。
- architecture-v2：在 group 宏观定位中加入"跨 group 边通道需求"。

此阶段改动较大，需 V1/V2 积累足够校准数据后再推进。

### 4.5 数据结构设计

```rust
/// 路由友好性评估报告
#[derive(Debug, Clone, Serialize)]
pub struct FriendlinessReport {
    /// 复合友好度分数（越低越友好，0 = 完美）
    pub score: f64,
    /// 五维子分数
    pub congestion_score: f64,
    pub long_edge_score: f64,
    pub gap_adequacy_score: f64,
    pub predicted_crossings: usize,
    pub port_conflict_score: f64,
    /// 热点区域（分数高的局部区域）
    pub hotspots: Vec<Hotspot>,
    /// 调整建议（V2 反馈模式消费）
    pub suggestions: Vec<Adjustment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hotspot {
    /// 热点类型
    pub kind: HotspotKind,
    /// 热点区域包围框
    pub bbox: (f64, f64, f64, f64),
    /// 局部严重度（0..1）
    pub severity: f64,
    /// 相关边索引
    pub edge_indices: Vec<usize>,
    /// 相关节点 / group ID
    pub element_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub enum HotspotKind {
    ChannelCongestion,
    LongEdgeSpan,
    GroupGapInsufficient,
    PredictedCrossing,
    PortConflict,
}

#[derive(Debug, Clone, Serialize)]
pub enum Adjustment {
    /// 膨胀节点 margin（推开邻居）
    InflateNode { node_id: String, extra_margin: f64 },
    /// 增加 group 间距
    IncreaseGroupGap { group1: String, group2: String, extra_gap: f64 },
    /// 沿方向位移节点
    PushNode { node_id: String, dx: f64, dy: f64 },
    /// 在中间层为长边预留通道
    ReserveChannel { edge_idx: usize, via_layer: usize },
}
```

**集成到 `LayoutHints`**：

```rust
pub struct LayoutHints {
    pub circular: Option<node::circular::CircularLayoutHints>,
    pub sequence: Option<node::sequence::SequenceLayoutHints>,
    pub edge_routing_style: EdgeRoutingStyle,
    /// 新增：路由友好性评估报告（V1 诊断模式输出）
    pub friendliness_report: Option<FriendlinessReport>,
}
```

---

## 5. 可行性与有效性分析

### 5.1 可行性

| 维度 | 评估 | 依据 |
|------|------|------|
| **算法可行性** | ✅ 高 | 五维度量均有成熟算法，复杂度 O(\|E\|) ~ O(\|E\|×\|V\|)，远低于完整路由 |
| **架构兼容性** | ✅ 高 | V1 诊断模式零侵入；V2 反馈模式复用 refine.rs 的回退机制；不推翻两阶段架构 |
| **数据可得性** | ✅ 高 | 所需数据（nodes / groups / edges / ranks）布局阶段全部可用 |
| **性能可行性** | ✅ 高 | RUDY 密度图 O(\|E\|)；穿障预测可用 ObstacleIndex 加速；总体 < 20% 路由时间 |
| **校准可行性** | ⚠️ 中 | 需积累 (布局, 路由质量) 数据对，但 drawify-eval 框架已具备 |

### 5.2 有效性

#### 5.2.1 有利证据

1. **VLSI 领域强证据**：SimPLR 证明 lookahead routing 反馈布局可显著减少线长与拥堵；RUDY 证明密度图估计与真实路由质量强相关。
2. **度量相关性**：Drawify 现有的 `edge_node_crossings`（[metrics.rs L25](../../crates/drawify-eval/src/metrics.rs)）与提案的"穿障预测度"高度相关——直线穿障是 Polyline 穿障的上界。
3. **填补已知空白**：[edge-routing-optimization-plan.md](./edge-routing-optimization-plan.md) §2.2 已识别"bezier/circular 不避障"问题，提案的穿障预测可前移该问题的检测时机。

#### 5.2.2 风险与不确定性

| 风险 | 来源 | 缓解措施 |
|------|------|---------|
| **度量-路由器相关性崩塌** | VLSI sub-14nm 教训（Chan 2017） | V1 诊断模式先做相关性校准，Pearson > 0.7 才进入 V2 |
| **单度量被愚弄** | GD 2025 警示 | 五维组合，不依赖单一指标 |
| **反馈引入新问题** | refine.rs 已遇到（推开引入新穿障） | 复用 refine.rs 回退机制；引入 momentum（VLSI DAC 2025） |
| **开销超预期** | 评估器本身可能慢 | RUDY 密度图 O(\|E\|) 是安全下界；穿障预测可用网格加速 |
| **与 Layout Intent 冲突** | [layout-intent-optimized.md](./layout-intent-optimized.md) 的 Pin/Align | 友好性调整需尊重 Intent 约束，pinned 节点不参与位移 |

#### 5.2.3 有效性验证方案

**阶段 1（V1 诊断模式）**：

1. 在 [drawify-eval](../../crates/drawify-eval/) 中新增 `friendliness_score` 指标。
2. 对 benchmarks/ 下的所有样例（[round01_baseline.json](../../benchmarks/round01_baseline.json) 等）计算友好度与事后度量的相关性。
3. **验收标准**：`friendliness_score` 与 `edge_node_crossings` 的 Pearson 相关系数 > 0.6；与 `edge_crossings` 的 Pearson > 0.5。

**阶段 2（V2 反馈模式）**：

1. 对比 V2 开启 / 关闭时的事后度量。
2. **验收标准**：`edge_node_crossings` 平均下降 > 30%；`total_edge_length` 平均下降 > 10%；不引入新的 node_overlap_pairs。

---

## 6. 分阶段实施路线

### Phase 0：校准数据积累（前置）

**目标**：在 V1 之前，先用现有 [drawify-eval](../../crates/drawify-eval/) 框架积累 (布局, 路由质量) 数据对。

**任务**：

- 扩展 `LayoutMetrics`，新增 `channel_congestion`、`group_gap_min`、`long_edge_count` 等候选预测度量。
- 对 benchmarks/ 全量样例计算这些度量与事后 `edge_node_crossings` / `edge_crossings` 的相关性。
- 确定哪些度量与真实路由质量强相关（Pearson > 0.6），作为 V1 评估器的五维基础。

**产出**：相关性分析报告，确定五维度量的权重 `w1..w5`。

### Phase 1：V1 诊断模式（最小侵入）

**目标**：实现评估器，只输出报告，不反馈布局。

**任务**：

1. 新增 [layout/friendliness/](../../crates/drawify-core/src/layout/) 模块：
   - `mod.rs`：`RoutingFriendlinessEvaluator` + `FriendlinessReport`
   - `congestion.rs`：RUDY 密度图
   - `long_edge.rs`：长边跨层度（需 Sugiyama 导出 rank）
   - `group_gap.rs`：group 间距充裕度
   - `crossing_predict.rs`：穿障预测（复用 `segment_intersects_aabb`）
   - `port_conflict.rs`：端口冲突度
2. 扩展 `LayoutHints`，新增 `friendliness_report: Option<FriendlinessReport>`。
3. 在 `compute_layout_with_plan` 中、`router.route` 之前调用评估器。
4. 在 drawify-eval 中新增 `friendliness_score` 指标，对比与事后度量的相关性。

**验收**：Pearson > 0.6；评估开销 < 20% 路由时间；现有测试全通过。

### Phase 2：V2 反馈模式（局部调整）

**目标**：评估后对热点区域做局部调整，再进入正式路由。

**任务**：

1. 新增 `FriendlinessAdjuster`，实现五类调整策略（见 §4.4.2 表格）。
2. 复用 [refine.rs](../../crates/drawify-core/src/layout/refine.rs) 的回退机制：调整后重新评估，若分数未改善则回退。
3. 引入 momentum 机制（借鉴 VLSI DAC 2025）：记录节点历史位移，避免回流到不友好位置。
4. 与 [Layout Intent](./layout-intent-optimized.md) 集成：pinned 节点不参与位移。

**验收**：`edge_node_crossings` 平均下降 > 30%；不引入新的 node_overlap_pairs；不违反 Layout Intent 约束。

### Phase 3：V3 布局目标集成（长期）

**目标**：将友好度作为布局算法的目标函数惩罚项。

**任务**：

1. Sugiyama 交叉最小化：在 barycenter 评分中加入长边跨层惩罚。
2. force-directed：在力计算中加入拥堵区排斥力（RUDY 密度梯度）。
3. architecture-v2：在 group 宏观定位中加入跨 group 边通道需求。

**验收**：各布局算法的事后度量全面优于 V2。

### Phase 4：ML 代理（可选，长期）

**目标**：若启发式评估器精度不足，训练 GNN/CNN 代理。

**前提**：Phase 1-3 积累足够 (布局, 路由质量) 数据集（建议 > 10000 样本）。

**借鉴**：RoutePlacer (DAC 2024) 的 RouteGNN 架构、DLRoute 的 CNN 从布局图像预测。

---

## 7. 风险与权衡

### 7.1 主要风险

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| 度量与真实路由质量脱节 | 中 | 高 | V1 先校准，Pearson < 0.6 则不进入 V2 |
| 反馈调整引入新问题 | 中 | 中 | 回退机制 + momentum |
| 评估开销超预期 | 低 | 中 | RUDY O(\|E\|) 是安全下界 |
| 与 Layout Intent 冲突 | 中 | 中 | pinned 节点不参与位移 |
| 五维权重难调 | 中 | 低 | Phase 0 校准数据驱动权重 |

### 7.2 权衡

| 决策点 | 选项 A | 选项 B | 推荐 |
|--------|--------|--------|------|
| 评估时机 | 路由前（提案） | 路由后（refine.rs 现状） | 路由前（前移问题检测） |
| 评估粒度 | 全图统一 | 分区域 / 分 group | 分区域（定位热点） |
| 反馈方式 | 局部调整（V2） | 全局重布局（V3） | 先 V2 后 V3 |
| 度量组合 | 启发式五维 | ML 代理 | 先启发式，数据足够后再 ML |
| 调整范围 | 节点位移 | group 重排 | 节点位移优先（侵入小） |

### 7.3 与现有反馈机制的关系

提案**不替代** [refine.rs](../../crates/drawify-core/src/layout/refine.rs) 的事后反馈，而是**前移**问题检测：

```
布局 → 友好性评估（提案，事前）→ 路由 → refine（现有，事后）
                ↓                          ↓
          局部调整布局               局部推开节点
```

两者互补：

- 友好性评估：基于拓扑预测，能发现"潜在"问题（如通道拥堵），但可能误报。
- refine：基于真实路由结果，能发现"实际"问题（如 Polyline 穿障），但时机靠后。

---

## 8. 与现有研究文档的关系

| 文档 | 关系 | 借鉴点 |
|------|------|--------|
| [Graphviz 算法研究](./graphviz-algorithms-research.md) | 互补 | dot 虚拟节点链思想（长边通道预留） |
| [Cytoscape.js 研究](./cytoscape-js-research.md) | 互补 | fcose 约束模型（友好性调整作为软约束） |
| [边路由优化方案](./edge-routing-optimization-plan.md) | 互补 | 提案前移问题检测，优化方案解决具体路由问题 |
| [Layout Intent 优化](./layout-intent-optimized.md) | 协同 | 友好性调整需尊重 Intent 约束（pinned 节点不位移） |
| [布局 Refinement TODO](./layout-refinement-todo.md) | 协同 | 友好性评估可作为 Grid Snap Phase 3 的输入 |

**关键区别**：

- 现有文档都是"布局 → 路由"单向流水线内的优化，本提案是**流水线之间的反馈层**。
- 现有 refine 是"事后补救"，本提案是"事前预测"。
- 现有 Layout Intent 是"用户意图约束"，本提案是"路由友好性约束"——两者需协同（Intent 优先级高于友好性）。

---

## 9. 附录

### 9.1 关键代码位置索引

| 模块 | 文件 | 关键函数 / 结构 | 复用价值 |
|------|------|----------------|---------|
| 布局调度 | [mod.rs](../../crates/drawify-core/src/layout/mod.rs) | `compute_layout_with_plan` L695 | 评估器插入点 |
| 布局结果 | [mod.rs](../../crates/drawify-core/src/layout/mod.rs) | `LayoutResult` L541 / `LayoutHints` L531 | 扩展 friendliness_report |
| 节点布局 | [mod.rs](../../crates/drawify-core/src/layout/mod.rs) | `NodeLayout` L51 / `GroupLayout` L82 | 评估输入 |
| refine 反馈 | [refine.rs](../../crates/drawify-core/src/layout/refine.rs) | `run_refine` L179 / `analyze_edge_node_crossings` L67 | 回退机制 + 穿障检测复用 |
| 穿障检测 | [refine.rs](../../crates/drawify-core/src/layout/refine.rs) | `segment_intersects_aabb` L262 | 穿障预测复用 |
| 可见性图 | [visibility.rs](../../crates/drawify-core/src/layout/edge/visibility.rs) | `ObstacleIndex` L103 | 通道可用性评估 |
| orthogonal 评分 | [scoring.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/scoring.rs) | `obstacle_penalty` L47 / `segment_near_misses_node` L159 | 惩罚函数前移 |
| orthogonal slot | [slot.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/slot.rs) / [mod.rs](../../crates/drawify-core/src/layout/edge/edge_routing_orthogonal/mod.rs) | `slot_fraction` L93 / `SLOT_PITCH` L50 | 端口冲突度复用 |
| Sugiyama rank | [sugiyama_v2/rank.rs](../../crates/drawify-core/src/layout/node/sugiyama_v2/rank.rs) | rank 分配 | 长边跨层度输入 |
| 架构图 group | [architecture_v2/two_phase.rs](../../crates/drawify-core/src/layout/node/architecture_v2/two_phase.rs) | group 宏观定位 | group 间距评估 |
| 评估指标 | [drawify-eval/metrics.rs](../../crates/drawify-eval/src/metrics.rs) | `LayoutMetrics` L20 | 度量定义复用 + 校准 |

### 9.2 学术引用

| 编号 | 论文 / 产品 | 来源 |
|------|------------|------|
| [SimPLR] | Kim, Hu, Lee, Markov — "SimPLR: A Fast and Simple Placement Algorithm with Routability-aware Mode", ICCAD 2011 | https://web.eecs.umich.edu/~imarkov/pubs/conf/iccad11-simplr.pdf |
| [RUDY] | Spindler, Johannes — "Fast and Accurate Routing Demand Estimation", DATE 2007 | https://www.researchgate.net/publication/221339455 |
| [DCGP] | Li et al. — "DCGP: Differentiable Congestion-Driven Global Placement", DAC 2025 | https://xingquan-li.github.io/docs/paper/25-DAC25-DCGP.pdf |
| [Chan2017] | Chan, Ho, Kahng, Saxena — "Machine Learning for Pre-Routing DRC Prediction", ISPD 2017 | https://vlsicad.ucsd.edu/Publications/Conferences/348/c348.pdf |
| [RoutePlacer] | RoutePlacer — "End-to-End Routability-aware Placer", DAC 2024 | https://dl.acm.org/doi/pdf/10.1145/3637528.3671895 |
| [GD2025-metrics] | Mooney et al. — "Universal Quality Metrics for Graph Drawings", GD 2025 | https://drops.dagstuhl.de/storage/00lipics/lipics-vol357-gd2025/LIPIcs.GD.2025.30/LIPIcs.GD.2025.30.pdf |
| [GD2025-shape] | van Wageningen et al. — "Same Quality Metrics, Different Graph Drawings", GD 2025 | https://arxiv.org/html/2508.15557v1/ |
| [Nachmanson] | Bereg, Holroyd, Nachmanson, Pupyrev — "Edge Routing with Ordered Bundles" | https://www.arxiv-vanity.com/papers/1209.4227/ |
| [ShapeFirst] | "A Walk on the Wild Side: a Shape-First Methodology for Orthogonal Drawings", GD 2025 | https://arxiv.org/html/2508.19416v1/ |
| [dot-router] | Dobkin, Gansner, Koutsofios, North — "Implementing a General-Purpose Edge Router" | https://graphviz.org/documentation/DGKN97.pdf |
| [yFiles] | yFiles EdgeRouter 文档 | https://docs.yfiles.com/yfiles-html/dguide/polyline_router/ |
| [DLRoute] | Al-Hyari et al. — "DLRoute: Deep Learning-Based Routability Prediction", TRETS 2021 | https://dl.acm.org/doi/pdf/10.1145/3465373 |
| [Haleem2019] | Haleem et al. — "Evaluating the Readability of Force Directed Graph Layouts: A Deep Learning Approach" | https://cse.hkust.edu.hk/~huamin/cga_hammad_2019.pdf |

### 9.3 术语对照

| VLSI 术语 | Drawify 对应 | 说明 |
|-----------|-------------|------|
| placement | 节点布局 | 节点 / group 坐标分配 |
| global routing | 边路由 | orthogonal / spline / bezier 路径计算 |
| routability | 路由友好性 | 布局对路由的友好程度 |
| congestion | 通道拥堵 | 边路径密集区域 |
| cell inflation | 节点 margin 膨胀 | 推开邻居节点 |
| lookahead routing | 快速友好性评估 | 路由前的轻量预测 |
| DRC violation | 穿障 / 交叉 | 路由质量缺陷 |
| HPWL | 边长总和 | 简化线长度量 |
