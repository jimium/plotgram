# 状态图 (state)

## 定位

描述**有限状态机**：系统在离散状态间如何迁移，以及触发迁移的条件或事件。

**核心问题**：*处于什么状态、在什么条件下变成什么状态？*

---

## 适用场景

| ✅ 适合 | ❌ 不适合 |
|---------|----------|
| 订单生命周期（待支付 → 已发货 → 已完成） | 无状态概念的流程步骤（用 `flowchart`） |
| TCP 连接状态（ESTABLISHED / FIN_WAIT） | 多方消息时序（用 `sequence`） |
| 支付状态机、工单状态 | 组件部署拓扑（用 `architecture`） |
| 含分支选择的状态转换 | 数据库表关系（用 `er`） |

---

## 语法入口

```drawify
diagram state {
    title: "订单状态机"

    entity init "初始" { type: initial }
    entity pending "待支付" { type: state }
    entity paid "已支付" { type: state }
    entity done "完成" { type: final }

    init -> pending
    pending -> paid "支付成功"
    paid -> done "确认收货"
}
```

> **书写提示**：`start` / `process` / `end` / `decision` 会自动归一化为 `initial` / `state` / `final` / `choice`。推荐直接使用规范 type。

---

## 允许的实体 type

| type | 语义 | 视觉形状 | 别名（自动归一化） |
|------|------|----------|-------------------|
| `initial` | 初始伪状态 | 实心圆（绿） | `start` → `initial` |
| `state` | 普通状态 | 圆角矩形（蓝） | `process` → `state` |
| `final` | 终止伪状态 | 双圆（红） | `end` → `final` |
| `choice` | 选择/分支点 | 菱形（黄） | `decision` → `choice` |

完整列表以 `STATE_ENTITY_TYPES`（`diagram/registry.rs`）为准。

**结构约束**：
- 全图最多 **1 个** `initial` 节点（校验 enforced）
- 可有多个 `final` 状态（多种终止路径）
- `choice` 出边标签应写清分支条件

---

## 关系箭头约定

在状态图中，relation 表示**状态转换**：

| 箭头 | 含义 | 视觉 |
|------|------|------|
| `->` | 状态迁移（事件/条件触发） | 实线 + 箭头 |
| `-->` | 较少使用；可表示异步完成后的迁移 | 实线 + 箭头 |
| `<->` | 双向可逆转换 | 实线 + 双向箭头 |

**写作规范**：
- 边上标签写**触发事件**或**守卫条件**（如 `"超时 30s"`、`"支付失败"`）
- 从 `initial` 出发有且仅有一条（或多条等价）进入路径
- 复杂分支用 `choice` 而非多个 unnamed 分叉

---

## 布局与视觉默认值

| 属性 | 默认值 | 说明 |
|------|--------|------|
| `layout-algo` | `circular` | 默认圆形布局，适合状态机与环状关系 |
| `edge-routing` | `circular` | 弧形边，配合圆形节点布局 |
| 样式方案 | `builtin.clean-light` | 亮色简洁主题 |

工作流、审批流、生命周期等“主路径明显”的状态图，可显式指定 `layout-algo: sugiyama-v2` 使用分层布局。若状态天然呈环状或网状，默认 `circular` 表达更自然。

---

## 分组 (group)

状态图一般**不使用** group — 状态应在同一平面展示转换关系。若状态极多，考虑拆成多张图。

---

## 写作规范

1. **必有 `initial`，建议有 `final`** — 读者需要明确入口与出口。
2. **状态名用名词或形容词短语** — `"待支付"` 而非 `"去支付"`。
3. **边上写触发条件** — 无标签的迁移在复杂图中难以解读。
4. **互斥分支用 `choice`** — 菱形节点后接多条出边。
5. **不要用 `flowchart` 的 `start`/`end` 语义** — 虽可别名，但 `initial`/`final` 才是规范写法。

---

## 实现状态

✅ 专属渲染器已实现。默认采用圆形布局；若需要分层展示，可显式使用 `layout-algo: sugiyama-v2`。写作时以本文档规范 type 为准。

---

## 示例

| 复杂度 | 路径 | 说明 |
|--------|------|------|
| 简单 | `showcase/state/s.traffic-light.dfy` | 三状态循环 |
| 简单 | `showcase/state/s.on-off.dfy` | 开关状态 |
| 正常 | `showcase/state/n.order-lifecycle.dfy` | 订单生命周期 |
| 复杂 | `showcase/state/c.payment-flow.dfy` | 支付状态机 |

---

## 参见

- [实体类型标准](../entity-types.md) — 状态专用 type 与别名
- [布局算法 — 状态图](../../../architecture/layout-algorithms.md)
- [流程图](./flowchart.md) — 步骤流 vs 状态机
- [language-spec.md](../../language-spec.md)
