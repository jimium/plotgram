# TreeLayout

> 状态：M1–M5 已落地；DemandBoard 抬 `layer_gap`；节点级 `subtree_placer` 有 `mixed/` 样例。`single-split` / `multi-layer` / `fixed` 保持 `Unsupported`
> 引擎注册名：`tree`
> 代码：`crates/plotgram-layout/src/layout/tree/`
> **目标架构真源**：[architecture.md](architecture.md)

## 签名

树（或可抽成林）上的递归子树放置。产品面是 **`ISubtreePlacer`**：组织图、mindmap、dendrogram 是同一核换放置器，不是按图种开管线。

## 基本逻辑

```text
根选定 → 生成林（非树边剥离）→ 每局部根一个 placer
  → 自叶向根合并 SubtreeShape → 边骨架由 placer 写 → Ink 展开
```

完整管线、IR、placer 契约与里程碑见 **[architecture.md](architecture.md)**。
yFiles 类与取舍见 **[vs-reference.md](vs-reference.md)**。

## 能力范围 · 非目标 · 典型域

见 [scope.md](scope.md)。摘要：

| | |
|--|--|
| **做** | 单根/森林、可插拔 subtree placer、四向、树边内建几何、非树边直连或 defer |
| **不做** | 一般 DAG 分层（→ [Hierarchical](../hierarchical/)）；时序轴（→ [Sequence](../sequence/)）；独立 MindmapLayout 注册名 |
| **典型域** | `layout: tree`（组织图默认 `single-layer`）；`profile: mindmap` → `single-split-layered`（M2） |

## 边几何

- **主路径**：本核 Builtin；风格由 **placer** 决定（正交 / 总线 / 直线），非整图单一 descriptor。
- **可选**：`edge_routing: Some` → `DeferToRouter`（非树边或作者冻结节点）。与 Sequence 相反：Tree **允许**独立 Router。
- 复杂正交不是本核去搜 Channel；placer 写不出的再 defer。

## 写权（本核）

| 自由度 | 写者 |
|--------|------|
| 根、生成林、兄弟序、`placer_of` | Compose |
| connector 方向 | Compose 调 `determine_child_connectors` |
| 节点框、SubtreeShape、总线轨、`TreeRoute` | Metric / 该节点 placer |
| path 点列 | Ink（只展开） |
| 物理朝向 | OrientationStage |

纪律全文：[写权纪律](../write-authority.md)。

## 现行参数

| 键 | 默认 | 说明 |
|----|------|------|
| `placer` | `single-layer` | M1–M4：`single-layer` / `level-aligned` / `single-split-layered` / `left-right` / `bus` / `double-layer` / `dendrogram` / `assistant` / `compact` / `aspect-ratio`；其余已知名 → Unsupported |
| `orientation` / `direction` | `top-to-bottom` | 四向 Stage |
| `node_gap` / `layer_gap` | 24 / 40 | preset `compact` / `spacious` 只改这两项，不得冒充 `placer: compact` |
| `routing_style` | `orthogonal` | `orthogonal` / `straight` / `polyline` / `orthogonal-at-root` |
| `root_alignment` | `center` | `center` / `median` / `leading` / `trailing` / `center-of-ports` / `leading-on-bus` / `trailing-on-bus` |
| `preferred_aspect_ratio` | `1` | `compact` / `aspect-ratio`；`compact` 下 `0` 表示只最小化面积 |
| `root` | indegree-0 | 显式根 id |
| `split_policy` | `half` | 仅 `single-split-*` |

目标参数与 placer 原子表见 [architecture §6](architecture.md)。

## 相关阅读

| 文档 | 用途 |
|------|------|
| **[architecture.md](architecture.md)** | 目标架构 |
| [scope.md](scope.md) | 能力 / 非目标 |
| [vs-reference.md](vs-reference.md) | yFiles TreeLayout / ISubtreePlacer 对照 |
| [phases/](phases/README.md) | placer 调用序 |
| [05 树与径向](../../../reference/yfiles/05-树与径向布局.md) | Buchheim / radial 算法证据 |
| [yFiles 产品条](../../../reference/yFiles-layouts-and-routing.md) | Tree / RadialTree 能力边界 |
