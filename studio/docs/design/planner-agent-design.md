# Planner Agent 设计方案

> 版本：1.0 | 状态：设计中

## 1. 背景与动机

### 1.1 现状问题

当前 `AgentLoop` 是一个单循环架构，将"理解用户意图 → 生成 DSL → 渲染 → 自修复"全部塞进一个 LLM 对话循环、一份 system prompt。这导致：

| 问题 | 表现 |
|------|------|
| **职责过载** | `prompt.ts` 的 SYSTEM_PROMPT 同时承担"理解需求"+"DSL 语法生成"+"错误修复"三种认知任务，prompt 膨胀且互相干扰 |
| **跳过分析** | LLM 倾向于直接吐 DSL，导致实体遗漏、关系方向错误、布局选择不当 |
| **不可解释** | 用户看不到 Agent 对需求的结构化理解，错了只能整图重画 |
| **不可复用** | 意图理解结果混在 DSL 里，无法缓存、无法跨格式复用、无法人工干预 |
| **图表类型知识缺失** | 不同图表类型的 entity type 闭集、默认布局、边路由能力各不相同，单 prompt 无法精确覆盖 |

### 1.2 核心思路

把"**理解画图需求 + 构造画图方案**"从执行循环中剥离，交给一个专门的 **Planner Agent**，产出结构化 `DrawingPlan`，再由现有 `AgentLoop`（改造为 Executor）消费。

```
用户消息 ──→ Planner ──→ DrawingPlan ──→ Executor ──→ DSL + SVG
              (理解+规划)   (结构化蓝图)    (翻译+渲染)
```

## 2. 角色划分

| 角色 | 输入 | 输出 | 关注点 | LLM 调用模式 |
|------|------|------|--------|-------------|
| **Planner**（新增） | 用户消息 + 历史 + 当前 DSL + 图表能力清单 | `DrawingPlan` + 可选澄清问题 | 语义正确性、完整性、布局合理性 | 单次调用，不迭代 |
| **Executor**（现有 AgentLoop 改造） | `DrawingPlan` + 当前 DSL | DSL 源码 + SVG + diff | DSL 语法正确性、渲染、自修复 | 多轮迭代（现有逻辑） |

Planner **不生成 DSL**，只生成与格式无关的语义蓝图；Executor **不做需求理解**，只做"计划 → DSL"的翻译与渲染。两者通过 `DrawingPlan` 解耦。

## 3. DrawingPlan 数据结构

### 3.1 核心结构

```ts
/** 画图规划 -- Planner 的唯一输出 */
interface DrawingPlan {
  /** 结构版本号，便于后续演进 */
  version: 1;
  /** 一次规划的唯一 ID，用于关联执行结果 */
  plan_id: string;
  /** 目标图表类型 */
  diagram_type: DiagramKind;
  /** 图表标题 */
  title: string;
  /** 一句话目的说明（给用户看） */
  summary: string;
  /** 置信度，低于阈值时触发澄清流程 */
  confidence: 'high' | 'medium' | 'low';
  /** 实体清单 */
  entities: PlanEntity[];
  /** 关系清单 */
  relations: PlanRelation[];
  /** 分组清单 */
  groups: PlanGroup[];
  /** 布局建议 */
  layout: PlanLayout;
  /** Agent 做出的假设（用户可见） */
  assumptions: string[];
  /** 需要向用户澄清的问题（非空时进入 Clarify 模式） */
  open_questions: PlanQuestion[];
  /** 相对上一版的变更意图（编辑场景） */
  edit_intent?: PlanEditIntent;
}
```

### 3.2 实体

```ts
interface PlanEntity {
  /** 实体 ID，符合 [a-z][a-z0-9_]* */
  id: string;
  /** 显示标签 */
  label: string;
  /** 实体类型（必须是 diagram_type 对应的规范闭集成员） */
  type: string;
  /** 语义图标（可选，如 redis/postgres） */
  semantic?: string;
  /** 所属分组 ID（可选） */
  group?: string;
  /** Planner 给 Executor 的备注（可选，如"这是入口节点"） */
  note?: string;
}
```

### 3.3 关系

