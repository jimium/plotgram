# Tree · 与 yFiles 对照

> 父页：[architecture.md](architecture.md)
> 上游：[Tree Layouts](https://docs.yfiles.com/yfiles-html/dguide/tree_layouts/) · [TreeLayout](https://docs.yfiles.com/yfiles-html/api/TreeLayout.html) · [ISubtreePlacer](https://docs.yfiles.com/yfiles-html/api/ISubtreePlacer.html)
> 算法证据：[05 树与径向](../../../reference/yfiles/05-树与径向布局.md)

学理念与能力边界，**不**复刻全量类层次。本表回答：yFiles 有什么、plotgram 取哪几个、为什么。

---

## 1. 产品结构对照

| yFiles | plotgram 目标 |
|--------|----------------|
| `TreeLayout` 核 + `defaultSubtreePlacer` + 节点级 `subtreePlacers` | `layout: tree` + `placer` + 节点 `subtree_placer` |
| `RadialTreeLayout`（**独立算法**） | 本核 `placer: radial`（不新注册名；必保内核只有 `tree`） |
| `TreeReductionStage` | Compose 内生成林；非树边 Straight 或 `DeferToRouter` |
| `OrientationStage` | 已有 `plotgram_algo::orientation` wrap |
| `ITreeLayoutPortAssigner` | M1：边侧由 connector 方向决定；沿边分布后置 |
| `fromSketchMode` + `IFromSketchSubtreePlacer` | 后置 |
| `allowMultiParent` + `MultiParentDescriptor` | 后置 |
| `ComponentLayout` / `SelfLoopRouter` / `ParallelEdgeRouter` | 自环/平行边进 extra；分量 = 森林根 |
| Integrated edge labeling | Demand 预留；完整联合求解后置 |

yFiles 默认 `defaultSubtreePlacer = SingleLayerSubtreePlacer`。plotgram 默认 `placer: single-layer`，与之对齐。

---

## 2. SubtreePlacer 全表

描述假定 canonical 朝向：孩子在根**下方**。

| yFiles 类 | 行为 | plotgram | 何时做 |
|-----------|------|----------|--------|
| **SingleLayerSubtreePlacer** | 子水平一排；`rootAlignment` 定父相对子的位置；边 orthogonal / polyline / straight / orthogonal-at-root | `single-layer` | **M1**（组织图默认） |
| **SingleSplitSubtreePlacer** | 孩子切成 primary/secondary 两套，对侧放置，委托两个 placer；文档点名 mind map | 泛化 `single-split` 后置 | 见下行复合 |
| **SingleSplit + 两侧 LevelAligned（ROTATE_LEFT / RIGHT）** | yFiles 导图标准配置（API 示例原文） | **`single-split-layered`** | **M2（mindmap 默认）** |
| **LevelAlignedSubtreePlacer** | 同树深一层；`busAlignment` × `layerSpacing` ≥ `spacing` 才能紧凑；可 `transformation` | `level-aligned` | **M2**（亦是 split-layered 的委托） |
| **LeftRightSubtreePlacer** | 子在竖直总线左右；`branchCount` 多条并排总线；`placeLastOnBottom` | `left-right` | M2（文件树 / 多直属，**不是** mindmap 默认） |
| **BusSubtreePlacer** | 子在向下总线两侧 | `bus` | M2 |
| **DoubleLayerSubtreePlacer** | 子分两行交错，行内水平对齐；水平总线 | `double-layer` | **M3** |
| **DendrogramSubtreePlacer** | 同一局部根的子树底边对齐（叶往往同层） | `dendrogram` | **M3** |
| **CompactSubtreePlacer** | 动态在预定义策略里选，使子树接近 `preferredAspectRatio`；助理节点自动走 Assistant | `compact` | **M4** |
| **AspectRatioSubtreePlacer** | 整棵服从给定长宽比 | `aspect-ratio` | **M4** |
| **AssistantSubtreePlacer** | 助理 = LeftRight；其余 = `childSubtreePlacer` | `assistant` | **M4** |
| **MultiLayerSubtreePlacer** | 子分多行；`BusPlacement`；可手写 layer index | `multi-layer` | 后置（宽扇出先用 double-layer / compact） |
| **DelegatingSubtreePlacer**（旧名；HTML 现为 SingleSplit） | 与 SingleSplit 同类 | 不单列 | 并入 `single-split` |
| **FixedSubtreePlacer** | 不改子树位置，只补水平总线 | `fixed` | 后置（from-sketch） |
| **GroupedSubtreePlacer** | 按 port group 聚类，再委托其它 placer | — | 不做（无强需求；port group 是 Hier 主场） |

**明确不搬**：yFiles 里每个 placer 的全部 setter。先做表内「行为」列；setter 只在该里程碑的 product 样例需要时才进 `TreeParams`。

---

## 3. `SingleLayerSubtreePlacer` 参数（默认 placer）

| yFiles | 默认直觉 | plotgram | 备注 |
|--------|----------|----------|------|
| `rootAlignment` | center / center-of-ports | `root_alignment` | M1：`center`；M3：`leading` / `trailing` / on-bus |
| `edgeRoutingStyle` | orthogonal | `routing_style` | M0 已有 orthogonal / straight；M3：`polyline` / `orthogonal-at-root` |
| `verticalDistance` / `horizontalDistance` / `spacing` | | `layer_gap` / `node_gap` | 核级 gap，不按 placer 再搞三套名字 |
| `minimumFirstSegmentLength` | 紧凑时 ≥ verticalDistance | `min_first_segment` | M1 起；Ink 不得用这段长发明中点 |
| `transformation`（`SubtreeTransform` 八向） | canonical | 整图仍走 OrientationStage | **局部**旋转只给 `level-aligned` / `single-split-layered` 的委托：`rotate-left` / `rotate-right` wrap，不复制八套算法 |

yFiles 紧凑提示（只对正交）：`minimumFirstSegmentLength ≥ verticalDistance`；`level-aligned` 还要求 `busAlignment * layerSpacing ≥ spacing`。这些是 **placer 内约束**，调不通先改 gap，不要在 Ink 把总线往上抠。

`rootAlignment = LEADING_ON_BUS / TRAILING_ON_BUS` 与 `ORTHOGONAL_AT_ROOT` 在 multi-parent 下 yFiles 也不支持。我们后置 multi-parent；M3 已落地这两项。

---

## 4. TreeLayout 核参数

| yFiles | plotgram | 状态 |
|--------|----------|------|
| `defaultSubtreePlacer` | `placer` | 目标 |
| `layoutData.subtreePlacers` | 节点 `subtree_placer` | M2 |
| `layoutData.singleSplitSubtreePlacerPrimaryNodes` | 节点 `split_side` + `split_policy` | M2 |
| `layoutData.treeRoot` | `root` | M0 已有 |
| `layoutOrientation` | `orientation` / `direction` | M0 已有 |
| `defaultPortAssigner` | connector 方向 → 边侧 | M1 简化；沿边分布后置 |
| `nodeLabelPlacement` | 尺寸已含标签盒则 CONSIDER | 与 content measure 衔接；不搬 GENERIC |
| `edgeLabelPlacement` INTEGRATED | Demand → `layer_gap` 预留标签高 | 完整落位后置 |
| `allowMultiParent` | `allow_multi_parent` | 后置 Unsupported |
| `fromSketchMode` | `from_sketch` | 后置 |
| `layoutData.childOrder` | `child_order` | 后置；与 from-sketch 互斥 |
| `layoutData.nodeTypes` | 后置 | 同类兄弟相邻 |
| `layoutData.nodeMargins` | 节点尺寸已含 padding | 不另开 margin 键 |
| `treeReductionStage.nonTreeEdgeRouter` | extra → Straight 或 defer | M0 straight；bundling 后置 |

---

## 5. RadialTreeLayout（M5 已落地）

独立算法，但 plotgram 收成 `placer: radial`（balloon 为圆盘变体）。复用核级 `layer_gap` / `node_gap`，不另开一大组键。交错子 / 链拉直 / `allowOverlaps` 仍后置，避免污染分层 placer：

| yFiles | 含义 | 备注 |
|--------|------|------|
| `rootChoice` | 入度 0 / 最小深度 / 中心性 | 与核 `root` 分工：显式 root 优先 |
| `minimumEdgeLength` | 最小边长 | |
| `compactnessFactor` | 越大越短越慢 | |
| `childAlignmentPolicy` | PLAIN / SAME_CENTER / COMPACT / … | |
| `interleavedNodes` | 宽扇出交错 | |
| `chainStraightening` | 链拉成直线 | |
| `allowOverlaps` | 微重叠换紧凑 | 默认关 |

角宽需求必须自底向上传播（[05 §2.2](../../../reference/yfiles/05-树与径向布局.md)），否则深层重叠。

---

## 6. 算法真源（不是 yFiles API）

分层默认 placer 的几何要对齐的是 **Buchheim–Jünger–Leipert 2002**（线性时间、任意度、RT 五条美学），不是 M0 的包围盒居中。实现要点在 [05 §1](../../../reference/yfiles/05-树与径向布局.md)：thread、ancestor、`shift`/`change` 分摊。D3 `d3.tree` 同族。

`compact` 不是「把 gap 改小」：yFiles 对每个局部根试若干 aspect 配置 + 记忆化有界搜索。preset `compact` 只缩 gap，**不得**冒充 `placer: compact`。

mindmap 不是 `left-right` + 整图 LTR。yFiles 导图示例是 `SingleSplitSubtreePlacer` 配一对旋转的 `LevelAlignedSubtreePlacer`（见 [API 示例](https://docs.yfiles.com/yfiles-html/api/SingleSplitSubtreePlacer.html)）。plotgram 把这一组合收成原子 `single-split-layered`。

---

## 7. 故意缩小的范围

- 不实现 yFiles 全部 `ILayoutStage` 栈（GroupHiding、Subgraph、GenericLabeling 默认关的那些）。
- 不把 `GroupedSubtreePlacer` 或 `left-right` 总线当 mindmap 方案（导图走 `single-split-layered`）。
- 不把 Tree 做成「Hier 的 tree-layering 模式」——那是另一核，交叉最小化会把树拉宽。
