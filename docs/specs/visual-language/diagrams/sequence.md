# 时序图 (sequence)

## 定位

描述**多个参与者按时间顺序**发送与接收消息。强调「谁先说话、谁后回应」，而非流程分支或组件拓扑。

**核心问题**：*各方按什么顺序交互？*

---

## 适用场景

| ✅ 适合 | ❌ 不适合 |
|---------|----------|
| HTTP 请求-响应 | 无时间顺序的步骤流（用 `flowchart`） |
| OAuth / SSO 登录流程 | 静态组件依赖图（用 `architecture`） |
| 微服务调用链、分布式事务 | 状态机转换（用 `state`） |
| 自调用（loop / 递归） | 表结构关系（用 `er`） |

---

## 语法入口

```drawify
diagram sequence {
    title: "请求-响应"

    entity client "客户端" { type: participant }
    entity server "服务端" { type: control }

    client -> server "GET /api/health"
    server --> client "200 OK"
}
```

> **书写提示**：`client` / `service` 等流程图/架构图常用 type 会自动归一化为时序图专用 type（见下表「别名」列）。推荐直接使用规范 type，减少歧义。

---

## 允许的实体 type

时序图采用 UML 风格的参与者分类：

| type | 语义 | 视觉形状 | 别名（自动归一化） |
|------|------|----------|-------------------|
| `participant` | 一般参与者 | 矩形（灰） | 默认值 |
| `actor` | 人类角色 | 矩形（蓝） | `person` → `actor` |
| `boundary` | 边界/接口 | 矩形（橙） | `client`, `gateway` → `boundary` |
| `control` | 控制/服务 | 矩形（绿） | `service` → `control` |
| `entity` | 领域实体 | 矩形（紫） | `queue` → `entity` |
| `database` | 数据库 | 圆柱体（黄） | — |

完整列表以 `SEQUENCE_ENTITY_TYPES`（`diagram/registry.rs`）为准。

---

## 关系箭头约定

在时序图中，relation 表示**消息**，按声明顺序自上而下排列（时间轴）：

| 箭头 | 含义 | 视觉 |
|------|------|------|
| `->` | 同步请求 / 主动消息 | 实线 + 实心箭头 |
| `-->` | 响应 / 返回消息 | **虚线** + **空心箭头** |
| `<->` | 双向消息 | 实线 + 箭头 |

**写作规范**：
- 请求用 `->`，响应用 `-->`，成对出现
- 消息标签写协议或业务语义（如 `"POST /orders"`、`"支付成功"`）
- 参与者声明顺序决定水平排列顺序

---

## 布局与视觉默认值

| 属性 | 默认值 | 说明 |
|------|--------|------|
| `layout-algo` | `sequence` | 专属时序布局：参与者水平铺开，消息垂直排列 |
| 样式方案 | `builtin.clean-light` | 亮色简洁主题 |

时序图 **不支持** `edge-routing` 属性；消息路径由 `layout_algo: sequence` 在布局阶段一并计算。

---

## 分组 (group)

时序图**不建议**使用 group — 参与者应在顶层平铺。若需表达逻辑归属，用 `meta.` 属性或在标签中注明。

---

## 写作规范

1. **参与者用规范 type** — 人形交互选 `actor`，服务选 `control`，数据库选 `database`。
2. **严格按时间顺序声明 relation** — 第 N 条消息在第 N 个时间步。
3. **请求-响应成对** — `A -> B` 后接 `B --> A`，标签对应。
4. **自调用** — `service -> service "校验 token"` 会渲染为 U 形自调用线。
5. **不要用 `flowchart` 的 `start`/`end`** — 时序图无起终点节点概念。

---

## 示例

| 复杂度 | 路径 | 说明 |
|--------|------|------|
| 简单 | `showcase/sequence/s.request-response.dfy` | 单次请求-响应 |
| 简单 | `showcase/sequence/s.ping-pong.dfy` | 双向 ping-pong |
| 正常 | `showcase/sequence/n.oauth-login.dfy` | OAuth 登录 |
| 复杂 | `showcase/sequence/c.distributed-saga.dfy` | 分布式 Saga |

---

## 参见

- [实体类型标准](../entity-types.md) — 时序专用 type 与别名
- [布局算法 — 时序图](../../../architecture/layout-algorithms.md)
- [流程图](./flowchart.md) — 步骤流转的替代选型
- [架构图](./architecture.md) — 组件拓扑的替代选型
