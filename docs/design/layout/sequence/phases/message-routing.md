# Sequence · 消息 Builtin 路由

> 父页：[architecture](../architecture.md) §5
> 上游：[axes](axes.md) · 下游：Metric terminals → Ink 展开

本文是 Sequence **边几何**的真源。它不是 Hier Channel，也不是独立 `EdgeRouter`。

> **前置**：本文 `AttachSpec.side` 用本核私有 `LifelineSide`（`East | West | Center`），**不**是 [`model::port::Side`](../../../../crates/plotgram-model/src/port.rs)（封闭四值，无 `Center`）。§3.3 说明对外 `PortRef` 的映射规则。§11 失败表已对齐当前 [`LayoutError`](../../../../crates/plotgram-engine-api/src/error.rs) 变体。

---

## 1. 问题定义

给定：

- 已冻结的 `lifeline_order` 与每条消息的 `MessagePlan`；
- Metric 给出的 `lifeline_x`、`row_y`、`activation_frames` / depth 偏移；

求每条消息的：

1. 端点像素 `terminals`；
2. 折线 `path`；
3. 可选生命线交叉装饰（缺口 / 跳线）。

约束：

- 同步消息默认**水平**（共 y）；
- 自调用为**右探 U 形**（默认）；
- 中间生命线被穿过**合法**；
- Ink 不得新增肘点拓扑或改行号。

---

## 2. 输入完备条件

每条消息进入端点展开前必须有：

```text
MessagePlan.kind
MessagePlan.timing / row
MessagePlan.attach.from / attach.to
MessagePlan.route
```

Metric 展开前另需：

```text
lifeline_x[from], lifeline_x[to]
row_y[send_row]（及 async recv_row）
activation 水平偏移（由 depth × activation_width）
self_loop_width（仅 SelfLoop）
message_endpoint_inset
```

缺任一字段 → `InternalInvariant`，禁止默认侧/默认 y。

---

## 3. 附着点（Attach）

### 3.1 语义

消息端点附着在**生命线中心轴**或其上的**激活条外缘**，不是参与者头部框的 North/South 端口模型。

```text
x_terminal = lifeline_x[id]
           + side_sign(side) * (activation_half_extent(depth) + inset)
y_terminal = row_y[row]
```

`side: LifelineSide` 是本核私有枚举（见 [architecture §3.1](../architecture.md)），不是 `model::port::Side`：

| `LifelineSide` | 含义 |
|----------------|------|
| `Center` | 落在轴上（少见；一般用于 stub / Lost-Found） |
| `East` | 轴右侧（去程朝右 / 自调用探出） |
| `West` | 轴左侧 |

### 3.2 默认侧选取（Compose）

对 `Horizontal` 且 `from != to`：

```text
if lifeline_order(to) > lifeline_order(from):
    from.side = East;  to.side = West
else:
    from.side = West;  to.side = East
```

平局（不应发生于不同生命线）→ `InternalInvariant`。
`activation_depth` 来自 ActivationWriter；无激活条时 depth=0。

### 3.3 与 PortRef 的关系

对外 [`EdgePlacement.from_port / to_port`](../../../../crates/plotgram-model/src/result.rs) 是 `Option<PortRef>`，`PortRef.side: model::port::Side`（封闭四值 NSEW）。映射规则：

| `AttachSpec.side` (`LifelineSide`) | `PortRef` | 说明 |
|-------------------------------------|-----------|------|
| `East` | `Some(PortRef { side: Side::East, along: Ordered{order=depth, count=…} })` | 合成端口供统一 render |
| `West` | `Some(PortRef { side: Side::West, along: Ordered{order=depth, count=…} })` | 同上 |
| `Center` | `None` | **不映射到 PortRef**：`Center` 仅用于 stub / Lost-Found，无对应 NSEW 端口；`EdgePlacement.from_port/to_port` 留 `None`，render 按 `LifelineSide::Center` 自行处理 |

