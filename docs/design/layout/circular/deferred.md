# Circular · 后置能力

> 父页：[architecture.md](architecture.md) · 对照：[vs-reference.md](vs-reference.md) · [scope.md](scope.md)
> 状态：**目标契约**（未实现必须 `Unsupported`；不是进度日记）
> 代码门闩：`crates/plotgram-layout/src/layout/circular/params.rs` `gate_m3`

M0–M3 已构成可出图默认路径（`bcc-compact` + 谱序 + balloon + interior/exterior + `circle:`）。本文钉死架构里程碑表里的 **后置** 项：目标允许、尚未消费。落地前 bind 硬失败；**禁止**静默降成 `cycle` / `interior` / 共半径。

姊妹页回答「yFiles 有什么、我们取哪几个」；本文回答「取了但还没做的，契约是什么」。

---

## 0. 怎么读

| 栏 | 含义 |
|----|------|
| **键** | DSL / `CircularParams` 原子。未注册的键走 bind unknown warning，不算落地 |
| **现行** | 作者写了会怎样（`Unsupported` / 忽略 / 键不存在） |
| **写者** | 落地后谁独占该自由度（[写权](../write-authority.md)） |
| **不做** | 落地时仍禁止的做法 |

**不**把 CompactDiskLayout / RadialGroupLayout / 独立 `radial` 注册名写进后置——那些是**另一产品**，见 §9。

---

## 1. 一览

| 能力 | plotgram 键 | yFiles | 现行 |
|------|-------------|--------|------|
| 盘内 CYCLE 变体 | `partition_style: disk` | `DISK` | `Unsupported` |
| 盘内有机 | `partition_style: organic` | `ORGANIC` | `Unsupported` |
| 密堆积盘 | `partition_style: compact-disk` | `COMPACT_DISK` | `Unsupported` |
| 自动选外弧 | `routing_policy: automatic` | `AUTOMATIC` | `Unsupported` |
| 边捆绑 | （未注册） | `edgeBundling` | 键不存在 |
| 从草图保圈序 | `from_sketch: true` / `order: from-sketch` | `fromSketchMode` | `Unsupported` |
| 关闭子圆共半径 | `place_children_on_common_radius: false` | 同名 | `Unsupported` |
| 骨架偏角上限 | `max_deviation_angle` | `maximumDeviationAngle` | **键未注册** |
| 骨架紧致因子 | `compactness_factor` | `compactnessFactor` | 键不存在 |
| 允许分区圆重叠 | `allow_overlaps` | `allowOverlaps` | 键不存在；默认禁止 |
| 星形子结构 | （未注册） | `starSubstructureStyle` | 键不存在 |
| 射线 / 水平节点标签 | （未注册） | integrated node labeling | Demand 只预留尺寸 |
| GENERIC 边标签落位 | （未注册） | `GenericLabeling` | 标签高可进 Radius / ExteriorSep；不落位 |
| 分量 packed-circle | （未注册） | `ComponentLayout` packed circle | 声明序水平装箱 |
| 端口沿边分布 | （未注册） | `PortPlacementStage` along | 端子 = 框边界朝向对方 |

架构 §11 后置行的压缩写法对应上表前七行加星形 / 射线标签；其余是同一类「未开键」。

---

## 2. 分区风格（`partition_style`）

现行只消费 `cycle`：区内全体在圆周上。另三个原子已进枚举，bind 即 `Unsupported`。

| 原子 | 几何直觉 | 写者（若落地） |
|------|----------|----------------|
| `disk` | 连向它区的点钉在圆周，其余可在盘内 | Metric `ShapeWriter`（该分区） |
| `organic` | 盘内力导；跨区点也可进内部 | 同上；**不得**调用独立 Organic 核 |
| `compact-disk` | 密堆积；宜配外弧 | 同上 |

要紧凑：先调 `node_gap` 与骨架二分。不要为了「看起来更挤」假装 `compact-disk`。

落地约束：

- 圈序仍由 Compose 写出；盘内只动**径向位置**（到圆心的距离），不重排 θ。
- `disk` 的「边界点」= 有 `Inter` / 骨架角色的成员；判定只读 Plan，Ink 不得挑选。
- 禁止每个分区嵌套跑一遍 `layout: organic` / Tree。

---

## 3. 边

### 3.1 `routing_policy: automatic`

在 `interior`（全弦）与 `exterior`（同区非邻接一律外弧）之间，按交叉 / 拥挤启发式挑哪些区内边出环。

| | |
|--|--|
| **写者** | Metric `RouteWriter`（或 Compose 预标 `edge_role` 的细分；二者只留一个） |
| **输入** | 圈序、区内边、Demand `ExteriorSep` |
| **输出** | 每条区内非邻接边：`Chord` 或 `ExteriorArc` |

落地约束：

- **禁止 Ink 选边**（architecture 反模式 2）。启发式在 RouteWriter freeze 前跑完。
- 区际 / 自环 / 邻接边仍不是外弧。
- 同一分区所有外弧同一取向（圈序方向），与 M2 `exterior` 相同。
- 确定性：平局按边声明下标；禁止 `HashMap` 序。

### 3.2 捆绑

对齐 yFiles：仅 `partition_style: cycle` 且 **非** `bcc-isolated`。外弧永不捆绑。

未注册键。落地时单开 `bundling`（或等价）布尔 / 强度，默认关。

