# Sequence · 目标架构设计

> 状态：**现行目标架构 v1**（驱动重建；非当前能力声明）
> 日期：2026-08-02（修订：2026-08-06，对齐 `tautcore-engine-api` Trait / `tautcore-model` 现状）
> 引擎注册名：`sequence`
> 代码落点（目标）：`crates/tautcore-layout/src/layout/sequence/`（与 Hier 同 crate；见 [ADR-006](../../adr/006-engine-io-and-crates.md)）
> 约束入口：[写权纪律](../write-authority.md) · [AGENTS.md](../../../../AGENTS.md) §1
> 证据：[19 序列图与一维排列](../../../reference/yfiles/19-序列图与一维排列.md) · dsl-spec §8.1 · [model-boundary 时序](../../model-boundary.md)
> v1 参考（功能与坑，非目录真源）：`crates/v1/.../recipes/sequence.rs`

本文钉死 Sequence 的**目标形态与跨相契约**，重点是**消息边几何（Builtin 路由）**：它与 Hier 的 Channel/OVG 不是同一条路径。
相级细节：[phases/](phases/README.md)。姊妹页：[README](README.md) · [scope](scope.md)。

> **前置依赖**：
>
> 1. **[ADR-009](../../adr/009-layout-result-decorations.md) 已落地（方案 A）**。[`LayoutResult`](../../../../crates/tautcore-model/src/result.rs) 与 [`LayoutOutput`](../../../../crates/tautcore-engine-api/src/traits.rs) 均有 `decorations`；finalize 只 translate。片段框（`FragmentFrame`）已随 M4 落地。
> 2. [`LayoutError`](../../../../crates/tautcore-engine-api/src/error.rs) 已含 `Unsupported` / `InvalidInput` / `InternalInvariant`。Sequence 经 `seq_err` 按消息前缀分类；失败表见 [message-routing §11](phases/message-routing.md)。
> 3. [`model::port::Side`](../../../../crates/tautcore-model/src/port.rs) 是封闭四值枚举（`North/South/East/West`），无 `Center`。Sequence 的「附着侧」用本核私有 `LifelineSide`（见 §3.1），不污染 model。

---

## 0. 一句话目标

```text
次轴：生命线一维排列（声明序硬约束优先）
  + 主轴：消息时间离散化（边声明序 = 时间序）
  + 组合相一次写完消息拓扑与附着点决策
  + 度量相一次写出生命线 x、消息 y、激活条、行高
  + Ink 只展开消息折线 / 自调用 U 形 / 装饰（零新决策）
  + 禁止独立 EdgeRouter 作本核主路径
```

**不做**：把消息轴塞进 Sugiyama；在 Ink 猜异步斜率；用 Hier Channel 搜时序箭头。

---

## 1. 硬约束

| # | 约束 | 含义 |
|---|------|------|
| S1 | **单写者** | 生命线序、消息行号、附着侧、路径拓扑、坐标各唯一写者 |
| S2 | **落笔零新决策** | Ink 不得发明行号、斜率、自调用宽度、激活深度 |
| S3 | **DemandBoard** | 标签宽、激活嵌套深、自调用高度 → 在上游坐标前 `max` 合并 |
| S4 | **确定性** | 禁止 `HashMap` 迭代驱动序；生命线/消息遍历稳定 |
| S5 | **无图种分支** | 引擎只认 `layout: sequence` + typed params（ADR-001） |
| S6 | **BuiltinEdges** | 本核写出最终消息 path；`edge_routing: Some` → 由本核 `layout()` 入口返回 [`LayoutError::LayoutCannotDeferEdges`](../../../../crates/tautcore-engine-api/src/error.rs) |
| S7 | **声明序 = 时间轴** | 无 `Edge::seq`；重排时间 = 重排 DSL 边序 |

### 1.1 状态词

与 Hier 相同：**目标 / 已落地 / 过渡 / 后置**。bind 成功但未消费的参数 = 未支持。

### 1.2 禁 Router 的执行点

引擎门面 [`run`](../../../../crates/tautcore-engine/src/run.rs) 在 `edge_routing.is_some()` 时设 `EdgeGeometryMode::DeferToRouter` 后**无条件**调用 router 覆盖 `output.edges`——它不替 layout 拒绝。因此 S6 的「禁独立 Router」检查**必须由本核 `layout()` 入口自己做**：

