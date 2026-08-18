# 04 · 力导向、Stress 与多级布局（Organic Layout）

> 对应 yFiles `OrganicLayout` / `SmartOrganicLayout` / `InteractiveOrganicLayout`，
> Graphviz `neato`(stress) / `sfdp`(multilevel FR)，OGDF `FMMM`，D3 `forceSimulation`，`ForceAtlas2`。
> 主线论文：Eades 1984（Spring embedder）、Fruchterman–Reingold 1991、Kamada–Kawai 1989、
> Barnes–Hut 1986、Hu 2005（multilevel + octree）、Gansner–Koren–North 2004（stress majorization）、
> Zheng–Pawar–Goodman 2018（SGD stress）。

---

## 0. 两个家族：Spring 模型 vs Stress 模型

| | Spring/Force（FR、ForceAtlas2） | Stress/MDS（KK、neato、SMACOF） |
|---|---|---|
| 目标 | 无显式全局目标，靠力平衡 | 显式：$\text{stress}(X)=\sum_{i<j} w_{ij}(\lVert x_i-x_j\rVert - d_{ij})^2$ |
| 距离 | 只用邻接（局部） | 用**图论最短路** $d_{ij}$（全局） |
| 收敛 | 需退火/阻尼，易震荡 | majorization 单调下降，**保证收敛** |
| 复杂度 | 每轮 $O(V^2)$ → BH 后 $O(V\log V)$ | 需全对最短路 $O(V^2\log V)$ 或采样 |
| 质量 | 局部结构好，全局可能扭 | 全局距离忠实，聚类清晰 |
| 适合 | 交互、动态、超大图 | 中等规模、静态、质量优先 |

**结论**：如果要"一个 organic 布局"，**优先实现 stress majorization（+ 多级 + 采样）**，它有明确目标函数、可复现、可加约束（07 篇）。力导向留给交互式实时场景。

---

## 1. Spring Embedder 家族

### 1.1 Eades 1984
- 边 = 弹簧：$f_{spring} = c_1 \log(d/c_2)$（对数，避免长边过强）
- 非邻接点 = 电斥：$f_{rep} = c_3/\sqrt{d}$
- 每轮所有点按合力移动 $c_4 \cdot F$，跑 100 轮。

### 1.2 Fruchterman–Reingold 1991（最广为实现）
```
k = C * sqrt(area / |V|)                  // 理想边长
for iter in 0..N:
    // 斥力：所有点对
    for (v,u) in pairs: 
        d = pos[v]-pos[u]
        disp[v] += (d/|d|) * k*k/|d|
    // 引力：仅边
    for (v,u) in edges:
        d = pos[v]-pos[u]
        disp[v] -= (d/|d|) * |d|*|d|/k
        disp[u] += ...
    // 移动，受温度限制
    for v: pos[v] += (disp[v]/|disp[v]|) * min(|disp[v]|, temp)
    temp = cool(temp)                     // 线性退火到 0
```
- **退火（temperature）是 FR 的灵魂**：初期允许大跳跃跳出局部最优，末期只微调。退火太快 → 卡在乱麻；太慢 → 抖动不收敛。
- 复杂度 $O(iter\cdot V^2)$。
- **grid variant**：只算相邻网格单元内的斥力，$O(V)$ 每轮，但会产生"格子伪影"。

### 1.3 ForceAtlas2（Jacomy et al. 2014）—— Gephi 默认
生产级改良集合，每一项都值得抄：
- **degree-based 引力/斥力缩放**：斥力 $\propto (\deg_i+1)(\deg_j+1)$ ⇒ hub 自动被推开，形成清晰的星形结构。
- **LinLog 模式**（Noack 2007）：引力用 $\log$ 而非线性 ⇒ 强化**聚类分离**。Noack 证明 LinLog 的能量最小化等价于最大化 modularity 的连续版本——即"力导向布局的最优解就是社区检测"。这是理论上最漂亮的结果之一。
- **自适应局部步长（local speed）**：每个节点独立步长，按其"摇摆度"（swinging = 本轮力与上轮力的方向差）缩放：
  $$\text{speed}_i = \frac{k_s\cdot \text{globalSpeed}}{1+\text{globalSpeed}\sqrt{\text{swinging}_i}}$$
  ⇒ 消除全局退火调参，收敛稳定。**强烈推荐实现这一项**。
