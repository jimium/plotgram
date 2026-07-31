# 02 · 正交布局与 Topology–Shape–Metrics（TSM）

> 对应 yFiles `OrthogonalLayout` / OGDF `PlanarizationLayout` + `OrthoLayout` / GDToolkit。
> 主线论文：Tamassia 1987（最小弯折正交表示）、Batini–Nardelli–Tamassia 1986（TSM 框架）、
> Fößmeier–Kaufmann 1996（Kandinsky，任意度节点）、Bridgeman et al. 1997（TurnRegularity 紧致化）。

正交布局把「所有边都是水平/竖直折线」做成一等目标。它与分层布局是**两种世界观**：
分层布局先定层再画线；正交布局先定拓扑（平面嵌入）→ 再定形状（每个弯的转向）→ 最后才定尺寸。

---

## 0. TSM 三段式总览

```
        ┌──────────────┐   写: 平面嵌入 (每个顶点的边循环序 + 外部面)
Step 1  │ Planarization│   目标: 交叉数最小
        └──────┬───────┘   交叉 → 变成"度4的假顶点(crossing dummy)"
               ▼
        ┌──────────────┐   写: 每条边段的转向序列 (left/right)、每个角的角度
Step 2  │ Orthogonaliz.│   目标: 弯折数最小 (Tamassia 的最小费用流)
        └──────┬───────┘   输出"正交表示 H"：只有形状，没有长度
               ▼
        ┌──────────────┐   写: 每条边段的长度、顶点坐标
Step 3  │ Compaction   │   目标: 总面积 / 总边长最小
        └──────────────┘
```

**这是最纯粹的"每个自由度一个写者"设计。** 交叉数在 Step 1 定死，弯折数在 Step 2 定死，Step 3 只能改尺度不能改形状。plotgram 的写权纪律实际上就是这套思想在分层管线上的移植。

---

## 1. Step 1 · Planarization（平面化）

### 1.1 平面图判定与嵌入
- **Hopcroft–Tarjan 1974**：$O(V)$ 平面性测试。
- **PQ-tree / Booth–Lueker**：$O(V)$，同时产出嵌入。
- **SPQR-tree**（Di Battista–Tamassia 1996）：表示**所有**平面嵌入的紧凑结构，是"在所有嵌入中选最优"的关键数据结构。三类节点：S（串联/环）、P（并联）、R（刚性三连通）。
- 现代实现（OGDF `BoyerMyrvold`）：$O(V)$，同时给出 Kuratowski 子图（用于诊断）。

### 1.2 非平面图 → 交叉最小化
交叉数问题 NP 难（Garey–Johnson 1983）。工业做法 **planar subgraph + edge insertion**：

```
1. 求极大平面子图 G'（贪心加边 + 平面性测试；或用 PQ-tree 的 maximal planar subgraph）
2. for e in 剩余边（按某序）:
       在 G' 的当前嵌入中，找 e 的两端之间"穿过面最少"的路径
         → 在对偶图上做 BFS（每穿一条边 = 一个交叉）
       把每个交叉替换成新的度-4 dummy 顶点，更新嵌入
3. 可选: 变嵌入的插入（fixed vs variable embedding）
```
- **固定嵌入插入**：对偶图 BFS，$O(V)$ 每条边，得到该嵌入下的最优插入。
- **可变嵌入插入**（Gutwenger–Mutzel–Weiskircher 2001）：用 SPQR-tree 在所有嵌入上求最优单边插入，仍是多项式。质量显著更好。
- **后处理 remove-reinsert（RR）**：把已插入的边逐条拆掉重插，迭代到无改进。OGDF `SubgraphPlanarizer` 的默认 pipeline，是实践中最强的启发式组合。

### 1.3 工程要点
- 交叉 dummy 在最终图里应渲染成"边跨越"（可加跨线小拱），并且**不能被当作真节点参与间距计算**。
- 连通性：先做连通分量分解，各自布局后 packing（见 05 篇末的装箱）。
- 双连通化：正交管线通常要求双连通（biconnected），需要加"虚拟边"再在末尾删除。

---

## 2. Step 2 · Orthogonalization（正交化 / 弯折最小化）

