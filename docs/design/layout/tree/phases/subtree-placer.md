# Tree · SubtreePlacer 相级契约

> 父页：[architecture.md](../architecture.md) · 对照：[vs-reference.md](../vs-reference.md)
> 上游：[ISubtreePlacer](https://docs.yfiles.com/yfiles-html/api/ISubtreePlacer.html)

本文钉死 Metric 相里 **placer 递归** 的输入/输出与写权。不在这里枚举每个 placer 的 setter（那是 vs-reference + 该里程碑的 params）。

---

## 1. 调用序

```text
Compose
  ForestWriter          → 林
  ChildOrderWriter      → children[v] 稳定序
  PlacerAssignWriter    → placer_of[v]
  对每个局部根（自叶向根的准备）：
    placer.determine_child_connectors(v) → child_connectors[c]
      （必须在 place(c) 之前，因为 c 的 connector 朝向依赖父的声明）

Metric（后序）
  for c in children(v): shape[c] = place(c)
  shape[v] = placer_of[v].place_subtree(node_shape[v], shapes[children])

Ink
  读 TreeRoute 骨架 → EdgePath
```

yFiles 保证：调用 `placeSubtree` 时所有后代 shape 已算完。tautcore 同样：**禁止** placer 回头改已返回的孙 shape 拓扑（只允许整体平移 shape，这是合并的一部分）。

---

## 2. `determine_child_connectors`

| | |
|--|--|
| **输入** | 局部根、`children` 序、该 placer 的配置 |
| **输出** | 每个孩子 → `ParentConnectorDirection`（canonical：N/S/E/W） |
| **写者** | 该 placer（Compose 末或 Metric 初；必须 freeze 在 `place(child)` 前） |
| **默认** | `single-layer` + TB：全部孩子 `South`（连向父从孩子北侧出） |

`left-right`：左孩子 connector East、右孩子 West（总线在中间）。方向写错会导致子 placer 把整棵子树转错侧。

`single-split-layered`：primary 孩子 connector 朝西（旋转后从根左侧出），secondary 朝东。必须在 `place(child)` 前 freeze，因为两侧 `level-aligned` 要按这个方向初始化自己的 connector。

失败：孩子数与 map 不全 → `InternalInvariant`。

---

## 3. `place_subtree`

| | |
|--|--|
| **输入** | 局部根的节点盒、各子 `SubtreeShape`（已含各自子树）、父对本根要求的 connector 方向 |
| **输出** | 合并后的 `SubtreeShape` + 本根→子的 `TreeRoute` 骨架 |
| **写者** | 该 placer（Metric） |
| **相对性** | 绝对原点任意；调度器可平移。不变量是相对几何 |

必须做：

1. 按该 placer 的规则摆子 shape（只平移/对齐，不拆开子 shape 内部）。
2. 摆局部根（`root_alignment` 等只在这里写）。
3. 写本根到每个子的边骨架（正交三段、竖总线、直线…）。
4. 并集：节点盒 ∪ 子 bounds ∪ 边占用（总线也占空间，否则兄弟会压线）。
5. 按父要求的方向初始化本根 connector。

禁止：

- 按子节点 id 查全局表改其它分支；
- 把「看起来空隙大」当许可去插入非 `compact` 声明的子树；
- 返回后还让 Ink 改根 x。

---

## 4. `SubtreeShape` 最小字段

见 [architecture §3.2](../architecture.md#32-subtreeshape-metric--placer-写)。分层 placer（M1）必须有可用的左右轮廓或等价的 Buchheim prelim/mod；总线 placer（M2）AABB + connector + `bus` 轨即可。`compact`（M4）需要能检测「插入空隙」的形状，不能只用包围盒。

---

## 5. `TreeRoute` 骨架（Ink 只读）

| 变体 | 谁写 | Ink 展开 |
|------|------|----------|
| `OrthoThreeSeg { mid_y }` | `single-layer` orthogonal | 四点折线 |
| `VerticalBus { x, parent_y, child_ys }` | `left-right` / `bus` | 父→轨→各子 |
| `HorizontalBus { start, bus_y, end }` | `double-layer` | 水平总线 |
| `Straight { a, b }` | `routing_style: straight`、extra、径向根辐条 | 两点 |
| `Polyline`（弧+径向段） | `radial` 深层边 | 已采样折线 |
| `Radial { … }` | — | 未单开变体；M5 用 `Straight` / `Polyline` |

Ink 不得把 `Straight` 升成总线，也不得把 `VerticalBus` 收成三段「看起来差不多」的折线（端口侧会错）。

`DeferToRouter`：本核不写树边骨架，path 空向量。

---

## 6. `single-layer`（M1）不变量

对齐 RT 美学（[05 §1.1](../../../../reference/yfiles/05-树与径向布局.md)）：

1. 同深度 y 相同（本 placer；非全局核不变量）。
2. 父 x = 对齐规则下的子跨度位置（默认中心）。
3. 镜像树 → 镜像布局。
4. 同构子树同形（与位置无关）—— **包围盒分层做不到**，必须 Buchheim。
5. 尽量窄：轮廓合并，不是整块 AABB 并排。

`distance(w, v) = w.width/2 + v.width/2 + node_gap`。实现必须含 thread / default_ancestor / `execute_shifts`。零件可放 `tautcore-algo`，本核 placer 调用。

---

## 7. `single-split-layered`（M2）不变量

定义见 [architecture §6.5](../architecture.md)。本相只钉调用与几何：

1. `determine_child_connectors` 按 `split_side` 把孩子分成西/东两套；未标的按 `split_policy`（默认声明序前半 primary）。
2. Processor 把 `level-aligned` + `rotate-left` / `rotate-right` 指派给该侧**全部后代**（只写 `placer_of` 与 transform，不改父子）。
3. 两侧 `place_subtree` 在各自局部「子在父下」坐标系完成，再 wrap 成 canonical TB 的左/右 shape；**禁止**为左枝复制一套从右向左的 Buchheim。
4. 根框只出现一次；两侧 bounds 在根处并，且在次轴（水平）上分离 ≥ `node_gap`。
5. 同侧、同一树深的节点主轴坐标相同（`level-aligned` 语义，旋转后即同一竖列）。

`TreeRoute`：根→左/右子用水平主段（`OrthoThreeSeg` 在局部旋转后变成水平优先），不是 `VerticalBus`。Ink 仍只展开骨架。

---

## 8. 失败类别

| 情况 | 类别 |
|------|------|
| 未知 placer 原子 | bind 错（与今日一致） |
| 已知名未实现 | `Unsupported`（消息含 placer 名） |
| 根 id 不存在 / 无根有环 | `InvalidInput` |
| 组不包含完整子树 | `InvalidInput`（组支持落地时） |
| place 后框非 finite / 孩子未全放置 | `InternalInvariant` |
