# 07 · 约束布局、增量布局与 Mental Map

> 对应 yFiles `LayoutConstraints`（sequence/layer constraints）、`FixNodeLayoutStage`、
> `InteractiveOrganicLayout`、`PartialLayout`、"from sketch" 模式。
> 主线论文：Dwyer–Koren–Marriott 2006（**IPSEP-COLA**）、Dwyer–Marriott–Stuckey 2006（**VPSC**）、
> Misue–Eades–Lai–Sugiyama 1995（**Mental Map** 原文）、Brandes–Wagner 1997（Bayesian 动态布局）、
> Dwyer et al. 2008（Dunnart：约束图编辑）、Diehl–Görg 2002（foresighted layout / 动画）。

自动布局在产品里从不是"一次算完"。用户会拖、会加节点、会要求"这两个必须并排"。本篇讲怎么把这些做成**一等约束**而不是补丁。

---

## 1. 核心武器：VPSC（Variable Placement with Separation Constraints）

**这是约束布局的基础设施，建议作为 plotgram 的公共求解器。**

### 1.1 问题
$$\min \sum_i w_i (x_i - x_i^{des})^2 \quad\text{s.t.}\quad x_j - x_i \ge g_{ij}\ \forall (i,j)\in C$$
即"在满足一组一维分隔约束的前提下，让每个变量尽量靠近其期望位置"。

用途极广：
- 消除节点重叠（x 方向：$x_j - x_i \ge (w_i+w_j)/2$）；
- 层内间距（01 篇 P4）；
- 正交紧致化（02 篇 Step 3）；
- nudging（03 篇 L4）；
- 对齐约束（等式 = 两个不等式）；
- 组框内边距、泳道边界。

### 1.2 算法（Dwyer–Marriott–Stuckey 2006）
活动集法 + **块合并（block merging）**：
```
1. 每个变量初始为独立 block，位置 = desired
2. 找违反的约束 c=(i→j)：把 i、j 所在 block 合并成一个 block，
   block 内相对位移固定（由 c 的 gap 决定），block 位置 = 加权最优点
   （block 最优位置 = Σ w_k(des_k - offset_k) / Σ w_k）
3. 合并可能引入新违反 → 重复
4. 若 block 内某约束出现"负 Lagrange 乘子"（想拉开），拆分 block（split）
5. 无违反且无负乘子 → 最优
```
- 复杂度：$O(|C|\log|C|)$ 摊还（优先队列 + 并查集式 block），实践极快。
- **保证全局最优**（凸 QP + 线性约束）。
- 约束图有环（矛盾约束）时需检测并报告不可行 —— 实践中按优先级丢弃冲突约束。

### 1.3 二维：交替 x/y（Dwyer 的 "satisfy_VPSC"）
2D 重叠消除不是凸问题。做法：
```
loop {
   生成 x 方向分隔约束（对每对重叠矩形，选"位移更小"的方向 x 或 y）
   solve_VPSC(x)
   同理 y
} until 无重叠
```
- 每对重叠矩形只在**一个**方向加约束（选代价小者）→ 这是唯一的启发式成分。
- 通常 2–5 轮收敛。
- 用扫描线生成约束（$O(n\log n)$）而不是 $O(n^2)$ 枚举。

---

## 2. 约束布局：IPSEP-COLA

Dwyer–Koren–Marriott 2006 *IPSep-CoLa: Incremental Procedure for Separation Constraint Layout*。
把 stress majorization（04 篇 §2.3）与 VPSC 结合：

```
每轮 SMACOF 迭代：
   1. 计算无约束的理想位置（解 L^w x = L^Z z）
   2. 把它作为 desired，跑 VPSC 投影到约束可行域
   3. 保证 stress 仍单调下降（投影是非扩张映射，配合 majorization 可证收敛）
```
支持的约束类型：
- **分隔（separation）**：$x_j - x_i \ge g$。
- **对齐（alignment）**：$x_i = x_j$（同轴），实现为一对分隔约束或引入"对齐变量"。
- **层次/流向（directed edge constraint）**：$y_v - y_u \ge g$ 对每条有向边 ⇒ **在力导向图上强加流向**。这是 IPSEP 最漂亮的能力：无需分层就得到"总体向下流"的 organic 图。
- **不重叠**：交替 x/y 生成。
- **包含（containment）**：组内节点在组框内 ⇒ 组框边界作为变量参与。
- **页面边界**。

**这是"organic + 语义约束"的最佳落点**，也是 WebCola 的核心。若 plotgram 要做"知识图谱但要体现层级"，此路线优于强行分层。

---

## 3. yFiles 风格的离散约束（分层布局里的）

分层布局的约束不是几何而是**离散序**：

| 约束 | 语义 | 实现落点 |
|---|---|---|
| **Layer constraint** | 同层 / 之上 / 固定层号 / 最上层 / 最下层 | P2 分层 LP 的额外不等式；同层 = 合并超点 |
| **Sequence constraint** | 层内顺序：a 在 b 左边 / 固定位次 / 头尾 | P3 定序时的受约束排序（见下） |
| **Port constraint** | 边必须从节点某侧/某点出入 | P3 交叉计数 + P4 端口偏移 + 路由端口（见 08 篇） |
| **Edge grouping** | 一组边共享同一端口/汇聚 | 端口分配 + bus routing |
| **Critical path** | 指定路径应画直、优先 | 提高该路径边权（P2 与 P4 的目标权重） |

### 3.1 受约束的层内排序
P3 用 barycenter 排序，加约束后需**保序投影**：
1. 无约束地算出每个节点的 barycenter 值；
2. 把序约束视为偏序，对每个约束链做 **PAV（Pool Adjacent Violators）** 合并：违反顺序的相邻组合并为一个"超组"，取加权平均值；
3. 按最终值排序，组内按约束序展开。