- **防重叠模式**：斥力按节点半径修正（$d \to d - r_i - r_j$，若 <0 则强斥力）。

### 1.4 Barnes–Hut / FMM 加速（必须做）
斥力是 n-body 问题：
- **Barnes–Hut**（四叉树）：把远处的一簇点用其质心近似，判据 $s/d < \theta$（$\theta \approx 0.5\!-\!1.2$）。$O(V\log V)$。
- **FMM / 多极展开**（OGDF `FMMM`，Hachul–Jünger 2004）：$O(V\log V)$ 但常数更好，精度可控。
- 实现要点：四叉树每轮重建（点动了）；重建 $O(V\log V)$，与遍历同阶。用 Morton code 排序可做到近线性且 cache 友好。

---

## 2. Stress 模型（推荐主力）

### 2.1 目标函数
$$\sigma(X)=\sum_{i<j} w_{ij}\,(\lVert x_i-x_j\rVert - d_{ij})^2,\quad w_{ij}=d_{ij}^{-2}$$
- $d_{ij}$ = 图论最短路 × 理想边长；$w_{ij}=d_{ij}^{-2}$ 使短距离更重要（这是标准选择，来自 Kruskal 的 stress-1）。

### 2.2 Kamada–Kawai 1989
用牛顿–拉夫森逐点优化，每次挑"力最大"的点优化到局部最优。收敛慢，仅小图。

### 2.3 Stress Majorization / SMACOF（Gansner–Koren–North 2004）——**核心算法**
用二次上界函数（majorizer）替代 stress，每轮解一个加权拉普拉斯线性系统：

$$L^w X^{(t+1)} = L^{Z} Z,\quad Z=X^{(t)}$$

其中
- $L^w$：$L^w_{ij}=-w_{ij}$，$L^w_{ii}=\sum_{j\ne i}w_{ij}$（固定，可预分解）
- $L^Z_{ij} = -w_{ij}d_{ij}/\lVert z_i-z_j\rVert$，$L^Z_{ii}=-\sum_{j\ne i}L^Z_{ij}$

**逐坐标独立求解**（x、y 分开），每轮：
```rust
fn smacof_iter(x: &mut Vec<f64>, y: &mut Vec<f64>, d: &Dist, w: &Weights) {
    // 每维一个 |V|x|V| 稀疏(实际稠密)线性系统，用共轭梯度或 Cholesky
    let (bx, by) = compute_rhs(x, y, d, w);   // L^Z Z
    solve_in_place(&L_w_factorized, &mut bx); // 固定 1 个点消除平移自由度
    ...
}
```
- **单调下降保证**：$\sigma(X^{(t+1)}) \le \sigma(X^{(t)})$。无需退火、无需调步长。**这是它压倒 FR 的关键工程优势。**
- $L^w$ 秩亏 1（平移不变），固定一个点或用伪逆。
- 每轮 $O(V^2)$（稠密 rhs）+ 线性解。用 CG 且预分解 $L^w$（Cholesky 一次 $O(V^3)$ 太贵）→ 实际用 **CG + Jacobi 预条件**，每轮 $O(V^2)$。

### 2.4 大图化：Sparse Stress
全对最短路 $O(V^2)$ 存不下。两条路线：
- **Pivot MDS / PivotMDS**（Brandes–Pich 2007）：选 $k$ 个 pivot（k-center 贪心：每次选离已选集最远的点），只算 pivot 到全体的 BFS 距离（$O(kE)$），用经典 MDS 的 Nyström 近似求初始坐标。**极快，质量好，最适合当初始布局。**
- **Sparse Stress Model**（Ortmann–Klimenta–Brandes 2016）：只保留每点到 pivot 的项 + 邻接项，把 stress 项数从 $O(V^2)$ 降到 $O(kV)$，用加权补偿保持无偏。$O(kV)$ 每轮。
- **SGD stress**（Zheng–Pawar–Goodman 2018）：随机抽 pair，对每对做一步"沿连线拉/推到目标距离"的投影更新，步长 $\mu=\min(w_{ij}\eta, 1)$，$\eta$ 按 schedule 衰减。
  ```
  for each epoch with step η:
      shuffle all pairs (or sampled pairs)
      for (i,j): 
          r = (|x_i-x_j| - d_ij)/2 * unit(x_i-x_j)
          μ = min(w_ij * η, 1)
          x_i -= μ*r; x_j += μ*r
  ```
  **实现只有 20 行、收敛比 SMACOF 快、质量更好**（论文实测）。若只实现一个 stress 求解器，选这个。

