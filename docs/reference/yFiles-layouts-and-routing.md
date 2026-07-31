# yFiles 布局算法与路由风格

> 日期：2026-07-28  
> 定位：plotgram 架构决策的**基础信息参考**（学理念与能力边界，不复刻全量栈）  
> 可读版：[layouts-and-routing.html](layouts-and-routing.html)  
> 上游产品文档：[yFiles Automatic Layouts](https://docs.yfiles.com/yfiles-html/dguide/automatic-layouts-main-chapter/)  
> 仓库纪律：[`AGENTS.md`](../../AGENTS.md) §1 · [`写权纪律`](../design/layout/write-authority.md)

---

## 0. 怎么读本文

yFiles for HTML 把自动图布局分成三类，**不要混用职责**：

| 类别 | 职责 | 节点 | 边路径 | 标签 |
|------|------|------|--------|------|
| **Layout** | 放置节点，通常同时生成边 | 写 | 通常写 | 部分集成 |
| **Edge routing**（`ILayoutStage`） | 节点冻结后的独立路由 | 不动 | 写 | 不动 |
| **Labeling** | 只摆标签 | 不动 | 不动 | 写 |

本文说的「profile」**不是** yFiles 产品专有名词，而是：各 Layout **内建路由档** + **可后接独立 Router** 的能力表。plotgram 的图种 profile / 内建路由 vs 独立路由，应对齐这一产品边界。

---

## 1. 一览

| 指标 | 数量 |
|------|------|
| 主布局风格 | 10 |
| 独立边路由器 | 6 |
| Hierarchical 内建路由档 | 4（orthogonal 默认 · polyline · octilinear · curved） |

---

## 2. 主布局算法 · 特色

### HierarchicalLayout（分层 / Sugiyama）

- **签名**：有向流、层序、最小化交叉  
- **典型域**：流程图、调用图、ER、泳道、生化通路  
- **特色**  
  - 节点分到 rank 层，多数边朝主方向  
  - 层内序优化交叉；端口 / 边组 / 增量 / group 一等公民  
  - plotgram Atlas 主路径最接近的产品族  
- **路由笔记**：默认 orthogonal；bus 靠 edge grouping；递归边另有 policy  

### OrganicLayout（力导向）

- **签名**：自然散布、显簇与对称  
- **典型域**：社交/企业网络、生物信息、网格、系统管理  
- **特色**  
  - 节点互斥、边如弹簧；布局紧凑、邻接贴近  
  - 边默认等长直线；另有 `InteractiveOrganicLayout`  
  - 适合无强方向的中大型无向图  
- **路由笔记**：内建直线；正交/曲线通常后接 `EdgeRouter` / `OrganicEdgeRouter`  

### TreeLayout（树 + 子树放置器）

- **签名**：树结构专用、高度可定制子树风格  
- **典型域**：组织树、数据流、软件结构、行政管理  
- **特色**  
  - 多种 `ISubtreePlacer`：正交、分层、压缩等  
  - 可控制边序、端口、子树朝向  
  - 非树边需预处理或换其它布局  
- **路由笔记**：路由风格 largely 由 subtree placer 决定，非整图单一 descriptor  

### RadialTreeLayout（气球 / 径向树）

- **签名**：子树绕父节点呈星/气球  
- **典型域**：层级浏览、相似规模子树、目录树  
- **特色**  
  - 子女落在父节点圆周；子树像气球  
  - 强调根到叶的径向层次感  
  - 边通常直线  
- **路由笔记**：内建直线；换风格需独立 EdgeRouter  

### OrthogonalLayout（正交构图）

- **签名**：边全正交、紧凑、少弯少交叉  
- **典型域**：UML、DB schema、VLSI、楼层规划、知识表示  
- **特色**  
  - 边仅为水平/垂直段交替  
  - 无节点重叠；适合中小稀疏图  
  - 与 Hierarchical 正交不同：**不**强调全局层流  
- **路由笔记**：路由风格即正交；定向边可配 monotonic 约束  

### CircularLayout（环 / 多环）

- **签名**：按连通分区成环，强调群组  
- **典型域**：社交网络、电信、WWW、电商拓扑  
- **特色**  
  - 单环或多环（分区盘）  
  - 可突出 group / tree 子结构  
  - 常配合边捆绑降 clutter  
- **路由笔记**：默认弦/直线；bundling 是常见风格扩展  

### RadialLayout（同心圆层）

- **签名**：节点落在绕中心的同心层上  
- **典型域**：聚类、社交、生物信息、树状径向变体  
- **特色**  
  - 多种 layering 策略（含 dendrogram）  
  - 中心–外围语义清晰  
  - 与 RadialTree 不同：不要求严格树  
- **路由笔记**：内建径向直线为主  

### SeriesParallelLayout（串并联）

- **签名**：单源单汇、边弯最少、突出主路径  
- **典型域**：电路、调用树、结构化流程图  
- **特色**  
  - 图须（或近似）串并联可分解  
  - 强调 source→sink 主方向  
  - 路径感强、弯点少  
- **路由笔记**：内建偏正交/折线，强调最少弯  

### RadialGroupLayout（仙人掌分组）

- **签名**：组成员绕组边界递归排布  
- **典型域**：深层嵌套 group、包图式浏览  
- **特色**  
  - 每组直接成员贴组边界排布  
  - 递归树感像 cactus  
  - 专为 hierarchically grouped 图  
- **路由笔记**：节点放置是主角；边风格常后处理  

### CompactDiskLayout（紧致圆盘）

- **签名**：节点塞进圆盘、最大化紧凑  
- **典型域**：集合展示、无强边语义的节点云  
- **特色**  
  - 优化圆盘内紧凑放置  
  - 弱化边几何，强调占用面积  
  - 适合一堆节点而非复杂拓扑  
- **路由笔记**：边非优化目标；需要路径时后接 Router  

---

## 3. 路由风格矩阵（Layout × Style）

图例：

| 标记 | 含义 |
|------|------|
| **内建** | 算法自带 routing descriptor / 固有几何 |
| **可后接** | 节点布局后再跑 EdgeRouter 等 |
| **有限** | 靠 grouping、placer 或变体间接得到 |
| **—** | 不适用 / 非目标 |

| Layout | Orthogonal | Polyline | Octilinear | Curved | Straight | Bus | Organic |
|--------|:----------:|:--------:|:----------:|:------:|:--------:|:---:|:-------:|
| Hierarchical | 内建 | 内建 | 内建 | 内建 | — | 有限 | 可后接 |
| Organic | 可后接 | 可后接 | 可后接 | 可后接 | 内建 | 可后接 | 内建 |
| Tree | 内建 | 内建 | 有限 | 可后接 | 内建 | 有限 | — |
| RadialTree | 可后接 | 可后接 | 可后接 | 可后接 | 内建 | — | — |
| Orthogonal | 内建 | — | — | — | — | 有限 | — |
| Circular | 可后接 | 可后接 | 可后接 | 有限 | 内建 | 有限 | 可后接 |
| Radial | 可后接 | 可后接 | 可后接 | 可后接 | 内建 | — | — |
| SeriesParallel | 内建 | 内建 | 有限 | 可后接 | 有限 | — | — |
| RadialGroup | 可后接 | 可后接 | 可后接 | 可后接 | 内建 | — | — |
| CompactDisk | 可后接 | 可后接 | 可后接 | 可后接 | 内建 | — | — |

---

## 4. 独立边路由器（节点冻结）

| 路由器 | 角色 | 风格 | 说明 |
|--------|------|------|------|
| `EdgeRouter` | 主力独立路由器 | orthogonal · octilinear · curved · bus | 节点冻结；增量重路由；可模仿 Hier/Ortho 的 monotonic 约束 |
| `OrganicEdgeRouter` | 有机曲线 | smooth curves around nodes | 绕节点平滑曲线；避重叠、减交叉 |
| `ParallelEdgeRouter` | 多边错开 | parallel multi-edges | 同对端点多边均匀错开 |
| `SelfLoopRouter` | 自环 | loops | 自环尺寸与曲率可配 |
| `BundledEdgeRouter` | 边捆绑 | edge bundles | 共享束降低 clutter、强调主流 |
| `StraightLineEdgeRouter` | 直线复位 | straight | 最简：端点直连 |

独立 Router 实现 `ILayoutStage`，可挂在任意 Layout 之后：先放节点，再换边几何。

---

## 5. 如何选（产品视角）

| 场景 | 建议 |
|------|------|
| **有主方向 / 流程** | 优先 Hierarchical（正交默认）。结构化单源单汇可试 Series-Parallel。需要泳道 / group 时 Hier 原生支持最强。 |
| **无方向网络** | Organic 看簇与对称；要正交外观则 Organic 放置 + EdgeRouter。Circular / Radial 强调环与中心外围。 |
| **严格树** | TreeLayout（正交/折线 placer）或 RadialTreeLayout（气球）。非树边先剥离或改 Hier/Organic。 |
| **已有节点坐标** | 只用 EdgeRouter / OrganicEdgeRouter / Bundled… 增量 Scope 可只重路由新增边，保 mental map。 |

---

## 6. 对 plotgram 的映射提示

- **轻量子集**：Hier ≈ Atlas 三相（组合 → 度量 → Ink）；Tree / Sequence / Circular 为其它内核。  
- **Hier 正交真源**：Channel Ink（独立正交 OVG 已删）。  
- yFiles 的「Layout 内建多种 routing style + 可后接 Router」双模式，对应产品上的 profile：**内建路由** vs **节点冻结后的独立路由**。  
- **不必复刻全表**；选算法时先问：主方向是什么？每个几何自由度的写者是谁？（见写权纪律）

---

## 7. 延伸阅读

- [yFiles Automatic Layouts](https://docs.yfiles.com/yfiles-html/dguide/automatic-layouts-main-chapter/)  
- [Hierarchical Layout](https://docs.yfiles.com/yfiles-html/dguide/hierarchical_layout/)  
- [Edge Routing](https://docs.yfiles.com/yfiles-html/dguide/polyline_router/)  
- 仓库：[`AGENTS.md`](../../AGENTS.md) §1 · [`写权纪律`](../design/layout/write-authority.md) · [`layout/`](../design/layout/README.md) · [`archive/atlas`](../archive/atlas/README.md)