```ts
interface PlanRelation {
  /** 起点实体 ID */
  from: string;
  /** 终点实体 ID */
  to: string;
  /** 关系标签（可选） */
  label?: string;
  /** 箭头类型：active(->) 或 passive(-->) */
  arrow: 'active' | 'passive';
  /** Planner 给 Executor 的备注（可选，如"数据流方向"） */
  note?: string;
}
```

### 3.4 分组

```ts
interface PlanGroup {
  /** 分组 ID */
  id: string;
  /** 分组标签 */
  label: string;
  /** 成员实体 ID 列表 */
  members: string[];
}
```

### 3.5 布局

```ts
interface PlanLayout {
  /** 布局方向 */
  direction: 'top-to-bottom' | 'left-to-right' | 'radial';
  /** 布局算法（可选，不指定则使用图表类型默认算法） */
  layout_algo?: string;
  /** 边路由算法（可选，不指定则使用图表类型默认算法） */
  edge_routing?: string;
}
```

### 3.6 澄清问题

```ts
interface PlanQuestion {
  /** 问题内容 */
  question: string;
  /** 可选选项（提供时 UI 渲染为选择按钮） */
  options?: string[];
  /** 默认假设（用户不回答时采用） */
  default?: string;
}
```

### 3.7 编辑意图

```ts
interface PlanEditIntent {
  /** 编辑模式 */
  mode: 'create' | 'append' | 'modify' | 'rebuild';
  /** 受影响的实体/关系 ID 列表 */
  targets: string[];
  /** 编辑说明（给用户看） */
  description: string;
}
```

### 3.8 设计要点

- `DrawingPlan` **不含任何 DSL 语法**，纯语义描述，便于跨格式、跨主题复用。
- `note` 字段允许 Planner 向 Executor 传递"为什么这么排"的提示，但不强制。
- `open_questions` 非空时不进入执行，先回到用户。
- `edit_intent` 让 Executor 知道是增量改还是重写，决定用 `apply_patch` 还是 `render`。
- `type` 字段必须是目标图表类型的规范闭集成员（由 Planner prompt 中的知识注入保证，由 `planValidator` 校验）。

## 4. 图表类型知识注入

Planner 需要精确掌握每种图表类型的约束，这些知识通过 **动态注入** 到 system prompt 中，而非硬编码在 prompt 模板里。

### 4.1 知识来源

| 知识项 | 来源 | 注入方式 |
|--------|------|----------|
| 图表类型列表 + 默认布局 + 默认边路由 | `wasm.layout_catalog()` | 调用后序列化注入 |
| 各图表类型的 entity type 闭集 | 内置静态映射表 | 模板渲染 |
| 各图表类型的推荐布局方向 | 内置静态映射表 | 模板渲染 |
| 当前 DSL 的已有实体/关系 | `wasm.parse_to_json()` | 条件注入（编辑场景） |

### 4.2 图表类型能力表（内置静态映射）

```ts
const DIAGRAM_PROFILES: Record<DiagramKind, DiagramProfile> = {
  flowchart: {
    entity_types: ['start', 'end', 'process', 'decision', 'service', 'database',
                   'person', 'client', 'gateway', 'queue', 'cache', 'storage', 'external'],
    default_type: 'process',
    recommended_layout: 'top-to-bottom',
    supports_groups: true,
    supports_edge_routing: true,
  },
  sequence: {
    entity_types: ['participant', 'actor', 'boundary', 'control', 'entity', 'database'],
    default_type: 'participant',
    recommended_layout: 'left-to-right',  // 时序图布局方向由算法内置
    supports_groups: false,
    supports_edge_routing: false,
  },
  architecture: {
    entity_types: ['frontend', 'backend', 'service', 'database', 'gateway',
                   'cache', 'queue', 'storage', 'external'],
    default_type: 'service',
    recommended_layout: 'left-to-right',
    supports_groups: true,
    supports_edge_routing: true,
  },
  state: {
    entity_types: ['initial', 'state', 'final', 'choice'],
    default_type: 'state',
    recommended_layout: 'radial',
    supports_groups: false,
    supports_edge_routing: true,
  },
  er: {
    entity_types: ['*'],  // ER 图不限制 type 闭集
    default_type: 'database',
    recommended_layout: 'top-to-bottom',
    supports_groups: true,
    supports_edge_routing: true,
  },
  mindmap: {
    entity_types: ['root', 'main', 'branch', 'leaf'],
    default_type: 'branch',
    recommended_layout: 'radial',
    supports_groups: false,
    supports_edge_routing: true,
  },
};
```

