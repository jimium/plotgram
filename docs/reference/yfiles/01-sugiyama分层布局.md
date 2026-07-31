# 01 · Sugiyama 分层布局（Hierarchical Layout）算法详解

> 对应 yFiles `HierarchicLayout` / Graphviz `dot` / ELK `layered` / OGDF `SugiyamaLayout` 的共同算法骨架。
> 原始出处：Sugiyama, Tagawa, Toda, *Methods for Visual Understanding of Hierarchical System Structures*, IEEE SMC 1981。

---

## 0. 全局视角：为什么是四相（+1）

分层布局把「画一张有向图」这个 NP 难的整体问题，**贪心地切成一串各自可控的子问题**，每相只写少数几个自由度：

| 相 | 名称 | 输入 | 输出（写的自由度） | 该相的目标函数 |
|---|------|------|------------------|--------------|
| P1 | Cycle removal / acyclic | 有向图 | 每条边的「是否反向」布尔位 | 反向边权重和最小（Feedback Arc Set） |
| P2 | Layer assignment | DAG | 每个节点的 `layer: i32` | 边跨度和 / 层数 / 层宽平衡 |
| P3 | Ordering (crossing minimization) | 分层 DAG（含虚节点） | 每层内节点的 `position: usize` | 相邻层交叉数 |
| P4 | Coordinate assignment | 定序分层图 | 每个节点的 `x`（次轴坐标） | 边的平直度 / 对称性 / 总宽度 |
| P5 | Edge routing / drawing | 带坐标的分层图 | 折线控制点、端口位置 | 弯折数、间距、可读性 |

**核心纪律（与本仓库 AGENTS.md 一致）**：
- 层号只由 P2 写；层内序只由 P3 写；次轴坐标只由 P4 写；主轴坐标由 layer→y 映射函数写。
- P5 **不得**推翻 P3 的序或 P4 的坐标；如果 P5 想动，必须上提到 P4（例如把「边要直」变成 P4 的约束）。
- 违背这条的典型症状：路由阶段"为了避让"把边穿过别的节点，或者悄悄挪节点导致 P3 的交叉数失效。

**关键机制：虚节点（dummy / bend node）。**
P2 之后所有跨层数 >1 的边被切成一串长度为 1 的段，每段之间插入虚节点。此后 P3/P4 只处理"proper hierarchy"（所有边只连相邻层）。虚节点数量是复杂度的主要来源：$|V_{dummy}| = \sum_{(u,v)\in E}(|layer(v)-layer(u)|-1)$。

---

## 1. P1 · 去环（Cycle Removal / Feedback Arc Set）

### 1.1 问题
找最小权边集使反向后图无环 = **Minimum Feedback Arc Set (MFAS)**，NP 难（Karp 1972）。等价于求顶点线序使逆序边权和最小。

### 1.2 DFS 反向法（Graphviz dot 的做法）
```rust
fn acyclic_dfs(g: &Graph) -> HashSet<EdgeId> {
    let mut on_stack = bitset(); let mut done = bitset();
    let mut reversed = HashSet::new();
    for r in roots_then_rest_in_stable_order(g) {   // ← 稳定序！
        dfs(r, |e| {
            if on_stack.contains(e.target) { reversed.insert(e.id); } // back edge
        });
    }
    reversed
}
```
- 复杂度 $O(V+E)$。
- **决定性陷阱**：起点集合与邻接遍历序若来自 `HashMap`，输出不确定 → 必须显式排序（本仓库已有踩坑记录）。
- 质量：可能反向远多于最优的边数。最坏 $O(E)$ 倍差。

### 1.3 Greedy-Cycle-Removal（Eades–Lin–Smyth 1993）
效果显著优于 DFS，仍线性时间，OGDF / ELK 默认之一。

```
S_left = [], S_right = []
while G 非空:
    while 存在 sink u:  移出 u, push_front(S_right, u)
    while 存在 source u: 移出 u, push_back(S_left, u)
    if G 非空:
        u = argmax_{v} (outdeg(v) - indeg(v))    // 平局需稳定 tie-break
        移出 u, push_back(S_left, u)
order = S_left ++ S_right
反向所有在 order 中逆序的边
```
- 保证：反向边数 $\le |E|/2 - |V|/6$。
- 用桶（bucket）按 `outdeg-indeg` 组织可做到 $O(V+E)$。
- **平局 tie-break 必须确定**（按节点稳定 id 最小）——否则同一输入两次运行出不同图。

