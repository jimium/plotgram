# Cytoscape.js 能力研究：Drawify 可借鉴的技术与取舍分析

> 版本：0.1.0-draft | 状态：研究完成

本文档梳理 [Cytoscape.js](https://js.cytoscape.org/) 的核心能力与布局/图论算法，分析其与 Drawify 的定位差异，并给出可借鉴的技术点与明确不应引入的部分。

**相关文档**：[Graphviz 算法研究](./graphviz-algorithms-research.md) | [布局意图与增量约束](./layout-intent-refinement.md) | [竞争策略](../product/competitive-strategy.md)

---

## 1. Cytoscape.js 是什么

Cytoscape.js 是多伦多大学起源、发表于 *Bioinformatics*（2016, 2023）的**图论（网络）可视化与分析库**（MIT 许可）。它在生物信息学领域广泛使用，也被 Amazon、Google、GitHub 等用于交互式网络图应用。

它与 Mermaid、Graphviz、Drawify 处于不同层次：

| 维度 | Cytoscape.js | Drawify |
|------|-------------|---------|
| 定位 | 通用图可视化 + 交互 + 图分析引擎 | AI 原生图表 DSL + Rust 渲染引擎 |
| 输入 | JSON 元素 + 样式表 | `.dfy` 语义 AST |
| 输出 | 浏览器 Canvas 实时渲染 | SVG / PNG / WebP / ASCII 静态导出 |
| 主要用户 | 开发者嵌入交互式网络图 | LLM 生成、人类阅读 |
| 运行时 | 纯 JavaScript（浏览器 / Node.js headless） | Rust 核心（CLI / HTTP / WASM） |

**核心结论**：Cytoscape.js 是「交互式图分析平台」，Drawify 是「AI 语义图表编译器」。不应作为运行时依赖引入，但可作为**布局算法与约束求解的参考实现**。

Drawify 当前代码库中**未引入** Cytoscape.js，这一决策是合理的。

---

## 2. 核心能力架构

```
┌─────────────────────────────────────────────────────────┐
│                    Cytoscape.js 核心                      │
├─────────────┬─────────────┬─────────────┬───────────────┤
│  图模型      │  布局引擎    │  图论算法    │  渲染与交互    │
│  有向/无向   │  内置 + 扩展 │  BFS ~      │  Canvas       │
│  多重边      │             │  PageRank   │  样式表        │
│  复合节点    │             │             │  pan/zoom/动画 │
└─────────────┴─────────────┴─────────────┴───────────────┘
```

### 2.1 图模型

- 支持有向图、无向图、混合图、多重边、自环
- **Compound nodes**（复合节点）：父节点包含子节点，父节点尺寸由子节点位置自动推导（类似 HTML DOM 嵌套）
- 数据与样式分离：`elements`（节点/边 JSON）+ `style`（selector 样式表）
- 完全可 JSON 序列化/反序列化

### 2.2 渲染与交互

- Canvas 渲染，支持复杂样式（箭头、标签、复合节点边框等）
- 内置 pan/zoom、触摸手势、元素动画
- 可通过 [cytosnap](https://github.com/cytoscape/cytosnap) 在 Node.js 端用 Puppeteer 截图

以上能力面向**交互式图探索器**，与 Drawify 的静态导出管线不匹配。

### 2.3 布局系统

布局是 Cytoscape.js 最有参考价值的部分。布局作为扩展注册，核心通过 `cy.layout({ name: '...' })` 调度。

**设计要点**：

- 布局作用于子图（`eles.layout()` 可对子集运行不同布局）
- `preset` 布局保留手动坐标，适合「算法初排 + 人工/约束微调」
- `transform` 回调可变换节点坐标（如切换流向 LR/TB）
- `nodeDimensionsIncludeLabels` 选项控制标签是否计入碰撞检测

---

## 3. 布局算法全景

### 3.1 内置布局

| 布局 | 算法类型 | 适用场景 |
|------|----------|----------|
| `breadthfirst` | BFS 分层遍历 | 树、森林；DAG 的 circle/grid 模式 |
| `circle` | 圆周等距排列 | 小规模环形图 |
| `concentric` | 按度量分圈的同心圆 | 按度/中心性分层的节点 |
| `cose` | Compound Spring Embedder 力导向 | 通用图，含复合节点支持 |
| `grid` | 网格排列 | 简单规整布局 |
| `preset` | 保留指定坐标 | 约束微调、布局 seed 复现 |
| `random` / `null` | 随机 / 占位 | 调试 |

### 3.2 扩展布局（生态精华）

| 扩展 | 本质 | 论文/来源 |
|------|------|-----------|
| **dagre** | Sugiyama 分层（含 Network Simplex） | Gansner et al. 1993 |
| **elk** | Eclipse Layout Kernel 适配器 | 多种欧洲布局算法 |
| **klay** | 分层 + compound 图布局 | 德国大学布局套件 |
| **fcose** | 快速 CoSE-Bilkent 力导向 | Dogrusoz et al. compound graph |
| **cose-bilkent** | 高质量力导向 + compound | 同上，更慢但更优 |
| **cola** | 约束力导向（边长、对齐约束） | Dwyer et al. |
| **spread** | CoSE 初排 + Gansner-North 扩散 | 两阶段布局 |
| **avsdf** | 圆形排列 + 最小化交叉 | 状态图变体 |
| **tidytree** | 非分层树布局 | 思维导图类 |

**fcose 的关键特性**（对 Drawify 最有启发）：

> 支持三类用户约束：固定位置（fixed position）、对齐（alignment）、相对放置（relative placement）。

这与 Drawify 的 [Layout Intent Refinement](./layout-intent-refinement.md) 方向高度吻合：固定位置对应 `Pin`，对齐对应 `Align*`，相对放置对应 `below` / `right_of` 等拓扑意图（经 overlay 表达，非 DSL 坐标 hint）。

---

## 4. 图论算法（分析层）

Cytoscape.js 内置完整图算法 API，可用于布局预处理或节点排序，而非产品终端功能。

### 4.1 路径搜索

| 算法 | API | 用途 |
|------|-----|------|
| BFS | `eles.breadthFirstSearch()` | 分层布局、可达性分析 |
| DFS | `eles.depthFirstSearch()` | 拓扑遍历、环检测 |
| Dijkstra | `eles.dijkstra()` | 最短路径 |
| A* | `eles.aStar()` | 启发式最短路径 |
| Floyd-Warshall | `eles.floydWarshall()` | 全对最短路径 |
| Bellman-Ford | `eles.bellmanFord()` | 负权边、负环检测 |

### 4.2 结构分析

| 算法 | API | Drawify 潜在用途 |
|------|-----|-----------------|
| 强连通分量 | `eles.tarjanStronglyConnected()` | 有向图去环前的 SCC 分解 |
| 双连通分量 | `eles.hopcroftTarjanBiconnected()` | `circular-v2` 的组件拆分 |
| 连通分量 | `eles.components()` | 多组件图分别布局 |
| 最小割 | `eles.kargerStein()` | 图分割（低优先级） |
| 最小生成树 | `eles.kruskal()` | 树形子结构提取（低优先级） |

### 4.3 中心性与聚类

| 算法 | API | 潜在用途 |
|------|-----|----------|
| 度/接近/介数中心性 | `eles.*Centrality()` | `concentric` 布局的节点分层排序 |
| PageRank | `eles.pageRank()` | 节点重要性排序 |
| Markov 聚类 | `eles.markovClustering()` | 生物网络聚类，与 Drawify 场景无关 |

---

## 5. 与 Drawify 现有布局栈的对照

Drawify 在 `drawify-core` 中已实现可插拔布局框架（`LayoutStrategy` trait），当前支持的算法：

| Drawify 布局 | 图表类型默认 | Cytoscape 对标 |
|-------------|-------------|----------------|
| `sugiyama` / `sugiyama-v2` | flowchart | dagre |
| `force-directed` | architecture, flowchart | fcose / cola / cose |
| `circular` / `circular-v2` | state | circle / avsdf |
| `radial` | — | concentric / twopi |
| `mindmap` | mindmap | tidytree |
| `sequence` / `timeline` | sequence | （无直接对标，Drawify 专属） |

边路由方面，Drawify 有独立的 `EdgeRoutingStrategy`（orthogonal、spline、bezier 等），Cytoscape.js 的边路由内嵌在 Canvas 渲染中，参考价值较低。

---

## 6. 可借鉴技术点（按优先级）

结合 [竞争策略](../product/competitive-strategy.md)（布局做到 80 分、核心壁垒在语义微调）和 [Graphviz 算法研究](./graphviz-algorithms-research.md) 的差距分析：

### 6.1 高优先级 — 算法参考，建议深入研究

| Cytoscape 能力 | Drawify 对应点 | 行动建议 |
|----------------|----------------|----------|
| **dagre**（Sugiyama + Network Simplex） | `sugiyama` / `sugiyama-v2` | 参考 `dagrejs/dagre` 的 `network-simplex.js`；补齐 rank 优化与交叉最小化 |
| **fcose / cola 约束** | `LayoutIntentOverlay`（Pin / Align / 拓扑意图） | 借鉴固定点 + 对齐 + 相对位置的约束求解，支撑「把 X 放右边」类语义微调 |
| **klay / elk compound** | `groups` 子图嵌套 | 研究组内布局 + 组间布局的两阶段策略，改善架构图分组场景 |
| **Tarjan SCC / 双连通分量** | 去环、`circular-v2` | 有向图分层前去环、无向图圆形布局的组件拆分 |
| **spread**（CoSE + Gansner-North） | 力导向后处理 | 初排后二次扩散，填满画布、减少节点重叠 |

### 6.2 中优先级 — 概念借鉴

| 能力 | 用途 |
|------|------|
| **compound nodes 模型** | 与 Drawify `group` 语义对齐：父框由子节点推导，不独立设尺寸 |
| **preset + transform** | 布局 seed 可复现 + 方向变换（LR/TB） |
| **concentric + centrality** | 状态图节点按重要性排圈 |
| **nodeDimensionsIncludeLabels** | 标签计入碰撞检测，对照 `label_avoidance` 模块 |

### 6.3 低优先级 — 明确不引入

| 能力 | 原因 |
|------|------|
| 整体作为运行时依赖 | Drawify 是 Rust 核心 + WASM，引入 JS 图库增加双栈维护成本 |
| Canvas 渲染 / 交互 / 动画 | 产品是静态导出，不是交互式图探索器 |
| PageRank、MCL 聚类等分析算法 | 生物网络分析场景，与 AI 图表 DSL 无关 |
| cytosnap 服务端截图 | 已有 Rust 原生 SVG 导出，更可控、无头浏览器依赖 |
| React/Vue/Angular 封装 | 编辑器侧已有自研 WASM 管线 |

---

## 7. 与 Graphviz 研究的关系

Cytoscape.js 与 Graphviz 在 Drawify 中的角色类似：**算法参考书，而非运行时依赖**。

| 来源 | 强项 | Drawify 主要借鉴方向 |
|------|------|---------------------|
| **Graphviz** | dot 引擎 Network Simplex、样条边路由、30 年工程积累 | Sugiyama 流水线、边路由质量 |
| **Cytoscape.js** | 约束布局（fcose/cola）、compound 图、JS 生态集成 | Layout Intent Refinement、分组布局、约束求解 API 设计 |
| **dagre**（Cytoscape 扩展） | Network Simplex 的 JS 经典实现 | `sugiyama-v2` 的 rank 优化参考 |

三者形成互补：Graphviz 提供分层布局的学术最优解，Cytoscape 生态提供约束微调与 compound 图的工程实践，Drawify 在 Rust 中自行实现并服务于 AI 语义管线。

---

## 8. 推荐行动路线

```
Phase 1（已在进行）
  └─ sugiyama-v2：参考 dagre network-simplex.js + Graphviz dot

Phase 2（布局意图）
  └─ 研究 fcose/cola 约束模型 → 设计 Drawify layout intent API
  └─ preset 式坐标保留 + 约束求解器

Phase 3（compound 布局）
  └─ 参考 klay/elk 的组内/组间两阶段布局
  └─ 对齐 compound nodes 的父框推导模型

Phase 4（图预处理）
  └─ Tarjan SCC 用于有向图去环
  └─ 双连通分量用于 circular-v2 组件拆分
```

---

## 9. 参考资料

- [Cytoscape.js 官方文档](https://js.cytoscape.org/)
- [Cytoscape.js 论文 (2016)](https://doi.org/10.1093/bioinformatics/btv557) — Franz et al., *Bioinformatics*
- [dagre](https://github.com/dagrejs/dagre) — Sugiyama + Network Simplex JS 实现
- [fcose](https://github.com/iVis-at-Bilkent/cytoscape.js-fcose) — 约束力导向布局
- [ELK](https://eclipse.dev/elk/) — Eclipse Layout Kernel
- Drawify 内部：[Graphviz 算法研究](./graphviz-algorithms-research.md)