### 4.3 注入时机

Planner 的 system prompt 在每次调用时动态构建，分三段拼接：

```
[固定段] 角色定义 + 输出格式 + 规则
[动态段] 图表类型能力表 + layout_catalog 摘要
[上下文段] 当前 DSL 解析结果（编辑场景）
```

## 5. Planner 工作流程

```
用户消息 + AgentContext
       │
       ▼
┌──────────────────────────────────┐
│ 1. 能力探测（同步，无 LLM 调用）   │
│    - 调 layout_catalog()          │
│    - 查 DIAGRAM_PROFILES          │
│    - 编辑场景: parse 当前 DSL      │
└──────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────┐
│ 2. 构建 Planner Prompt            │
│    - 固定段 + 动态段 + 上下文段    │
│    - 附 few-shot 示例              │
└──────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────┐
│ 3. 规划调用（单次 LLM 调用）       │
│    - 强制 JSON schema 输出         │
│    - 产出 DrawingPlan JSON        │
└──────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────┐
│ 4. Plan 校验（同步，无 LLM 调用）  │
│    - JSON 解析                     │
│    - id 唯一性                     │
│    - 关系引用存在性                 │
│    - type 合法性（对照闭集）        │
│    - 分组闭合性                    │
│    - layout 合法性                 │
└──────────────────────────────────┘
       │
       ├── 校验通过 ──→ 步骤 5
       │
       └── 校验失败 ──→ 重试一次（回灌错误给 LLM）
                         │
                         ├── 重试成功 ──→ 步骤 5
                         └── 重试仍失败 ──→ 降级走旧路径
       │
       ▼
┌──────────────────────────────────┐
│ 5. 路由决策                       │
│    - open_questions 非空 → Clarify │
│    - confidence=low 且无问题       │
│      → Clarify                    │
│    - 否则 → 交付 Executor          │
└──────────────────────────────────┘
       │
       ▼
   交付 / 澄清 / 降级
```

### 5.1 关键设计决策

**Planner 只调用一次 LLM**（不迭代），保持轻量。校验失败时最多重试 1 次，把错误回灌给 LLM 修正。两次都失败则降级。

**不使用 Tool-Calling**：Planner 不调用 render/validate 等工具。它需要的知识（layout_catalog、当前 DSL 结构）在调用前同步获取并注入 prompt。

## 6. Plan 校验规则

`planValidator` 在 Planner 产出后、路由决策前执行，是纯同步函数，不调用 LLM 或 WASM。

| 校验项 | 规则 | 错误级别 |
|--------|------|----------|
| JSON 解析 | 必须是合法 JSON，符合 DrawingPlan schema | 致命 |
| plan_id | 非空字符串 | 致命 |
| diagram_type | 必须是 6 种合法枚举之一 | 致命 |
| entity id 唯一性 | 所有 entity.id 互不相同 | 致命 |
| entity type 合法性 | 每个 entity.type 必须属于 diagram_type 对应的闭集（ER 图豁免） | 致命 |
| relation 引用存在性 | relation.from 和 relation.to 必须引用已定义的 entity.id | 致命 |
| relation 自环 | from === to 时发出警告（仅 decision/actor 允许） | 警告 |
| group 成员存在性 | group.members 中每个 id 必须引用已定义的 entity.id | 致命 |
| group 嵌套 | 不允许 group 嵌套（最大嵌套 1 层） | 致命 |
| group 互斥 | 同一 entity 不能属于多个 group | 警告 |
| layout.direction | 必须是合法枚举值 | 致命 |
| layout.layout_algo | 如果指定，必须在 layout_catalog 的 layouts 列表中 | 警告 |
| layout.edge_routing | 如果指定，必须在 layout_catalog 的 edge_routings 列表中；且 diagram_type 必须支持 edge_routing | 警告 |
| 空实体列表 | entities 不能为空（创建场景） | 致命 |
| 空关系列表 | relations 可以为空（如思维导图） | -- |

致命错误触发重试，警告仅记录日志不阻断。

## 7. 与现有架构的集成

### 7.1 改造后的调用链