- **真源**是 `AttachSpec`（含 `LifelineSide` + `activation_depth`）；`PortRef` 是派生投影，不得让 Hier 式 FREE 端口分配改写消息端点。
- M0 若不接激活条（depth 恒 0），`along` 用 `Ordered{order:0, count:1}` 占位即可。
- 禁止为 `Center` 强行塞 `Side::East` 之类的「兜底」——会在 verifier 里误判端点侧。

---

## 4. 拓扑展开

### 4.1 Horizontal

```text
points = [ terminal_from, terminal_to ]
```

不变量：

- `y1 == y2`（数值量化后仍共线）；
- x 方向与 `lifeline_order` 一致；
- 不插入中间折点「躲开」生命线。

### 4.2 SelfLoop

默认右探（`side: East`）：

```text
x0 = lifeline_x + inset_at_depth
x1 = x0 + self_loop_width
y0 = row_y[start]
y1 = row_y[end]          # DoubleRow 时 end = start+1；SingleTall 时 end=start 且用高度盒

points = [
  (x0, y0),
  (x1, y0),
  (x1, y1),
  (x0, y1),
]
```

| `self_loop_row_policy` | Plan / Metric |
|------------------------|---------------|
| `DoubleRow` | Self 消息占用两逻辑行；`y0/y1` 为两行中心 |
| `SingleTall` | 单行但 `row_height` ≥ 自调用需求；`y0/y1 = center ± half` |

两种策略必须在 Plan 明示一种；Ink 不得自行加高。

### 4.3 Slanted（后置）

仅当 `timing = Async { send_row, recv_row }` 且两行不同：

```text
points = [ terminal_at(send_row), terminal_at(recv_row) ]
```

斜率完全由两行 y 差决定。禁止 Ink 在同行上「稍微倾斜」装异步。

### 4.4 Lost / Found（后置）

短水平 stub + 开放端；端点一侧无生命线。未实现 → bind/执行期 `Unsupported`。

---

## 5. 样式 ≠ 拓扑

| Graph / Arrow | 对路由的影响 |
|---------------|--------------|
| `Arrow::Forward` | 实线 + 实心箭头（render） |
| `Arrow::Response` | 虚线 + 开放箭头；**拓扑仍 Horizontal/SelfLoop** |
| `Arrow::Bidirectional` | 双箭头装饰；path 仍两端点 |

Compose 可把 `kind = Reply` 标在 MessagePlan 上供诊断，但 **不得**因此改 `MessageRouteTopo`。

---

## 6. 穿越生命线

### 6.1 判定

对 `Horizontal` / `Slanted` 消息，任一生命线 `L` 满足：

```text
min(x_from, x_to) < lifeline_x[L] < max(x_from, x_to)
```

且 `L ∉ {from, to}` → `L` 在该消息的相关 row（水平为单行；斜线为 y 覆盖行集）被穿越。

### 6.2 记录

```text
lifeline_crossings[L].push(row_or_segment_key)
```

稳定排序后供 Ink。

### 6.3 装饰策略（`lifeline_gap_style`）

| 值 | 行为 |
|----|------|
| `notch`（默认） | 生命线在交叉 y ± gap_half 留空（v1 `LIFELINE_MESSAGE_GAP_HALF`） |
| `none` | 生命线连续画过；消息画在上层 |
| `hop` | 后置；bind 硬失败。消息在交叉处小跳线（可与正交跳线零件共享） |

装饰**不**改变 MessageRouteTopo，不新增 Compose 决策。

### 6.4 Verifier

- **不**因穿越生命线报碰撞；
- **要**报：穿越未记录却画了 notch、或 notch 改动了 path 端点。

---

## 7. Demand 与路由质量

路由观感依赖上游预算，不在 Ink 修补：

| 需求 | 效果 |
|------|------|
| 消息标签宽度 | `LifelineGap(from,to)` 下界 ≥ label_width + padding |
| 同屏并行消息 cutwidth | 可选加大 `message_gap`（后置） |
| 激活 max depth | `LifelineMinWidth` |
| 自调用 | `RowHeight` 或额外 RowSpan |

