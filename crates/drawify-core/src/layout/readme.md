# Layout Algorithms

`crates/drawify-core/src/layout` 负责 Drawify 的节点布局与边路由。架构分为两层：

- **`LayoutStrategy`**：负责节点与分组的几何布局。
- **`EdgeRoutingStrategy`**：负责在节点布局完成后计算边路径。

若新方案对旧实现的改动超过 30% 或核心逻辑发生颠覆性变化，应以独立文件新增，避免破坏历史版本，便于 A/B 测试。

---

## 目录

1. [节点布局算法](#1-节点布局算法)
   - [Sugiyama（分层布局）](#11-sugiyama分层布局)
   - [Sugiyama-v2（增强分层布局）](#12-sugiyama-v2增强分层布局)
   - [Mindmap（思维导图布局）](#13-mindmap思维导图布局)
   - [Circular（圆形布局）](#14-circular圆形布局)
   - [Sequence（时序图布局）](#15-sequence时序图布局)
   - [Force-Directed（力导向布局）](#16-force-directed力导向布局)
   - [Architecture（架构图布局）](#17-architecture架构图布局)
2. [边路由算法](#2-边路由算法)
   - [Straight（直线路由）](#21-straight直线路由)
   - [Orthogonal（正交路由）](#22-orthogonal正交路由)
   - [Bezier（贝塞尔路由）](#23-bezier贝塞尔路由)
   - [Circular（弧形路由）](#24-circular弧形路由)
3. [图形类型与算法选择](#3-图形类型与算法选择)
4. [算法使用示例](#4-算法使用示例)
5. [算法效果对比](#5-算法效果对比)
6. [算法配置块扩展指南](#6-算法配置块扩展指南)
7. [研发演进建议](#7-研发演进建议)
8. [依赖与合规](#8-依赖与合规)

---

## 1. 节点布局算法

### 1.1 Sugiyama（分层布局）

**文件**：`node/sugiyama.rs`

#### 算法原理

Sugiyama 分层布局是有向图分层绘制的工业事实标准，源自 Sugiyama、Tagawa & Toda（1981）的分层图绘制框架，后经 Gansner et al.（1993）在 Graphviz `dot` 中完善。核心思想是将图中的节点按拓扑关系分配到水平层级，使所有边方向一致（自上而下或自左而右），从而清晰表达数据/控制流方向。

#### 实现方式

算法分为四个阶段流水线执行：

1. **去环（Cycle Removal）**：通过 DFS 遍历检测回边（back edge），将回边反转方向使图变为 DAG。DFS 从每个未访问节点出发，维护递归栈 `on_stack`；若发现出边指向栈中节点则标记为回边并反转。

2. **层分配（Layer Assignment）**：使用最长路径法（Longest Path）。对 DAG 做拓扑排序，每个节点的层级 = max(所有前驱层级) + 1。源节点层级为 0。孤立节点放置在最高层之下。时间复杂度 O(V + E)。

3. **交叉最小化 + 弯曲优化（Crossing Minimization & Bending Optimization）**：在相邻层之间反复上下扫描（默认 6 轮迭代），按综合评分排序节点。评分函数为 `CROSSING_WEIGHT * barycenter + BENDING_WEIGHT * bending_score`，其中 barycenter 是邻居位置的重心（减少交叉），bending_score 是邻居位置的标准差（减少弯曲）。权重系数：交叉权重 1.0，弯曲权重 0.5，边长权重 0.3。

4. **坐标分配（Coordinate Assignment）**：先按层内等间距分配初始坐标（层内居中），然后进行 3 轮迭代优化，根据邻居位置调整节点 x/y 坐标以减少边长和弯曲。ER 图使用真实节点尺寸中心坐标，跳过固定格网迭代。

支持 `top-to-bottom`（默认）和 `left-to-right` 两种方向。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(V + E + I × L × C) | I = 交叉最小化迭代轮次（6），L = 层数，C = 最大层内节点数 |
| 空间复杂度 | O(V + E) | petgraph DiGraph + 层映射 + 位置映射 |

#### 适用场景与局限性

- **适用**：流程图、审批流、状态图、ER 依赖图、一般 DAG
- **局限**：层分配使用最长路径法，未做紧边压缩，复杂 DAG 层间跨度可能偏大；交叉最小化使用重心法而非 Sifting/Stress，大图交叉数非最优；坐标分配未采用 Brandes-Kopf，横向分布均匀性有提升空间
- **确定性**：是
- **建议规模**：20–300 节点

---

### 1.2 Sugiyama-v2（增强分层布局）

**文件**：`node/sugiyama_v2/`

#### 算法原理

Sugiyama-v2 沿着 Graphviz `dot` 的研究路线，向 Network Simplex 与 Brandes-Kopf 逼近。相比旧版 Sugiyama，在去环、分层、排序、坐标分配四个阶段均有升级，目标是获得更紧凑的层级和更均匀的横向分布。

#### 实现方式

1. **Greedy 去环**：替代简单 DFS 去环。按出度与入度之差排序节点，贪心地选择将节点放入 S₁（源集）或 S₂（汇集），使得反转边数最少。相比 DFS 去环，通常反转更少的边，保留更多原始边的方向语义。

2. **NS-style 层分配（Network Simplex Style）**：先用最长路径法获取初始层级，然后对每个弱连通分量独立执行紧边压缩迭代：遍历所有边，若 `rank[to] - rank[from] > 1`（存在松弛），则尝试将 to 节点层级上移至 `rank[from] + 1`，同时不违反其他边的约束。最多迭代 32 轮。最终对齐使最小层级为 0。

3. **加权中位数排序 + 转置（Weighted Median + Transpose）**：构建 Proper Layer Graph（长边拆分为 dummy 节点链），然后执行 8 轮上下扫描。每轮对每层按邻居的加权中位数排序，排序后执行相邻节点转置（交换相邻节点对），若交叉数减少则保留交换。相比旧版重心法，加权中位数对 dummy 节点链的对齐更友好。

4. **BK-style 坐标分配（Brandes-Kopf Style）**：四趟对齐与压缩。对每层节点计算中心坐标，通过上下对齐使 dummy 节点链垂直对齐，然后压缩层间距以减少总宽度。最终按真实节点尺寸转换为 `NodeLayout`。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(V + E + S × L × C) | S = 排序扫描轮次（8），L = 层数，C = 最大层内节点数；NS 压缩额外 O(32 × E) |
| 空间复杂度 | O(V + E + D) | D = dummy 节点数（长边拆分产生） |

#### 与旧版 Sugiyama 的关键差异

| 阶段 | Sugiyama | Sugiyama-v2 |
|------|----------|-------------|
| 去环 | DFS 反转回边 | Greedy 最少反转 |
| 层分配 | 最长路径法 | 最长路径 + NS-style 紧边压缩 |
| 排序 | 重心法 + 弯曲评分 | 加权中位数 + 转置 |
| 坐标分配 | 等间距 + 迭代优化 | BK-style 四趟对齐与压缩 |
| 长边处理 | 无 | 拆分为 dummy 节点链 |

#### 适用场景与局限性

- **适用**：流程编排、复杂依赖 DAG、需要更紧凑层级与更均匀横向分布的图
- **局限**：当前是工程实现版，不等同于 Graphviz `dot` 的完整 Network Simplex（feasible tree / cut value pivot）和 Brandes-Kopf（完整冲突检测与垂直对齐）
- **确定性**：是
- **建议规模**：20–320 节点

---

### 1.3 Mindmap（思维导图布局）

**文件**：`node/mindmap.rs`

#### 算法原理

思维导图布局是面向树形结构的专用布局算法，核心思想是以根节点为中心（或起点），子节点按层级向外展开。与通用图布局不同，它假设图的拓扑为树或近似树结构，利用这一约束实现更紧凑、更美观的布局。

#### 实现方式

支持三种展开方向：

- **Radial（默认）**：根节点居中，一级子节点左右交替辐射展开。偶数序号分支向右，奇数序号分支向左。每个子树递归布局，y 方向按子树高度累进，x 方向按深度等间距。
- **Top-to-Bottom**：根节点在上方，子树向下展开。每层按深度等间距，层内按子树宽度累进。
- **Left-to-Right**：根节点在左侧，子树向右展开。逻辑与 Top-to-Bottom 对称。

节点尺寸策略按类型差异化：`root` 节点 128–148×128px，`main` 节点 132–200×54px，`leaf` 节点 108–176×46px，`branch` 节点 120–188×50px。宽度根据标签字符数自适应。

根节点识别优先级：1) 标记为 `type: root` 的实体；2) 入度为 0 的节点；3) 第一个实体。

孤立节点（不在根的可达子树中）放置在已布局区域下方。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(V + E) | 一次 DFS 遍历 + 递归布局 |
| 空间复杂度 | O(V) | 子树映射 + 位置映射 |

#### 适用场景与局限性

- **适用**：思维导图、知识树、议题拆解、会议记录、学习路径图
- **局限**：假设树形拓扑，对多父节点或复杂回边图不适合；不支持分组包围框；孤立节点仅线性排列
- **确定性**：是
- **建议规模**：10–200 节点

---

### 1.4 Circular（圆形布局）

**文件**：`node/circular/`

#### 算法原理

圆形布局将节点排列在圆周上，使环结构和闭环关系直观可见。当前实现支持两种模式：单环模式（所有节点均匀排列在一个圆上）和多环模式（基于双连通分量分解，每个分量独立成圆，割点靠内侧）。

#### 实现方式

1. **双连通分量分解**：使用 DFS 计算割点（articulation points）和双连通分量（biconnected components, BCC）。若图有多个 BCC 或存在割点，则启用多环模式。

2. **单环模式**：所有节点按 BFS 邻接顺序排列在圆周上。圆半径根据节点尺寸和数量自适应计算（保证相邻节点不重叠）。节点从 -π/2（12 点钟方向）开始均匀分布。

3. **多环模式**：每个 BCC 独立成圆，BFS 重排序使相邻节点在圆上相邻。割点放置在比普通节点更靠近圆心的位置（内缩 `CUTPOINT_OFFSET`）。多个圆水平排列，间距 `COMPONENT_GAP`。

4. **圆半径计算**：基于节点尺寸和数量，确保相邻节点的边界框不重叠。多环模式下每个 BCC 独立计算半径，上限 `MAX_CIRCLE_RADIUS`。

边几何由专用 `edge_routing: circular` 计算（弧形贝塞尔）。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(V + E + R) | R = BCC 分解 + 圆半径计算 + 边路由修正 |
| 空间复杂度 | O(V + E) | 图结构 + BCC 映射 + 位置映射 |

#### 适用场景与局限性

- **适用**：状态图、闭环审批、轮转关系图
- **局限**：多环模式不是完整的 Graphviz `circo` 式双连通分量布局（割点共享语义未完全实现）；节点数超过 120 时圆周拥挤，边交叉严重
- **确定性**：是
- **建议规模**：10–120 节点

---

### 1.5 Sequence（时序图布局）

**文件**：`node/sequence.rs`

#### 算法原理

时序图是强语义图，不适合复用通用布局算法。核心思想是：参与者水平展开，消息按声明顺序在垂直方向依次排列（每条消息占据一个时间步），形成经典的时序交互图。

#### 实现方式

1. **参与者布局**：按声明顺序水平铺开，等间距排列（间距 `NODE_SPACING = 80px`）。所有参与者 y 坐标相同（顶部对齐）。

2. **消息布局**：按声明顺序分配 y 坐标，每条消息间距 `MESSAGE_SPACING = 50px`。消息从发送者生命线（中心 x）到接收者生命线画水平直线。自调用消息绘制为 U 形（从生命线右探出 `SELF_MESSAGE_WIDTH = 40px` 后返回）。

3. **标签避让**：估算标签宽度（ASCII 6.5px/字符，CJK 11px/字符），检测标签与参与者框的 AABB 重叠，将压在节点上的标签推到节点下方。

4. **边几何内置**：布局阶段直接生成 `EdgeLayout`，设置 `produces_edge_geometry() = true`，避免被通用边路由覆盖。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(V + E × V) | V = 参与者数，E = 消息数；标签避让为 O(E × V) |
| 空间复杂度 | O(V + E) | 节点映射 + 边列表 |

#### 适用场景与局限性

- **适用**：接口交互、服务调用链、业务流程时序分析
- **局限**：只适用于 sequence 图，不适合一般节点网络；不支持分组；消息不支持异步/并行语义
- **确定性**：是（极高）
- **建议规模**：10–200 消息

---

### 1.6 Force-Directed（力导向布局）

**文件**：`node/force_directed.rs`

#### 算法原理

基于 Fruchterman-Reingold（1991）力导向模型，并增强分组语义。核心思想是模拟物理系统：节点间存在库仑斥力（防止重叠），边连接的节点间存在弹簧引力（保持连接紧凑），通过模拟退火迭代至平衡状态。分组增强使同组节点互相吸引形成簇，组间互相排斥保持分离。

#### 实现方式

1. **节点尺寸**：根据标签宽度自适应（`unicode-width` 估算），宽度范围 96–240px，高度默认 50px。

2. **位置初始化**：按连通分量分组，每个分量内的节点沿圆周均匀分布。分量间水平排列，间距 `COMPONENT_GAP = 120px`。

3. **FR 迭代（200 轮）**：
   - **全局斥力**：所有节点对计算库仑斥力 `F_rep = k² / d`，其中 `k = √(area / V)`，`d` 为节点间距离。组间斥力增强（系数 `1 + GROUP_REPULSION = 1.3`）。
   - **边引力**：边连接的节点对计算弹簧引力 `F_att = d² / k × EDGE_ATTRACTION_MULT`，引力倍增系数 1.3。
   - **分组引力**：同组节点朝组质心移动，引力系数 `GROUP_GRAVITY = 0.06`。质心按节点面积加权计算。
   - **分组凸包约束**：若节点离组质心超过阈值（`√(members) × 80 + 16`px），施加弹性拉力。
   - **全局中心引力**：弱引力（`CENTER_GRAVITY = 0.008`），防止整体漂移。
   - **模拟退火**：初始温度 40px，冷却系数 0.95，每轮位移限制在温度范围内。

4. **重叠消除**：迭代结束后检测所有节点对的 AABB 重叠，沿重叠最小轴推开。

5. **分量打包**：多连通分量按拓扑排序水平排列，组内节点紧凑排布。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(I × (V² + E)) | I = 迭代轮次（200），斥力为 O(V²)，引力为 O(E) |
| 空间复杂度 | O(V + E) | 图索引 + 位置映射 + 位移映射 |

#### 适用场景与局限性

- **适用**：架构图、流程图、微服务拓扑、关系复杂且需自然散开的图
- **局限**：非完全确定性（初始位置和迭代顺序可能影响结果）；大图 O(V²) 斥力成本较高；节点超过 180 时收敛慢且布局质量下降
- **确定性**：否（但同一输入多次运行结果通常接近）
- **建议规模**：10–180 节点

---

### 1.7 Architecture-v2（架构图布局）

**文件**：`node/architecture_v2.rs`

#### 算法原理

专为架构图设计的混合布局算法，结合 Sugiyama 分层布局的层级优化能力和分组感知的紧凑排列能力。核心思路是两层 Sugiyama：宏观层将分组视为"超级节点"做拓扑分层，微观层对组内节点做 Sugiyama 分层。

#### 实现方式

1. **宏观层**：将每个分组视为超级节点，构建分组间的拓扑图，做 Sugiyama 分层确定组的层级位置。
2. **微观层**：组内节点做 Sugiyama 分层，同时考虑跨组边的端口位置。
3. **坐标分配**：Brandes-Kopf 风格，确保组边界紧凑。
4. **后处理**：重叠消除 + 宽高比优化。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(V + E + G × S) | G = 分组数，S = 组内 Sugiyama 成本 |
| 空间复杂度 | O(V + E + G) | 分组映射 + 组内布局 |

#### 适用场景与局限性

- **适用**：架构图、微服务拓扑图、有分组语义的有向图
- **局限**：仅适用于 `DiagramType::Architecture`；分组间拓扑复杂时宏观层分层可能不够紧凑
- **确定性**：是
- **建议规模**：20–250 节点

---

## 2. 边路由算法

### 2.1 Straight（直线路由）

**文件**：`edge/edge_routing.rs`

#### 算法原理

最简单的边路由策略：每条边画一条从源节点到目标节点的直线。通过规范方向计算垂直单位向量，为同一对节点间的平行边（双向边/多边）做垂直偏移，避免重叠。

#### 实现方式

1. **平行边分组**：按无向节点对分组，计算每条边的偏移量（交替正负，间距 `DEFAULT_EDGE_OFFSET`）。
2. **边界交点**：从节点中心向目标中心发射射线，计算与节点边界的交点作为起止点。
3. **偏移应用**：先计算无偏移的边界交点，再沿法线方向平移，避免射线截断导致偏移量被压缩。
4. **端口选择**：根据交点相对节点中心的位置，确定性选择连接侧（上/下/左/右）。
5. **标签定位**：路径中点，沿法线额外偏移避免贴在箭头上。
6. **标签避让**：检测标签-标签和标签-节点的 AABB 重叠，迭代推开。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(E × (V + E)) | 平行边分组 O(E)，标签避让 O(E² + E × V) |
| 空间复杂度 | O(E) | 偏移映射 + 边列表 |

#### 适用场景

- 简单关系图、ER 图
- 仅适用于 `DiagramType::Er`

---

### 2.2 Orthogonal（正交路由）

**文件**：`edge/edge_routing_orthogonal/`

#### 算法原理

正交路由生成仅包含水平和垂直线段的折线路径，是流程图和架构图的标准边样式。核心设计是固定磁吸点（slot）方案：每个矩形节点的每条边上有固定数量的候选连接点，实际锚点按边的序号均匀分布，保证不重叠且对称。

#### 实现方式

1. **端口选择**：根据两节点的几何关系确定性选出连接侧（上/下/左/右），而非对 16 种端口组合打分。对齐且尺寸相同的节点对自然生成平行直线。

2. **磁吸点分配**：每个 (节点, 边) 上的连接点按 `(rank+1)/(count+1)` 分布在节点边上，保留 12% 边界余量。相邻磁吸点间距默认 `slot_pitch = 40px`。

3. **路径构建**：按连接度排序边（高度数节点的边优先路由），逐边构建路径。对每条边生成多条候选路径（直线、Z 形、U 形等），选择惩罚分最低的路径。惩罚项包括：
   - 路径长度
   - 折点数（每折 `BEND_PENALTY = 16`）
   - 穿过节点（`NODE_CROSSING_PENALTY = 10000`）
   - 与已路由边段重叠（`EDGE_OVERLAP_PENALTY = 1200`）

4. **路径简化**：移除共线中间点，保留短桩（stub）避免一出线就折回。

5. **标签放置**：最长线段中点，沿线段法线偏移。

6. **分组 Border Shell**（`layout/group/`）：路径段 vs 分组边框关系分类（Interior / Crossing / Transit / 贴边平行），统一由 [`GroupRoutingContext`](group/context.rs) 供评分、走廊与 snap 后投影消费。流程图分治布局在 `LayoutHints.group_routing` 注入堆叠走廊；详见 [group-border-shell-refactoring-plan.md](../../../../docs/architecture/布局优化/group-border-shell-refactoring-plan.md)。

6. **标签避让**：与直线路由相同的 AABB 避让逻辑。

可调参数：`slot_pitch`（磁吸点间距）、`channel_margin`（侧通道留白）。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(E × (V + P)) | P = 已路由段数，路径打分需检查障碍物和重叠 |
| 空间复杂度 | O(E + P) | 已路由段列表 + 边列表 |

#### 适用场景

- 流程图、架构图（与 Sugiyama 布局组合为默认推荐）
- 适用于 `Flowchart`、`Architecture`、`State`、`Er`

---

### 2.3 Bezier（贝塞尔路由）

**文件**：`edge/edge_routing_bezier.rs`

#### 算法原理

为每条边计算三次贝塞尔曲线路径，控制点沿端口方向自适应延伸，产生平滑的曲线连接。适合需要柔和视觉风格的场景。

#### 实现方式

1. **平行边分组与偏移**：与直线路由相同的逻辑。
2. **控制点计算**：根据起止点和端口方向，沿端口法线方向延伸控制点。延伸距离与起止点距离成正比，受 `tension` 参数调节（默认 0.5，范围 0–2）。tension 越大曲线弧度越大。
3. **标签定位**：贝塞尔曲线 t=0.5 处，沿法线额外偏移。

可调参数：`tension`（弧度系数，0.0–2.0）。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(E) | 每条边独立计算控制点 |
| 空间复杂度 | O(E) | 控制点映射 |

#### 适用场景

- 思维导图、轻视觉图、需要柔和曲线的场景
- 适用于所有图形类型

---

### 2.4 Circular（弧形路由）

**文件**：`edge/edge_routing_circular.rs`

#### 算法原理

圆形布局专用弧形边路由。同圆内边走弧形贝塞尔（控制点沿圆弧外侧凸出），跨圆边走端口贝塞尔。自环边走小圆弧。

#### 实现方式

1. **圆簇解析**：从布局提示或节点坐标反推圆簇信息（中心、半径、节点位置）。
2. **同圆内边**：计算起止点在圆上的步数（前向/后向取较短路径），根据步数确定凸出因子（1.05–1.48）。凸出点位于起止点中点沿径向外推的位置。控制点从起止点向凸出点延伸 55%。
3. **跨圆边**：使用通用贝塞尔控制点计算。
4. **自环边**：从节点中心沿径向外探出小圆弧。
5. **平行边偏移**：同对节点间多条边按 `PARALLEL_SPACING = 0.10` 弧度偏移。
6. **标签避让**：迭代检测标签 AABB 重叠并推开，额外检测标签与节点的径向距离。

#### 复杂度

| 指标 | 复杂度 | 说明 |
|------|--------|------|
| 时间复杂度 | O(E × (V + E)) | 标签避让为 O(E² + E × V) |
| 空间复杂度 | O(E + C) | C = 圆簇数 |

#### 适用场景

- 状态图、环图（与 Circular 布局配合使用）
- 仅适用于 `State`、`Er`

---

## 3. 图形类型与算法选择

### 3.1 图形拓扑特征与算法适配

不同的图形拓扑特征决定了最适合的布局算法：

| 拓扑特征 | 推荐算法 | 原因 |
|----------|----------|------|
| **有向无环图（DAG）** | `sugiyama-v2` | 分层布局天然适配 DAG，层级方向与边方向一致 |
| **有环有向图** | `sugiyama-v2` | 去环阶段处理环，分层后环变为跨层回边 |
| **树/近似树** | `mindmap` | 专用树布局，根节点突出，子树紧凑 |
| **强连通图/环结构** | `circular` | 圆周排列使环结构直观可见 |
| **网状/弱层级图** | `force-directed` | 力导向自然散开，不强制分层 |
| **分组语义强** | `architecture` | 两层 Sugiyama 保持组内紧凑 |
| **时序语义** | `sequence` | 强语义专用布局，参与者+时间步 |

### 3.2 算法选择依据

选择布局算法时应考虑以下维度：

1. **图的方向性**：有向图优先分层布局（Sugiyama 系列），无向图或弱方向图用力导向。
2. **层级结构强度**：强层级（流程、审批）用 Sugiyama，弱层级（网络拓扑）用力导向，无层级（环）用圆形。
3. **分组需求**：有明确分组用 `architecture` 或 `force-directed`（分组增强），无分组用 `sugiyama-v2`。
4. **确定性要求**：需要完全确定性输出时避免 `force-directed`。
5. **规模**：超过 200 节点优先 `sugiyama-v2`（线性复杂度），避免 `force-directed`（O(V²)）。
6. **视觉风格**：需要柔和曲线选 `bezier` 路由，需要清晰折线选 `orthogonal` 路由。

### 3.3 推荐选型表

| 业务场景 | 首选布局 | 首选路由 | 次选布局 | 原因 |
|----------|----------|----------|----------|------|
| 流程图 / 审批流 | `sugiyama-v2` | `orthogonal` | `sugiyama` | 新版层压缩与坐标分配更紧凑 |
| 高密度流程 DAG | `sugiyama-v2` | `orthogonal` | `sugiyama` | 新版层压缩更积极 |
| 组织架构 / 树型依赖 | `mindmap` | `bezier` | `force-directed` | 中心展开更适合层级阅读 |
| 思维导图 | `mindmap` | `bezier` | `force-directed` | 主题节点表达更强 |
| 状态机 / 闭环关系 | `circular` | `circular` | `sugiyama-v2` | 环结构表达更自然 |
| 架构图 / 分组组件图 | `architecture` | `orthogonal` | `force-directed` | 分层+分组，力导向作备选 |
| 网络拓扑 / 自由依赖图 | `force-directed` | `bezier` | `mindmap` | 力导向更适合弱层级关系 |
| 时序交互 | `sequence` | 内置 | 无 | 强语义专用布局 |
| ER 图 | `sugiyama-v2` | `straight` | `sugiyama` | 实体关系图用直线更清晰 |

---

## 4. 算法使用示例

### 4.1 基本用法

通过 diagram 属性切换布局和路由：

```text
diagram flowchart {
  layout_algo: sugiyama-v2
  edge_routing: orthogonal
}
```

多词算法名使用连字符，如 `sugiyama-v2`、`architecture`、`force-directed`。

### 4.2 带配置块

```text
diagram flowchart {
  layout_algo: sugiyama-v2 {
    group_padding: 20
  }
  edge_routing: orthogonal {
    slot_pitch: 40
    channel_margin: 18
  }
}
```

简写 `edge_routing: bezier` 等价于空配置块，全部使用算法默认值。

### 4.3 各场景示例

**流程图**：

```text
diagram flowchart {
  layout_algo: sugiyama-v2
  layout: top-to-bottom
  edge_routing: orthogonal

  entity start "开始" { type: start }
  entity process "处理" { type: process }
  entity decision "判断" { type: decision }
  entity end "结束" { type: end }

  start -> process
  process -> decision
  decision -> end
  decision -> process
}
```

**思维导图**：

```text
diagram mindmap {
  layout_algo: mindmap
  layout: radial
  edge_routing: bezier

  entity topic "核心主题" { type: root }
  entity a "分支A" { type: main }
  entity b "分支B" { type: main }
  entity a1 "细节A1" { type: leaf }

  topic -> a
  topic -> b
  a -> a1
}
```

**状态图**：

```text
diagram state {
  layout_algo: circular
  edge_routing: circular

  entity idle "空闲" { type: initial }
  entity running "运行中" { type: state }
  entity paused "暂停" { type: state }

  idle -> running "启动"
  running -> paused "暂停"
  paused -> running "恢复"
  running -> idle "停止"
}
```

**架构图**：

```text
diagram architecture {
  layout_algo: architecture
  edge_routing: orthogonal

  group frontend {
    entity web "Web 前端"
    entity mobile "移动端"
  }
  group backend {
    entity api "API 网关"
    entity auth "认证服务"
  }

  web -> api
  mobile -> api
  api -> auth
}
```

**时序图**：

```text
diagram sequence {
  entity client "客户端"
  entity server "服务端"
  entity db "数据库"

  client -> server "请求"
  server -> db "查询"
  db --> server "结果"
  server --> client "响应"
}
```

**力导向图**：

```text
diagram architecture {
  layout_algo: force-directed
  edge_routing: bezier

  entity a "服务A"
  entity b "服务B"
  entity c "服务C"
  entity d "服务D"

  a -> b
  b -> c
  c -> d
  d -> a
  a -> c
}
```

---

## 5. 算法效果对比

### 5.1 节点布局算法对比

| 维度 | sugiyama | sugiyama-v2 | mindmap | circular | sequence | force-directed | architecture |
|------|----------|-------------|---------|----------|----------|----------------|-----------------|
| **时间复杂度** | O(V+E+I×L×C) | O(V+E+S×L×C) | O(V+E) | O(V+E+R) | O(V+E×V) | O(I×(V²+E)) | O(V+E+G×S) |
| **空间复杂度** | O(V+E) | O(V+E+D) | O(V) | O(V+E) | O(V+E) | O(V+E) | O(V+E+G) |
| **确定性** | 是 | 是 | 是 | 是 | 是 | 否 | 是 |
| **交叉控制** | 强 | 强 | 不适用 | 中 | 不适用 | 不适用 | 强 |
| **边长控制** | 强 | 强 | 中 | 中 | 强 | 中-高 | 强 |
| **空间利用率** | 中-高 | 高 | 中 | 中 | 高 | 高 | 高 |
| **分组表达** | 中 | 中 | 无 | 弱 | 无 | 强 | 强 |
| **根节点表达** | 中 | 中 | 强 | 弱 | 不适用 | 弱 | 中 |
| **建议规模** | 20-300 | 20-320 | 10-200 | 10-120 | 10-200消息 | 10-180 | 20-250 |

### 5.2 边路由算法对比

| 维度 | straight | orthogonal | bezier | circular |
|------|----------|------------|--------|----------|
| **时间复杂度** | O(E×(V+E)) | O(E×(V+P)) | O(E) | O(E×(V+E)) |
| **空间复杂度** | O(E) | O(E+P) | O(E) | O(E+C) |
| **视觉风格** | 直线 | 折线 | 曲线 | 弧线 |
| **平行边支持** | 是 | 是 | 是 | 是 |
| **标签避让** | 是 | 是 | 否 | 是 |
| **可调参数** | 无 | slot_pitch, channel_margin | tension | 无 |
| **适用图形** | Er | Flowchart, Architecture, State, Er | 全部 | State, Er |

### 5.3 视觉效果对比

| 算法 | 边长度控制 | 空间利用率 | 分组表达 | 根节点表达 | 适合 A/B 替换对象 |
|------|------------|------------|----------|------------|------------------|
| `sugiyama` | 强 | 中-高 | 中 | 中 | `sugiyama-v2` |
| `sugiyama-v2` | 强 | 高 | 中 | 中 | `sugiyama` |
| `force-directed` | 中-高 | 高 | 强 | 弱 | `mindmap` |
| `circular` | 中 | 中 | 弱 | 弱 | — |
| `mindmap` | 中 | 中 | 无 | 强 | — |
| `architecture` | 强 | 高 | 强 | 中 | `force-directed` |

---

## 6. 算法配置块扩展指南

`layout_algo` 与 `edge_routing` 支持 `algo { option: value }` 配置块。option key 不在 `language-spec.md` 的 diagram 属性表里维护，而是由布局/路由实现自行消费。

### 6.1 当前已登记的 option

| 属性 | 算法 | option key | 默认值 | 读取位置 |
|------|------|------------|--------|----------|
| `layout_algo` | `sugiyama`, `sugiyama-v2`, `flowchart`, `er` | `group_padding` | 28 | `SugiyamaLayoutConfig` → `group_bounds` |
| `layout_algo` | `mindmap` | `padding` | 48 | `MindmapLayoutConfig` |
| `layout_algo` | `mindmap` | `level_gap` | 200 | `MindmapLayoutConfig` |
| `layout_algo` | `mindmap` | `branch_gap` | 70 | `MindmapLayoutConfig` |
| `layout_algo` | `mindmap` | `node_gap` | 22 | `MindmapLayoutConfig` |
| `layout_algo` | `mindmap` | `center_gap` | 100 | `MindmapLayoutConfig` |
| `layout_algo` | `sequence` | `group_padding` | 20 | `SequenceLayoutConfig` |
| `layout_algo` | `sequence` | `node_spacing` | 80 | `SequenceLayoutConfig` |
| `layout_algo` | `sequence` | `message_spacing` | 50 | `SequenceLayoutConfig` |
| `layout_algo` | `force-directed` | `group_padding` | 20 | `ForceDirectedLayoutConfig` |
| `layout_algo` | `force-directed` | `padding` | 48 | `ForceDirectedLayoutConfig` |
| `layout_algo` | `force-directed` | `component_gap` | 120 | `ForceDirectedLayoutConfig` |
| `layout_algo` | `architecture` | `group_padding` | 28 | `ArchitectureV2LayoutConfig` |
| `layout_algo` | `architecture` | `padding` | 40 | `ArchitectureV2LayoutConfig` |
| `layout_algo` | `circular` | `group_padding` | 20 | `CircularLayoutConfig` |
| `layout_algo` | `circular` | `padding` | 48 | `CircularLayoutConfig` |
| `layout_algo` | `circular` | `component_gap` | 40 | `CircularLayoutConfig` |
| `edge_routing` | `bezier`, `spline` | `tension` | 0.5 | `BezierConfig` |
| `edge_routing` | `orthogonal` | `slot_pitch` | 40 | `OrthoConfig` |
| `edge_routing` | `orthogonal` | `channel_margin` | 18 | `OrthoConfig` |

### 6.2 LayoutPlan 数据流

```text
prepare()
  → LayoutPlan::resolve(diagram, profile)   // 写入 PreparedDiagram
validate()
  → validate_layout_plan_warnings()          // 未知 key + 非法值警告
compute_layout_with_plan(diagram, plan)
  → registry::build_*_strategy(algo, plan)   // 参数化实例
  → strategy.compute / router.route          // 只读 self.config，不再读 AST
```

option **schema** 定义在算法模块（`option_specs()`）；**profile** 可通过 `default_layout_options` 补默认值；**解析结果** 挂在 `PreparedDiagram.layout_plan()`。

### 6.3 新增 option 步骤

1. 在 `algorithm_config.rs`（或算法模块）声明 `AlgorithmOptionSpec`，并在 `option_specs()` 返回。
2. 在算法 struct 增加 config 字段，实现 `from_options(&ResolvedAlgoOptions)`。
3. 在 `layout/registry.rs` 的 `build_*_strategy` 工厂里注入 plan 中的 option。
4. 在 `plan.rs` / `validation/mod.rs` 无需额外改动（自动走 `LayoutPlan::resolve` + `validate_layout_plan_warnings`）。

### 6.4 相关代码入口

| 文件 | 职责 |
|------|------|
| `layout/plan.rs` | `LayoutPlan` / `ResolvedAlgoOptions` |
| `layout/registry.rs` | `LAYOUT_ALGORITHM_NAMES` / `EDGE_ROUTING_NAMES` + 工厂 |
| `layout/algorithm_config.rs` | `OptionsReader`、`SugiyamaLayoutConfig` |
| `layout/mod.rs` | `compute_layout_with_plan` |
| `ast.rs` | `PreparedDiagram` 携带 `layout_plan` |
| `pipeline.rs` | prepare 时解析 plan |
| `validation/mod.rs` | 算法 option 校验 |

---

## 7. 研发演进建议

结合 `docs/architecture/graphviz-algorithms-research.md` 的研究结论，后续优先级：

1. **`sugiyama_v2.rs`**：继续从 NS-style 演进到完整 Network Simplex feasible tree / cut value pivot；继续从 BK-style 演进到完整 Brandes-Kopf 冲突检测与垂直对齐。
2. **`force_directed.rs`**：考虑 Barnes-Hut 或多尺度近似，降低 O(V²) 斥力成本。
3. **边路由**：在 `edge_routing_bezier.rs` 之外新增障碍避让样条路由（可见性图 + 最短路径 + 曲线拟合）。
4. **`circular.rs`**：升级为双连通分量驱动的 `circo` 风格布局。
5. **多分量支持**：抽取独立 `pack.rs`，将分量横向拼接升级为真正的 pack 算法。

---

## 8. 依赖与合规

### 8.1 当前直接依赖

| 依赖 | 用途 | 决策 |
|------|------|------|
| `petgraph` | Sugiyama 等图算法底座 | 保留 |
| `unicode-width` | 标签宽度估算 | 保留 |

### 8.2 已评估但未引入

| 候选库 | 可能用途 | 暂缓原因 |
|--------|----------|----------|
| `rust-sugiyama` | Network Simplex、Brandes-Kopf | 需先评估与 `LayoutResult` 的适配成本 |
| `nalgebra` | Stress Majorization 线性系统求解 | 当前算法不需要矩阵求解 |
| `geo` | 可见性图、多边形障碍避让 | 仅样条避让时才有必要 |

### 8.3 结论

- 第一阶段优先"独立新算法文件 + 保留旧版本"。
- 第二阶段按研究结论引入更重的算法依赖，并补充 License、维护状态、包体积三项正式审查记录。
- 不新增依赖可避免额外 License 审查、WASM 包体积增长和编译时间抖动。