| | |
|--|--|
| **写者** | Metric `RouteWriter` 写控制多边形；Ink 只采样 |
| **不做** | 独立 `edge_routing: circular` 当捆绑真源（v1 债）；正交图改用力导向 bundling |

### 3.3 端口沿边分布

现行：弦 / spoke 端子 = 框边界朝向对方（或对方分区圆心）。沿边多边错开后置。平行边 M2 已做法向微偏，**不是** along-port 分配。

落地若做：Plan 或 Metric 写 `AlongSpec`；Ink 不发明 slot。

---

## 4. 圈序 `from-sketch`

| 键 | 现行 |
|----|------|
| `from_sketch: true` | `Unsupported` |
| `order: from-sketch` | `Unsupported`（`CircleOrder::from_atom`） |

按输入坐标的极角排圈序，保 mental map。与 `spectral` / `bfs` / `declaration` 互斥。

| | |
|--|--|
| **写者** | Compose `CircleOrderWriter` |
| **输入** | 节点现有框中心（Contract 若无坐标 → `InvalidInput`，不静默改谱序） |
| **平局** | `decl_index` |

禁止用 Sequence `linear_arrange`（直线 MinLA）当圆序。禁止 Ink / Metric 重排。

---

## 5. 骨架旋钮

balloon 排**分区圆**已落地（M1）。下列是同一 BackboneWriter 的未消费参数。

### 5.1 `place_children_on_common_radius: false`

默认 `true`：同一父的子分区圆心共半径。`false` 更紧（子圆可以不同 D），实现更绕。现行 `false` → `Unsupported`。

落地：仍是 BackboneWriter；不得改成对作者节点跑 Tree `BalloonPlacer`。

### 5.2 `max_deviation_angle`

yFiles 默认 90°：骨架边尽量过子圆圆心，允许偏离上限。架构曾写「M2 再消费」——M2 未开键，几何已是过圆心（偏离 0）。

落地时再 bind（度数或弧度在 bind 层钉死一种）；默认 90° 与现状兼容。禁止 Ink 为「看起来更直」弯 spoke。

### 5.3 `compactness_factor` / `allow_overlaps`

未注册。先调 `node_gap` / 骨架二分 lo。`allow_overlaps: true` 若落地也不得成为默认；verifier 今日兄弟圆盘不重叠是硬不变量。

---

## 6. 星形子结构

yFiles `starSubstructureStyle` + `edgeDirectedness`：把星从 BCC 里提出来特殊画。

未注册键。落地时 Compose 增量子结构标记；Metric 只读。禁止按 `profile == er` 猜星。

---

## 7. 标签

| 能力 | 现行 | 后置 |
|------|------|------|
| 节点标签占位 | `NodeSizes`（CONSIDER）进框 / 弧长 | 射线 / 水平 integrated 政策 |
| 边标签 | 高度可 `max` 进 `Radius` 或 `ExteriorSep` | `GenericLabeling` 联合落位 |

射线标签沿半径向外；水平标签与 Tree 径向同一套问题。写者不得落到 Ink。GENERIC 是独立阶段，不是 Circular 内核发明坐标。

---

## 8. 弱连通分量装箱

现行：各分量骨架算完后，按声明序**水平**拼接，缝 = `component_gap`。

yFiles 默认 `ComponentLayout` packed circle。后置；未开键。落地是 Component pack 写者，**不要**对分量再跑一遍 BCC。

---

## 9. 明确不搬（不是后置）

这些**不会**从 `Unsupported` 变成本核功能：

| 项 | 原因 |
|----|------|
| 独立 `layout: radial` 注册名 | 树状径向 = Tree `placer: radial` / `balloon` |
| 嵌套 `layout: tree` 当骨架 | 分区树 ≠ 作者节点树 |
| CompactDiskLayout / RadialGroupLayout 产品 | 另一算法，不是 `partition_style` 的第四个同义词 |
| `nodeComparator` 任意比较器 | 圈序只有 params `order` |
| `circleIdsResult` 产品 API | debug trace 可带 partition id |
| Stage 栈当运行时配置面 | PortPlacement / GroupHiding / Subgraph / GenericLabeling 不暴露成可插拔 Stage |
| 按 `profile == state` 分支 | ADR-001 |

---

## 10. 现行失败类别

作者写出后置能力时：

| 输入 | 类别 |
|------|------|
| `partition_style` ∈ disk / organic / compact-disk | bind `Unsupported`（消息含该原子） |
| `routing_policy: automatic` | 同上 |
| `from_sketch: true` 或 `order: from-sketch` | 同上 |
| `place_children_on_common_radius: false` | 同上 |
| 未注册键（bundling、max_deviation_angle、…） | bind **unknown warning**；几何不变 |
| Ink 里按交叉改弦/外弧 | 实现 bug → `InternalInvariant`（不准变成功能） |

未知 `partitioning` / `order` 原子仍是 `InvalidInput`（拼写错误），与「知名未实现」的 `Unsupported` 分开。

---

## 11. 落地纪律

1. 先改 `gate_*` 放行，再写 Writer；不要先画再补门闩。
2. 默认值保持 yFiles 对齐：`cycle` + `interior` + 共半径 true。后置项默认关。
3. 每项至少一条 facade 测试：打开后几何与默认可区分；关闭后与今日 bit-identical。
4. 双跑仍须 bit-identical（谱 / 启发式固定步数与平局）。
5. 不在 `plotgram-layout` 里 `if er` / `if state`。
