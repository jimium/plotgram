# 流程图 (flowchart)

## 定位

描述**步骤、分支、循环**的业务或技术流程。读者应能沿箭头方向理解「先做什么、再做什么、在什么条件下分叉」。

**核心问题**：*这件事按什么顺序、在什么条件下推进？*

---

## 适用场景

| ✅ 适合 | ❌ 不适合 |
|---------|----------|
| 用户登录/注册流程 | 多方按时间顺序交互（用 `sequence`） |
| 审批流、工单处理 | 系统组件拓扑（用 `architecture`） |
| 决策分支、循环回退 | 状态机生命周期（用 `state`） |
| 软件发布流水线 | 数据库表关系（用 `er`） |

---

## 语法入口

```drawify
diagram flowchart {
    layout: top-to-bottom    // 或 left-to-right
    title: "用户登录流程"

    entity start "开始" { type: start }
    entity login "用户登录" { type: process }
    entity end "结束" { type: end }

    start -> login
    login -> end
}
```

---

## 允许的实体 type

| type | 语义 | 视觉形状 | 使用建议 |
|------|------|----------|----------|
| `start` | 流程起点 | 体育场形（圆角胶囊） | 每张图建议 1 个；无属性时默认 `process` |
| `end` | 流程终点 | 体育场形 | 可与 `start` 配对 |
| `process` | 处理步骤 | 圆角矩形 | 最常用，无 `type` 时的默认值 |
| `decision` | 决策/分支 | 菱形 | 允许自环（`a -> a`） |
| `service` | 微服务/后端 | 圆角矩形 | 技术流程中标注具体服务 |
| `database` | 数据库 | 圆柱体 | 读写数据步骤 |
| `person` | 人/角色 | 人形图标 | 人工操作节点 |
| `client` | 客户端 | 圆角矩形 | 用户侧应用 |
| `gateway` | 网关 | 圆角矩形 | API 网关、入口 |
| `queue` | 消息队列 | 圆角矩形 | 异步消息环节 |
| `cache` | 缓存 | 圆角矩形 | 缓存读写 |
| `storage` | 文件/对象存储 | 圆角矩形 | 对象存储访问 |
| `external` | 外部系统 | 圆角矩形 | 第三方依赖 |

完整列表以 `FLOWCHART_ENTITY_TYPES`（`diagram/registry.rs`）为准。

---

## 关系箭头约定

| 箭头 | 在流程图中的含义 | 视觉 |
|------|-----------------|------|
| `->` | 流程推进、调用、数据流出 | 实线 + 箭头 |
| `-->` | 返回、回调、异步响应 | **虚线** + 箭头 |
| `<->` | 双向依赖或同步 | 实线 + 双向箭头 |

**约束**：
- 禁止自环，除非源节点 `type: decision`
- relation 只能写在 diagram 顶层，不能写在 group 内

---

## 布局与视觉默认值

| 属性 | 默认值 | 说明 |
|------|--------|------|
| `layout-algo` | `sugiyama-v2` | 分层布局，适合 DAG 与含回边流程 |
| `edge-routing` | `orthogonal` | 正交折线，便于阅读分支 |
| `layout` | `top-to-bottom` | 可改为 `left-to-right` |
| 样式方案 | `builtin.clean-light` | 亮色简洁主题 |

布局算法细节见 [layout-algorithms.md §流程图](../../../architecture/layout-algorithms.md)。

---

## 分组 (group)

### 需求结论

| 维度 | 判断 |
|------|------|
| 是否需要语法支持 | **需要** — 复杂流程按阶段/子域归类是常见诉求 |
| 是否每张图都要有 | **不需要** — 简单线性流程可以没有 group |
| 与架构图是否同一概念 | **部分相同** — 都是视觉+逻辑分组，但布局角色不同 |

流程图**应该有** `group`，用于复杂流程的分段阅读；简单图可以没有。它是**增强项**，不是流程图的定义性特征 — 与架构图里「分层即本体」的 group 层级不同。

### 为什么需要 group

