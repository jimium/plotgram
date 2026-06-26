# Connector → AST → Web 渲染：高价值场景分析

> 版本：0.1.0-draft | 状态：产品探索

本文档梳理 **编程/连接器直接产出 AST、在 Web 页面即时渲染** 所能开启的企业级场景。这些场景通常不会从「画图工具」视角自然想到，但具备明确的实用价值。

相关文档：

- [使用场景与案例](use-cases.md) — AI 生成图表等核心场景
- [企业规模化架构图战略](../enterprise/scale-diagram-strategy.md) — Connector + Compose 管线设计
- [企业能力路线图](../enterprise/capability-roadmap.md) — AST 双入口、Diff、Server API

---

## 1. 能力边界

**规模化不靠 DSL 里写 `for`，靠「数据源 → Connector → AST Builder → 渲染」的企业级管线。**

| 层级 | 职责 | 说明 |
|------|------|------|
| Drawify DSL | 语义表达 | 服务 AI Agent、架构师手写、PR 中小幅修改；保持声明式、低语法空间 |
| Connectors | 数据接入 | 从 K8s、Terraform、CMDB、APM 等拉取结构化数据 |
| Compose | 聚合与视图 | 折叠 Pod、分层级、过滤、命名；批量生成 AST |
| Core | 渲染引擎 | 校验、布局、渲染；消费已展开的 AST，不理解业务域 |