```
useAgent (hook)
       │
       ▼
runPlanner(userMessage, context, plannerConfig)   ← 新增
       │  返回 PlannerResult { plan, clarify?, degraded? }
       │
       ├── degraded=true → 走旧路径 runAgentLoop(userMessage, context, config)
       │
       ├── clarify 非空 → 返回用户（等待回答后重新规划）
       │
       └── plan 有效 → 继续
              │
              ▼
       runAgentLoop(plan, context, config)         ← 改造：入参从 userMessage 改为 plan
              │  Executor prompt 改为"按 plan 生成 DSL"
              │
              ▼
       AgentResult
```

### 7.2 Executor 的改造点

| 文件 | 改动 |
|------|------|
| `AgentLoop.ts` | 签名从 `runAgentLoop(userMessage, context, config)` 改为 `runAgentLoop(plan, context, config)`，plan 类型为 `DrawingPlan \| string`（string 时走旧逻辑） |
| `prompt.ts` | SYSTEM_PROMPT 删掉"理解用户需求"部分，改为"你收到一份 DrawingPlan，把它翻译成 Drawify DSL"；`buildMessages` 增加 plan 序列化注入 |
| `types.ts` | `AgentConfig` 增加 `enablePlanner` 和 `planMode` 字段；`AgentContext` 增加 `currentPlan` 字段 |
| `context.ts` | 增加 `updateContextPlan` 函数 |

### 7.3 useAgent Hook 的改造点

| 改动 | 说明 |
|------|------|
| `sendMessage` 增加 Planner 阶段 | 先调 `runPlanner`，再根据结果决定走 Executor 还是 Clarify |
| 新增 `pendingPlan` 状态 | 存储待确认的 DrawingPlan，供 UI 展示 |
| 新增 `confirmPlan` 方法 | 用户确认 plan 后触发 Executor |
| 新增 `answerQuestion` 方法 | 用户回答澄清问题后重新规划 |
| 降级逻辑 | Planner 失败时自动回退到旧路径，UI 提示"规划失败，已使用直接模式" |

### 7.4 配置开关

```ts
interface AgentConfig {
  // ... 现有字段
  /** 是否启用 Planner（关闭则回退到旧行为） */
  enablePlanner: boolean;
  /** 规划模式 */
  planMode: 'auto' | 'confirm' | 'clarify';
}
```

| planMode | 行为 | 适用场景 |
|----------|------|----------|
| `auto` | Planner 产出 plan → 直接交 Executor | 简单需求、置信度高 |
| `confirm` | Planner 产出 plan → UI 展示 plan 摘要 → 用户确认/编辑 → Executor | 默认推荐，复杂图表 |
| `clarify` | Planner 产出 open_questions → 问用户 → 重新规划 → Executor | 需求模糊、关键字段缺失 |

`enablePlanner: false` 时直接走旧的 `runAgentLoop(userMessage, ...)`，便于灰度与对比。

配置持久化到 `localStorage`，与现有 `configStorage.ts` 的模式一致。

## 8. 编辑场景处理

编辑场景（用户在已有图表上提出修改）是 Planner 的关键场景，需要特殊处理。

### 8.1 编辑意图识别

Planner 通过对比"用户修改请求"和"当前 DSL 结构"来推断 `edit_intent.mode`：

| 模式 | 触发条件 | Executor 策略 |
|------|----------|--------------|
| `create` | 当前无 DSL（空白画布） | 整体生成，用 `render` |
| `append` | 当前有 DSL，用户要求"加一个 XX" | 用 `apply_patch` 增量添加 |
| `modify` | 当前有 DSL，用户要求"把 A 改成 B" | 用 `apply_patch` 精确修改 |
| `rebuild` | 当前有 DSL，但用户需求与现有结构差异过大 | 整体重写，用 `render` |

### 8.2 当前 DSL 注入

编辑场景下，Planner prompt 需要注入当前 DSL 的结构化信息：

```
当前图表结构:
  类型: architecture
  实体(5): api_gw(网关), user_svc(服务), order_svc(服务), redis(缓存), pg(数据库)
  关系(4): api_gw -> user_svc, api_gw -> order_svc, user_svc -> redis, order_svc -> pg
  分组(1): backend [user_svc, order_svc]
```