读者理解流程图时通常要同时把握两件事：

1. **顺序** — 先做什么、后做什么（靠 `entity` + `relation`）
2. **归属** — 某步属于哪个阶段/部门/子系统（靠 `group`）

典型场景：

- 发布流水线：构建 → 测试 → 部署各成一框
- 政务/审批流：申请、审核、办结分段
- 跨系统流程：前端交互、后端处理、第三方回调分块
- 复杂决策树：用 group 标出「资格判断」「材料准备」等阶段

语法与约束见 [language-spec.md §7](../../language-spec.md)；跨图表通用分组能力见 [features.md §F5](../../../product/features.md)。

### 与架构图 group 的区别

| | 架构图 | 流程图 |
|---|--------|--------|
| group 的角色 | **语义舞台** — 先定组，再在框内摆节点 | **阅读辅助** — 主轴仍是控制流/数据流 |
| 布局驱动 | `architecture-v2` 两阶段：组内 → 组间 | `sugiyama-v2` 全局分层，group 为后验包围框 |
| group 属性 | 支持 `layout`、`group_sizing: uniform` 等 | 语法支持 group，暂无组内/组间专用布局 hint |
| 推荐使用强度 | **强烈推荐** | **按需使用** |

架构图 group 是系统分层/域边界的本体；流程图 group **不参与连线**，仅作视觉与逻辑归类，帮助读者分段阅读。详见 [architecture.md §分组](./architecture.md#分组-group)。

### 什么时候不该用 group

- 步骤少、一条链读到底 → 不必强行分组
- 重点是**系统拓扑**而非**步骤顺序** → 改用 `architecture`
- 需要严格泳道（按角色/部门约束节点位置）→ 单靠 group 不够，属于泳道/swimlane 类需求（尚未作为一等类型）

### 语法示例

```drawify
group backend "后端处理" {
    entity api "API" { type: service }
    entity db "数据库" { type: database }
}

group frontend "前端交互" {
    entity web "Web 客户端" { type: client }
}

web -> api
api -> db
```

- 最多嵌套 2 层
- relation 写在 diagram 顶层，不能写在 group 内
- group 内 entity ID 全局唯一

### 实现现状

- **解析与渲染**：流程图已支持 group 声明与包围框绘制
- **布局**：`sugiyama-v2` 先全局摆节点，再经 `compute_group_bounds` 计算包围框；尚未像架构图那样以 group 驱动组内/组间布局
- **路由友好性**：布局评估含 `group_gap`，用于检测跨 group 边的通道是否充裕

若未来需要「阶段条带等宽、组内独立排版」等能力，可再引入流程图专用的 group 布局策略。

---

## 写作规范

1. **有明确起点时标注 `type: start`**，避免读者不知从何读起。
2. **决策节点用 `decision`**，出边标签写清条件（如 `"余额充足?"`）。
3. **技术流程混用 `process` 与 `service`** — 抽象步骤用 `process`，已确定的服务用 `service`。
4. **回环边表达重试/循环**，引擎会自动处理回边布局。
5. **避免在流程图中画过多组件类型** — 若重点是系统拓扑而非步骤顺序，改用 `architecture`。
6. **复杂流程按需加 group** — 按阶段或子系统分段；简单线性链不必强行套框。

---

## 示例

| 复杂度 | 路径 | 说明 |
|--------|------|------|
| 简单 | `showcase/flowchart/s.linear-chain.dfy` | 线性三步 |
| 简单 | `showcase/flowchart/s.decision-loop.dfy` | 决策与回环 |
| 正常 | `showcase/flowchart/n.user-auth.dfy` | 用户认证 |
| 复杂 | `showcase/flowchart/c.software-release.dfy` | 发布流水线 |

---

## 参见

- [实体类型标准](../entity-types.md) — 跨图表 type 矩阵与别名
- [语言规范 §5 entity type](../../language-spec.md)
- [布局算法 — 流程图](../../../architecture/layout-algorithms.md)
- [时序图](./sequence.md) — 交互时序的替代选型
