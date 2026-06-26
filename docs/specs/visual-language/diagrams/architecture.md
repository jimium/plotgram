# 架构图 (architecture)

## 定位

描述**系统由哪些组件构成、彼此如何依赖**。强调组件角色与连接关系，而非步骤顺序或消息时序。

**核心问题**：*系统由什么组成、如何连接？*

---

## 适用场景

| ✅ 适合 | ❌ 不适合 |
|---------|----------|
| 三层架构、微服务拓扑 | 逐步业务流程（用 `flowchart`） |
| 云原生技术栈、事件驱动架构 | 请求时序（用 `sequence`） |
| 组件依赖、数据流向 | 状态迁移（用 `state`） |
| 按层/域分组的系统设计 | ER 数据模型（用 `er`） |

---

## 语法入口

```drawify
diagram architecture {
    title: "三层架构"

    entity user "用户" { type: frontend }
    entity api "API 服务" { type: service }
    entity db "主库" { type: database }

    user -> api "HTTP"
    api -> db "SQL"
}
```

---

## 允许的实体 type

架构图使用**组件语义**的 type，与流程图的步骤语义不同：

| type | 语义 | 视觉形状 | 别名（自动归一化） |
|------|------|----------|-------------------|
| `frontend` | 前端/客户端 | 矩形（蓝） | `client`, `person` → `frontend` |
| `backend` | 后端聚合层 | 圆角矩形（绿） | — |
| `service` | 微服务/后端服务 | 圆角矩形（绿） | `start`, `end`, `process`, `decision` → `service` ⚠️ |
| `database` | 数据库 | 圆柱体（橙） | — |
| `gateway` | API 网关 | 六边形（青） | — |
| `cache` | 缓存 | 菱形（红） | — |
| `queue` | 消息队列 | 倾斜矩形（黄） | — |
| `storage` | 对象/文件存储 | 矩形（棕） | — |
| `external` | 外部系统 | 虚线边框矩形（灰） | — |

完整列表以 `ARCHITECTURE_ENTITY_TYPES`（`diagram/registry.rs`）为准。

> ⚠️ 流程图专用 type（`start`/`end`/`process`/`decision`）在架构图中会被归一化为 `service`，校验器会发出**警告**，建议改用上表中的组件 type。

---

## 关系箭头约定

| 箭头 | 在架构图中的含义 | 视觉 |
|------|-----------------|------|
| `->` | 调用、数据流、依赖方向 | 实线 + 箭头 |
| `-->` | 响应、回调 | 实线 + 箭头（架构图默认不虚线） |
| `<->` | 双向依赖、对等连接 | 实线 + 双向箭头 |

**写作规范**：
- 标签注明协议或数据类型（`"gRPC"`, `"Kafka"`, `"HTTPS"`）
- 自环会触发警告（架构图中通常无意义）
- 数据流方向应与技术现实一致（客户端 → 服务 → 数据库）

---

## 布局与视觉默认值

| 属性 | 默认值 | 说明 |
|------|--------|------|
| `layout-algo` | `architecture-v2` | 默认分组分层；`force-directed` 适合关系复杂、需自然散开的拓扑 |
| `edge-routing` | `orthogonal` | 正交折线表达依赖与绕行 |
| 样式方案 | `builtin.blueprint` | 蓝图风格，偏技术图纸感 |

含 `group` 时，分组包围框在力导向结果之上叠加绘制。

---

## 分组 (group)

### 需求结论

| 维度 | 判断 |
|------|------|
| 是否需要语法支持 | **需要** — 分层、域边界是架构图的核心阅读结构 |
| 是否每张图都要有 | **视复杂度而定** — 简单三层可省略；多服务/多域系统**强烈推荐** |
| 与流程图是否同一概念 | **部分相同** — 都是视觉+逻辑分组，但布局角色不同 |

架构图**应该有** `group`，且对复杂系统接近**定义性特征** — group 表达「系统由哪些层/域构成」，而不只是套框装饰。简单拓扑（如 client → api → db）可以没有 group；一旦涉及微服务、云原生、多业务域，group 通常是读者理解图的第一入口。

### 为什么需要 group

读者理解架构图时通常要同时把握两件事：

1. **连接** — 谁依赖谁、数据如何流动（靠 `entity` + `relation`）
2. **边界** — 组件属于哪一层、哪个域、哪个子系统（靠 `group`）

典型场景：

- 三层/多层架构：接入层、业务层、数据层
- 微服务拓扑：按业务域（订单、用户、支付）或按技术层分组
- 云原生全栈：Ingress、K8s 集群（含嵌套子组）、可观测性、持久化
- 数据流水线：采集 → 计算 → 存储各成等宽阶段条带
- 企业/K8s 聚合视图：Namespace、Workload 等自动映射为 group（见 [scale-diagram-strategy.md](../../../enterprise/scale-diagram-strategy.md)）

