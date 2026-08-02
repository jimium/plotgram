# Sequence · 生命线次轴与时间主轴

> 父页：[architecture](../architecture.md) §6
> 下游：[message-routing](message-routing.md)

消息路由的端点与行号来自本相；本相不写 path 点列。

---

## 1. 生命线收集

- 参与者 = `NodeRole::Entity` 的节点（sequence 图中头部框）。
- 收集序 = `Graph::all_node_ids()` 声明序（或仅顶层实体，须在 params 钉死一种）。
- `GroupAnchor` 不作为生命线。
- 容器：`Vec` / `IndexMap`；禁止 `HashMap` 迭代导出顺序。

---

## 2. 次轴排列

### 2.1 策略

| `lifeline_order` | 算法 |
|------------------|------|
| `declaration` | 收集序原样（默认） |
| `greedy` | 按消息时间序，对未放置端点贪心插入使 ΔMinLA 最小 |
| `local` | 在 greedy/declaration 初值上邻交换 / 插入移动 |

代价：

```text
cost(π) = Σ_messages w(e) * |π(from) - π(to)|
```

默认 `w = 1`；可选对 Reply 降权。tie-break：`(cost, cutwidth, lexicographic order)`。

### 2.2 硬约束

- LayoutData 显式 pin / before 偏序：优化不得违反；
- 冲突 → `InfeasibleConstraint`；
- 「用户书写序」在 `declaration` 模式下即答案，不做暗默重排。

### 2.3 复用

实现落在 `plotgram-algo` 一维排列器；Sequence 只投影生命线加权图 + 约束。
同一零件服务泳道/组内序（Hier）时，**参数不同、代码同一**。

---

## 3. 时间主轴（行号）

### 3.1 默认同步模型

```text
for (i, edge) in edges_in_declaration_order():
    row[edge] = i   # 或 Self 时按 policy 跳号
```

- 无偏序求解；声明序即全序。
- 产品调整时间 = 调整 DSL 边序（dsl-spec §8.1）。

### 3.2 SelfCall 行占用

| 政策 | 行号 |
|------|------|
| DoubleRow | 该消息占用 `r` 与 `r+1`，后续消息行号顺延 |
| SingleTall | 只占 `r`，Demand 提高 `row_height[r]` |

### 3.3 异步（后置）

建事件偏序：同生命线本地序 + send→recv。
环 → 诊断。线性扩展可用最长路。
未实现前不得把倾斜当默认同步的「美化」。

---

## 4. 激活条派生（Compose）

简化栈模型（目标）：

```text
depth = 0 on each lifeline
for msg in time order:
  if Call to L: push activation on L at current row; depth++
  if Reply from L: close innermost activation on L; depth--
  if SelfCall: push then immediately affect nested depth on same lifeline
```

输出 `activation_spans: { lifeline, start_row, end_row, depth }`。
配对失败（回复无匹配调用）→ warning 或硬失败（由 `activation_strict` 参数定）；禁止静默负深度。

Metric 把 span × depth 变成矩形；Attach 用 depth 算水平偏移。

---

## 5. 行高与 y（Metric）

```text
row_height[i] = max(
  message_gap 下限,
  该行标签高,
  自调用需求,
  片段标题带,
  激活条美学最小高
)
row_y[0] = header_bottom + first_message_offset + row_height[0]/2
row_y[i+1] = row_y[i] + row_height[i]/2 + row_height[i+1]/2
```

这是相邻不等式的前缀和最优解；一般**不必**上 VPSC。
若未来多软对齐再引入求解器，不得改变「行序 = 时间序」真源。

---

## 6. 生命线 x（Metric）

```text
width[L] = max(participant_header_width, activation_min_width(max_depth[L]))
gap[L,L'] ≥ max(param_gap, label_demand(L,L'))
x 中心 = 前缀累积
```

`label_demand` 来自跨越 `L…L'` 的消息标签宽度（DemandBoard epoch B）。

---

## 7. PlanVerifier（轴相关）

- `lifeline_order` 是全部参与者的排列；
- 每条消息 row 落入 `[0, row_count)`；
- Self + DoubleRow 不与下一条消息抢同一逻辑行；
- 激活 depth 非负；span 起止有序；
- 显式 pin 在最终 order 中位置正确。
