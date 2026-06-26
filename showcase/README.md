# Drawify Showcase

经典示例集，按图表类型组织，复杂度编码在文件名前缀中，便于与 [Mermaid](https://mermaid.js.org/) 等工具对比渲染效果与语法表达。

## 目录结构

```
showcase/
├── flowchart/          # 流程图（graph TD/LR）
├── sequence/           # 时序图（sequenceDiagram）
├── architecture/       # 架构图（subgraph / C4）
├── state/              # 状态图（stateDiagram-v2）
├── er/                 # ER 图（erDiagram）
└── mindmap/            # 思维导图（mindmap）
```

每个类型目录下的文件以复杂度前缀命名：`s.`（简单）、`n.`（正常）、`c.`（复杂）。文件后缀为 `.dfy`。

## 复杂度前缀

| 前缀 | 级别 | 节点数 | 特征 | 对比目的 |
|------|------|--------|------|----------|
| `s.` | simple | ≤4 | 线性链、单次交互 | 验证最基础语法与布局 |
| `n.` | normal | 5-10 | 决策分支、分组、属性 | 验证日常业务场景 |
| `c.` | complex | 10+ | 多决策、回环、嵌套分组 | 压力测试布局引擎 |

示例：`flowchart/s.linear-chain.dfy`、`sequence/n.oauth-login.dfy`、`er/c.ecommerce-schema.dfy`

按复杂度筛选：`showcase/flowchart/s.*.dfy`

## 代表样例

除了基础语法样例，`showcase/` 现在也包含一批面向生产环境的复杂场景，适合对外演示 Drawify 在大规模系统、发布治理、故障恢复、合规流程中的表达能力。

| 主题 | 推荐文件 | 说明 |
|------|----------|------|
| 大规模 K8s 全景 | `architecture/c.k8s-multi-namespace-overview.dfy` | 多命名空间生产集群，按聚合后的业务工作负载组织 |
| 多集群与容灾 | `architecture/c.k8s-multi-cluster-federation.dfy` | 跨地域主备、共享平台能力与复制关系 |
| 平台栈视图 | `architecture/c.k8s-platform-stack.dfy` | 业务工作负载 + Ingress + CNI/CSI + 可观测组件 |
| 蓝绿发布 | `architecture/c.k8s-blue-green-release-topology.dfy` | 流量切换、共享后端、健康信号驱动发布 |
| 多租户隔离 | `architecture/c.k8s-tenant-isolation.dfy` | 多租户 SaaS 集群与共享平台服务 |
| 混合云灾备 | `architecture/c.hybrid-cloud-dr-topology.dfy` | 本地数据中心 + 公有云灾备拓扑 |
| 数据血缘平台 | `architecture/c.data-lineage-platform.dfy` | 数据采集、治理、血缘、分析消费全链路 |
| AI 文档自动化 | `architecture/c.ai-agent-docops-pipeline.dfy` | Agent 生成、校验、修复、Diff、发布闭环 |
| 支付清结算 | `architecture/c.payment-clearing-platform.dfy` | 支付、记账、清分、结算、对账一体化平台 |
| 供应链控制塔 | `architecture/c.supply-chain-control-tower.dfy` | 计划、履约、物流、可视化与告警协同 |
| 发布链路 | `sequence/c.k8s-rolling-update.dfy` | 从 CI 到 K8s 滚动发布与切流 |
| 回滚链路 | `sequence/c.k8s-canary-rollback.dfy` | 金丝雀失败后的观测驱动回滚 |
| 节点恢复 | `sequence/c.k8s-node-failure-recovery.dfy` | 节点故障后的重调度与自动扩容 |
| 跨团队协同 | `sequence/c.cross-team-incident-escalation.dfy` | SRE、应用、数据库、安全团队联动排障 |
| AI 变更闭环 | `sequence/c.ai-agent-change-loop.dfy` | 代码变更到图表修复与 PR 回写 |
| 实时风控 | `sequence/c.real-time-risk-decisioning.dfy` | 特征、规则、模型串联的实时决策 |
| 故障响应 | `flowchart/c.k8s-incident-response.dfy` | 节点、发布、流量三类故障分支 |
| 变更审批 | `flowchart/c.change-approval-workflow.dfy` | 合规场景下的审批、验证、回滚与归档 |
| PR 架构评审 | `flowchart/c.pr-architecture-review.dfy` | AI 自动生成与人工 gate 结合的评审流程 |
| 反洗钱调查 | `flowchart/c.aml-case-investigation.dfy` | 告警、复筛、人工调查、监管报送 |
| 发布状态机 | `state/c.k8s-rollout-state-machine.dfy` | 渐进式发布中的 pause / verify / rollback |
| 节点状态机 | `state/c.k8s-node-pressure-lifecycle.dfy` | 节点压力、驱逐、修复、替换流程 |
| 降级状态机 | `state/c.service-degradation-lifecycle.dfy` | 服务告警、降级、切换、恢复生命周期 |
| 文档同步状态机 | `state/c.document-sync-lifecycle.dfy` | 文档从过期到生成、评审、发布 |
| 清结算状态机 | `state/c.settlement-reconciliation-lifecycle.dfy` | 清分、结算、对账、异常处理生命周期 |

## D2 对照基准

从 [D2](https://d2lang.com/) 官方示例转换的基准图，用于与 D2 渲染效果做视觉对比、驱动布局/形状/主题能力迭代。文件头部注释保留原始 D2 源码与有损映射说明。

| 主题 | 推荐文件 | 说明 |
|------|----------|------|
| 基站网络拓扑 | `architecture/n.d2-cell-tower-network.dfy` | D2 Terminal 主题风格网络图；含嵌套分组、多种形状、虚线边 |

## 按场景浏览

如果你是按业务或平台主题找样例，而不是按图类型找，可以直接从这里开始：

| 场景 | 推荐文件 |
|------|----------|
| `K8s / 平台工程` | `architecture/c.k8s-multi-namespace-overview.dfy`、`architecture/c.k8s-platform-stack.dfy`、`sequence/c.k8s-rolling-update.dfy` |
| `发布治理 / 回滚` | `architecture/c.k8s-blue-green-release-topology.dfy`、`sequence/c.k8s-canary-rollback.dfy`、`state/c.k8s-rollout-state-machine.dfy` |
| `故障恢复 / 稳定性` | `flowchart/c.k8s-incident-response.dfy`、`sequence/c.k8s-node-failure-recovery.dfy`、`state/c.service-degradation-lifecycle.dfy` |
| `多租户 / 混合云 / 合规` | `architecture/c.k8s-tenant-isolation.dfy`、`architecture/c.hybrid-cloud-dr-topology.dfy`、`flowchart/c.change-approval-workflow.dfy` |
| `AI Agent / DocOps` | `architecture/c.ai-agent-docops-pipeline.dfy`、`sequence/c.ai-agent-change-loop.dfy`、`flowchart/c.pr-architecture-review.dfy` |
| `数据治理 / 血缘` | `architecture/c.data-lineage-platform.dfy`、`state/c.document-sync-lifecycle.dfy` |
| `金融 / 风控 / 清结算` | `architecture/c.payment-clearing-platform.dfy`、`sequence/c.real-time-risk-decisioning.dfy`、`flowchart/c.aml-case-investigation.dfy`、`state/c.settlement-reconciliation-lifecycle.dfy` |
| `供应链协同` | `architecture/c.supply-chain-control-tower.dfy` |
| `D2 渲染对照` | `architecture/n.d2-cell-tower-network.dfy` |

## 快速使用

```bash
# 一次性渲染全部示例为 SVG（默认）
./showcase/render-all.sh

# 同时渲染 SVG + PNG（便于截图对比 Mermaid）
./showcase/render-all.sh -a

# 指定格式
./showcase/render-all.sh -f png

# 渲染前先验证
./showcase/render-all.sh --validate -a

# 在本地 HTTP 服务下浏览画廊（含历史版本对比）
python3 -m http.server --directory showcase 4173
# 打开 http://localhost:4173/index.html

# 渲染单个示例
cargo run -p drawify-cli -- render showcase/flowchart/s.linear-chain.dfy

# 验证语法
cargo run -p drawify-cli -- validate showcase/sequence/n.oauth-login.dfy
```

## SVG 历史版本

`render-all.sh` 在重新生成 SVG 时，若输出与现有文件内容不同，会把**旧版 SVG** 自动归档到 `showcase/.history/`，并更新 `showcase/.history/manifest.json`。

在 `index.html` 大图预览中：

- 侧栏 **版本历史** 列出当前版本与全部历史快照（按时间倒序）
- `[` / `]` 在同一图的历史版本间切换
- **对比相邻版本** 可左右并排查看新旧差异

历史 SVG 默认不入库（见 `showcase/.gitignore`），`manifest.json` 可提交以便团队共享版本索引。

## 与 Mermaid 对照

| Drawify 目录 | Mermaid 关键字 | 代表示例 |
|-------------|-------------|----------|
| `flowchart/` | `graph` / `flowchart` | `c.k8s-incident-response` ↔ 多分支故障处理 |
| `sequence/` | `sequenceDiagram` | `c.k8s-rolling-update` ↔ 发布与切流链路 |
| `architecture/` | `graph` + `subgraph` | `c.k8s-multi-namespace-overview` ↔ 大规模集群聚合视图 |
| `state/` | `stateDiagram-v2` | `c.k8s-rollout-state-machine` ↔ 发布状态迁移 |
| `er/` | `erDiagram` | `s.user-post` ↔ `USER \|\|--o{ POST` |
| `mindmap/` | `mindmap` | `s.brainstorm` ↔ 中心主题三分支 |

每个 `.dfy` 文件头部注释中标注了对应的 Mermaid 写法。

## 实体 type 约定

示例中的 `entity type` 遵循 [视觉语言标准](../docs/specs/visual-language/entity-types.md)：

| 图表 | 规范 type 示例 |
|------|----------------|
| `flowchart` | `start` `process` `decision` `service` `database` … |
| `sequence` | `actor` `boundary` `control` `database` `queue` |
| `architecture` | `frontend` `service` `database` `gateway` `external` … |
| `state` | `initial` `state` `choice` `final` |
| `er` | `database`（推荐） |
| `mindmap` | `root` `main` `branch` `leaf` |

## 推荐浏览顺序

如果想快速了解 Drawify 在生产场景中的表达能力，可以按下面顺序看：

1. `architecture/c.k8s-multi-namespace-overview.dfy`：先看大规模系统全景。
2. `architecture/c.ai-agent-docops-pipeline.dfy`：再看 AI 与文档自动化能力。
3. `architecture/c.payment-clearing-platform.dfy`：再看高价值业务场景。
4. `sequence/c.k8s-rolling-update.dfy`：看发布链路。
5. `flowchart/c.k8s-incident-response.dfy`：看故障处理流程。
6. `state/c.k8s-rollout-state-machine.dfy`：最后看状态机表达。

## 渲染状态

> `flowchart`、`sequence`、`architecture` 已有稳定专属渲染；`state`、`er`、`mindmap` 有专属渲染器但可能继续调整。
