# Circular · 分区与圈序

> 父页：[architecture.md](../architecture.md) · 对照：[vs-reference.md](../vs-reference.md)
> 证据：[14 §3.2](../../../../reference/yfiles/14-图论与优化工具箱.md) · [05 §4.1](../../../../reference/yfiles/05-树与径向布局.md)

本文钉死 Compose 里 **分区 + 圈序** 的输入/输出与写权。半径与骨架见 [backbone-and-ink](backbone-and-ink.md)。

---

## 1. 弱连通分量

| | |
|--|--|
| **输入** | 无向化后的实体图（忽略方向、自环仍占端点） |
| **输出** | `components[]`，每分量一张诱导子图 |
| **写者** | `ComponentWriter` |
| **序** | 分量代表元 = 声明下标最小的节点；分量按代表元声明序 |

自环、平行边不参与 BCC 边栈（Tarjan 用简单无向边）。平行边在 Plan 里仍在，角色 `Parallel`。

失败：无。空图 → 空 Plan。

---

## 2. BCC（Tarjan）

| | |
|--|--|
| **输入** | 一个弱连通分量的无向简单图 |
| **输出** | 块列表（每块一组边）+ 割点集 |
| **写者** | algo 零件 `bcc`；Compose 消费 |
| **序** | 邻接表按对端 `decl_index`；块 id = 块内最小边声明下标 |

实现要点（[14 §3.2](../../../../reference/yfiles/14-图论与优化工具箱.md)）：**边栈**不是点栈；割点属于多块。

孤立点：零条边的块，单点分区。

桥：两个点的 BCC（一条边）。`bcc-compact` 下仍是一个分区（两点一「圆」，实际是一段间距）。

---

## 3. 割点归属

### `bcc-compact`（默认）

每个节点恰好一个 `partition_of`。

割点 `c` 属于多个块时，派给 **块 id 最小** 的那一块（块 id = 最小边声明下标）。其余块在块割树上仍通过 `c` 相连，但 `c` 的几何只出现一次，落在归属圆的圆周上。

### `bcc-isolated`

每个割点单独成分区（单点，半径 0）。BCC 块（去掉割点后的内部点，若还有）各自成圆。骨架：割点分区夹在相邻块分区之间。

失败：同一节点写入两个 `partition_of` → `InternalInvariant`。

---

## 4. `single-cycle`

该弱连通分量全体一个分区，圈序见 §5。不跑 BCC。显式政策；**默认不得**把 `bcc-compact` 静默降成单环（architecture 反模式 7）。

---

## 5. 圈序

| | |
|--|--|
| **输入** | 分区成员 + 诱导边 |
| **输出** | 成员的循环排列（`partitions[p] = [n0, n1, …]`，`n0` 邻接 `n_last`） |
| **写者** | `CircleOrderWriter` |
| **默认** | `spectral` |

### `spectral`

1. 分区诱导子图拉普拉斯 `L = D - A`（无向、无权；平行边当 1）。
2. Fiedler 向量（第二小特征；[14 §5.1](../../../../reference/yfiles/14-图论与优化工具箱.md) 幂迭代，去掉全 1）。
3. 按分量升序排列成员；平局 `decl_index`。
4. 定向：使排列的「相邻边权和」不差于反向；再平局取 `n0` = 声明序最小者旋转到起点。

点数 < 3 或迭代后范数非有限 → 回退 `bfs`，`order_method` 记实。

### `bfs`

从声明序最小成员起 BFS；未达点按声明序追加。

### `declaration`

成员声明序。

下游 Metric **只读**该排列，不得重排。

---

## 6. 自定义分区（M3）

节点 `circle:` atom（模型也认 `partition:`，但 DSL 里 `partition` 是 PartitionGrid 保留字，作者键用 `circle:`）。同一 atom = 同一分区。未标节点：对该分量的未标子图跑 `bcc-compact`，再把自定义区当额外超点；若超图有环，区际多余边当弦/骨架外 `Inter`，warning，不删边。

未知 atom 字符集：与 node id 相同（`[a-z][a-z0-9_]*`）。空 atom → `InvalidInput`。`circle` 与 `partition` 同时出现且值不同 → `InvalidInput`。

仅 `partitioning` 缺省（`bcc-compact`）或显式 `custom` 时生效；`single-cycle` / `bcc-isolated` 下忽略并 warning。

---

## 7. 失败类别

| 情况 | 类别 |
|------|------|
| 未实现的 `partitioning` / `order` | bind `Unsupported` |
| `partition_of` 缺口或重复 | `InternalInvariant` |
| 谱结果非有限仍当成功 | `InternalInvariant`（应已回退） |
| 自定义空分区 id | `InvalidInput` |