### 2.1 正交表示（Orthogonal Representation）
Tamassia 的定义：给平面嵌入的每个面 $f$，一个序列 $(e_i, s_i, a_i)$，其中 $s_i \in \{$ 边在该面上的弯折转向序列 $\}$，$a_i \in \{90°,180°,270°,360°\}$ 是顶点处的角度。合法性条件：

1. **面的角度和**：内部面 $\sum (2 - a_i/90°) = 4$；外部面 $= -4$。（等价于"绕一圈净转 4 个直角"）
2. **顶点的角度和**：每个顶点周围角度和 = 360°。
3. 一条边在两侧面上的弯折序列互为反向取反。

### 2.2 Tamassia 的最小费用流模型（**核心，值得精读**）
把"角度"和"弯折"都看成**单位流**在一个网络里流动：

- 节点集：图的顶点 $V$ ∪ 面 $F$。
- 每个图顶点 $v$ 供给 $4$ 单位流（对应 4 个直角配额）。
- 每个面 $f$ 需求 $2\deg(f) - 4$（内面）或 $2\deg(f)+4$（外面）。
- 弧：
  - $v \to f$（v 在 f 上）：容量 $\infty$（实际 ≤4），费用 0 —— 表示 v 在 f 处贡献的角数。
  - $f \to g$（f、g 相邻）：容量 $\infty$，费用 **1** —— 每单位流 = 公共边上一个弯折。
- **总费用 = 总弯折数。** 求最小费用流即得最小弯折正交表示。

复杂度：$O(V^2\log V)$（原始），后续改进到 $O(V^{7/4}\sqrt{\log V})$（Garg–Tamassia），实用实现常用 $O(V^2)$ 的 SSP/网络单纯形。

**限制**：只适用于**最大度 ≤ 4** 的图（一个顶点只有 4 个正交方向）。这是纯 Tamassia 的致命约束。

### 2.3 Kandinsky / Quasi-orthogonal（度 > 4 的解法）
Fößmeier–Kaufmann 1996：允许顶点为**盒子**，多条边接同一侧，角度可为 0°（"空角"）。
- 网络流模型扩展：加入 $\varepsilon$-容量弧表示 0° 角，并加"每条边至少 1 弯"的 bend-or-not 约束。
- 已知**Kandinsky 最小弯折是 NP 难**（Bläsius–Krug–Rutter–Wagner 2012），所以工业实现用：
  - **ILP / SAT** 求小实例最优；
  - 或 **启发式**：把大度顶点"膨胀"成一个小环（cage / vertex expansion），每个端口成为环上一个度-3 顶点 → 回到 Tamassia 可解范围，之后收缩回盒子。**这是最常用的落地技巧**：
    ```
    inflate(v with deg d) -> cycle of d dummy vertices, each carrying one original edge
    run Tamassia on inflated graph
    contract each cycle back into a box; 环上的边成为节点边界上的"端口位置"
    ```
    收缩时环的周长 → 决定节点最小尺寸，天然支持"节点尺寸由端口数决定"。
- **端口约束的天然落点**：膨胀环上 dummy 的顺序就是端口序，固定它即固定端口分配。

### 2.4 简化替代：Giotto / 基于分层的正交
若不追求最小弯折，也可跳过 Step 1–2，直接用"分层 + 正交路由"（见 01/03 篇）。工程取舍：
- TSM 路线：弯折最优、面积紧凑，但**流程语义丢失**（没有"流向"概念），且实现重（平面化 + 网络流 + SPQR）。
- 分层 + 正交路由：保留方向语义，实现渐进，产品可控性好。**架构图/流程图应选此路线**；ER 图、电路图、状态机紧凑图可考虑 TSM。

---

## 3. Step 3 · Compaction（紧致化）

给定正交表示（形状固定），赋长度使面积/边长最小。

### 3.1 一维紧致化（水平/竖直分离）
把边段按方向分成水平段集 $H$ 与竖直段集 $V$：
- 竖直段的 $x$ 坐标 ⇒ 由"水平方向的约束图"决定；水平段的 $y$ 同理。
- 构造 **约束图**：同一"面"内相邻段之间加 $x_j - x_i \ge 1$（或 ≥ 所需间距）的弧，求**最长路** ⇒ 最小坐标。$O(V)$ 的 DAG DP。
- 两个方向可独立做（因为形状已定）→ 这是"分离变量"的经典应用。

