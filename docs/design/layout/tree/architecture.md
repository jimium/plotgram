# Tree · 目标架构设计

> 状态：**现行目标架构 v1**（驱动重建；非当前能力声明）
> 日期：2026-08-15
> 引擎注册名：`tree`
> 代码落点：`crates/plotgram-layout/src/layout/tree/`（与 Hier / Sequence 同 crate；见 [ADR-006](../../adr/006-engine-io-and-crates.md)）
> 约束入口：[写权纪律](../write-authority.md) · [AGENTS.md](../../../../AGENTS.md) §1
> 证据：[05 树与径向](../../../reference/yfiles/05-树与径向布局.md) · [yFiles TreeLayout 产品条](../../../reference/yFiles-layouts-and-routing.md) · 上游 [Tree Layouts](https://docs.yfiles.com/yfiles-html/dguide/tree_layouts/) · [TreeLayout API](https://docs.yfiles.com/yfiles-html/api/TreeLayout.html) · [ISubtreePlacer](https://docs.yfiles.com/yfiles-html/api/ISubtreePlacer.html)
> 对照：[vs-reference.md](vs-reference.md)（yFiles placer 全表 → 取舍）

本文钉死 Tree 的**目标形态与跨相契约**。产品面不是「一种树算法」，而是 **递归框架 + `ISubtreePlacer`**：组织架构图、mindmap、dendrogram、目录树都是同一核换放置器，不是按图种开平行管线（ADR-001）。

姊妹页：[README](README.md) · [scope](scope.md) · [phases/](phases/README.md)。

> **当前代码现实（M1–M5 + 收口）**：Compose 抽生成林 + `placer_of` / split / assistant Processor；Metric 用 Buchheim 做 `single-layer` / `level-aligned`，`single-split-layered` 为两侧旋转 wrap，`left-right` / `bus` 为竖直总线，`double-layer` 双行交错 + 水平总线，`dendrogram` 叶底对齐，`assistant` 把标记子放到左右总线、其余在下，`compact` 对预定义装箱策略做有界搜索，`aspect-ratio` 按目标宽高比切行/列且根钉在左上角，`radial` 为 Eades 同心圆（角需求自底向上），`balloon` 为子树圆盘绕父（二分求环半径）。**DemandBoard** 在 placer 合并前 `max` 进节点尺寸下界，并把 `min_first_segment` / 树边标签抬进 `layer_gap`。Ink 只展开 `TreeRoute`。已知名未实现 placer（`single-split` / `multi-layer` / `fixed`）诚实 `Unsupported`。

---

## 0. 一句话目标

```text
生成林（根 + 子序）一次写完
  + 每个局部根选定一个 SubtreePlacer
  + 自叶向根：placer 合并 SubtreeShape、写相对坐标与连向父的 connector
  + 边几何由 placer 风格决定（正交总线 / 折线 / 直线）；Ink 只展开
  + 核心只实现 canonical TB；朝向走 OrientationStage
  + 非树边不假装是树边；径向是本核的 placer，不是第二注册名
```

**不做**：把树塞进 Sugiyama；按 `mindmap` / `org-chart` 图种分支；在 Ink 发明总线 x 或子树朝向。

---

## 1. 硬约束

| # | 约束 | 含义 |
|---|------|------|
| T1 | **单写者** | 生成树、子序、placer 指派、connector 方向、节点坐标、边拓扑各唯一写者 |
| T2 | **落笔零新决策** | Ink 不得发明子树相对位置、总线轨、根对齐、端口侧 |
| T3 | **DemandBoard** | 标签宽、节点 margin、首段最小长 → 在 placer 合并前 `max` 进形状 |
| T4 | **确定性** | 禁止 `HashMap` 迭代驱动序；子节点序 = 声明序（或显式 `child_order`）；平局 `(decl_index, NodeId)` |
| T5 | **无图种分支** | 引擎只认 `layout: tree` + typed params（ADR-001）。mindmap / 组织图差异只经 profile 展开成默认 placer |
| T6 | **Builtin 为主** | 树边几何由 placer 内建；独立 `edge_routing` **允许**（与 Sequence 相反），用于非树边或作者显式冻结节点后重路由 |
| T7 | **核保持纯粹** | 输入到 placer 递归时必须已是林。图→树归约是 Compose 的 stage，不是 Metric 里的特判 |

### 1.1 状态词

与 Hier / Sequence 相同：**目标 / 已落地 / 过渡 / 后置**。bind 成功但未消费的参数 = 未支持；未实现的 placer 必须 `Unsupported`，不得静默当成 `single-layer`。

### 1.2 与 Sequence / Hier 的边几何立场

| | Hierarchical | Sequence | Tree |
|--|--------------|----------|------|
| 默认 | 内建 Channel 正交 | **仅** Builtin 消息路由 | **内建**（由 placer 决定风格） |
| 独立 Router | 可后接 | **禁止**（`LayoutCannotDeferEdges`） | **可后接**（非树边 / 作者显式） |
| 风格写者 | `routing_style` 整图 | 消息拓扑 | **每个局部根的 placer**（非整图单一 descriptor） |

T6 的执行点：`DeferToRouter` 时本核仍写节点框与端口决议，树边 path 留空给门面 router；**不得**在 Ink 里对树边再发明折线。

---

## 2. 总体架构

### 2.1 递归框架，不是单次分层

yFiles `TreeLayout` 的算法核极薄：从叶到根，对每个局部根调用其 `ISubtreePlacer.placeSubtree`。真正的产品多样性全部在 placer。plotgram 对齐这一分层，而不是把「居中分层 / 总线 / 径向」写成三套互不相通的 `if`。

```text
                    ┌─ OrientationStage（核心只实现 TB；已有 wrap）
                    ├─ TreeReduction（Compose 内：图 → 林 + extra edges）
Domain / DSL ──────►│
  profile expand    │   TreeCore
  LayoutContract ──►│     Compose → Metric（placer 递归）→ Ink
  (无 profile 名)   │
                    └─ Normalize / Diagnostics
                         │
                         ▼
                   LayoutOutput
                     可选 EdgeRouter（仅当 edge_geometry = DeferToRouter
                     或 extra edges 显式交给 router）
```

| 层 | 输入 | 输出 | 性质 |
|----|------|------|------|
| **Compose** | Graph + `TreeParams` + 节点级 LayoutData | **`TreePlan`** | 离散：根、生成林、子序、每节点 `PlacerId`、connector 方向、树边/非树边划分 |
| **Metric** | Plan + sizes + DemandBoard | **`TreeMetric`** | 自叶向根调用 placer：节点框、connector 几何、总线轨坐标、合并后的 `SubtreeShape` |
| **Ink** | Plan + Metric | **边折线** | 纯展开 placer 已写的拓扑 + 坐标 |

与 Hier 对照：Tree **没有** 全局 rank / 层内 crossing minimization。同深度节点对齐是 **某个 placer**（`single-layer` / `level-aligned` / `dendrogram`）的局部选择，不是核不变量。`compact` 允许子树插入兄弟空隙，同深度 y 可以不同。

### 2.2 输出契约

```text
LayoutOutput
  nodes:       实体框（Metric / placer）
  edges:       树边 path 由 placer 风格展开；非树边见 §5.3
  groups:      弱 group：仅当组包含完整子树（yFiles 同款限制）；否则 InvalidInput
  decorations: 首期空。总线是边几何，不是 decoration
  diagnostics: 非树边计数、未支持 placer、params_hash
```

生命线式 decoration **不是**本核产物。树的「总线」是边路径上的共享水平/垂直段，由 placer 写坐标、Ink 画出来。

### 2.3 模块边界（目标）

```text
plotgram-layout/layout/tree/
  params.rs          # TreeParams · PlacerId · bind
  compose/           # 根选择 · 生成林 · 子序 · placer 指派 · connector 方向
  plan.rs            # TreePlan
  metric/            # 递归 place_subtree 调度
    shape.rs         # SubtreeShape
    placer/          # 各 ISubtreePlacer 实现（一文件一族）
  ink/               # 按 Plan 边拓扑 + Metric 坐标展开
  demand.rs          # 标签 / margin → 形状下界
  verify.rs

plotgram-algo/       # Buchheim first/second walk 可下沉为可单测零件
                     # OrientationStage 已有，继续复用
```

落点与 Hier 同 crate，注册经 `Registry::standard`。**禁止**为 mindmap 再注册一个 layout 名。

---

## 3. 核心 IR

### 3.1 `TreePlan`（Compose 写，下游只读）

| 字段 | 含义 |
|------|------|
| `roots` | 林根，声明序（显式 `root:` 优先，否则 indegree-0） |
| `children` / `parent` | 生成林；子列表序 = 放置序 |
| `depth` | 生成树上的深度（仅诊断 / dendrogram 层对齐需要时只读） |
| `placer_of` | 每个局部根 → `PlacerId`（缺省 = `default_placer`） |
| `tree_edge_ids` | 用作父→子的有向边 |
| `extra_edge_ids` | 环、第二父、无向、自环、非实体端点 |
| `child_connectors` | 每个子 → `ParentConnectorDirection`（placer 在递归**前**声明，见 §4） |

M0 骨架已有 roots / children / parent / depth / tree·extra ids。目标补 `placer_of` 与 `child_connectors`。

### 3.2 `SubtreeShape`（Metric / placer 写）

对齐 yFiles：子树用形状表示，而不是只记包围盒。相对位置任意；Tree 核在合并后做最终对齐。

| 字段 | 含义 | 谁写 |
|------|------|------|
| `frame` | 局部根节点框（canonical TB） | 该节点的 placer |
| `bounds` | 含子树与本地边的并（至少 AABB；M1 `single-layer` 用左右轮廓） | 该节点的 placer |
| `contour` | 可选：逐层左右包络（Buchheim / 轮廓合并） | `single-layer` 等分层 placer |
| `connector` | 连向**父**的方向 + 附着点（相对 shape） | 该节点的 placer，遵守父已声明的 `ParentConnectorDirection` |
| `bus` | 可选：本地总线轨（水平 y 或垂直 x） | `left-right` / `bus` / `double-layer` |

下游 Ink 只读 `frame` + 每条树边的 terminals / 中间轨，不回头改 shape。

### 3.3 `TreeMetric`

```text
frames:        node_id → Rect
tree_routes:   edge_id → TreeRoute   # 拓扑已在 Plan，这里是点列坐标的「骨架」
extra_policy:  Straight | Defer
```

`TreeRoute` 是 typed 骨架（`OrthoThreeSeg { mid_y }` / `VerticalBus { x, ys }` / `Straight`），不是 Ink 再猜的折点。Ink 把骨架展开成 `EdgePath`。

---

## 4. `ISubtreePlacer` 契约

这是本核与 yFiles 对齐的**主产品面**。接口语义对齐 [`ISubtreePlacer`](https://docs.yfiles.com/yfiles-html/api/ISubtreePlacer.html)，不复刻类层次。

每个 placer 必须：

1. **`determine_child_connectors(local_root)`** — 在子树被放置**之前**声明每个孩子连向本根的方向（North/South/East/West，canonical TB 下孩子默认 South）。子 placer 用它初始化自己的 connector。
2. **`place_subtree(root_shape, child_shapes)`** —
   - 只决定**相对**位置（绝对平移由调度器做）；
   - 路由本根 → 各子的边（写 `TreeRoute` 骨架）；
   - 计算并集 shape（含边占用）；
   - 按父要求的 connector 方向初始化本根连向父的 connector；
   - 返回合并后的 `SubtreeShape`。
3. 保证调用时所有后代 shape 已算完（后序）。

调度器（Metric）伪代码：

```rust
fn place(id: NodeId, plan: &TreePlan) -> SubtreeShape {
    let kids: Vec<SubtreeShape> = plan.children_of(id).iter().map(|c| place(c, plan)).collect();
    let placer = placer_for(plan.placer_of[id]);
    placer.place_subtree(node_shape(id), &kids)
}
```

**写权落点**：相对坐标、总线轨、根对齐、本地边拓扑 → **该 placer 的 `place_subtree`**（Metric 相）。Compose 只写「用哪个 placer」和 connector 方向的**输入约束**。Ink 不实现第二套放置。

相级细节：[phases/subtree-placer.md](phases/subtree-placer.md)。

### 4.1 为何不是「整图一个 routing_style」

yFiles 文档原话：树的路由风格 largely 由 subtree placer 决定。同一棵树可以根用 `single-layer`、某部门用 `left-right`、助理用 `assistant`。plotgram 允许：

```plotgram
layout: tree { placer: single-layer }     // defaultSubtreePlacer
node assistants_boss { subtree_placer: left-right }
```

节点级 `subtree_placer` 是 LayoutData，不是第二套核。未实现的 id → `Unsupported`。

---

## 5. 生成林、根、非树边

### 5.1 根选择（Compose）

1. 显式 `layout: tree { root: <id> }` → 单根；id 必须是实体节点。
2. 否则：indegree-0 的实体，声明序。
3. 若为空（整图有环）→ `InvalidInput`，不静默挑一个。
4. 未从已知根走到的分量：按声明序升为额外林根（森林合法）。

### 5.2 生成树（TreeReduction）

对齐 yFiles `TreeReductionStage` 的**架构模式**（把图变成树是 stage，树算法保持纯粹）：

- 有向边、两端皆实体、非自环：按声明序 BFS/DFS 首次访问为树边（M0 已是 BFS 首次访问）。
- 其余进 `extra_edge_ids`：第二父、回边、无向、自环。
- 子节点序 = 树边声明序。`from_sketch` / 自定义 comparator 后置。

目标：首次访问策略可参数化（`reduction: bfs | dfs`），默认 BFS 以保持 M0 行为。不要在 Metric 里再改父子关系。

### 5.3 非树边

| 策略 | 何时 | 几何 |
|------|------|------|
| **Straight**（默认 Builtin） | `edge_geometry = Builtin` | 端点直连；诊断 warning 计数 |
| **DeferToRouter** | 作者写了 `edge_routing:` | 本核不写这些 path |
| **Bundling** | 后置 | 对齐 yFiles `TreeReductionStage.edgeBundling` |

禁止：把非树边当成第二父重新放置节点。

### 5.4 多父（`allow_multi_parent`）

yFiles 把「共享同一组前驱与后继」的块认成 multi-parent，并排放置、总线连接。仅部分 placer 和谐（`single-layer` / `bus` / `left-right` / `dendrogram`）。**后置**；未开时第二父 = 非树边。

---

## 6. 参数体系

```text
1. TreeParams::default()           // placer = single-layer, orientation = TB, …
2. TreePreset.apply（仅 gaps）
3. profile expand（mindmap → 默认 placer / orientation，见 §6.3）
4. layout: tree { … }              // 覆盖
5. 节点 LayoutData（subtree_placer / assistant / child_order）
```

### 6.1 核级参数（`TreeParams`）

| 键 | 默认 | 说明 |
|----|------|------|
| `placer` | `single-layer` | **defaultSubtreePlacer**。合法原子见 §6.2；未知 → 错；已知名未实现 → `Unsupported` |
| `orientation` / `direction` | `top-to-bottom` | 四向；核心 TB，Stage 转置（已落地） |
| `node_gap` / `node_distance` | 24 | 兄弟间距下界；各 placer 可读为 `spacing` |
| `layer_gap` / `layer_distance` | 40 | 父子主轴间距；分层 placer 用 |
| `routing_style` | `orthogonal` | **仅当**当前 default placer 承认它时生效（`single-layer` 的边风格）。`left-right` 的总线不吃这个键改拓扑 |
| `root` | （indegree-0） | 显式根 |
| `preset` | `default` | `compact` / `spacious` 只改 gap |
| `allow_multi_parent` | false | 后置；true 未实现时 `Unsupported` |
| `split_policy` | `half` | 仅 `single-split-*`：`half`（声明序前半 primary）/ `alternate`；节点 `split_side` 覆盖 |
| `preferred_aspect_ratio` | `1` | `compact` / `aspect-ratio`；宽/高目标。`compact` 下 `0` 只最小化面积 |

节点级：

| 键 | 说明 |
|----|------|
| `subtree_placer` | 覆盖该局部根的 placer |
| `split_side` | `primary` / `secondary`（别名 `left` / `right`）；只对祖先链上的 `single-split-*` 生效 |
| `assistant` | bool；`assistant` / `compact` placer 用：该子走 left-right 总线 |
| `child_order` | 后置；与 `from_sketch` 互斥 |

### 6.2 placer 原子（DSL）

对齐 yFiles 类名，用 kebab-case。别名只为迁就 M0 bind。

| 原子 | yFiles 对应 | 里程碑 | 一句话 |
|------|-------------|--------|--------|
| `single-layer`（`layered` / `default` / `centered`） | `SingleLayerSubtreePlacer` | **M1** | 子水平排、父按 `root_alignment` 对齐；组织图默认 |
| `single-split-layered`（`split-layered`） | `SingleSplitSubtreePlacer` + 两侧 `LevelAligned` 对向旋转 | **M2** | **mindmap 主放置器**：根左右切开，每侧按树深层对齐 |
| `level-aligned` | `LevelAlignedSubtreePlacer` | **M2** | 同树深对齐；`single-split-layered` 的委托实现，也可单独用 |
| `left-right` | `LeftRightSubtreePlacer` | M2 | 子在竖直总线左右；文件树 / 多直属，**不是** mindmap 默认 |
| `bus` | `BusSubtreePlacer` | M2 | 子在向下总线两侧 |
| `double-layer` | `DoubleLayerSubtreePlacer` | M3 | 子分两行交错，省宽度 |
| `dendrogram` | `DendrogramSubtreePlacer` | M3 | 叶底对齐 |
| `compact` | `CompactSubtreePlacer` | **M4** | 按目标长宽比在预定义策略中搜 |
| `aspect-ratio` | `AspectRatioSubtreePlacer` | **M4** | 整棵服从长宽比；根默认左上角 |
| `assistant` | `AssistantSubtreePlacer` | **M4** | 助理走 `left-right`，其余走 `child_placer`（默认 `single-layer`） |
| `radial` | *独立算法* `RadialTreeLayout` | **M5** | 同深度同心圆；角需求自底向上；**本核 placer，不新注册名** |
| `balloon` | Balloon 变体 | **M5** | 子树圆盘绕父；二分求环半径 |
| `single-split` / `multi-layer` / `fixed` | 任意委托的 SingleSplit 等 | 后置 | 见 [vs-reference](vs-reference.md) |

M0 bind 已接受 `single-layer`（及别名 `layered` / `default` / `centered`）。M1–M5 已消费 `single-layer` / `level-aligned` / `single-split-layered` / `left-right` / `bus` / `double-layer` / `dendrogram` / `assistant` / `compact` / `aspect-ratio` / `radial` / `balloon`。其余已知名（`single-split` / `multi-layer` / `fixed`）bind 为 `Unsupported`（不是 unknown，也不得静默降级）。

### 6.3 profile 展开（编排层，非引擎）

引擎看不见 `profile:`。展开表（目标）：

| `profile` | 默认 `layout` | 建议写入的 tree 选项 |
|-----------|---------------|----------------------|
| `mindmap` | `tree` | `placer: single-split-layered`（M2 起）。整图 `orientation` 保持 TB：左右开由 placer **内部**旋转完成，不要再套一层 LTR |
| （无 / 其它）+ 显式 `layout: tree` | 作者写的 | 默认 `single-layer` + TB → 组织图 / 目录树 |

组织架构图**不是**新 profile。作者写 `layout: tree`（或将来若加 `profile: orgchart` 也只许展开参数）。禁止 `if mindmap` 出现在 `plotgram-layout`。

M2 落地前：`profile: mindmap` 只保证 `layout: tree`（今日行为）；showcase 的 mindmap 样例会先以分层树出现，这是过渡，不是目标观感。

`left-right` **不是** mindmap 主路径：那是每个节点一棵竖直总线（文件树 / 多直属）。经典导图是根一次切开、两侧分层向外长，见 §6.5。

### 6.4 `single-layer` 的子参数（M1–M3）

对齐 `SingleLayerSubtreePlacer`：

| 键 | 默认 | 说明 |
|----|------|------|
| `root_alignment` | `center` | `center` / `median` / `leading` / `trailing` / `center-of-ports`；`leading-on-bus` / `trailing-on-bus` M3 |
| `routing_style` | `orthogonal` | `orthogonal` / `straight` / `polyline`；`orthogonal-at-root` M3 |
| `min_first_segment` | 与 `layer_gap` 协调 | yFiles 紧凑提示：至少 ≥ 垂直间距，见 vs-reference |

这些键挂在 `layout: tree { … }` 上，由 **default placer** 消费。节点若换了 placer，该节点忽略不适用的键。

### 6.5 `single-split-layered`（mindmap 主放置器）

对齐 yFiles demo / API 的标准导图配置，**一等原子**，作者不必手拼两个 placer：

```text
single-split-layered ≡ SingleSplitSubtreePlacer {
  primaryPlacer:   LevelAlignedSubtreePlacer { transformation: ROTATE_LEFT  }
  secondaryPlacer: LevelAlignedSubtreePlacer { transformation: ROTATE_RIGHT }
}
```

证据：[SingleSplitSubtreePlacer](https://docs.yfiles.com/yfiles-html/api/SingleSplitSubtreePlacer.html) 文档示例即此组合；产品说明写明 *can be used for creating mind maps*。

语义（写权落点都在该 placer / 其 Processor，Ink 不参与）：

1. **只在被指派的局部根切开一次**（通常是整棵树的根）。孩子分成 primary / secondary 两套，两套都仍以**同一根节点**为局部根。
2. **切分**（Compose 可预写，placer 消费）：默认按兄弟声明序，前 `ceil(n/2)` → primary（左），其余 → secondary（右）。节点 `split_side` 覆盖。禁止 `HashMap` 序。
3. **左侧**用 `level-aligned` 在局部坐标系放置后 `ROTATE_LEFT`（canonical 的「子在父下」变成「子在父左」）；右侧对称 `ROTATE_RIGHT`。同侧、同一树深的节点共一层。
4. **Processor** 把对应的旋转 `level-aligned` 指派给该侧全部后代，所以不必给每个子孙写 `subtree_placer`。
5. 两侧 shape 在根处合并；边从根向左/右出。根本身不套整图 `orientation: left-to-right`——再套会把已旋转的枝拧到上下。

相对 `left-right`：总线 placer 让**每个**节点的孩子趴在竖轨两侧；导图要的是**根左右各一棵分层树**。两套都做，默认跟 yFiles 导图走 split-layered。

泛化的 `single-split`（任意 `primary_placer` / `secondary_placer`）后置；首期只钉死 layered 这一对委托。

局部旋转 ≠ 整图四向：整图仍只走 OrientationStage。`SubtreeTransform` 是 placer 内部把「子在父下」映到左/右的 wrap，只实现 `rotate-left` / `rotate-right`（mindmap 所需），不复制八套 Buchheim。

---


## 7. 写权与确定性

| Writer | 自由度 |
|--------|--------|
| `ForestWriter`（Compose） | 根、树边/非树边、父子 |
| `ChildOrderWriter`（Compose） | 兄弟序 |
| `PlacerAssignWriter`（Compose） | `placer_of` |
| `ConnectorDirWriter`（Compose 调 placer.determine_*） | 子 → 父 connector 方向 |
| `ShapeWriter`（Metric / 该节点 placer） | 相对坐标、`SubtreeShape`、总线轨、`TreeRoute` 骨架 |
| `OrientationWriter`（Stage） | TB → 物理四向 |
| `InkWriter` | path 点列（只展开骨架） |

确定性：

- 节点 / 边遍历一律声明序或 Plan 已排好的 `children`；
- 林根、未访问分量升根：声明序；
- placer 合并兄弟：从左到右（canonical），禁止 `HashMap` 收集孩子；
- Buchheim 的 `apportion` / `shift` 必须与 [05](../../../reference/yfiles/05-树与径向布局.md) 一致，不得「简化成每次重算包围盒」。

---

## 8. 验真

| Verifier | 最低断言 |
|----------|----------|
| Plan | 每个非根恰好一个 parent；根无 parent；`placer_of` 全覆盖；tree/extra 划分互斥且盖全边 |
| Metric | 框 finite、无 NaN；兄弟按 child order 在次轴上分离 ≥ `node_gap`（该 placer 的分离语义）；`TreeRoute` terminals 落在对应边框上 |
| Ink | path 首尾 = terminals；不发明 Plan 没有的肘点拓扑；`DeferToRouter` 时树边 path 为空 |
| Facade | 双跑 bit-identical；未知 placer 不静默降级 |

视觉门禁走 showcase：`single-layer` 的 product 必须「父在子跨度中心、镜像对称、同构子树同形」（RT 美学 2–4 条）。M0 包围盒居中**不**满足子树一致性，不得当 M1 验收。

---

## 9. 里程碑

| 里程碑 | 交付 | 验收 | 前置 |
|--------|------|------|------|
| **M0（已落地）** | 生成林 · 包围盒居中分层 · 正交三段/直线 · 四向 Stage · 未知/径向 placer 硬失败 | 简单树可出图；非树边 warning | 无 |
| **M1（已落地）** | `SubtreeShape` + Buchheim `single-layer`（thread / ancestor / shift-change）· `placer` canonical 名 · Plan.placer_of | RT 五条美学；宽节点 `distance = w/2+w/2+gap`；showcase `tree/single-layer` product 好看 | M0 |
| **M2（已落地）** | `level-aligned` + 局部 `SubtreeTransform` wrap + **`single-split-layered`** · 节点 `split_side` · mindmap profile 展开到此 placer · 顺带 `left-right` / `bus` | 根左右开、同侧同深对齐；showcase `tree/single-split-layered` mindmap product 好看；同一拓扑换 `left-right` 得到总线 | M1 |
| **M3（已落地）** | `double-layer` · `dendrogram` · `root_alignment` / `orthogonal-at-root` | 宽扇出明显变窄；聚类树叶对齐 | M1 |
| **M4（已落地）** | `assistant` · `compact`（有界策略搜索）· `aspect-ratio`（行/列切分） | 组织图助理在侧；给定 `preferred_aspect_ratio` 面积/长宽比可观测 | M2 |
| **M5（已落地）** | `radial` / `balloon` 作为本核 placer | 角需求自底向上；深树不重叠；balloon 子盘绕父 | M1 |
| **后置** | multi-parent · from-sketch · integrated labeling · extra-edge bundling · 泛化 `single-split` · `multi-layer` / `fixed` · 组含完整子树 | 显式 `Unsupported` 直至落地 | — |

---

## 10. 反模式

1. 按 `profile == mindmap` 在 layout crate 分支
2. Ink 根据「看起来像总线」发明竖轨 x
3. 为每种朝向复制一套 placer（整图用 OrientationStage；mindmap 左右用 placer 内部 `SubtreeTransform` wrap，不要第三套）
4. 用 Hier rank 模拟树深度
5. 把 Radial 注册成第二个 layout 名（必保内核表只有 `tree`）
6. Compact 用包围盒贪心冒充 yFiles 的策略搜索，却声称 `placer: compact`
7. 非树边拉节点（破坏已放置的 shape）
8. `HashMap` 收集 children 导致兄弟序抖动
9. bind 了 `placer: bus` 却按 `single-layer` 静默画
10. 在 Metric 之后再跑一遍「居中修正」推翻 placer

---

## 11. 文档关系

| 资产 | 角色 |
|------|------|
| **本文** | Tree 目标架构真源 |
| [subtree-placer](phases/subtree-placer.md) | placer 接口、connector、Shape 合并 |
| [scope](scope.md) | 能力 / 非目标 / 典型域 |
| [vs-reference](vs-reference.md) | yFiles 类与参数 → 取舍 |
| [05](../../../reference/yfiles/05-树与径向布局.md) | Buchheim / radial / balloon 算法证据 |
| M0 代码 | `layout/tree/` 骨架（包围盒，非 RT） |

---

## 12. 收敛命题

> **Tree = 生成林 + 可插拔 SubtreePlacer 的后序合并；**
> 组织图、mindmap、dendrogram 是 placer / orientation 参数，不是图种管线；
> mindmap 默认 `single-split-layered`（根一次切开 + 两侧 level-aligned），不是 `left-right` 总线；
> 边几何由 placer 写入骨架，Ink 永不发明总线与子树位置；
> 径向是本核的一种 placer，独立 EdgeRouter 只处理非树边或作者显式冻结。
