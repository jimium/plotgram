# Graphviz 核心算法研究：Drawify 可借鉴的技术与 Rust 实现路线

> 版本：0.1.0-draft | 状态：研究完成

本文档梳理 Graphviz 项目中经过 30+ 年学术打磨的核心算法，分析 Drawify 现有布局引擎的差距，并给出用 Rust 自行实现的优先级排序与技术路线。

---

## 1. Graphviz 算法全景图

Graphviz 包含 8 个布局引擎，每个引擎背后是一套完整的算法流水线：

| 引擎 | 算法类型 | 适用场景 | 论文基础 |
|------|----------|----------|----------|
| **dot** | 有向图分层布局 | 流程图、架构图、依赖图 | Gansner et al., IEEE TSE 1993 |
| **neato** | 应力最小化（弹簧模型） | 无向图、网络拓扑 | Kamada-Kawai 1989; Gansner-Koren-North 2004 |
| **fdp** | Fruchterman-Reingold 力导向 | 无向图、聚类图 | Fruchterman & Reingold, SPE 1991 |
| **sfdp** | 多尺度力导向 | 大规模图（数千节点） | fdp 的多尺度扩展 |
| **circo** | 双连通分量圆形布局 | 网络拓扑、状态图 | Six & Tollis, GD'99 |
| **twopi** | 径向布局 | 树/层次结构、组织架构 | Wills, GD'97 |
| **patchwork** | Squarified Treemap | 层次数据占比可视化 | Bruls et al. |
| **osage** | 数组式聚类布局 | 聚类可视化 | — |

此外，Graphviz 还有一套**独立的边路由系统**（Spline-o-Matic），被所有引擎共享。

---

## 2. 有向图分层布局（dot 引擎）— 最高优先级

### 2.1 dot 的四阶段流水线

**论文**：Gansner, Koutsofios, North, Vo — "A Technique for Drawing Directed Graphs", IEEE TSE 1993

```
rank()        → 最优层级分配（Network Simplex）
ordering()    → 层内节点排序（交叉最小化）
position()    → 最优节点坐标（辅助图 + Network Simplex）
make-splines() → 样条边路由
```

### 2.2 Drawify 现状对比

| 阶段 | Graphviz dot | Drawify sugiyama.rs | 差距 |
|------|-------------|---------------------|------|
| Phase 1: 去环 | Greedy FAS（贪心反馈弧集） | DFS 反转回边 | 基本对齐，Graphviz 的贪心 FAS 反转更少边 |
| Phase 2: 层分配 | **Network Simplex**（最优） | 最长路径法 | **最大差距**：最长路径法不是最优的 |
| Phase 3: 交叉最小化 | Weighted Median + Barycenter + AdjExchange | 综合评分法（barycenter + bending） | 部分对齐，缺少 WMedian 和相邻交换 |
| Phase 4: 坐标分配 | 辅助图 + Network Simplex | 简单迭代优化 | **重大差距**：Graphviz 构建辅助图再做 NS |

### 2.3 Network Simplex（网络单纯形）— 核心中的核心

这是 Graphviz dot 的灵魂算法，在 Phase 2 和 Phase 4 都用到。

**原理**：

- 将层级分配问题建模为**带约束的最小化问题**
- 每条边有 `minlen` 约束（最小层级差），目标是最小化总边长
- 构建一棵**可行生成树**（feasible tree），通过不断交换树边/非树边来降低总代价
- 类似线性规划中的单纯形法，但在网络流图上操作

**算法步骤**：

```
1. 初始可行解：用最长路径法分配初始 rank
2. 构建紧生成树（tight spanning tree）：
   - 从任意节点开始
   - 只加入满足 rank[v] - rank[u] = minlen 的边
   - 扩展直到覆盖所有节点
3. 计算每条树边的 cut value：
   - cut value = 移除该边后，树被分成两半
   - 一半中所有边的权重和（考虑方向）
4. 找到 cut value < 0 的非树边
5. 交换：移除 cut value 最低的树边，加入该非树边
6. 重新计算 rank
7. 重复步骤 3-6 直到所有 cut value ≥ 0
```