### 1.4 更强的启发式（需要更好质量时）
- **Berger–Shor**（1990）：随机化，期望保留 $\ge |E|/2 + \Omega(E/\sqrt{\Delta})$ 条边。
- **Sorting-based / Insertion**：把 MFAS 看成"最小逆序对排序"，用 KwikSort 类近似。
- **精确解**：ILP / 分支定界，只在 $|V|\lesssim 50$ 可用；cutting plane 用 cycle 不等式 $\sum_{e\in C} x_e \ge 1$。
- **实践建议**：图小（<200 边）时可用 greedy + 局部 2-opt（尝试把某节点移到别处看逆序边是否减少）。

### 1.5 工程要点
- **自环**：直接摘出，P5 单独画成小圆弧；不进 P2。
- **平行边**：合并为一条带 multiplicity 的边进入 P2/P3，P5 再散开（否则虚节点翻倍）。
- **双向对边** `a→b` 与 `b→a`：一条必被反向；P5 要保证两条不重合（本仓库已有"反向对边重合"修复记录，根因常在这里：反向标记丢失导致两条边路由到同一通道）。
- **反向位要一路保留到 P5**：绘制时箭头方向按原始方向，几何按反向后的层序。忘记还原 = 箭头画反。

---

## 2. P2 · 分层（Layer Assignment / Ranking）

给每个节点整数层 $l(v)$，要求每条边 $l(v) \ge l(u)+1$（proper DAG 约束）。

### 2.1 三个经典目标

**(a) Longest-Path Layering** —— 最少层数
```rust
l(v) = if preds(v).is_empty() { 0 } else { 1 + max(l(u) for u in preds(v)) }
```
$O(V+E)$，层数 = 最长路长度（最优高度）。缺点：**层宽极不均衡**，源节点全挤在第 0 层，虚节点多。

**(b) Coffman–Graham Layering** —— 宽度受限
给定每层最大节点数 $W$，产生高度 $\le (2-2/W)\cdot h_{opt}$ 的分层。
1. 传递归约（transitive reduction）。
2. 用 lexicographic 标号给节点排序：节点 $u$ 的键 = 其已标号后继的标号集合（降序），字典序最小者先标。
3. 从底向上按标号逆序贪心填层，层满 $W$ 或有后继在当前层则开新层。
复杂度 $O(V^2)$（含传递归约）。适合"层不能太宽"的画布约束。

**(c) 最小边跨度 Layering** —— 虚节点最少（**质量最好，dot 采用**）
$$\min \sum_{(u,v)\in E} \omega(u,v)\,(l(v)-l(u)) \quad s.t.\ l(v)-l(u)\ge \delta(u,v)$$
这是**对偶于最小费用流**的 LP，且约束矩阵是网络矩阵 → 整数最优解可由网络单纯形高效求得。

Gansner et al. 1993 的实现（`dot` 的 `rank.c`）：
```
1. tight_tree(): 取一棵所有边都"紧"（l(v)-l(u)==δ）的最大生成树
2. init_cutvalues(): 对树边算 cut value = 跨割的边权净差
3. while 存在 cut value < 0 的树边 e:
       f = 找一条能替换 e 的非树边（跨同一割，方向相反，slack 最小）
       exchange(e, f); 更新 cut values（增量 O(树路径长)）
4. normalize(); balance()   // 把可自由移动的节点放到较空的层
```
- 实际近线性；论文报告对真实图迭代次数很少。
- `δ(u,v)` 可 >1：这就是"至少跨 2 层"约束的挂点，用于 `minlen`。
- **balance 阶段**：入度=出度且有多层可选的节点，放到最空层——这是"层宽均衡"的免费收益。

### 2.2 层间距（主轴坐标）
$y(v) = \sum_{i<l(v)} (h_i + gap_i)$，其中 $h_i$ = 该层最高节点高度。
- **可变层间距**：若某两层之间要塞很多水平走的边（路由通道），gap 需按"该层间的边通道数 × 通道宽"加宽。这是 P2/P5 的耦合点——**推荐做法：P5 先做"通道需求估计"，把 demand 回写为 P2 的层间距参数，而不是 P5 自己挪节点**（本仓库"水平 demand 缝预留"即此思路）。

### 2.3 分层的扩展约束
- **同层约束（same-layer / rank=same）**：等价于把节点缩成一个超点，或加两条 $\delta=0$ 的双向约束。
- **层上界/下界（min/max/source/sink rank）**：LP 里加 $l(v)=c$ 或 $l(v)\ge c$。
- **分组（group）**：见 08 篇；分层要求组内节点层区间连续 → 转成额外的不等式或递归分层。
- **泳道（swimlane）**：泳道是**次轴分区**，与层正交，见 08 篇 partition grid。