标签避让：优先加宽间距；禁止 Metric 后由 Ink 把标签推离 path 导致与邻消息重叠却不回报。

---

## 8. Ink 流程

```text
1. 读取 MessageRouteTopo + terminals
2. 按 §4 展开 points
3. 共线合并 / 去零长段
4. 量化（固定单位）
5. 箭头缩进 / 虚线样式（L5）
6. 写 lifeline gap marks（若 notch）
7. InkVerifier
```

### 可做

- 端点缩进、箭头 inset；
- 自调用转角轻微圆角（半径 ≤ 半段长）；
- notch 几何。

### 不可做

- 缺 topo 时猜 Horizontal；
- 为避标签改 `lifeline_x`；
- 把回复画成另一套折线；
- 绕开中间生命线增加折点；
- 调用独立 EdgeRouter。

---

## 9. InkVerifier（消息专项）

1. 每边 path 点数：Horizontal ≥ 2；SelfLoop ≥ 4；
2. 首尾等于 Metric terminals（容差内）；
3. Horizontal：所有点 y 相同；
4. SelfLoop：起终点 x 同轴侧，探出宽度 = 参数；
5. 无 NaN；无多余折点（相对 Plan topo）；
6. Reply 与 Call 若同行同端，path 几何类相同；
7. 未启用 hop 时 path 无跳线控制点。

---

## 10. 与 v1 的对照（迁什么 / 不迁什么）

| 迁（功能） | 不迁 / 重写 |
|------------|-------------|
| 声明序铺生命线 + 声明序消息 y | `HashMap` 存节点 |
| 水平消息 + 右探 U 形自调用 | 布局后通用 label_avoidance 当终态修 |
| 生命线交叉缺口 hints | 图种 `applicable_diagram_types` 分支 |
| `produces_edge_geometry` 语义 | 无 Plan 的一步 product 糊浆 |

---

## 11. 失败表

对齐当前 [`LayoutError`](../../../../crates/plotgram-engine-api/src/error.rs) 变体：`MissingNodeSize` / `UnknownLayout` / `UnknownRouter` / `LayoutCannotDeferEdges` / `UnsupportedRouteScene` / `Unsupported` / `InvalidInput` / `InternalInvariant` / `Message(String)`。

Sequence 经 [`seq_err`](../../../../crates/plotgram-layout/src/layout/sequence/mod.rs) 把带前缀的诊断分到后三类（`invariant:` → `InternalInvariant`，`unsupported:` → `Unsupported`，其余 → `InvalidInput`）。`Message` 留给尚未迁移的其它内核。

| 情况 | `LayoutError` 变体 | 备注 |
|------|---------------------|------|
| `edge_routing: Some(_)`（S6 禁 Router） | [`LayoutCannotDeferEdges { layout: "sequence" }`](../../../../crates/plotgram-engine-api/src/error.rs) | 本核 `layout()` 入口自检（见 [architecture §1.2](../architecture.md)），门面不替 layout 拦 |
| 消息端点不是参与者节点（非顶层 Entity / 是 GroupAnchor） | `InvalidInput` | Display 保留 `sequence: invalid: …` |
| Self 但 route ≠ SelfLoop | `InternalInvariant` | verifier |
| Sync 但 y 不共线（InkVerifier） | `InternalInvariant` | 同上 |
| `lifeline_gap_style: hop` 等后置特性 | `Unsupported { feature }` | 诚实硬失败，不静默 |
| Lost/Found 未实现却触发 | `Unsupported` | 后置 |
| 标签 Demand 未满足导致溢出 | `InternalInvariant` | 预算相序错误 |
| 激活 depth 负 / span 起止倒序 | `InternalInvariant` | |
| 生命线序 pin 冲突 / 片段部分交叠 | `InvalidInput` | 作者约束不可行 |

**禁止**在文档里写不存在的设计类目当已落地。