```rust
fn layout(&self, input: LayoutInput<'_>) -> Result<LayoutOutput, LayoutError> {
    if input.edge_geometry == EdgeGeometryMode::DeferToRouter {
        return Err(LayoutError::LayoutCannotDeferEdges { layout: "sequence".into() });
    }
    // ...
}
```

门面 [`LayoutError::LayoutCannotDeferEdges`](../../../../crates/tautcore-engine-api/src/error.rs) 的语义（"layout does not support deferred edge routing"）正好对上。本核不得依赖门面拦截。

---

## 2. 总体架构

### 2.1 两轴 + 三相

```text
                    ┌─ Validate（禁 edge_routing；校验端点为参与者）
Domain / DSL ──────►│
  profile expand    │   SequenceCore
  LayoutContract ──►│     Compose → Metric → Ink
  (无 profile 名)   │
                    └─ Normalize / Diagnostics
                         │
                         ▼
                   LayoutOutput（含消息 path；无 Router）
```

| 层 | 输入 | 输出 | 性质 |
|----|------|------|------|
| **Compose** | Graph + `SequenceParams` + sizes | **`SeqPlan`** | 离散决策：生命线序、消息行、拓扑、附着 |
| **Metric** | Plan + sizes + DemandBoard | **`SeqMetric`** | 生命线 x、消息 y、激活条框、行高 |
| **Ink** | Plan + Metric | **边折线 + 生命线缺口提示** | 纯展开 |

与 Hier 对照：

| | Hierarchical | Sequence |
|--|--------------|----------|
| 主轴 | rank（算法求） | 时间行（声明序语义固定） |
| 次轴 | 层内 order | 生命线排列（可优化） |
| 边几何 | Channel / 可选 Router | **仅 Builtin 消息路由** |
| 穿越 | 非法穿组须 gate | **穿越中间生命线合法** |

### 2.2 输出契约

**目标形态**（ADR-009 落地后）：

```text
LayoutResult
  nodes:       参与者头部框（Metric）
  groups:      弱 group 框（finalize 从 Graph::groups 包络；本核不直写。片段框不走此通道）
  edges:       消息 EdgePlacement（path 必填）
  decorations: LifelineDecoration[] / ActivationBar[] / FragmentFrame[]
  diagnostics: LayoutDiagnostics
```

**当前现实**（M4）：

```text
LayoutOutput { nodes, edges, groups, owns_group_frames, diagnostics, decorations }
LayoutResult { …, decorations }   // finalize 合并：只 translate decorations
```

| 通道 | 状态 |
|------|------|
| `groups`（弱 group 框） | finalize 仅在 `owns_group_frames = false` 时从 `Graph::groups` 包络。Sequence 置 `owns_group_frames = true` 且 `groups: []` |
| `decorations`（生命线/激活条/片段框） | **已落地**。Metric 写 `Lifeline`（含 `gaps`）/ `Activation` / `FragmentFrame`；render 按 kind 消费，禁止按 `layout.name` 猜线 |
| `LayoutOutput` → `LayoutResult` 合并 | finalize 透传并平移 decorations（方案 A） |

生命线、激活条与片段框是**派生几何**，不进入 `Graph`；须有稳定 id（如 `lifeline:{node_id}`、`activation:{edge_id}:{ordinal}`、`fragment:{author_id}`），供 render 与 verifier 引用。
契约真源：[ADR-009](../../adr/009-layout-result-decorations.md)（`LayoutResult.decorations`）；禁止仅靠 render 再猜激活区间。

### 2.3 模块边界（目标）

```text
tautcore-layout/layout/sequence/
  params.rs
  compose/     # lifeline order · message rows · route topo · attachments
  plan/
  metric/      # x / y / activation / row height
  ink/         # message path expand · lifeline gap marks
  demand.rs
  verify.rs

tautcore-algo/   # 1D arranger（复用）；非 sequence 私有第二宇宙
```

落点与 Hier 同 crate（[`tautcore-layout`](../../../../crates/tautcore-layout/src/layout/)），注册经 [`Registry::standard`](../../../../crates/tautcore-engine/src/registry.rs)；不依赖门面 `run`。