### 2.4 工程坑
- 长边（跨 30 层）在稠密图里产生海量虚节点 → P3/P4 复杂度爆炸。缓解：把长边整段视为一个"链"对象，P3 中链只需一个序变量（ELK 有 `nodePlacement` 的 linear segment 概念，dot 的 `virtual chain` 亦然）。
- 反向边在 P2 中按反向后方向算，输出时才还原。
- 层号必须**归一化**（最小层 = 0）后再用；否则负数层导致数组下标越界。

---

## 3. P3 · 层内定序（Crossing Minimization）

### 3.1 问题难度
- 两层图（bipartite）**一侧固定另一侧排**：One-Sided Crossing Minimization，NP 难（Eades–Wormald 1994），即使树也难。
- 多层同时优化：更难。因此所有实现都用 **layer-by-layer sweep**。

### 3.2 交叉数计数
给定两层的序，跨层交叉数 = 邻接序列中的**逆序对数**：
- 朴素 $O(|E_{ij}|^2)$。
- **Barth–Jünger–Mutzel 2002 累加树法**：$O(|E|\log|V_{small}|)$，用 BIT/accumulator tree。工程上强烈建议实现这个，因为 sweep 里要反复计数。

```rust
// 计数 (layer_i 序固定, layer_j 序固定) 之间交叉
fn cross_count(south_seq: &[usize] /* 按北层序展开的南层位置流 */) -> u64 {
    let mut tree = Bit::new(next_pow2(len));
    let mut cross = 0;
    for &p in south_seq {
        cross += tree.query_greater_than(p); // 已插入且位置更大者 = 逆序
        tree.add(p, 1);
    }
    cross
}
```

### 3.3 初始序
- **BFS/DFS 序**（dot 用 BFS 的 `init_order`）：稳定、质量尚可。
- 多次随机重启取最优（OGDF 默认多 run）→ **牺牲决定性**，若要确定性就固定种子且记录。

### 3.4 单层重排启发式（sweep 的内核）

| 方法 | 规则 | 复杂度 | 质量 |
|---|---|---|---|
| **Barycenter**（Sugiyama 1981） | $b(v)=\frac{1}{deg}\sum pos(u)$，按 $b$ 排序 | $O(E + V\log V)$ | 好，最常用 |
| **Median**（Eades–Wormald 1994） | $m(v)$ = 邻居位置中位数 | 同上 | 略优于 barycenter，有 3-近似保证（在某类图上） |
| **Greedy switch** | 相邻对交换若减少交叉则交换，直到稳定 | 每轮 $O(V^2)$ 或 $O(E)$ 增量 | 局部最优，作为 barycenter 的 post-pass |
| **Greedy insert** | 逐个把节点插到交叉最少的位置 | $O(V^2)$ | 好但慢 |
| **Split**（类快排） | 选 pivot，按与 pivot 的交叉方向二分 | $O(V\log V)$ | 中等 |

**dot 的 `mincross` 组合**（工业级配方，建议照抄思路）：
```
init_order()                     // BFS
for pass in 0..MaxIter(24):
    wmedian(pass)                // 交替 forward/backward sweep 的 median 排序
    transpose(reverse = pass%4 >= 2)   // greedy switch，含"相等也交换"的抖动
    if crossings < best { save_best(); }
    if crossings == 0 { break }
restore_best()
```
细节要点：
- `wmedian` 的中位数定义：偶数邻居时按左右两侧"拉力"加权插值（dot 用 $\frac{m_l\cdot right + m_r\cdot left}{left+right}$ 形式），比简单取中位数更平滑。
- 度为 0 或无邻居的节点**保持原位**（median = -1 表示"不动"），否则会乱跑。
- `transpose` 里"交叉数相等时也交换"（reverse 模式）用于跳出平台期，但必须**每 4 轮才开**，否则不收敛。
- 保存最优解：sweep 不单调下降，必须记 best。

### 3.5 虚节点的特殊处理
- 虚节点间交叉（dummy-dummy）在 P4 会变成"长边的弯折"，视觉代价高于普通交叉 → 给它更高的交叉权重（dot 中 `virtual-virtual` 权重 8，`virtual-real` 2，`real-real` 1）。这一权重设计是可读性大幅提升的关键，**容易被忽略**。
- 同理 P4 的对齐优先级也偏向虚节点链（保证长边直）。

### 3.6 精确 / 更强方法
- ILP：变量 $x_{uv}\in\{0,1\}$（u 在 v 前）+ 传递性三角约束 + 交叉变量 $c_{uvwz}$；分支切割可解 $|V|\sim 100$ 的两层实例（Jünger–Mutzel 1997）。
- **Planarization 路线**（见 02 篇）：先求最大平面子图再插边，适合正交布局而非分层。
- 近年：**sifting**（Matuszewski et al. 1999）—— 把单个节点在整层里滑过所有位置取最优，$O(deg\cdot V)$，质量优于 barycenter，OGDF 有实现。值得实现，因为它对"长链"友好。

