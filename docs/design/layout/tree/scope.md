# Tree · 能力范围与典型域

> 父页：[README.md](README.md) · 目标架构：[architecture.md](architecture.md)

## 1. 能力范围（做）

| 能力 | 说明 |
|------|------|
| 单根树 / 森林 | 显式 `root:` 或 indegree-0；未达分量升为额外根 |
| 子树递归放置 | 每个局部根一个 `ISubtreePlacer`；缺省 `single-layer` |
| 朝向 | 四向 `orientation`；核心 TB + OrientationStage |
| 内建边几何 | 正交折线 / 总线 / 直线，由 **placer** 决定，非整图单一 router |
| 节点级 placer | `subtree_placer` 覆盖局部根；同一棵树可混风格 |
| 非树边 | 归约为 extra；默认直连，可 `DeferToRouter` |
| 变宽节点 | `distance(u,v) = w_u/2 + w_v/2 + gap`（分层 placer） |

### 1.1 与「图种」的关系

`profile: mindmap` 只在编排层展开成 `layout: tree` + 默认 placer（目标：`single-split-layered`）。组织架构图写 `layout: tree`（默认 `single-layer` + TB）。引擎内禁止 `DiagramType` 分支。

## 2. 非目标（故意不做）

| 非目标 | 归属 / 说明 |
|--------|-------------|
| 一般有向图分层 / 减交叉 | → [Hierarchical](../hierarchical/) |
| 时序消息轴 / 生命线 | → [Sequence](../sequence/) |
| 圆环 / BCC 多环 | → [Circular](../circular/)；**不要**把 balloon 树塞进 circular |
| 独立 `MindmapLayout` / `OrgChartLayout` 注册名 | 禁止；都是 tree + placer |
| 把 Radial 做成第二个引擎注册名 | `placer: radial` 留在本核（M5） |
| 非完整子树的 group | yFiles 同款：组必须包完整子树，否则 `InvalidInput` |
| Treemap / Sunburst / Icicle | 面积编码视图，非节点-链接树核 |
| 在 Ink 做「再居中一遍」 | 推翻 placer；对称性由 Buchheim / 该 placer 保证 |

## 3. 典型域

| 域 | 为何适合 Tree | 参数直觉 |
|----|---------------|----------|
| 组织架构图 | 严格树、父居中、正交边 | `placer: single-layer`，TB；助理 → `assistant`（M4） |
| 思维导图 | 根左右各一棵分层树 | `profile: mindmap` → `single-split-layered`（M2） |
| 目录 / 文件树 | 总线下挂 | `bus` 或 `left-right` |
| 聚类 / 系统发生树 | 叶对齐 | `dendrogram` |
| 宽扇出分类 | 双行交错或紧凑搜 | `double-layer` / `compact` |
| 焦点+上下文大树 | 径向层次 | `radial` / `balloon`（M5） |

不适合：多 DAG 汇合的流程图（用 Hier）；共享多个父且要当树画（开 `allow_multi_parent` 后置，或改 Hier）。

## 4. 与产品能力的差距（设计记债）

相对 yFiles TreeLayout / RadialTreeLayout 产品面：

1. **SubtreePlacer 族** — 目标对齐主集合（见 [vs-reference](vs-reference.md)）；mindmap 钉死 `single-split-layered`。不承诺泛化 `single-split` / `Fixed` / `Grouped` 首期。
2. **Integrated labeling** — 边标签高已进 Demand `layer_gap`；完整联合求解后置。
3. **from-sketch / child_order** — 后置；M1 子序 = 声明序。
4. **multi-parent** — 后置；默认第二父为非树边。
5. **RadialTreeLayout 的交错子、链拉直、允许微重叠** — 后置；不污染分层 placer 参数。`radial` / `balloon` 几何本身已在 M5 落地。
