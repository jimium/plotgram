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
| 组合片段（渐进） | `alt` / `loop` 等区间框：主轴区间 × 次轴连续生命线带 |

### 自调用 vs profile 自环

[`profile.rs`](../../../../crates/plotgram-model/src/profile.rs) 给 `DiagramType::Sequence` 设了 `allow_self_loop: false`。这**不**与 SelfCall 矛盾，原因是二者不在同一层闸门：

- `allow_self_loop` 是 profile 层的「是否允许 `A -> A` 进入图模型」开关。Sequence 关闭它，是因为 v1 把 self-loop 当作通用图的「孤立回环」语义（dsl-spec §4.2 / W003），而 sequence 的自调用是**消息时间轴上的自调用消息**，语义不同。
- **Sequence 的 SelfCall 走另一条解析路径**：DSL 中 `A -> A` 在 `profile: sequence` 下应被当作 **SelfCall 消息**（`MessagePlan.kind = SelfCall`，`route = SelfLoop`），而不是被 profile 自环闸门拒绝。

落地要求（**待实现时钉死**）：

1. parse/lift 阶段：`profile: sequence` + `A -> A` → 不走 `allow_self_loop` 拒绝分支，直接进 `Graph::edges`。
2. Compose 阶段：`from == to` 的边分类为 `SelfCall`，挂 `SelfLoop { East }` 拓扑。
3. 若 `allow_self_loop` 闸门后续被接到 parse/lift，须为 sequence 显式豁免，或在 profile 层把 Sequence 的 `allow_self_loop` 改为 `true` 并在文档注明「sequence 自环 = SelfCall」。

> **当前 `allow_self_loop` 字段在重建代码里 defined-but-unused**（全仓仅 `profile.rs` 提及，未接校验）。这是潜在矛盾点：一旦接上校验，`A -> A` 会被拒，SelfLoop 拓扑不可达。本核实现前必须确认闸门归属。

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
| MSC 风格交互 | 同上；片段框渐进 | 同左 |
| 简单调用链叙事 | 声明序即故事序 | 默认声明序，可选排列优化 |

## 4. 与产品能力的差距（设计记债）

1. **激活条嵌套** — 目标有；首期可先画消息再补条。
2. **组合片段** — 区间嵌套器渐进；不挡消息主路径。
3. **异步多行** — send/recv 分行是可选能力；默认同步同行水平。
4. **一维排列优化** — 默认声明序；MinLA / 局部搜索为参数开关，非第二布局器。
5. **甘特 / 火焰图** — 验证抽象后置，不阻塞 sequence MVP。