这正是 isotonic regression 的算法，$O(n)$，**保证在满足约束下最接近无约束解**。比"排完再冒泡修"干净得多。

### 3.2 关键路径拉直（critical path）
给指定路径上的边设置极高权重（P2 的边跨度权重 + P4 的对齐权重）→ 该路径自然变成直线主干。**不要**用"事后把这些节点强制对齐"，那会破坏别处间距。

---

## 4. 增量 / 局部布局（Partial Layout）

场景：图上加了几个新节点，已有部分不能动（或尽量少动）。

### 4.1 yFiles `PartialLayout` 的模型
- 节点分为 **fixed**（不动）与 **partial**（待布局）。
- 步骤：
  1. 把 fixed 部分当障碍；
  2. 对 partial 节点做**子图分组**（连通性 + 与 fixed 的连接关系）；
  3. 为每组找放置位置（候选：靠近其 fixed 邻居的重心、空白区域；用网格/象限搜索 + 打分）；
  4. 组内用常规布局（分层/organic）；
  5. 边路由（只重路由受影响边）。
- 打分项：与 fixed 邻居的距离、与已放组的重叠、方向一致性（若整图自上而下，新节点应遵循）。

### 4.2 "From Sketch" 模式（用户草图作为提示）
用户已手拖大致位置，希望"整理但保持我的意图"：
- 分层布局：用当前 y 坐标推导层号（聚类 y 值），用当前 x 排序推导层内序 ⇒ 后续正常跑 P4。
  这是 ELK 的 `INTERACTIVE` 策略 / yFiles 的 `fromSketchMode`。**实现成本低、用户价值极高，强烈推荐**。
- Organic：用当前坐标作为 stress 的初值（而非随机），并给"离原位远"加惩罚：
  $$\sigma'(X)=\sigma(X)+\alpha\sum_i \lVert x_i - x_i^{orig}\rVert^2$$
  $\alpha$ 即"保持原样的强度"，做成用户滑杆。

### 4.3 增量的稳定性技巧
- **锚定（anchoring）**：固定图的"重心"或用户选中节点（yFiles `FixNodeLayoutStage`），避免整图平移导致视口跳变。
- **hysteresis**：路由/序在两方案代价接近时保持旧方案（03 篇 §5）。
- **温度分区**：交互式力导向中，离改动点远的节点温度置 0（不动）。

---

## 5. Mental Map（心智地图）保持

Misue–Eades–Lai–Sugiyama 1995 定义三个需保持的性质：
1. **Orthogonal ordering**：任意两点的上下/左右关系不变（a 在 b 左，之后仍在左）。
2. **Proximity relations**：邻近关系不变（Delaunay/Voronoi 邻接不变）。
3. **Topology**（层次/聚类结构）不变。

### 5.1 可实施的手段
- **正交序保持**：把"a 在 b 左"作为分隔约束喂 VPSC ⇒ 直接保证。这是 PRISM/VPSC 消重叠优于 scanline-push 的原因。
- **邻近保持**：PRISM（04 篇 §4.1）在 Delaunay 三角化上施加约束，天然保邻近。
- **变化最小化**：目标里加 $\alpha\lVert X-X_{old}\rVert^2$。
- **动画过渡**：即使位置变了，平滑动画（staged animation：淡出→移动→淡入；或 Diehl–Görg 的 foresighted layout 多帧联合优化）也能大幅降低认知负担。

### 5.2 稳定性度量（可自动断言）
- **位移**：$\frac{1}{n}\sum\lVert x_i^{new}-x_i^{old}\rVert$，归一化到图直径。
- **正交序违反数**：对点对统计 sign(dx)/sign(dy) 变化数（$O(n^2)$，抽样即可）。
- **邻接保持率**：k-NN 集合的 Jaccard 相似度。

把这三项加入回归基准 ⇒ "改了算法会不会让用户的图大变"变成可测的（见 11 篇）。

---

## 6. 动态图布局（时间序列）

- **离线（offline / foresighted）**：已知全部帧 → 联合优化。把所有帧的节点副本放进一个超级图，同一节点跨帧副本之间加"弹簧"（时间正则项），一次求解。质量最好（Diehl–Görg 2002；Brandes–Corman 2003 *Visual Unrolling*）。
- **在线（online）**：只知过去 → 用前帧作初值 + 位移惩罚。
- **Supergraph 方案**：对所有帧的并图布局一次，每帧只显示子集 ⇒ 完美稳定但单帧质量差。折中：并图布局作为"锚"，每帧在锚附近微调。

---

## 7. 与 plotgram 的对接建议

1. **把 VPSC 做成 crate 内的公共求解器**（`plotgram-engine` 的 `solver` 模块），供：层内间距、消重叠、nudging、正交紧致化、组框边距共用。这是本文库里**投入产出比最高的单一建议**。
2. **约束分两类明确区分**：
   - *离散约束*（层、序、端口）→ 影响 P2/P3，必须在对应相实现（受约束排序用 PAV 投影）；
   - *几何约束*（对齐、间距、包含、边界）→ 统一进 VPSC。
   不要混：几何约束去改层号 = 破坏写权。
3. **from-sketch 模式优先级高**：从当前坐标反推层与序，成本低、用户感知强。
4. **稳定性作为一等指标**：任何"改善观感"的改动都要报告位移与正交序违反数，防止改一个图种毁掉另一个（本仓库已有 route_snapshot / benchmarks 基础设施，接上这三个指标即可）。
5. **不可行约束的处理策略要显式**：给约束定优先级，冲突时按优先级丢弃并在诊断输出里报告，而不是静默忽略或崩溃。
