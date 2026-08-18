# Sequence · 能力范围与典型域

> 父页：[README.md](README.md) · 目标架构：[architecture.md](architecture.md)

## 1. 能力范围（做）

| 能力 | 说明 |
|------|------|
| 参与者 / 生命线 | 节点 = 参与者；布局派生生命线竖线；声明序为默认次轴序 |
| 消息时间轴 | 边声明序 = 时间序（无 `Edge::seq`）；布局写出消息边几何 |
| 内建消息路由 | **BuiltinEdges**：水平消息、自调用 U 形、返回消息同几何异样式 |
| 自调用（SelfCall） | `from == to` 的消息；走 `SelfLoop` 拓扑，是 sequence 一等能力（见下「自调用 vs profile 自环」） |
| 激活条（目标） | 由调用/返回配对派生；嵌套深度影响生命线有效宽 |
| 消息标签预留 | 标签宽进 Demand → 生命线间距 / 行高 |
| 穿越生命线 | 中间生命线不断开语义连接；可选缺口/跳线风格 |
| 组合片段 | `alt` / `loop` 等区间框：主轴区间 × 次轴连续生命线带；输出 `FragmentFrame` decoration |

### 自调用 vs profile 自环

[`profile.rs`](../../../../crates/tautcore-model/src/profile.rs) 给 `DiagramType::Sequence` 设了 `allow_self_loop: true`。Sequence 的 `A -> A` 是**消息时间轴上的自调用**（`MessagePlan.kind = SelfCall`，`route = SelfLoop`），不是通用图的孤立回环：

1. parse/lift：`profile: sequence`（或显式 `layout: sequence`）放行 `A -> A` 进入 `Graph::edges`。
2. Compose：`from == to` 分类为 `SelfCall`，挂 `SelfLoop { East }`。
3. 其它图种的自环闸门不变。

## 2. 非目标（故意不做）

| 非目标 | 归属 / 说明 |
|--------|-------------|
| Sugiyama 分层 / Channel / OVG | → [Hierarchical](../hierarchical/)；独立 EdgeRouter |
| 用 Hier「模拟」消息轴 | 禁止；主轴是时间语义，不是 rank |
| 独立 `edge_routing:` 主路径 | dsl-spec：sequence 默认 `edge_routing: None`；显式独立路由为 **Unsupported** |
| 组内消息作一等时间轴 | 产品消息写顶层；组内边非一等时序能力 |
| 甘特连续时间轴 / 火焰图 | 同抽象后置；首期只做离散消息行 |
| 异步倾斜箭头由 Ink 发明 | recv 行号差须在 Compose 决定；Ink 不猜斜率 |
| ArchitectureLayout / 泳道布局器 | 无关 |

## 3. 典型域

| 域 | 为何适合 Sequence | profile 直觉 |
|----|-------------------|--------------|
| UML / 协作时序 | 生命线 + 消息时间序 | `profile: sequence` → `layout: sequence` |
| MSC 风格交互 | 同上；片段框已落地 | 同左 |
| 简单调用链叙事 | 声明序即故事序 | 默认声明序，可选排列优化 |

## 4. 与产品能力的差距（设计记债）

1. **激活条嵌套** — 已落地（M2）：Compose 写 `activation_depth`；Metric 展开条宽。
2. **组合片段** — 已落地（M4）：一等 `fragment` 块（dsl-spec §8.2）或边属性 `fragment*`；嵌套包含、部分交叠硬失败。`alt`/`par` 的互斥 `else` 返回不警告 unpaired reply。
3. **异步多行** — send/recv 分行是可选能力；默认同步同行水平。
4. **一维排列优化** — 已落地（M3）：默认声明序；`greedy` / `local` + `lifeline_pin` / `lifeline_before`。
5. **甘特 / 火焰图** — 验证抽象后置，不阻塞 sequence MVP。