**AST 是 Drawify 的一等公民。** 文本 DSL 只是 AST 的一种序列化形式；企业规模化主路径是 **JSON AST → Diagram → 渲染**（见 [AST 规范](../specs/ast-spec.md)、[能力路线图 §4.2](../enterprise/capability-roadmap.md#42-p0--双入口解析)）。

因此「想象力」应放在：

- **谁**往 AST 里写数据
- **多久**写一次（一次性 / 定时 / 实时订阅）
- 用户在 Web 上**如何交互**（下钻、时间轴、Diff 高亮）

而不是让人手写数百行 `.dfy`。

---

## 2. 高价值场景（按惊喜度与实用性）

### 场景 1：事故战情室 — 一张会「呼吸」的架构图

**做法**：Connector 订阅 PagerDuty / Oncall + K8s Watch + APM 采样，每 5–30 秒对 AST 做 Patch（节点着色、边变粗、标注 QPS/错误率），WASM 在浏览器里亚秒重绘。

**反直觉之处**：架构图不是文档里的静态插图，而是**与告警、指标绑定的 live dashboard**。SRE 不必在 Grafana、K8s Dashboard、服务目录之间来回切换。

**实用价值**：on-call 时 30 秒内回答「这次故障波及哪些域、从哪条调用链进来」。

**主要图表类型**：`architecture`（可加 `sequence` 叠加关键调用）

---

### 场景 2：发布过程 — 滚动更新「序列图回放」

**做法**：监听 Deployment / ReplicaSet 事件，将每次 Pod 创建、就绪、摘除写成 `sequence` AST 帧；Web 端像播放器一样 scrub 时间轴。

静态示意见 `showcase/sequence/c.k8s-rolling-update.dfy`；Connector 将其变为**真实发布录像**。

**反直觉之处**：序列图通常表达「设计意图」；这里表达的是**实际发生的时序**，与设计图对比可立刻发现 maxUnavailable、探针配置等问题。

**实用价值**：发布失败复盘；与 Argo CD 等变更系统联动做合规留档。

**主要图表类型**：`sequence`

---

### 场景 3：合规视角 — 数据出境 / 跨区流动图

**做法**：聚合 Terraform 安全组、K8s NetworkPolicy、服务目录 `data_classification`、APM 流量采样 → `architecture` AST；关系边标注 `meta.protocol`、`meta.zone`、`meta.allowed`（见 [能力路线图 §3.1](../enterprise/capability-roadmap.md#31-p0--企业元数据约定meta-字段)）。

**反直觉之处**：银行 / GDPR 要的不是拓扑美观，而是**「敏感数据有没有未经批准的路径」**——这是图论 + 策略引擎问题，AST 适合表达实体关系与治理属性。

**实用价值**：季度审计、架构评审一键出图；与上季度快照 Diff 标红新增跨区边。

**主要图表类型**：`architecture`

---

### 场景 4：PR 语义影响图 — 不只是代码 Diff

**做法**：CI 分析本次 PR 改动的包 / 服务 / 表 / Topic，Connector 生成**受影响子图** AST；与主干 baseline AST 做 Diff 高亮渲染，贴入 PR 评论。

**反直觉之处**：开发者习惯看 line diff；Tech Lead 需要的是**「这个改动在系统里撬动了哪几块」**——尤其在微服务、共享库、数据库 migration 场景。

**实用价值**：降低「小 PR 大事故」；与 CODEOWNERS 自动 @ 相关团队。

**主要图表类型**：`architecture`、`flowchart`

**依赖能力**：AST Diff 高亮渲染（[能力路线图 §6.2](../enterprise/capability-roadmap.md#62-p0--diff-高亮渲染模式)）

---

### 场景 5：分布式 Saga — 失败路径热力图

**做法**：从消息队列死信、分布式 trace、业务事件日志统计「最近 N 次失败最常经过的节点序列」，生成带权重的 `flowchart` 或 `sequence` AST（边粗细 = 失败频次）。

**反直觉之处**：故障分析图通常画 happy path；这里画的是**统计意义上的 unhappy path**，直接指导该加熔断、重试还是改补偿逻辑。

**实用价值**：支付、清结算、订单域；与 showcase 中 saga、支付场景天然契合。

**主要图表类型**：`flowchart`、`sequence`

---

### 场景 6：K8s 节点压力 — 状态机实况 + 操作建议

**做法**：K8s Node condition（MemoryPressure、DiskPressure、Cordoned…）驱动 `state` AST 当前状态高亮；规则引擎在 choice 节点旁渲染「建议：drain / replace」。

静态示意见 `showcase/state/c.k8s-node-pressure-lifecycle.dfy`；Connector 将其变为**集群内每个 Node 的状态面板**。

**实用价值**：值班人员不必背状态转换表；新人培训可用同一套 AST 做模拟演练。

**主要图表类型**：`state`

---

### 场景 7：Schema 漂移 — ER 图的「时间滑块」

**做法**：定时从 information_schema / migration 历史拉 ER AST 快照；Web 上拖动时间轴，Diff 高亮「新增表、删列、改外键」。

**反直觉之处**：ER 图很少做成**版本化资产**；与 API 契约、数据 lineage 连在一起才有企业价值。

**实用价值**：大促前检查 staging 与 prod 表结构一致性；制药 GxP 数据血缘证据。

**主要图表类型**：`er`

---

### 场景 8：Feature Flag — 爆炸半径图

**做法**：从 LaunchDarkly / 自研开关 + 静态调用图 + 运行时 trace 采样，生成「打开 flag X 时可能触达的 service / frontend」子图 AST。

**实用价值**：灰度前给 PM 和法务一张图，而不是一堆服务名列表。

**主要图表类型**：`architecture`

---

### 场景 9：合同测试（Pact）— 集成健康拓扑

**做法**：消费 Pact broker 的 consumer-provider 关系 → `architecture` AST；CI 将失败契约标红节点 / 边。

**反直觉之处**：契约测试报告通常是表格；变成图后**一眼看出「谁断了、会不会形成孤岛」**。

**实用价值**：微服务团队的集成治理与发布门禁。

**主要图表类型**：`architecture`

---

### 场景 10：多 Agent 协作 — 图 Patch 工作流

**做法**：Planner Agent 产出骨架 AST；Diagram Agent 只 Patch 实体 / 关系；Validator 返回结构化错误 → Patch Agent 修 AST（不碰全文 DSL）；WASM 每步预览。

管线示意见 `showcase/architecture/c.ai-agent-docops-pipeline.dfy`。**直接消费 JSON AST** 比让 Agent 反复改文本稳定得多。

**实用价值**：把 LLM 生成图的正确率从「猜语法」提升为「改结构化对象」。

**主要图表类型**：各类；重点是 **AST Patch** 而非文本生成

---

## 3. 优先级矩阵

| 场景 | 反直觉点 | 落地难度 | 主要买单方 |
|------|----------|----------|------------|
| 事故 live 拓扑 | 图 = 监控界面 | 中（多源聚合） | SRE / 平台 |
| 发布序列回放 | 序列图 = 录像 | 中 | DevOps |
| 合规数据流图 | 图 = 审计证据 | 中高 | 金融 / 出海 |
| PR 语义影响图 | 图 = Code Review 界面 | 低中 | 工程效能 |
| 失败路径热力 | 图 = 可靠性分析 | 中 | 支付 / 核心交易 |
| Node 状态机实况 | 状态图 = 运维面板 | 低中 | K8s 运维 |
| ER 时间滑块 | 图 = Schema 版本库 | 中 | DBA / 数据平台 |
| Flag 爆炸半径 | 图 = 变更风险评估 | 低中 | 发布 / 产品 |
| Pact 健康拓扑 | 图 = 集成测试地图 | 低 | 微服务团队 |
| Agent AST Patch | 图 = Agent 的 IO 协议 | 低（能力已有） | AI 平台 |

---

## 4. 产品形态上的反直觉结论

### 4.1 图不是附件，是 API 的返回类型

`GET /views/payment-prod?level=domain` → JSON AST 或 SVG；文档站、Backstage、飞书卡片共用同一套交付物。

### 4.2 同一份数据，多种「镜头」

同一集群数据可产出：

- 全景 → `architecture`
- 一次故障 → `sequence`
- 节点生命周期 → `state`

由 Compose 规则切换视图，而非维护三套手工图。

### 4.3 Diff 往往比单张图更值钱

企业客户常问：「昨晚变更了什么？」**两张 AST 的高亮 Diff** 比单张漂亮图更能支撑 Architecture Compare、合规留档（见 [规模化战略 §4](../enterprise/scale-diagram-strategy.md#4-高价值企业场景按落地优先级)）。

### 4.4 默认聚合，按需下钻

100 个 Pod 不画 100 个节点：Namespace → Deployment →「+12 replicas」，点击再展开子 AST。这是 **Web + AST Patch** 才能做好的交互，静态 SVG 难以承载。

### 4.5 浏览器 WASM = 零后端预览

Connector 在服务端产 AST，前端只收 JSON Patch + 本地 WASM 渲染——敏感拓扑不出内网，适合银行私有化部署。

---

## 5. 与现有场景文档的关系

| 文档 / 能力 | 关系 |
|-------------|------|
| [use-cases.md](use-cases.md) | 聚焦 AI 生成 DSL 文本；本文档聚焦 **程序化 AST** 与实时 Web |
| [scale-diagram-strategy.md](../enterprise/scale-diagram-strategy.md) | 本文档的场景是该战略的 **场景层展开** |
| [capability-roadmap.md](../enterprise/capability-roadmap.md) | AST 双入口、Diff 高亮、Server API 是本文档场景的 **技术前置** |

---

## 6. 一句话总结

**「编程 / 连接器直接产 AST + Web 渲染」** 的真正价值在于：把图变成**可订阅的系统状态视图、可版本化的合规资产、可 Diff 的变更语言、可 Patch 的 Agent 协作介质**——而不是「又一种画图工具」。