这通过 `wasm.parse_to_json()` + 简化格式化实现，而非注入完整 DSL 源码（避免 token 浪费）。

### 8.3 增量规划

`edit_intent.targets` 列出受影响的实体/关系 ID。Executor 据此决定：
- `append`/`modify`：只生成变更部分的 patch，保留未变更部分
- `rebuild`：生成完整新 DSL

## 9. 降级策略

Planner 可能因各种原因失败，需要可靠的降级路径。

### 9.1 降级触发条件

| 条件 | 处理 |
|------|------|
| LLM 返回非法 JSON | 重试 1 次，仍失败则降级 |
| Plan 校验两次致命错误 | 降级 |
| LLM 调用超时/网络错误 | 直接降级 |
| `enablePlanner: false` | 跳过 Planner，走旧路径 |

### 9.2 降级行为

降级时直接调用旧的 `runAgentLoop(userMessage, context, config)`，与当前行为完全一致。UI 侧追加一条系统消息：

```
规划模式失败，已切换为直接生成模式
```

降级不计入错误，不影响用户体验。

## 10. Planner System Prompt 设计

### 10.1 固定段

```
你是 Drawify Studio 的图表规划师。你的职责是把用户的自然语言需求
转成结构化的 DrawingPlan，你不写 DSL 代码。

## 你的职责
1. 识别图表类型（若用户未明说，根据内容推断并在 assumptions 说明）
2. 枚举所有实体（宁可多列一个，不要遗漏关键角色）
3. 枚举实体间的关系与方向
4. 合理分组（架构图必备，流程图通常不需要）
5. 选择布局方向和算法
6. 列出你做的假设
7. 凡是会显著影响结果的不确定点，放进 open_questions

## 你不能做的
- 不输出 Drawify DSL
- 不臆测用户没提到的关键实体（改放 assumptions）
- 不在 confidence=high 时塞 open_questions 凑数
- 不使用不属于目标图表类型的 entity type

## 输出格式
严格输出符合 DrawingPlan JSON schema 的 JSON，不要包裹 markdown 代码块。
```

### 10.2 动态段（示例）

```
## 当前可用的图表类型能力

### flowchart
- entity type 闭集: start, end, process, decision, service, database, person, client, gateway, queue, cache, storage, external
- 默认 type: process
- 推荐布局: top-to-bottom
- 支持分组: 是
- 支持边路由: 是

### architecture
- entity type 闭集: frontend, backend, service, database, gateway, cache, queue, storage, external
- 默认 type: service
- 推荐布局: left-to-right
- 支持分组: 是
- 支持边路由: 是

...（其他图表类型）

### 可用布局算法
- flowchart: sugiyama-v2（默认）
- architecture: architecture-v2（默认）
- sequence: sequence（内置，不可选）
- state: circular（默认）
- er: er（默认）
- mindmap: mindmap（默认）

### 可用边路由算法
- orthogonal（默认，flowchart/architecture）
- bezier（默认，mindmap）
- straight（默认，er）
- circular（默认，state）
- sequence 不支持边路由
```

### 10.3 上下文段（编辑场景）

```
## 当前图表结构

类型: architecture
标题: 电商系统架构
实体(5):
  - api_gw [gateway] "API 网关"
  - user_svc [service] "用户服务"
  - order_svc [service] "订单服务"
  - redis [cache] semantic=redis "Redis 缓存"
  - pg [database] semantic=postgres "PostgreSQL"
关系(4):
  - api_gw -> user_svc "路由请求"
  - api_gw -> order_svc "路由请求"
  - user_svc -> redis "读写缓存"
  - order_svc -> pg "持久化"
分组(1):
  - backend [user_svc, order_svc] "后端服务"
```

### 10.4 Few-shot 示例

提供 3 个示例，覆盖不同场景：

1. **创建场景**：用户说"画一个微服务架构图" → 完整 DrawingPlan
2. **编辑场景**：用户说"在架构图里加一个 Kafka 消息队列" → 带 edit_intent 的 DrawingPlan
3. **模糊场景**：用户说"画一个系统图" → 带 open_questions 的 DrawingPlan

## 11. Executor Prompt 改造

### 11.1 新 System Prompt