---

## 3. 核心 IR

### 3.1 SeqPlan

| 字段 | 含义 | 写者 |
|------|------|------|
| `lifeline_order: NodeId[]` | 次轴排列（稳定） | LifelineOrderWriter |
| `messages: EdgeId → MessagePlan` | 见下 | MessageWriter |
| `fragments` | 组合片段主轴/次轴覆盖（**已落地** M4） | FragmentWriter |
| `activation_spans` | 激活起止消息行（可在 Compose 派生） | ActivationWriter |

```text
MessagePlan {
  edge_id
  from / to: NodeId          # = Graph original endpoints
  kind: Call | Reply | SelfCall | Lost | Found   # Lost/Found 后置
  timing: SyncSameRow | Async { send_row, recv_row }
  row: RowId                 # Sync 时唯一行；Async 时 send 行（recv 另存）
  attach: { from: AttachSpec, to: AttachSpec }
  route: MessageRouteTopo
}

AttachSpec {
  lifeline: NodeId
  side: LifelineSide         # East | West | Center（本核私有枚举；见下）
  activation_depth: u32      # 0 = 贴生命线；>0 = 嵌套条外缘
}

MessageRouteTopo =
  Horizontal                   # 同行两端点水平
  SelfLoop { side: East }      # 默认右探 U 形
  Slanted                      # 仅当 Async 且 send_row != recv_row；行列已定
  StubLost | StubFound         # 后置
```

**`LifelineSide` 是本核私有枚举，不映射到 [`model::port::Side`](../../../../crates/tautcore-model/src/port.rs)**（后者封闭四值 `North/South/East/West`，无 `Center`，且 `Side::parse` 对未知 atom 直接报错）。原因：

- 序列图附着侧语义是「相对生命线中心轴的左/右/中」，与节点框的 NSEW 端口模型不同构；
- 不污染 model 的端口词表，避免影响 Hier 的 PortRef/PortConstraint/render 端口逻辑；
- 对外（`EdgePlacement.from_port / to_port`）若需统一 render，由 Ink 把 `LifelineSide::East/West` 映射为合成 `PortRef { side: Side::East/West, along: Ordered{order=depth,count=…} }`；`Center` 不映射到 PortRef（仅用于 stub / Lost-Found，见 [message-routing §3.3](phases/message-routing.md)）。

**禁止**：Ink 在缺少 `MessageRouteTopo` 时 `unwrap_or(Horizontal)`；禁止把 `Arrow::Response` 误当成另一套拓扑（样式 ≠ 拓扑）。

### 3.2 SeqMetric

| 字段 | 写者 |
|------|------|
| `participant_frames` | CoordWriter |
| `lifeline_x: NodeId → f64` | 生命线中心轴 x |
| `lifeline_half_width` | 含激活嵌套预留 |
| `row_y: RowId → f64` | 消息行中心 y |
| `row_height` | 含标签 / 自调用额外高 |
| `activation_frames` | 激活条矩形 |
| `fragment_frames?` | 片段框 |
| `message_terminals: EdgeId → { from: Point, to: Point }` | 由 attach × 坐标展开 |
| `lifeline_crossings: NodeId → RowId[]` | 被穿越处（供缺口/跳线） |

### 3.3 DemandBoard（Sequence 键）

| Epoch | 来源 | key | 消费时机 |
|-------|------|-----|----------|
| **A** | 自调用结构、片段标题行 | `RowSpan` / 逻辑行数 | 行号固化后、y 前 |
| **B** | 消息标签宽、激活 max depth、自调用探出宽 | `LifelineGap` / `LifelineMinWidth` / `RowHeight` | x/y 求解前 |

纪律：长标签加宽生命线间距必须在 **x 确定之前**；Ink 不得为避让挪生命线。

---

## 4. 管线（目标）

