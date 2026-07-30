# Plotgram Showcase

经典示例集，**按布局体系组织**（便于测试），**文件名即角色**（`{role}.{slug}.pgm`）。

> 现行分类反映 ADR-001「图类型不进引擎」后的布局体系划分。DSL 语法遵循 [`docs/specs/dsl-spec.md`](../docs/specs/dsl-spec.md) v2.6（`diagram { profile: … }` + `node id { label, archetype, … }`）。

## 目录结构

```
showcase/
├── hierarchical/       # 分层布局（profile: flowchart / architecture）— 主力开发
│                        #   原 flowchart/ + architecture/ 合并；DSL 已按 v2.6 改造
├── _backup/            # 暂不支持的图形 dsl 备份（保留旧语法，等布局开发后迁出）
│   ├── sequence/       #   时序布局（profile: sequence）
│   ├── state/          #   状态图（profile: state，layout 待定）
│   ├── tree/           #   树布局（profile: mindmap）
│   └── circular/       #   环形布局（profile: er）
└── assets/             # 画廊品牌资源
```

### 为什么按布局体系划分

ADR-001 后引擎不收图种名，只认 `LayoutContract`（算法名 + 参数）。测试关注的是**布局算法的覆盖**，而非图种标签。`flowchart` 与 `architecture` 共用 `hierarchical` 布局，合并到同一目录便于回归测试。其余布局（sequence / tree / circular）尚未开发，dsl 暂存 `_backup/`。

## 角色前缀

每个文件按角色命名：`{layout}/{role}.{slug}.pgm`。角色决定**门禁策略**与画廊默认展示，与节点数无关。

| 前缀 | 角色 | 观感 | 门禁默认 |
|------|------|------|----------|
| `smoke.` | smoke | 必须干净 | 冒烟；正确性硬 |
| `product.` | product | **必须好看** | **精选进质量硬棘轮** |
| `demo.` | demo | 好看优先 | 观测 / 可债 |
| `stress.` | stress | 可妥协 | 正确性硬；质量默认 WARN（`--strict-stress` 改硬） |
| `mech.` | mech | 不追美观 | 机制断言 / 专项集 |

UI / 工具按文件名第一段解析角色：`path.split('/').last().split('.')[0] → role`。

判定口诀：

- **product**：用户下周就可能画成这样
- **demo**：给销售与官网看的大图
- **stress**：故意造来打爆布局/路由的
- **mech**：为钉死一条规则而写的最小反例

## DSL 语法（v2.6）

`hierarchical/` 下所有 `.pgm` 已按新 spec 改造，要点：

```plotgram
diagram {
    profile: flowchart                      // 或 architecture
    title: "..."
    layout: hierarchical { direction: top-to-bottom }   // 可选，覆盖 profile 默认

    node login { label: "登录", archetype: start }
    node db { label: "用户库", archetype: database
        meta.semantic: postgres             // 旧 semantic → meta.semantic（保留信息，渲染器忽略）
    }

    group auth {
        label: "认证服务"
        node api { label: "API", archetype: service, status: healthy }
    }

    login -> api "提交"                      // 中点 label 糖（§7.5）
    api --> login { label: "结果" }          // 响应箭头
}
```

改造映射（旧 → 新）：

| 旧语法（v1） | 新语法（v2.6） |
|----|----|
| `diagram flowchart {` | `diagram { profile: flowchart` |
| `config { direction: tb }` | `layout: hierarchical { direction: tb }` |
| `entity[database] db "用户库" { semantic: postgres }` | `node db { label: "用户库", archetype: database, meta.semantic: postgres }` |
| `group auth "认证" {` | `group auth { label: "认证"` |
| `constrain a -> b` | `// constrain`（新规范暂无对应，注释保留） |
| `owner: "团队"` | `meta.owner: "团队"` |

`archetype` 封闭集见 [`archetype-spec.md`](../docs/specs/archetype-spec.md) CSV（database/cache/queue/storage/gateway/external/service/client/decision/start/end/actor/root）。旧 kind 中 `frontend` / `process` 不在 CSV，保留为 `archetype:` 值（spec 允许未知 archetype，不报错不展开），等 CSV 扩展后自动生效。

## 门禁清单

门禁清单按角色分集，落在 [`../benchmarks/`](../benchmarks/)：

| 清单 | 用途 |
|------|------|
| `benchmarks/sets/product-regression-set.txt` | 日常质量硬门禁（精选） |
| `benchmarks/sets/stress-probe-set.txt` | 正确性硬 + 质量观测 |
| `benchmarks/sets/mech-set.txt` | 机制探针 |
| `benchmarks/sets/demo-observe-set.txt` | 观测 / 可债 |

> 注意：门禁清单内的路径仍指向旧目录（`flowchart/...` / `architecture/...`），需后续更新为 `hierarchical/...`。脚本（`render-all.sh` / `update-gallery-manifest.py`）同理，待 CLI 就绪后一并迁移。

## 代表样例

| 主题 | 推荐文件 | 说明 |
|------|----------|------|
| 云原生 / 微服务 | `hierarchical/product.cloud-native.pgm` | 主流云原生拓扑（嵌套 group） |
| 典型微服务 | `hierarchical/product.typical-microservice-architecture.pgm` | 网关 + 服务 + 数据层 |
| 扁平 REST | `hierarchical/product.flat-rest-api.pgm` | 不画分层框 |
| 三层架构 | `hierarchical/product.three-tier.pgm` | 经典三层业务系统 |
| 用户认证 | `hierarchical/product.user-auth.pgm` | 登录 / 鉴权分支 |
| 退款流程 | `hierarchical/product.refund-process.pgm` | 退款业务泳道 |
| 泳道图 | `hierarchical/product.swimlane-order-process.pgm` | 跨部门 group 分区 |
| yFiles 管道 | `hierarchical/stress.layout-stress-yfiles-pipeline.pgm` | 23 节点 / 31 边 DAG 压测 |

## D2 对照基准

`hierarchical/demo.d2-cell-tower-network.pgm`：从 [D2](https://d2lang.com/) 官方示例转换，用于视觉对比。

## 与 Mermaid 对照

| Plotgram 布局 | Mermaid 关键字 | 代表示例 |
|-------------|-------------|----------|
| `hierarchical/` | `graph` / `flowchart` + `subgraph` | `product.cloud-native` ↔ 云原生拓扑 |
| `_backup/sequence/` | `sequenceDiagram` | `product.oauth-login` ↔ OAuth 2.0 |
| `_backup/state/` | `stateDiagram-v2` | `product.payment-flow` ↔ 支付状态迁移 |
| `_backup/circular/` | `erDiagram` | `product.blog-schema` ↔ ER |
| `_backup/tree/` | `mindmap` | `product.tech-stack` ↔ 技术栈 |

## 渲染状态

> 当前阶段：**CLI 尚未就绪**，`hierarchical/` 下 dsl 已按 v2.6 改造但**未经渲染验证**。`_backup/` 下保留旧语法，待对应布局开发后迁出改造。
>
> 渲染脚本（`render-all.sh` 等）与门禁清单路径仍指向旧目录结构，待 CLI 落地后一并更新。