### 2.5 初始化很重要
- 随机初始化 → 易缠绕、结果不确定。
- **推荐**：PivotMDS 或谱布局（拉普拉斯特征向量，用幂迭代/Lanczos）作为初值，再跑 stress。这样结果**近乎确定**（只依赖 pivot 选择，而 pivot 用确定性 k-center）。

---

## 3. 多级方法（Multilevel）—— 大图的唯一正解

思路：粗化 → 在最粗层布局 → 逐层细化并局部优化。使全局结构在小图上确定，避免大图陷局部最优。

```
fn multilevel(g: Graph) -> Coords {
    let mut levels = vec![g];
    while levels.last().len() > THRESHOLD {
        levels.push(coarsen(levels.last()));
    }
    let mut pos = layout_small(levels.last());     // 最粗层随便跑
    for lvl in levels.iter().rev().skip(1) {
        pos = interpolate(pos, lvl);               // 把粗层坐标撒给细层
        pos = refine(lvl, pos);                    // 少量 FR/stress 迭代
    }
    pos
}
```

### 3.1 粗化策略
- **Edge collapsing / Heavy-edge matching**：求极大匹配（优先收缩权重大的边），把匹配的两点合并。$O(E)$。ELK/FMMM 用之。
- **Maximal Independent Set (MIS) filtration**（Hu 2005 / sfdp）：取极大独立集作为下一层顶点，距离用 3-hop 近似。收缩率更稳定（~1/3）。
- **Solar system / galaxy 划分**（FMMM）：选 "太阳"节点，把周围 2-hop 内的节点归给它，形成 solar system 收缩。
- 关键指标：**每层收缩率**应稳定在 0.4–0.6，否则层数不可控。

### 3.2 细化
- 插值：合并节点的坐标 + 小随机扰动（避免完全重合导致斥力爆炸）。
- 每层只跑少量迭代（FR 温度按层重置为该层理想边长的一个比例）。
- 总复杂度：$O(V\log V)$（BH）或 $O(V + E)$ 每层，层数 $O(\log V)$。

### 3.3 多级 + Stress
- Hu 2005 (`sfdp`) 是多级 FR + BH。
- **Maxent-stress**（Gansner–Hu–North 2013）：把 stress 与最大熵原则结合，只用短距离项 + 熵项撑开远处，避免全对最短路。质量接近全 stress，复杂度近线性。这是"大图 organic"当前最佳权衡之一。

---

## 4. 节点重叠消除（真实节点有尺寸）

力导向/stress 把节点当质点，落地必须消重叠：

1. **PRISM**（Gansner–Hu 2009）：Delaunay 三角化 → 对每条三角边施加"至少分开"的比例约束 → stress 式求解 → 迭代直到无重叠。质量最好（保持相对位置/mental map），$O(V\log V)$ 每轮。**推荐。**
2. **VPSC / 分离约束**（Dwyer–Marriott–Stuckey 2006）：x、y 交替求解一维最小位移分离（见 07 篇），$O(V\log V)$。可与 stress 联合（IPSEP-COLA）。
3. **Scan-line + push**：按 x 扫描发现重叠就推开。快但破坏结构、易震荡。
4. **在力模型里加斥力项**：简单但只能"缓解"，无法保证零重叠。

**架构建议**：把"消重叠"做成独立的 post-stage（yFiles 的 `RemoveOverlapsStage` 即此），输入=坐标+尺寸，输出=坐标；不与布局算法耦合。