```
你是 Drawify Studio 的图表执行 Agent。你收到一份 DrawingPlan，
需要把它翻译成 Drawify DSL 并渲染为可视化图表。

## 工作流程
1. 仔细阅读 DrawingPlan 中的每个实体、关系、分组
2. 按 plan 生成 Drawify DSL
3. 调用 render 工具渲染
4. 如果渲染失败，调用 validate 获取错误诊断，自动修复后重试
5. 如果是编辑场景（edit_intent.mode 为 append/modify），优先使用 apply_patch 做增量修改
6. 完成后用自然语言简要说明你做了什么

## DrawingPlan → DSL 翻译规则
- plan.entities → diagram 中的 entity 声明
- plan.relations → diagram 中的 relation 声明
- plan.groups → diagram 中的 group 声明
- plan.layout.direction → diagram 的 layout 属性
- plan.layout.layout_algo → diagram 的 layout_algo 属性
- plan.layout.edge_routing → diagram 的 edge_routing 属性
- entity.note 是给你的提示，不需要写入 DSL

## 注意事项
- 严格按照 plan 翻译，不要自行增删实体或关系
- entity id 只允许 [a-z][a-z0-9_]*，用下划线不用连字符
- 优先使用 semantic 属性匹配图标
- 回复用户时用中文，简洁说明你做了什么变更
```

### 11.2 Plan 注入方式

`buildMessages` 将 DrawingPlan 序列化为 system 消息注入：

```ts
messages.push({
  role: 'system',
  content: `## 当前 DrawingPlan\n\n${JSON.stringify(plan, null, 2)}`,
});
```

## 12. UI 改造

### 12.1 Plan Preview 卡片

新增 `PlanPreview` 组件，在 `confirm` 模式下展示 plan 摘要：

```
┌─────────────────────────────────────────┐
│  图表规划                                │
│                                         │
│  类型: architecture    布局: left-to-right │
│  标题: 电商系统架构                        │
│                                         │
│  实体 (7)                               │
│  ┌──────┐ ┌──────┐ ┌──────┐             │
│  │ 网关  │ │ 服务  │ │ 缓存  │ ...       │
│  └──────┘ └──────┘ └──────┘             │
│                                         │
│  关系 (6)                               │
│  api_gw → user_svc → redis              │
│  api_gw → order_svc → pg                │
│                                         │
│  分组 (1)                               │
│  后端服务: [user_svc, order_svc]          │
│                                         │
│  假设:                                   │
│  • 假设使用 Redis 作为缓存                │
│                                         │
│  [确认执行]  [编辑]  [取消]               │
└─────────────────────────────────────────┘
```

### 12.2 Clarify 交互

`open_questions` 非空时，在 ChatPanel 中渲染为可交互的问题卡片：

```
┌─────────────────────────────────────────┐
│  需要确认几个问题：                       │
│                                         │
│  1. 你需要哪种数据库？                    │
│     [PostgreSQL]  [MySQL]  [MongoDB]     │
│                                         │
│  2. 是否需要缓存层？                      │
│     [是]  [否]                           │
└─────────────────────────────────────────┘
```

用户选择后，答案追加到上下文，重新触发 Planner。

### 12.3 ChatMessage 扩展

```ts
interface ChatMessage {
  // ... 现有字段
  /** 附带的 DrawingPlan（Planner 产出时填充） */
  plan?: DrawingPlan;
  /** 是否为澄清问题消息 */
  isClarify?: boolean;
}
```

## 13. 文件组织

```
studio/src/agent/
├── AgentLoop.ts          # 改造：Executor 主循环
├── Planner.ts            # 新增：Planner 主流程（runPlanner）
├── planTypes.ts          # 新增：DrawingPlan 类型定义
├── planPrompt.ts         # 新增：Planner system prompt 模板 + few-shot
├── planValidator.ts      # 新增：DrawingPlan 结构校验
├── planProfiles.ts       # 新增：图表类型能力映射表（DIAGRAM_PROFILES）
├── prompt.ts             # 改造：Executor prompt
├── tools.ts              # 基本不变（Executor 仍用现有工具）
├── context.ts            # 改造：增加 currentPlan 字段
├── types.ts              # 改造：扩展 AgentConfig、AgentContext、ChatMessage
└── index.ts              # 改造：导出 Planner 相关

