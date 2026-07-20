# Plotgram Showcase

经典示例集，按图表类型组织，**文件名即角色**（`{role}.{slug}.pgm`），便于与 [Mermaid](https://mermaid.js.org/) 等工具对比渲染效果与语法表达。

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

## 角色前缀

每个文件按角色命名：`{type}/{role}.{slug}.pgm`。角色决定**门禁策略**与画廊默认展示，与节点数无关。

| 前缀 | 角色 | 观感 | 门禁默认 |
|------|------|------|----------|
| `smoke.` | smoke | 必须干净 | 冒烟；正确性硬 |
| `product.` | product | **必须好看** | **精选进质量硬棘轮** |
| `demo.` | demo | 好看优先 | 观测 / 可债 |
| `stress.` | stress | 可妥协 | 正确性硬；质量默认 WARN（`--strict-stress` 改硬） |
| `mech.` | mech | 不追美观 | 机制断言 / 专项集 |

旧 `s./n./c./x.` 规模前缀**已废弃**。UI / 工具按文件名第一段解析角色：

```text
path.split('/').last().split('.')[0]  →  role
```

判定口诀：

- **product**：用户下周就可能画成这样
- **demo**：给销售与官网看的大图
- **stress**：故意造来打爆布局/路由的
- **mech**：为钉死一条规则而写的最小反例

## 门禁清单

门禁清单按角色分集，落在 [`../benchmarks/`](../benchmarks/)：

| 清单 | 用途 |
|------|------|
| `benchmarks/sets/product-regression-set.txt` | 日常质量硬门禁（精选） |
| `benchmarks/sets/stress-probe-set.txt` | 正确性硬 + 质量观测 |
| `benchmarks/sets/mech-set.txt` | 机制探针 |
| `benchmarks/sets/demo-observe-set.txt` | 观测 / 可债 |

日常宣称「无退化」默认只引用 **product-gate**。详见 [重构方案](../docs/architecture/重构方案/showcase-基线分层重构-2026-07.md)。

## 代表样例

画廊默认显示 `product.` + `demo.`；`stress.` / `mech.` 折叠在「工程探针」区。

| 主题 | 推荐文件 | 说明 |
|------|----------|------|
| 云原生 / 微服务 | `architecture/product.cloud-native.pgm` | 主流云原生拓扑 |
| 典型微服务 | `architecture/product.typical-microservice-architecture.pgm` | 网关 + 服务 + 数据层 |
| 三层架构 | `architecture/product.three-tier.pgm` | 经典三层业务系统 |
| CDN 缓存 | `architecture/product.cdn-cache.pgm` | 边缘缓存回源链路 |
| 电商基础 | `architecture/product.ecommerce-platform.pgm` | 电商系统骨架 |
| 用户认证 | `flowchart/product.user-auth.pgm` | 登录 / 注册 / 鉴权分支 |
| 退款流程 | `flowchart/product.refund-process.pgm` | 退款业务泳道 |
| OAuth 登录 | `sequence/product.oauth-login.pgm` | 标准 OAuth 2.0 时序 |
| 支付网关 | `sequence/product.payment-gateway.pgm` | 支付网关交互时序 |
| 订单生命周期 | `state/product.order-lifecycle.pgm` | 订单状态迁移 |
| 支付流程状态机 | `state/product.payment-flow.pgm` | 支付状态迁移 |
| 博客 ER | `er/product.blog-schema.pgm` | 博客系统 ER |
| SaaS 多租户 ER | `er/product.saas-schema.pgm` | 多租户 SaaS ER |
| 技术栈脑图 | `mindmap/product.tech-stack.pgm` | 前后端技术栈 |

K8s / 故障恢复 / 大型业务大图多在 `demo.*`，从画廊切换「工程探针」可看到 `stress.*` 与 `mech.*`。

## D2 对照基准

从 [D2](https://d2lang.com/) 官方示例转换的基准图，用于与 D2 渲染效果做视觉对比、驱动布局/形状/主题能力迭代。文件头部注释保留原始 D2 源码与有损映射说明。

| 主题 | 推荐文件 | 说明 |
|------|----------|------|
| 基站网络拓扑 | `architecture/demo.d2-cell-tower-network.pgm` | D2 Terminal 主题风格网络图；含嵌套分组、多种形状、虚线边 |

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

# 在本地 HTTP 服务下浏览画廊
python3 -m http.server --directory showcase 4173
# 打开 http://localhost:4173/index.html

# 渲染单个示例
cargo run -p plotgram-cli -- render showcase/flowchart/product.linear-chain.pgm

# 验证语法
cargo run -p plotgram-cli -- validate showcase/sequence/product.oauth-login.pgm
```

`render-all.sh` 渲染时会对比输出与上次内容的 SHA256，在控制台标注 `[新建]`、`[无变化]` 或 `[已变化]`，并自动更新 `index.html` 内嵌的 `SAMPLE_PATHS` 列表。

## 与 Mermaid 对照

| Plotgram 目录 | Mermaid 关键字 | 代表示例 |
|-------------|-------------|----------|
| `flowchart/` | `graph` / `flowchart` | `product.user-auth` ↔ 登录/鉴权分支 |
| `sequence/` | `sequenceDiagram` | `product.oauth-login` ↔ 标准 OAuth 2.0 |
| `architecture/` | `graph` + `subgraph` | `product.cloud-native` ↔ 云原生拓扑 |
| `state/` | `stateDiagram-v2` | `product.payment-flow` ↔ 支付状态迁移 |
| `er/` | `erDiagram` | `product.blog-schema` ↔ `USER \|\|--o{ POST` |
| `mindmap/` | `mindmap` | `product.tech-stack` ↔ 前后端技术栈 |

每个 `.pgm` 文件头部注释中标注了对应的 Mermaid 写法。

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

如果想快速了解 Plotgram 在生产场景中的表达能力，可以按下面顺序看：

1. `architecture/product.cloud-native.pgm`：先看主流云原生拓扑。
2. `architecture/product.typical-microservice-architecture.pgm`：再看典型微服务分层。
3. `architecture/product.ecommerce-platform.pgm`：再看高价值业务场景。
4. `sequence/product.oauth-login.pgm`：看时序图基础表达。
5. `flowchart/product.user-auth.pgm`：看流程图分支与回环。
6. `state/product.payment-flow.pgm`：最后看状态机表达。

## 渲染状态

> `flowchart`、`sequence`、`architecture` 已有稳定专属渲染；`state`、`er`、`mindmap` 有专属渲染器但可能继续调整。
