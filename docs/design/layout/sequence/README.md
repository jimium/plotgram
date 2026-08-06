# SequenceLayout

> 状态：现行设计档（重建中；目标架构已立）
> 引擎注册名：`sequence`
> 代码（目标）：`crates/plotgram-layout/src/layout/sequence/`（与 Hier 同 crate）
> 参考实现：`crates/v1/plotgram-core/src/layout/recipes/sequence.rs`
> **目标架构真源**：[architecture.md](architecture.md)

> **落地前置**：M1 起的生命线/激活条/片段框依赖 [ADR-009](../../adr/009-layout-result-decorations.md) 在 `plotgram-model` / `plotgram-engine-api` 落地 `decorations` 通道；M0 可在「只发 nodes+edges」窗口启动。详见 architecture.md 顶部「前置依赖」。

## 签名

参与者（生命线）轴 + 消息时间轴；**布局阶段产出边几何**（BuiltinEdges）。不可并入 Hierarchical；不可把独立 EdgeRouter 当主路径。

## 基本逻辑

```text
① 次轴    生命线排列（默认声明序；可选一维优化）
② 主轴    消息行号 = 边声明序（无 Edge::seq）
③ 组合    消息拓扑 / 附着 / 激活深度（显式 Plan）
④ 度量    生命线 x、消息 y、激活条、标签 Demand
⑤ 落笔    Horizontal / SelfLoop /（后置 Slanted）纯展开
```

完整管线、IR、消息路由与里程碑见 **[architecture.md](architecture.md)**。
消息边几何细节见 **[phases/message-routing.md](phases/message-routing.md)**。

## 能力范围 · 非目标 · 典型域

见 [scope.md](scope.md)。摘要：

| | |
|--|--|
| **做** | 生命线、水平消息、自调用 U 形、返回消息同拓扑异样式、标签预留下间距 |
| **不做** | Sugiyama/Channel；独立 edge_routing；Ink 发明斜率；组内边当一等时间轴 |
| **典型域** | `profile: sequence` → `layout: sequence` |

## 边几何

- **唯一主路径**：本核 Builtin Ink（`EdgeGeometryMode::Builtin`）。
- **禁止**：`edge_routing: Some(...)`（目标：硬失败 `Unsupported`，与 dsl-spec 表一致）。
- 穿越中间生命线**合法**；可选 notch/hop 装饰，不改路径拓扑。

## 写权（本核）

| 自由度 | 写者 |
|--------|------|
| 生命线序 | Compose（LifelineOrderWriter） |
| 消息行号 / kind / route topo / 附着 | Compose |
| 激活深度与起止行 | Compose → Metric 出框 |
| 生命线 x、消息 y、terminals | Metric |
| path 点列、箭头/虚线、缺口 | Ink（只展开） |

纪律全文：[写权纪律](../write-authority.md)。

## 相关阅读

| 文档 | 用途 |
|------|------|
| **[architecture.md](architecture.md)** | 目标架构 |
| [phases/](phases/README.md) | 轴 + 消息路由细节 |
| [19 序列图与一维排列](../../../reference/yfiles/19-序列图与一维排列.md) | 算法证据 |
| [dsl-spec §8.1](../../../specs/dsl-spec.md) | 声明序 = 时间轴 |
| [model-boundary 时序](../../model-boundary.md) | 派生几何不进 Graph |
| [ADR-009](../../adr/009-layout-result-decorations.md) | LayoutResult.decorations 契约 |
| [ADR-003](../../adr/003-edge-structural-fields.md) | 边结构字段；无 seq |