### 3.2 难点：约束图不是 DAG 怎么办
朴素做法在非"turn-regular"的正交表示上会产生环（无法确定谁在谁左边）。解决：

**Turn-Regularity（Bridgeman, Fanto, Garg, Tamassia, Vismara 1997）**
- 定义面上的 **kitty corner** 对：绕面走时旋转数（rotation）相等的两个 reflex corner（270° 角）。
- 定理：正交表示可一维紧致化为唯一最小解 ⟺ 每个面无 kitty corner（turn-regular）。
- 若非 turn-regular：对每对 kitty corner 加一条**任选方向的"saturating edge"**（把矛盾解开），把面切成 turn-regular 的子块。选哪个方向 → 启发式/枚举，影响面积。
- 之后：两方向各求最长路，$O(V)$。

**替代路线：Flow-based compaction**（Klau–Mutzel 1999）
- 建成最小费用流（对偶于 LP），可求**面积最优的一维紧致化**（在给定形状下），甚至可与形状联合优化（"Optimal Compaction of Orthogonal Grid Drawings"）。质量最好，代价是流求解。
- OGDF 的 `FlowCompaction` 即此。

### 3.3 后处理
- **段合并 / 去冗余弯**：形状允许时删掉"进出同向"的多余弯。
- **Bendpoint straightening**：在不改变拓扑与重叠约束下把 zigzag 拉直（属于合法的"展开上游"，因为不改变形状类）。
- **面积 vs 边长权衡**：LP 目标写成 $\alpha\cdot\text{area} + \beta\sum \text{edgeLen}$；实践里最小总边长的图往往比最小面积的更好看。

---

## 4. 正交布局的质量指标与已知界

| 指标 | 已知结论 |
|---|---|
| 面积 | 任意平面图有 $O(n)\times O(n)$ 正交网格画法；度 ≤4 平面图可 $0.76n^2$ 面积（Papakostas–Tollis） |
| 弯折数 | 度 ≤4 平面图（非 3-connected 三角剖分特例外）总弯折 ≤ $2n+2$；每边弯折 ≤ 2 可达 |
| 每边最多 1 弯 | 只对特定子类可行 |
| 交叉数 | NP 难；启发式 RR + 可变嵌入插入是当前实践最优 |

**实证美学研究**（Purchase 1997、Ware et al. 2002）结论对正交布局尤为重要：
1. **减少交叉**收益最大；
2. **减少弯折**次之；
3. 边长均匀、对称性收益较小但可测。
这印证了 TSM 的相位顺序（交叉 → 弯折 → 尺寸）恰好是收益递减序。

---

## 5. 与分层混用：正交分层（Orthogonal Hierarchical）

产品里最常见的形态：**层内序与坐标用 Sugiyama，边形状用正交**。
- 边只在层间空隙拐弯（H-V-H 或 V-H-V 形态）；
- 层间空隙的水平段分 track（区间图着色，见 03 篇）；
- 弯折数天然 ≤ 4（对同层邻接可能更多）。

关键约束：**节点端口方向固定**（上进下出 for top-to-bottom）→ 弯折数与形状几乎被决定，剩下的自由度只有 track 分配和 nudging。这就是为什么正交分层的实现远比 TSM 简单却能做到产品级观感。

---

## 6. 实现建议（plotgram）

1. **不要一开始上 TSM**。先做"分层 + 正交路由"（01 + 03 篇），能覆盖架构/流程/BPMN 绝大多数需求。
2. 若后续要做**紧凑正交图种**（ER、电路、状态图），按下述最小可用子集：
   - 平面性/嵌入：直接依赖成熟算法思路（Boyer–Myrvold），或简化为"用分层结果诱导一个嵌入"；
   - 度 >4：用**顶点膨胀成环**的技巧，避开 Kandinsky 的 NP 难；
   - 紧致化：先做 turn-regular 检测 + 双向最长路（$O(V)$，代码量小），面积不满意再上流模型。
3. **紧致化的约束图 = 你已有的间距系统**。若 P4 已用 VPSC（07 篇），正交紧致化可直接复用同一个求解器（都是差分约束）——这是很大的架构复用机会。
4. Turn-regularity 的 kitty-corner 检测建议实现，因为它解释了"为什么有时紧致化会把两个节点算成互不约束然后重叠"这类 bug 的根因。