### 3.7 约束定序
- **顺序约束（sequence constraint）**：某些节点必须相邻/有先后 → 在排序时用受约束拓扑排序，或把 barycenter 值做"投影"（先算无约束值，再按约束链做 pool adjacent violators 投影，等价于 isotonic regression）。
- 端口约束（左出右进等）会限制虚节点的插入位置，见 08 篇。

---

## 4. P4 · 次轴坐标分配（X-Coordinate Assignment）

### 4.1 目标
在保持层内序不变的前提下，给每节点 $x$，使
1. 同层相邻节点间距 $\ge$ 最小间距（含节点宽/标签）
2. 边尽量竖直（虚节点链尽量共线）
3. 图尽量窄
4. 结构对称的子图看起来对称

这是**二次规划 / LP** 问题。三条主流路线：

### 4.2 路线 A：Priority Method（Sugiyama 原版 / 快而糙）
迭代 sweep：按层扫，每个节点想移到邻居重心位置，能否移动取决于"优先级"（度数高者优先，虚节点最高），高优先级可推低优先级。
- 简单、$O(iter\cdot V)$，但结果依赖迭代次数，易产生"锯齿"。
- 仍被用于超大图的快速模式。

### 4.3 路线 B：网络单纯形 / LP（Gansner et al. 1993，dot 默认）
把"边平直"写成绝对值目标：
$$\min \sum_{(u,v)\in E}\Omega(u,v)\,\omega(u,v)\,|x(u)-x(v)|$$
s.t. 同层相邻 $x(v_{i+1}) - x(v_i) \ge \rho(v_i,v_{i+1})$。
- $|x_u-x_v|$ 线性化：引入辅助节点，转成与 P2 同构的**最小费用流对偶** → 复用网络单纯形代码（这是 dot 设计的优雅之处：ranking 与 x-coord 用同一个求解器）。
- $\Omega$ 是边类型权重：real-real = 1, real-virtual = 2, virtual-virtual = 8（同 P3 的偏好）。
- 输出质量最好之一；对大图偏慢（网络单纯形迭代次数不可控），dot 有 `nslimit` 截断。

### 4.4 路线 C：Brandes–Köpf（2002）线性时间对齐法（**推荐首实现**）
《Fast and Simple Horizontal Coordinate Assignment》。保证：
- $O(V+E)$；
- 每条边最多 2 个弯折（长边"直到底"）；
- 结果左右/上下对称美观。

四步：
```
1. 标记 type-1 conflicts：内部段(inner segment，即 dummy-dummy 边) 与
   非内部段交叉时，非内部段让路（不允许对齐）。
2. 四遍 vertical alignment + horizontal compaction：
   方向 ∈ {down, up} × {left, right}，共 4 个候选布局
   - vertical_alignment: 每层节点尝试与其 median 邻居对齐，形成"blocks"
     （链表 align/root 表示），若与已对齐的边冲突（会交叉）则跳过
   - horizontal_compaction: 对每个 block 求 class 内最紧坐标，
     再在 block 类之间做偏移传播（sink 图上的最短路）
3. 4 个候选中取宽度最小者为基准，把另外 3 个对齐到它（左右翻转/上下翻转还原）
4. 逐节点取 4 个候选坐标的**中位数平均**（两个中间值的均值）
```
关键细节（实现者常错的点）：
- **median 邻居**：偶数邻居时 down 遍取下中位、up 遍取上中位，这样 4 遍才互补对称。
- **type-1 conflict** 的定义必须精确：内部段是两端都是 dummy 的段。检测用一次 $O(E)$ 扫描（对每层维护 $k_0,k_1$ 上界指针）。
- **type-2 conflict**（两内部段相交）在正确的 P3 里不会出现；若出现说明 P3 允许了 dummy-dummy 交叉，需在此阶段任选一条让路，否则死循环。
- compaction 用的是「块图」上的最长路（DAG 上 DP），不是通用 LP。
- 4 候选平均会略微牺牲"绝对直"，换来对称。若你更想"直"，可只取最小宽度候选（有些实现提供开关）。

