# EdgeRouter · 能力范围与非目标

> 状态：现行目标
> 姊妹页：[README](README.md) · [architecture](architecture.md)
> 注册名（目标）：`orthogonal`（独立正交主路径）

---

## 1. 做什么

独立 `EdgeRouter` 解决的问题：

```text
节点框已冻结（布局写出，或测试手写）
  + 端口已决议（布局写出，或测试手写）
  → 为每条边求正交折线，避障、少弯、确定性
```

| 能力 | 说明 |
|------|------|
| 节点避障 | 路径不得穿非端点节点内部（障碍 inflate 后） |
| 正交折线 | 轴对齐肘点；端点处垂直出入端口侧 |
| 稳定序路由 | 边处理序、tie-break 全链确定性 |
| 自由位置场景 | 节点不必分层；可用 OVG / reduced interesting lines |
| Defer 衔接 | `edge_routing: Some("orthogonal")` 时替换 path，不改 node/port |
| 手写夹具 | 可不经任何 LayoutAlgorithm，直接构造 `RouteScene` 开发与验真 |

---

## 2. 非目标（故意不做）

| 非目标 | 去向 |
|--------|------|
| 发明 / 移动端口 | Layout Compose（或测试夹具显式给出） |
| 移动节点框 / 改 rank·order | Layout Metric |
| Hier 分层通道主路径 | Hier Builtin Channel Ink |
| Sequence 消息几何 | Sequence BuiltinEdges；禁独立 Router |
| 均匀网格当产品路径 | 仅允许作原型验证，不得进主路径 |
| 增量拖拽重路由（首期） | 后置；先做批处理全量路由 |
| Bus / Steiner 干线聚合（首期） | 后置；可复用 `edge_group` 语义 |
| 曲线 / 样条路由 | 非本 router；另注册名 |
| 按 DiagramType 分支 | ADR-001；只认 options / typed params |

---

## 3. 与其它路径的边界

| 场景 | 边几何写者 |
|------|------------|
| Hier 默认 | Builtin Channel → Ink |
| Hier / Tree / Circular + `edge_routing: orthogonal` | **本 Router** |
| Sequence | Builtin 消息路由；`edge_routing: Some` → Unsupported |
| 用户/测试冻结节点后重布 | **本 Router** |

Hier Channel 与本 Router **共用** `route/core` 无策略原语（肘线规范化、锚点、矩形运算）；**不共用**搜索图构造策略。

---

## 4. 典型消费方

- Hierarchical / Tree / Circular 在 `DeferToRouter` 模式下交出端子。
- CLI / 单测：程序手写 `NodePlacement` + `PortRef` + 边 stub，直接调 `EdgeRouter::route`。
- 将来：交互拖拽后的局部重路由（同一契约，增量实现后置）。

---

## 5. 代码落点

| 模块 | 职责 |
|------|------|
| `plotgram-engine-api` | `EdgeRouter` / `RouteScene`（真输入）+ 过渡 `RouteInput` |
| `plotgram-router/src/core` | 无策略原语（锚点、肘线、矩形、折线规范化、段重叠） |
| `plotgram-router/src/orthogonal` | 本 Router 实现（ovg / search / track） |
| `plotgram-router/src/{verify,score}` | 几何验收门 + 质量度量 |
| `plotgram-algo` | 可复用零件（VPSC nudging、交叉计数等） |