语法与约束见 [language-spec.md §7](../../language-spec.md)；跨图表通用分组能力见 [features.md §F5](../../../product/features.md)。

### 与流程图 group 的区别

| | 架构图 | 流程图 |
|---|--------|--------|
| group 的角色 | **语义舞台** — 先定组，再在框内摆节点 | **阅读辅助** — 主轴仍是控制流/数据流 |
| 布局驱动 | `architecture-v2` 两阶段：组内 → 组间 | `sugiyama-v2` 全局分层，group 为后验包围框 |
| group 属性 | 支持 `layout`、`group_sizing: uniform` 等 | 语法支持 group，暂无组内/组间专用布局 hint |
| 推荐使用强度 | **强烈推荐** | **按需使用** |

架构图 group 是系统分层/域边界的本体；流程图 group **不参与连线**，仅作视觉与逻辑归类。详见 [flowchart.md §分组](./flowchart.md#分组-group)。

### 什么时候不该用 group

- 仅 2～3 个组件、无分层语义 → 可省略 group，避免空框
- 表达的是**步骤顺序**而非**组件拓扑** → 改用 `flowchart`
- 需要严格泳道（按角色/部门约束位置）→ group 可表达域边界，但不等价于泳道布局

### 组内布局 `layout`

```drawify
group process "数据计算层" {
    layout: fan-out   // auto | horizontal | vertical | fan-out
    ...
}
```

### 等宽阶段条带 `group_sizing`

流水线/分层类图可在 diagram 级声明等宽分组：

```drawify
diagram architecture {
    group_sizing: uniform   // fit（默认）| uniform
    ...
}
```

`uniform` 时所有顶层 group 拉齐到最宽者，组内节点水平居中，形成整齐的阶段条带。见 `showcase/architecture/n.data-pipeline.dfy`。

```drawify
group backend "后端服务" {
    style: dashed

    entity api "API Gateway" { type: gateway }
    entity order "订单服务" { type: service }
    entity user_svc "用户服务" { type: service }
}

group data "数据层" {
    entity pg "PostgreSQL" { type: database }
    entity redis "Redis" { type: cache }
}
```

- 最多嵌套 2 层
- `style: solid | dashed | dotted` 区分边界强度
- group 内 entity 可与任意其他 entity 建立 relation
- relation 写在 diagram 顶层，不能写在 group 内
- group 内 entity ID 全局唯一

### 实现现状

- **解析与渲染**：架构图完整支持 group 声明、嵌套与包围框绘制
- **布局**：含顶层 group 时，`architecture-v2` 走两阶段布局（组内 Sugiyama/按 `layout` hint 排版 → 组间宏观定位 → 全局坐标回填）；无 group 时退化为分组感知的单层布局
- **组内 hint**：`layout: auto | horizontal | vertical | fan-out` 控制组内排版；diagram 级 `group_sizing: uniform` 拉齐顶层 group 宽度
- **力导向**：`force-directed` 模式下节点先力导布局，group 包围框后验叠加
- **路由友好性**：跨 group 边通道评估见布局友好性体系中的 `group_gap`

---

## 写作规范

1. **用组件 type，不用流程 type** — 写 `service` 而非 `process`，写 `frontend` 而非 `client`（虽可别名，但语义更清晰）。
2. **一张图聚焦一个抽象层级** — 不要混画「用户故事步骤」与「K8s Pod」。
3. **外部系统标 `external`** — 虚线边框与普通组件区分。
4. **网关、队列、缓存用专属 type** — 形状自带语义，减少标签负担。
5. **复杂系统用 group 分区** — 按业务域或技术层分组；简单三层拓扑可省略。
6. **分层图配合 `group_sizing: uniform`** — 流水线/阶段类架构形成等宽条带，便于横向对比各层。

---

## 示例

| 复杂度 | 路径 | 说明 |
|--------|------|------|
| 简单 | `showcase/architecture/s.client-api-db.dfy` | 三层单向流 |
| 简单 | `showcase/architecture/s.three-tier.dfy` | 经典三层 |
| 正常 | `showcase/architecture/n.microservices.dfy` | 微服务分组 |
| 复杂 | `showcase/architecture/c.cloud-native.dfy` | 云原生全栈 |

---

## 参见

- [实体类型标准](../entity-types.md) — 组件 type 与流程 type 区分
- [布局算法 — 架构图](../../../architecture/layout-algorithms.md)
- [流程图](./flowchart.md) — 步骤语义 vs 组件语义
- [企业场景 — 规模化架构图战略](../../../enterprise/scale-diagram-strategy.md)