**为什么比最长路径法好**：

- 最长路径法只保证约束满足，不优化总边长
- Network Simplex 在满足约束的前提下**最小化总边长**
- 结果更紧凑、边更短、布局更美观

**Rust 实现参考**：

- [rust-sugiyama](https://lib.rs/crates/rust-sugiyama) — 已用 Rust 实现了 Network Simplex 的 rank assignment
- [dagre](https://github.com/dagrejs/dagre) 的 `network-simplex.js` — JavaScript 经典实现
- [autog](https://github.com/nulab/autog) — Go 实现的完整 Sugiyama 流水线

**Rust 实现骨架**：

```rust
struct NetworkSimplex {
    tree: HashSet<EdgeIndex>,
    ranks: Vec<i32>,
    cut_values: HashMap<EdgeIndex, f64>,
}

impl NetworkSimplex {
    fn rank(&mut self, graph: &DiGraph<NodeData, EdgeData>) -> Vec<i32> {
        // 1. 初始可行解（最长路径法）
        self.initial_rank(graph);
        // 2. 构建紧生成树
        self.tight_tree(graph);
        // 3. 迭代优化
        while let Some(e) = self.find_negative_cut_edge(graph) {
            self.exchange(e, graph);
        }
        self.ranks.clone()
    }

    fn tight_tree(&mut self, graph: &DiGraph<NodeData, EdgeData>) {
        // 从任意节点开始，只加入满足 delta = rank[v] - rank[u] = minlen 的边
    }

    fn find_negative_cut_edge(
        &self,
        graph: &DiGraph<NodeData, EdgeData>,
    ) -> Option<EdgeIndex> {
        // 遍历非树边，找到 cut value < 0 的
    }

    fn exchange(&mut self, incoming: EdgeIndex, graph: &DiGraph<NodeData, EdgeData>) {
        // 移除 cut value 最低的树边，加入 incoming
        // 重新计算 ranks
    }
}
```

### 2.4 Brandes-Köpf 坐标分配

**论文**：Brandes & Köpf, "Fast and Simple Horizontal Coordinate Assignment", GD 2001

Graphviz 原版用辅助图 + Network Simplex 做坐标分配，但 Brandes-Köpf 提供了一个**更简洁、更快**的替代方案，效果接近最优。

**原理**：

- 四趟扫描：左对齐 → 右对齐 → 取平均
- 保证同层节点不重叠
- 边弯曲最小化

**Rust 实现**：`rust-sugiyama` crate 已经实现了此算法，可直接参考或集成。

### 2.5 Greedy FAS（贪心反馈弧集）

**原理**：比 DFS 反转回边更优的去环方法。

- 对图做 DFS，按完成时间排序
- 将节点分为 S（已选）和 T（未选）两组
- 每次选择使反馈弧最少的节点加入 S
- 反转从 T 到 S 的所有边

**Rust 实现**：petgraph 已有 `greedy_feedback_arc_set` 函数，可直接使用。

---

## 3. 无向图力导向布局（neato / fdp 引擎）— 高优先级

### 3.1 Drawify 现状

`force-directed.rs` 实际上是**分组感知布局**（分组拓扑排序 + 网格排列），不是真正的力导向。需要重写。

### 3.2 Stress Majorization（应力优化）— 推荐首选

**论文**：Gansner, Koren, North — "Graph Drawing by Stress Majorization", GD 2004

**原理**：

- 定义应力函数：`stress(X) = Σ w_ij * (||X_i - X_j|| - d_ij)²`
  - `d_ij`：节点 i、j 的图论距离
  - `w_ij = 1 / d_ij²`：权重
- 通过 majorization 迭代，每步求解一个线性系统来降低应力
- 保证单调递减，不会振荡

**算法步骤**：

```
1. 计算所有节点对的图论距离 d_ij（BFS/DFS）
2. 计算权重 w_ij = 1 / d_ij²
3. 初始化节点位置（circular 或 random）
4. 迭代：
   a. 构建加权拉普拉斯矩阵 L_w
   b. 构建 b 向量（基于当前位置和目标距离）
   c. 求解 L_w * X' = b（线性系统）
   d. 如果 stress(X') < stress(X) - ε，继续；否则停止
5. 返回最终位置
```

**优势**（相比 Kamada-Kawai）：

- 保证单调递减（不会振荡）
- 收敛更快
- 支持稀疏模型（只计算部分节点对距离，O(n) 而非 O(n²)）
- 可与 subspace restriction 结合，大幅加速

**Rust 实现骨架**：

```rust
use nalgebra::{DMatrix, DVector};

fn stress_majorization(
    adj: &HashMap<usize, Vec<usize>>,
    iterations: usize,
) -> HashMap<usize, (f64, f64)> {
    let n = adj.len();
    // 1. 计算图论距离矩阵 d_ij
    let dist = all_pairs_shortest_path(adj);
    // 2. 计算权重 w_ij = 1 / d_ij^2
    let weights = compute_weights(&dist);
    // 3. 初始化位置
    let mut pos = initialize_positions(n);

    for _ in 0..iterations {
        // 构建拉普拉斯矩阵 L_w
        let laplacian = build_weighted_laplacian(&weights, n);
        // 构建 b 向量
        let (b_x, b_y) = build_b_vectors(&weights, &dist, &pos, n);
        // 求解 L_w * X' = b
        let new_x = laplacian.lu().solve(&b_x).unwrap();
        let new_y = laplacian.lu().solve(&b_y).unwrap();
        // 更新位置
        update_positions(&mut pos, &new_x, &new_y);
    }

    pos
}
```

### 3.3 Fruchterman-Reingold 力导向 — 简单有效

**论文**：Fruchterman & Reingold, "Graph Drawing by Force-directed Placement", SPE 1991

**算法步骤**：

```
1. 随机初始化位置
2. 每轮迭代：
   a. 计算所有节点对间的斥力（库仑力）
   b. 计算相邻节点间的引力（弹簧力）
   c. 根据合力更新位置
   d. 降温（模拟退火）
3. 直到温度低于阈值
```

**sfdp** 是 fdp 的多尺度版本，用四叉树/多格方法加速斥力计算，可处理数千节点。

**Rust 实现**：纯 Rust 手写，无需外部依赖。适合作为轻量级力导向选项。

---

## 4. 径向布局（twopi 引擎）— 中等优先级

**论文**：Wills, "Radial Layout", GD 1997

**原理**：

- 选择根节点放中心
- BFS 计算到根的距离
- 距离为 k 的节点放在第 k 个同心圆上
- 同一圆上的节点按父节点角度扇形展开

**Drawify 现状**：`circular.rs` 是圆形布局（所有节点在一个圆上），不是径向布局。

**适合场景**：思维导图、组织架构图、依赖树

**Rust 实现骨架**：

```rust
fn radial_layout(
    root: &str,
    diagram: &Diagram,
) -> LayoutResult {
    // 1. BFS 计算每个节点到 root 的距离
    let distances = bfs_distances(root, diagram);
    // 2. 按距离分层
    let rings = group_by_distance(&distances);
    // 3. 每个环上按父节点角度扇形展开
    let mut nodes = HashMap::new();
    for (ring_idx, ring_nodes) in rings.iter().enumerate() {
        let radius = PADDING + ring_idx as f64 * RING_GAP;
        let angle_per_node = 2.0 * PI / ring_nodes.len() as f64;
        for (i, node_id) in ring_nodes.iter().enumerate() {
            let angle = angle_per_node * i as f64;
            let x = center_x + radius * angle.cos();
            let y = center_y + radius * angle.sin();
            // ...
        }
    }
    // 4. 边路由
}
```

---

## 5. 双连通分量圆形布局（circo 引擎）— 中高优先级

**论文**：Six & Tollis, GD'99 & ALENEX '99; Kaufmann & Wiese, GD'02

**原理**：

- 识别双连通分量（biconnected components）
- 每个分量的节点排在一个圆上
- 割点（cut vertices）连接多个分量
- 用递归径向算法布局 block-cutpoint tree
- 如果分量是外平面图，得到平面布局
- 边交叉最小化：尽量将边放在圆周上

**Drawify 现状**：`circular.rs` 是简单圆形排列，没有双连通分量分析。

**适合场景**：状态图、网络拓扑图

**Rust 实现**：petgraph 已有 biconnected components 算法，可直接使用。

---

## 6. 边路由算法 — 最高优先级

### 6.1 Spline-o-Matic（样条路由）— Graphviz 的核心边路由

**两阶段算法**：

**Phase 1：最短折线路径**

- **多边形内路由**：Overmars-Welzl 算法（高效）
- **障碍物绕行**：构建可见性图（visibility graph），O(N³) 但可预计算复用
- 输出：一条避开所有障碍的最短折线

**Phase 2：折线 → 样条曲线**

- 输入：Phase 1 的折线 + 屏障线段
- 输出：不触碰任何屏障的贝塞尔曲线
- 算法：Proutespline — 基于约束优化的样条拟合

### 6.2 Drawify 现状对比

| 能力 | Graphviz | Drawify |
|------|----------|---------|
| 正交路由 | 基础（有已知缺陷） | **更好**（磁吸点 + 侧通道绕行 + 标签避让） |
| 贝塞尔路由 | 障碍避让 + 样条拟合 | **无障碍避让**，边可能穿过节点 |
| 样条路由 | 可见性图 + 最短路径 + 样条拟合 | 不支持 |

### 6.3 可见性图实现

**原理**：

- 将所有节点视为多边形障碍物
- 构建障碍物顶点之间的可见性图
- 在可见性图上用 Dijkstra 求最短路径
- 将折线路径拟合为平滑样条

**Rust 实现骨架**：

```rust
struct VisibilityGraph {
    vertices: Vec<Point>,
    edges: Vec<(usize, usize)>,
    obstacle_ids: Vec<usize>,
}

impl VisibilityGraph {
    fn build(obstacles: &[Polygon]) -> Self {
        // 1. 收集所有障碍物顶点
        // 2. 对每对顶点，检查连线是否穿过任何障碍物
        // 3. 不穿过的加入可见性图
        // O(N^3) 但可预计算
    }

    fn shortest_path(&self, from: Point, to: Point) -> Vec<Point> {
        // Dijkstra 在可见性图上求最短路径
    }
}

fn route_spline(
    path: &[Point],
    barriers: &[LineSegment],
) -> CubicBezier {
    // 1. 将折线拟合为贝塞尔曲线
    // 2. 确保不触碰任何 barrier
    // 3. 返回平滑样条
}
```

---

## 7. 其他 Graphviz 技术

| 技术 | 说明 | Drawify 适用性 |
|------|------|---------------|
| **pack 库** | 多连通分量独立布局后打包排列 | 高：当前不支持多分量 |
| **overlap removal** | Voronoi / prism 方法消除节点重叠 | 高：力导向后必须有 |
| **concentrate** | 合并平行边为一条 | 中：架构图场景有用 |
| **cluster 支持** | 分组/聚类布局 | 已有（force-directed.rs 的分组感知） |
| **patchwork** | Squarified Treemap 布局 | 低 |
| **osage** | 数组式聚类布局 | 低 |

---

## 8. 优先级排序与实施建议

### P0：立即实现（核心体验差距）

| 算法 | 预估工作量 | 影响 | Rust 实现策略 |
|------|-----------|------|--------------|
| **Network Simplex 层分配** | 2-3 周 | 流程图/架构图质量飞跃 | 参考 `rust-sugiyama` crate，或从 dagre 移植 |
| **Brandes-Köpf 坐标分配** | 1-2 周 | 节点间距更均匀 | 直接用 `rust-sugiyama` 或手写 |
| **可见性图 + 障碍避让样条路由** | 3-4 周 | 边不再穿过节点 | 参考 Graphviz pathplan 库 |

### P1：短期实现（丰富布局能力）

| 算法 | 预估工作量 | 影响 | Rust 实现策略 |
|------|-----------|------|--------------|
| **Stress Majorization 力导向** | 2-3 周 | 架构图/依赖图质量大幅提升 | 用 `nalgebra` 解线性系统 |
| **Fruchterman-Reingold 力导向** | 1-2 周 | 简单场景快速布局 | 纯 Rust 手写 |
| **径向布局（twopi）** | 1 周 | 思维导图/组织架构图 | BFS + 角度分配 |
| **多连通分量 pack** | 1 周 | 所有布局算法受益 | 参考 Graphviz pack 库 |

### P2：中期实现（专业级能力）

| 算法 | 预估工作量 | 影响 | Rust 实现策略 |
|------|-----------|------|--------------|
| **双连通分量圆形布局（circo）** | 2 周 | 状态图/网络拓扑 | petgraph 已有 biconnected components |
| **sfdp 多尺度力导向** | 3-4 周 | 大规模图（1000+ 节点） | 四叉树加速 + 多格求解 |
| **overlap removal** | 1-2 周 | 力导向后处理 | Voronoi 方法 |
| **Greedy FAS 去环** | 1 周 | 比 DFS 反转更少边 | petgraph 已有 `greedy_feedback_arc_set` |

---

## 9. 与 Drawify 架构的集成方案

### 9.1 现有架构

根据 `layout/mod.rs` 的 `LayoutStrategy` trait 和 `EdgeRoutingStrategy` trait，新算法可以无缝插入：

```rust
// 现有调度逻辑
pub fn compute_layout(diagram: &Diagram) -> LayoutResult {
    let result = match algo {
        "sugiyama" => sugiyama::SugiyamaLayout.compute(diagram),
        "sugiyama-v2" => sugiyama_v2::SugiyamaV2Layout.compute(diagram),
        "force-directed" => force-directed::ForceDirectedLayout.compute(diagram),
        // 新算法只需在此添加
        _ => sugiyama_v2::SugiyamaV2Layout.compute(diagram),
    };
    // 边路由后处理
    router.route(diagram, result)
}
```

### 9.2 建议的文件组织

```
layout/
  mod.rs                  ← 调度入口
  sugiyama.rs             ← 升级：Network Simplex + Brandes-Köpf
  sugiyama_v2.rs          ← 新版 Sugiyama（默认分层主路径）
  force-directed.rs       ← 重写：真正的力导向（Stress Majorization / FR）
  circular.rs             ← 升级：双连通分量分析
  mindmap.rs              ← 保持
  sequence.rs             ← 保持
  timeline.rs             ← 保持
  radial.rs               ← 新增：径向布局（twopi）
  network_simplex.rs      ← 新增：独立的 NS 模块（被 sugiyama 和坐标分配复用）
  visibility.rs           ← 新增：可见性图（被样条路由复用）
  edge_routing.rs         ← 保持
  edge_routing_orthogonal.rs ← 保持
  edge_routing_bezier.rs  ← 保持
  edge_routing_spline.rs  ← 新增：障碍避让样条路由
  edge_routing_circular.rs ← 保持
  pack.rs                 ← 新增：多分量打包
  overlap.rs              ← 新增：重叠消除
```

### 9.3 Network Simplex 模块化设计

Network Simplex 被 Phase 2（层分配）和 Phase 4（坐标分配）复用，应设计为独立模块：

```rust
// layout/network_simplex.rs

pub struct NetworkSimplexSolver {
    // 内部状态
}

impl NetworkSimplexSolver {
    /// 最优层级分配（Phase 2）
    pub fn optimal_rank(&mut self, graph: &DiGraph<NodeData, EdgeData>) -> Vec<i32> {
        // ...
    }

    /// 最优坐标分配（Phase 4）
    /// 构建辅助图后调用 NS 求解
    pub fn optimal_position(
        &mut self,
        layers: &[Vec<NodeIndex>],
        graph: &DiGraph<NodeData, EdgeData>,
    ) -> HashMap<NodeIndex, f64> {
        // ...
    }
}
```

---

## 10. Rust 生态可用资源

| Crate | 用途 | 适用场景 |
|-------|------|----------|
| [petgraph](https://crates.io/crates/petgraph) | 图数据结构、BFS/DFS、拓扑排序、FAS | 所有布局算法的基础 |
| [rust-sugiyama](https://crates.io/crates/rust-sugiyama) | 完整 Sugiyama 实现（含 Network Simplex + Brandes-Köpf） | P0 直接集成或参考 |
| [nalgebra](https://crates.io/crates/nalgebra) | 线性代数（矩阵求解） | Stress Majorization |
| [spade](https://crates.io/crates/spade) | Delaunay 三角化 / Voronoi | overlap removal |
| [geo](https://crates.io/crates/geo) | 计算几何（线段相交、多边形操作） | 可见性图 |

---

## 11. 总结

| 维度 | Graphviz | Drawify 现状 | 建议行动 |
|------|----------|-------------|---------|
| 有向图布局 | Network Simplex（最优） | 最长路径法（次优） | **升级到 NS** |
| 坐标分配 | 辅助图 NS / Brandes-Köpf | 简单迭代 | **实现 BK** |
| 力导向 | Stress Majorization / FR | 伪力导向（分组排列） | **重写为真力导向** |
| 边路由 | 可见性图 + 样条拟合 | 正交（好）/ 贝塞尔（无避让） | **加障碍避让** |
| 径向布局 | twopi | 无 | **新增** |
| 圆形布局 | circo（双连通分量） | 简单圆形 | **升级** |
| 多分量 | pack 库 | 不支持 | **新增** |

**核心结论**：Drawify 最需要从 Graphviz 借鉴的不是"更多布局算法"，而是 **Network Simplex 最优层级分配** 和 **可见性图障碍避让样条路由** 这两项核心技术。前者决定了有向图布局的质量上限，后者决定了边的可读性。这两项用 Rust 自行实现的难度中等（有 `rust-sugiyama` 和 `nalgebra` 等生态支持），但收益极大。

---

## 参考文献

1. Gansner E R, Koutsofios E, North S C, et al. A technique for drawing directed graphs[J]. IEEE Transactions on Software Engineering, 1993, 19(3): 214-230.
2. Brandes U, Köpf B. Fast and simple horizontal coordinate assignment[C]. International Symposium on Graph Drawing. Springer, 2001: 31-44.
3. Kamada T, Kawai S. An algorithm for drawing general undirected graphs[J]. Information processing letters, 1989, 31(1): 7-15.
4. Gansner E R, Koren Y, North S. Graph drawing by stress majorization[C]. International Symposium on Graph Drawing. Springer, 2004: 239-250.
5. Fruchterman T M J, Reingold E M. Graph drawing by force-directed placement[J]. Software: Practice and experience, 1991, 21(11): 1129-1164.
6. Six J M, Tollis I G. Circular drawings of biconnected graphs[C]. International Symposium on Algorithms and Computation. Springer, 1999: 352-363.
7. Wills G J. NicheWorks—interactive visualization of very large graphs[J]. Journal of computational and graphical statistics, 1999, 8(2): 190-212.
8. Barth W, Mutzel P, Jünger M. Simple and efficient bilayer cross counting[J]. Journal of Graph Algorithms and Applications, 2004, 8(2): 179-194.