studio/src/components/
├── PlanPreview.tsx       # 新增：Plan 预览卡片
├── ClarifyCard.tsx       # 新增：澄清问题交互卡片
├── ChatMessage.tsx       # 改造：支持 plan 和 clarify 渲染
└── ChatPanel.tsx         # 改造：支持 confirmPlan / answerQuestion

studio/src/hooks/
└── useAgent.ts           # 改造：增加 Planner 阶段
```

## 14. 实施任务分解

### Phase 1：基础设施（无 UI 改动）

| # | 任务 | 涉及文件 |
|---|------|----------|
| 1.1 | 创建 `planTypes.ts`：定义 DrawingPlan 全部类型 | `planTypes.ts`（新建） |
| 1.2 | 创建 `planProfiles.ts`：定义 DIAGRAM_PROFILES 映射表 | `planProfiles.ts`（新建） |
| 1.3 | 创建 `planValidator.ts`：实现 DrawingPlan 校验函数 | `planValidator.ts`（新建） |
| 1.4 | 创建 `planPrompt.ts`：实现 Planner prompt 构建函数 | `planPrompt.ts`（新建） |
| 1.5 | 创建 `Planner.ts`：实现 `runPlanner` 主流程 | `Planner.ts`（新建） |
| 1.6 | 单元测试：planValidator 各校验规则 | `planValidator.test.ts`（新建） |

### Phase 2：Executor 改造

| # | 任务 | 涉及文件 |
|---|------|----------|
| 2.1 | 改造 `AgentLoop.ts`：支持 `DrawingPlan \| string` 入参 | `AgentLoop.ts` |
| 2.2 | 改造 `prompt.ts`：新 Executor prompt + plan 注入 | `prompt.ts` |
| 2.3 | 扩展 `types.ts`：AgentConfig 增加 enablePlanner/planMode | `types.ts` |
| 2.4 | 扩展 `context.ts`：增加 currentPlan 字段 | `context.ts` |

### Phase 3：Hook 与 UI

| # | 任务 | 涉及文件 |
|---|------|----------|
| 3.1 | 改造 `useAgent.ts`：增加 Planner 阶段 + pendingPlan 状态 | `useAgent.ts` |
| 3.2 | 创建 `PlanPreview.tsx`：Plan 预览卡片 | `PlanPreview.tsx`（新建） |
| 3.3 | 创建 `ClarifyCard.tsx`：澄清问题交互卡片 | `ClarifyCard.tsx`（新建） |
| 3.4 | 改造 `ChatMessage.tsx`：支持 plan 和 clarify 渲染 | `ChatMessage.tsx` |
| 3.5 | 改造 `ChatPanel.tsx`：支持 confirmPlan / answerQuestion | `ChatPanel.tsx` |
| 3.6 | 配置持久化：planMode 存入 localStorage | `configStorage.ts` |

### Phase 4：集成测试与调优

| # | 任务 | 涉及文件 |
|---|------|----------|
| 4.1 | 端到端测试：创建/编辑/澄清/降级四种场景 | -- |
| 4.2 | Planner prompt 调优：few-shot 示例优化 | `planPrompt.ts` |
| 4.3 | 性能测试：Planner + Executor 总延迟 vs 旧路径 | -- |

## 15. 开放问题与后续演进

| # | 问题 | 建议 |
|---|------|------|
| 1 | Planner 是否需要 Tool-Calling？ | 本期不需要。Planner 只读 layout_catalog（调用前同步注入），不调 render/validate，保持单次 LLM 调用 |
| 2 | Plan 缓存与审计 | 后续可把 `DrawingPlan` 存入 `ChatMessage.plan`，用于"基于上次 plan 微调"和审计。本期不做 |
| 3 | Plan 可编辑性 | `confirm` 模式下用户能否直接编辑 plan JSON？本期仅支持确认/取消，编辑留后续 |
| 4 | 多轮规划 | 用户回答澄清问题后重新规划，是否需要多轮？本期支持 1 轮澄清，多轮留后续 |
| 5 | 成本与延迟 | 多一次 LLM 调用，但 Planner 单次、Executor 迭代次数预期下降（plan 更准 → render 一次过），总 token 不一定增加 |
| 6 | Planner 模型选择 | 是否允许 Planner 和 Executor 使用不同模型（如 Planner 用便宜模型，Executor 用强模型）？后续可扩展 |
