# tree 样例

按 **SubtreePlacer 族**分桶（见上级 [README](../README.md) 与 [tree 架构](../../../docs/design/layout/tree/architecture.md)）。

目录名 = 布局内核注册名 `tree`，**不是**图种。禁止 `showcase/mindmap/`。

| 目录 | 对应 placer | 状态 |
|------|-------------|------|
| [`single-layer/`](single-layer/) | yFiles `SingleLayerSubtreePlacer`（默认） | M1 Buchheim；组织图主路径 |
| [`single-split-layered/`](single-split-layered/) | `SingleSplit` + 两侧旋转 `LevelAligned` | **M2 mindmap 主路径** |
| [`left-right/`](left-right/) | `LeftRight` / `Bus` | M2；文件树，不是导图默认 |
| [`double-layer/`](double-layer/) | `DoubleLayerSubtreePlacer` | **M3**；宽扇出交错两行 |
| [`dendrogram/`](dendrogram/) | `DendrogramSubtreePlacer` | **M3**；叶底对齐 |
| [`assistant/`](assistant/) | `AssistantSubtreePlacer` | **M4**；助理在侧 |
| [`compact/`](compact/) | `CompactSubtreePlacer` | **M4**；有界策略搜索 |
| [`aspect-ratio/`](aspect-ratio/) | `AspectRatioSubtreePlacer` | **M4**；按长宽比切行/列 |
| [`radial/`](radial/) | `placer: radial` | **M5**；Eades 同心圆 |
| [`balloon/`](balloon/) | `placer: balloon` | **M5**；子树圆盘绕父 |
| [`mixed/`](mixed/) | 节点级 `subtree_placer` | 默认分层 + 局部 `left-right` |

角色前缀仍为 `smoke.` / `product.` / `demo.` / `stress.` / `mech.`。

### `single-layer/`

| 文件 | 主题 |
|------|------|
| [`smoke.root-branches.pgm`](single-layer/smoke.root-branches.pgm) | 单根三子 |
| [`smoke.orientation-ltr.pgm`](single-layer/smoke.orientation-ltr.pgm) | 同一拓扑 `left-to-right` |
| [`product.org-chart.pgm`](single-layer/product.org-chart.pgm) | 组织架构（父居中、镜像对称） |
| [`mech.layout-params.pgm`](single-layer/mech.layout-params.pgm) | `placer` / gap / routing_style 声明 |
| [`mech.root-alignment.pgm`](single-layer/mech.root-alignment.pgm) | `root_alignment: leading` |
| [`mech.orthogonal-at-root.pgm`](single-layer/mech.orthogonal-at-root.pgm) | `routing_style: orthogonal-at-root` |
| [`stress.asymmetric-fanout.pgm`](single-layer/stress.asymmetric-fanout.pgm) | 不对称扇出 + 深链 |

### `single-split-layered/`

`profile: mindmap` 填空默认 `placer: single-split-layered`（作者写了 `placer:` 则保留）。整图 `orientation` 保持 TB。

| 文件 | 主题 |
|------|------|
| [`product.tech-stack.pgm`](single-split-layered/product.tech-stack.pgm) | 分类树，根左右开 |
| [`demo.knowledge-map.pgm`](single-split-layered/demo.knowledge-map.pgm) | 多层知识树 |

### `left-right/`

| 文件 | 主题 |
|------|------|
| [`smoke.file-tree.pgm`](left-right/smoke.file-tree.pgm) | `placer: left-right` 竖直总线 |
| [`smoke.bus.pgm`](left-right/smoke.bus.pgm) | `placer: bus` 末子朝下 |

### `double-layer/`

| 文件 | 主题 |
|------|------|
| [`smoke.wide-fanout.pgm`](double-layer/smoke.wide-fanout.pgm) | 八子交错两行 |

### `dendrogram/`

| 文件 | 主题 |
|------|------|
| [`product.taxonomy.pgm`](dendrogram/product.taxonomy.pgm) | 深度不齐的分类树，叶底对齐 |

### `assistant/`

| 文件 | 主题 |
|------|------|
| [`product.org-assistants.pgm`](assistant/product.org-assistants.pgm) | 组织图：助理在侧、主链在下 |

### `compact/`

| 文件 | 主题 |
|------|------|
| [`smoke.wide-fanout.pgm`](compact/smoke.wide-fanout.pgm) | 八子；策略搜索接近正方形 |

### `aspect-ratio/`

| 文件 | 主题 |
|------|------|
| [`smoke.wide-fanout.pgm`](aspect-ratio/smoke.wide-fanout.pgm) | 八子；按长宽比切行/列，根在左上角 |

### `radial/`

| 文件 | 主题 |
|------|------|
| [`product.taxonomy.pgm`](radial/product.taxonomy.pgm) | 分类树；同深度同心圆 |

### `balloon/`

| 文件 | 主题 |
|------|------|
| [`smoke.uneven-subtrees.pgm`](balloon/smoke.uneven-subtrees.pgm) | 不对称子树；大盘占更大圆心角 |

### `mixed/`

| 文件 | 主题 |
|------|------|
| [`product.org-mixed-placers.pgm`](mixed/product.org-mixed-placers.pgm) | 组织图默认分层；`ops` 局部 `left-right` |