### 4.5 路线 D：QP / 约束求解（现代，质量最高但重）
- **Linear-segment / IPSEP** 风格：把每条长边的所有 dummy 绑成一个变量（linear segment），目标 = $\sum w(x_u-x_v)^2$，约束 = 层内分隔不等式，用**主动集法 + 分离约束求解器（satisfy_VPSC）**求解（Dwyer/Marriott 的 VPSC，见 07 篇）。
- ELK 的 `BRANDES_KOEPF` / `LINEAR_SEGMENTS` / `NETWORK_SIMPLEX` 三个选项就分别对应 C / D 变体 / B。
- 好处：可自然加入"端口对齐""标签占位""组框内边距"等额外不等式，这正是产品级需求的落点。**如果你预见到大量额外约束，直接上 VPSC 比在 Brandes–Köpf 上打补丁更省事。**

### 4.6 层内间距 $\rho$ 的构成（易漏项清单）
$\rho(v_i,v_{i+1}) = \frac{w_i}{2}+\frac{w_{i+1}}{2} + \max(\text{node gap}, \text{label 需求}, \text{端口外伸}, \text{自环预留}, \text{边通道需求})$
- 虚节点"宽度"应等于**该边的线宽 + 边间距**，不是 0；否则两条长边贴在一起。
- 若边带标签，标签在分层布局里常实现为**尺寸等于标签的虚节点**（label dummy），这样间距自动预留 —— 这是很干净的技巧（ELK/yFiles 均如此）。

---

## 5. P5 · 边绘制（在分层框架内）

分层布局的边绘制有三种风格，选择影响前面各相的参数：

1. **Polyline（折线）**：直接连 dummy 点。最简单，但相邻长边会"发散"，需要 nudging。
2. **Orthogonal（正交）**：边只走水平/竖直段。需要在层间预留**水平通道（channel）**并做通道分配（见 03 篇）。分层 + 正交是架构图/流程图的主流形态。
3. **Spline（贝塞尔）**：Graphviz 的 `piecewise spline in a polygonal region`（Gansner 1993 §5）：
   - 先由 dummy 链和相邻节点边界构造一个**通道多边形（region）**；
   - 在该多边形内用递归"分裂-拟合"算法找不出界的 Bezier：`fit(region, p0, p1)`：先试直线，不行则在最"顶"的障碍点分裂递归。
   - 优点：视觉柔和；缺点：难以做端口精确对齐。

**正交分层的通道模型（重点，架构图核心）：**
- 层间空隙被离散成 $k$ 条水平 track；每条要"横向走一段"的边占用一条 track。
- track 数 = 该层间的"重叠区间集合"的最大团 ≈ **区间图着色**问题 → 贪心按左端点排序着色即最优（区间图是 perfect graph），$O(m\log m)$。
- 分配好 track 后回写"该层间需要 $k\cdot \text{trackGap}$ 的额外高度" → **回到 P2 调整 y**。这是唯一合法的"下游影响上游"方式：**以参数回写，而不是直接改坐标**。

---

## 6. 复杂度与规模总表

| 相 | 推荐算法 | 复杂度 | 决定性 |
|---|---|---|---|
| P1 | Greedy-Cycle-Removal | $O(V+E)$ | 需稳定 tie-break |
| P2 | Network simplex ranking | ~$O(VE)$ 最坏，实测近线性 | 是（若初始树稳定） |
| P3 | median sweep + transpose，24 轮 | $O(iter\cdot E\log V)$ | 是（固定初始序） |
| P4 | Brandes–Köpf | $O(V+E)$ | 是 |
| P5 | 区间着色 + nudging | $O(E\log E)$ | 是 |

规模经验值（单线程、release）：10k 节点 / 20k 边的分层图，纯 P1–P4 应在 ~1s 内；瓶颈通常是**虚节点数**和 P3 的轮数。优化顺序：先砍虚节点（P2 目标里加大权重 / 链聚合），再降 P3 轮数（早停：交叉数无改进连续 4 轮即停）。

---

## 7. 与 plotgram 的对照与建议

1. **写权表**照第 0 节建立，编译期就把"谁能写 x"约束住（例如坐标字段只在对应 phase 的模块可变）。
2. **P4 建议起步用 Brandes–Köpf**，把"端口/标签/组框"的额外需求先塞进层内间距 $\rho$；等约束多到打补丁时再整体换成 VPSC（路线 D），接口保持 `assign_x(order, spacing_provider) -> Coords` 不变。
3. **P3 一定要实现边类型权重（1/2/8）与 best-snapshot**，这两项是"看起来专业"的最大性价比来源。
4. **通道 demand 回写 P2** 作为唯一合法的反向影响通路，写进架构约束。
5. **确定性**：P1 的 tie-break、P3 的初始序、P4 的 4-候选合并全部要显式排序；测试里对同一输入跑两次比较 JSON 快照。