```text
I.0  Bind + validate          禁 edge_routing；端点须为实体参与者
I.1  Collect lifelines        声明序收集；稳定 id
I.2  Lifeline order            L1 声明 / 可选 L2–L3 排列（硬约束钉住显式序）
I.3  Message classify          Call / Reply / Self；Arrow 只影响样式位
I.4  Row assign                声明序 → 稠密 RowId；Self 可占两逻辑行
I.5  Activation derive         调用栈深度与起止行
I.6  Attach + RouteTopo        附着侧 + Horizontal / SelfLoop / Slanted
I.7  Freeze SeqPlan + PlanVerifier

II.0 Freeze MetricBudget
II.1 Lifeline x                头部宽 ∪ 激活宽 ∪ 标签 Demand
II.2 Row y                     前缀和（一维相邻约束；通常无需 VPSC）
II.3 Activation / fragment frames
II.4 Terminal points + crossings
II.5 MetricVerifier

III.1 Expand MessageRouteTopo → path points
III.2 Lifeline gap / hop marks（装饰，不改拓扑）
III.3 Arrow / dash 装饰（只读 Arrow 语义）
III.4 InkVerifier
```

---

## 5. 消息路由架构（本核正交主路径）

> 细节真源：[phases/message-routing.md](phases/message-routing.md)

### 5.1 写权分层

| 层 | 自由度 | 写者 | 禁止 |
|----|--------|------|------|
| L1 | 消息行 / 异步行差 | Compose | Ink 发明斜率 |
| L2 | 路径拓扑（水平 / U / 斜） | Compose | Metric 改拓扑 |
| L3 | 附着深度（激活条） | Compose → Metric 展开像素 | Ink 猜 depth |
| L4 | 端点像素、行 y、轴 x | Metric | Ink 挪生命线 |
| L5 | 圆角、虚线、箭头、缺口 | Ink | 改 L1–L4 |

### 5.2 与「独立 EdgeRouter」的边界

| 场景 | 后端 |
|------|------|
| Sequence 消息 | **本核 Builtin Ink** |
| Hier / Tree 等节点冻结后重布 | 独立 `EdgeRouter`（可 OVG） |

Sequence **不得**把消息委托给 Hier Channel：时间轴与生命线穿越语义不同（穿越合法、无组 gate）。

### 5.3 合法穿越

中间生命线被水平消息穿过是**常态**：

1. path 几何连续，不断成多段「绕开」；
2. Metric 记录 `lifeline_crossings`；
3. Ink/Render 可选：生命线在交叉 y 留缺口，或画跳线；
4. Verifier **不**把「穿过生命线」当碰撞失败。

### 5.4 拓扑选型

| 条件 | `MessageRouteTopo` |
|------|-------------------|
| `from != to` 且 SyncSameRow | `Horizontal` |
| `from == to` | `SelfLoop { East }`（默认） |
| Async 且 send_row ≠ recv_row | `Slanted`（端点行已定） |
| Lost / Found | 后置 stub |

`Arrow::Response` / `Forward` / `Bidirectional` **不**改变拓扑类；只改变 stroke/arrowhead（render 或 Ink L5）。

---

## 6. 次轴与主轴（路由前提）

### 6.1 生命线序

| 策略 | 参数 | 说明 |
|------|------|------|
| **Declaration**（默认） | `lifeline_order: declaration` | 节点声明序；多数图已最优 |
| Greedy insert | `…: greedy` | 按消息序插入未出现生命线（19 §2.2 L2） |
| Local search | `…: local` | 邻交换；显式钉住者不动 |

硬约束：DSL/LayoutData 显式 `before` / 固定端点 **永不被优化覆盖**。
实现复用 `tautcore-algo` 一维排列器，不在 sequence 内私有复制。

### 6.2 时间行

- 默认同步：每条消息（含 reply）占一行，序 = `edges_in_declaration_order`。
- SelfCall：可占两逻辑行或加高单行（参数二选一，须在 Plan 明示）。
- 偏序/异步：后置；环 → `InvalidInput`，不静默修。

---

## 7. 参数体系

```text
1. SequenceParams::default()
2. SequencePreset.apply（仅 gaps）
3. layout: sequence { … }
4. SequenceLayoutData（元素级：lifeline pin、fragment 等）
```