---

## 5. 其它 organic 变体（yFiles 有对应能力）

- **Clustering-aware organic**：先社区检测（Louvain / label propagation / betweenness），把社区当超点布局，再展开——本质是"语义驱动的一层多级"。
- **Circular layout**：把双连通分量各自排成圆，分量间用树布局连接（yFiles `CircularLayout`）。适合网络拓扑环。
- **Substructure detection**（yFiles Organic 的杀手特性）：识别**星形（star）**、**链（chain）**、**环（cycle）**、**并行结构**，对这些子结构用专门的精确布局（星形放辐射状、链拉直），其余用力导向。视觉提升巨大且实现不难：
  ```
  detect_stars: 度=1 的叶子挂在同一个中心 → 把叶子均匀铺在中心周围扇形
  detect_chains: 度=2 的路径 → 拉成直线/圆弧
  detect_cycles: 无弦环 → 排成正多边形
  ```
  这些子结构在布局主循环中被"冻结成刚体"（rigid body），只优化刚体的位置和旋转 ⇒ 自由度大减，收敛更快。
- **Interactive/organic 增量**：持续跑的物理模拟，用户拖动=施加外力。需要"分层锁定"（未动的区域降温），见 07 篇。

---

## 6. 参数标定与陷阱

| 参数 | 含义 | 经验值/说明 |
|---|---|---|
| 理想边长 $k$ | 决定整体尺度 | ≈ 平均节点直径 × 1.5–3；或 $\sqrt{\text{area}/V}$ |
| 迭代次数 | FR: 50–300；SGD: 15–30 epoch | 早停：位移中位数 < ε |
| BH $\theta$ | 精度/速度 | 0.9 平衡，1.2 快，0.5 精 |
| 权重 $w_{ij}$ | stress 中的距离偏好 | $d^{-2}$ 标准；$d^{-1}$ 更重视远距离 |
| pivot 数 $k$ | sparse stress | 50–200，与 $V$ 弱相关 |

**陷阱清单**：
- 完全重合的点 → 力为 NaN。必须对 $d=0$ 加 $\epsilon$ 抖动（且抖动要用**确定性 PRNG + 固定种子**）。
- 不连通图：$d_{ij}=\infty$。做法：分量各自布局 + 装箱（packing），或给跨分量对赋一个大常数距离。
- **确定性**：随机初始化、随机 pair 抽样、四叉树遍历序都要固定种子/固定序。要求"同输入同输出"就必须把 PRNG 作为显式参数传递，禁止全局 rng。
- 悬挂叶子（度 1）大量存在时 stress 会把它们堆在一起 → 先摘叶子布局主体，再把叶子扇形铺开（Graphviz 的 `-Gmode=hier` 类技巧 / "leaf bundling"）。

---

## 7. 装箱（Component Packing）

多个连通分量各自布局后要拼在一起：
- **多边形近似 + 贪心装箱**：算每个分量的凸包/包围盒，按面积降序用 "first-fit decreasing on shelves" 或 **多边形装箱**（Freivalds–Dogrusoz–Kikusts 2002 的 polyomino packing）。
- 目标：总包围盒接近给定长宽比。
- yFiles `ComponentLayout` 提供多种 style（rows / packed rectangles / packed circles / nested rows）。
- 实现优先级：先 rows/grid（10 行代码），再 polyomino。

---

## 8. tautcore 落地建议

1. **只实现一条 organic 主线**：`PivotMDS 初始化 → SGD stress（sparse pairs）→ PRISM 消重叠 → 装箱`。全链确定性可控、无退火调参、代码量可控。
2. 需要交互实时时，另加 `ForceAtlas2 + BH + local speed` 的模拟器，与静态布局共用坐标模型。
3. **substructure detection（星/链/环）优先级高于力模型调参**：同样的工作量，视觉收益大得多。
4. 消重叠必须是独立 stage，不要写进力循环。
5. 所有随机性通过显式 `seed` 传递；测试断言 stress 值单调下降 + 最终坐标 JSON 快照。