| 组 | 字段 | 默认直觉 |
|----|------|----------|
| Gaps | `participant_gap` / `message_gap` | 48 / 28 |
| Self | `self_loop_width` / `self_loop_row_policy` | 40 / DoubleRow |
| Activation | `activation_inset` / `activation_width` | 4 / 10 |
| Order | `lifeline_order` | declaration |
| Crossing | `lifeline_gap_style` | **notch**（默认）\| none；`hop` 后置硬失败 |
| Label | `label_to_gap` | true（标签宽回写间距） |

`orientation`：首期只实现 **生命线水平展开、时间向下**；四向变换后置，禁止四套消息拓扑。

---

## 8. Writer 与确定性

| Writer | 自由度 |
|--------|--------|
| `LifelineOrderWriter` | 次轴序 |
| `MessageWriter` | 行号、kind、timing、route topo |
| `ActivationWriter` | 深度与起止行 |
| `AttachWriter` | 附着侧与 depth（可并入 MessageWriter） |
| `CoordWriter` | x / y / frames / terminals |
| `InkWriter` | path 点列与装饰 |

确定性：

- 参与者、消息一律声明序；
- 排列平局 `(cost, declaration_index, NodeId)`；
- 禁止 `HashMap` 驱动生命线收集（v1 坑）。

---

## 9. 验真

| Verifier | 最低断言 |
|----------|----------|
| Plan | 每边有 MessagePlan；Self ⇒ SelfLoop；Sync ⇒ 单 row；order 含全部参与者 |
| Metric | finite；生命线 x 单调；row_y 单调；terminal 落在对应轴 ± depth；Demand 满足 |
| Ink | path 首尾 = terminals；Horizontal 共 y；SelfLoop 三点以上且回到同轴；不发明行 |
| Facade | 无 Router 改写；双跑 bit-identical；`edge_routing` 未静默忽略 |

---

## 10. 里程碑

| 里程碑 | 交付 | 验收 | 前置 |
|--------|------|------|------|
| **M0（已落地）** | Params bind · 声明序生命线 · 声明序消息行 · Horizontal + SelfLoop Ink | 简单时序可出图；禁 Router | 无 |
| **M1（已落地）** | Demand（标签宽→间距、自调用行高）· Plan/Metric/Ink verifier · **ADR-009：`LayoutOutput.decorations` + finalize 合并** · 生命线/激活条 decoration | 长标签不压邻线；render 消费 decorations | ADR-009 |
| **M2（已落地）** | 激活条派生与附着 depth · lifeline crossings/notch | 嵌套调用可读 | M1 |
| **M3（已落地）** | 一维排列可选策略 · 显式 pin | 优化不破坏声明硬约束 | M0 |
| **M4（已落地）** | 组合片段框（走 decorations.FragmentFrame，**不**进 `Graph::groups`） | 嵌套包含、无非法交叠 | M1 |
| **后置** | Async slanted · Lost/Found · 甘特连续轴 | 显式 `LayoutError` 直至落地 | — |

---

## 11. 反模式

1. Ink 按 `Arrow::Response` 改路径拓扑
2. Ink 发明异步斜率或自调用宽度
3. 长标签溢出后在 Ink 挪生命线
4. 把「穿过生命线」当碰撞失败而绕行
5. sequence 启用独立 EdgeRouter / Channel 搜索
6. `HashMap` 收集生命线导致序抖动
7. 优化器覆盖用户声明的生命线序
8. 激活条仅由 render 猜测、布局无 span
9. 组内边冒充顶层时间轴
10. 用 Hier rank 模拟消息行号

---

## 12. 文档关系

| 资产 | 角色 |
|------|------|
| **本文** | Sequence 目标架构真源 |
| [message-routing](phases/message-routing.md) | 消息 Builtin 路由细节 |
| [axes](phases/axes.md) | 生命线序与时间行 |
| [scope](scope.md) | 能力 / 非目标 |
| [19](../../../reference/yfiles/19-序列图与一维排列.md) | 算法证据 |
| v1 `recipes/sequence.rs` | 功能与坑参考 |

---

## 13. 收敛命题

> **Sequence = 固定时间主轴 + 生命线次轴排列 + Builtin 消息路由；**
> 消息拓扑在 Compose 一次写完，Metric 只展开坐标，Ink 永不发明行号与斜率；
> 穿越生命线合法；独立 EdgeRouter 不是本核主路径。
