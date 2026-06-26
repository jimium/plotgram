# GitHub App：Actions Workflow 可视化

> 版本：0.1.0-draft | 状态：方案设计

## 核心结论

做一个免费的 GitHub App：用户在 PR 或 Issue 评论中 `@drawify-bot`，即触发对 `.github/workflows/*.yml` 的解析，用 **Drawify 渲染 SVG**，将图片链接回贴到评论线程。

- **触发方式**：用户主动 `@`，不做每次 push 自动刷屏
- **输出格式**：仅 Drawify SVG，不使用 Mermaid（竞品，且无法展示 Drawify 渲染能力）
- **存储方案**：Cloudflare R2 短生命周期缓存 + 内容去重，目标月成本 $0–10
- **品牌目标**：PR 评论与 SVG 页面带 Drawify 标识，引流至官网 / Playground

---

## 1. 产品定位

### 1.1 解决什么问题

GitHub Actions workflow 以 YAML 描述，当 job 数量增多、`needs` 依赖变复杂、引入 reusable workflow 后，纯文本难以快速理解执行结构。本工具将 workflow 转为可视化 pipeline 图，降低 review 与 onboarding 成本。

### 1.2 价值最高的场景

| 场景 | 价值 |
|------|------|
| 复杂 CI（多 job 并行/串行、matrix、reusable workflow） | 高 |
| PR 修改了 `.github/workflows/` 后的 review | 高（后续可做 diff 图） |
| 新人理解「代码怎么到生产」 | 中高 |
| 简单三段式流水线（build → test → deploy） | 低 |

### 1.3 与现有路线图的关系

[success-roadmap.md](success-roadmap.md) 中规划的 GitHub Action 方向是 **`.dfy` → SVG**（用户提交图表，CI 渲染）。本 App 是反向入口：**workflow YAML → Drawify → SVG**，与 Action 形成同一生态的两侧：

```
用户写 .dfy 架构图  ──Action──►  SVG 附到 PR
用户改 CI workflow  ──App───►   SVG 附到 PR
```

---

## 2. 用户体验（仓库视角）

### 2.1 一次性安装

1. 用户在 GitHub Marketplace 安装 **Drawify Workflow Visualizer** App
2. 选择要授权的仓库（或整个 org）
3. 无需修改现有 workflow 文件

### 2.2 日常使用

某 PR 或 Issue 的评论中：

```text
@drawify-bot visualize
```

或指定文件：

```text
@drawify-bot visualize .github/workflows/ci.yml
```

几秒后，机器人在同一线程回复：

```markdown
@user 这是 `ci.yml` 的 pipeline 结构图：

![CI Pipeline](https://assets.drawify.app/svg/a1b2c3....svg)

---
[在 Drawify 中打开](https://drawify.app/...) · Powered by [Drawify](https://drawify.app)
```

### 2.3 为什么不默认自动出图

自动在每次 push 或 workflow 变更时评论，容易被视为噪音，且徒增渲染与存储成本。`@` 触发让用户可控，更符合开发者工具习惯，也更适合免费服务。

后续可在「PR 修改了 workflow 文件」时仅发一行提示，引导用户 `@drawify-bot visualize`，而不直接贴大图。

---

## 3. 技术架构

```text
GitHub issue_comment webhook
        │
        ▼
  Webhook 接收（验签、去重）
        │
        ▼
  解析 @drawify-bot 指令
        │
        ▼
  GitHub API 拉取 workflow YAML
        │
        ▼
  YAML → Drawify DSL 转换
        │
        ▼
  drawify-core / drawify-server 渲染 SVG
        │
        ├── 内容 hash 已存在于 R2 ──► 复用 URL
        │
        └── 不存在 ──► 写入 R2 ──► 返回 URL
        │
        ▼
  GitHub API 回复 PR 评论（Markdown 图片语法）
```

### 3.1 可复用的现有组件

| 组件 | 用途 |
|------|------|
| `drawify-core` | 布局与 SVG 渲染 |
| `drawify-server` | HTTP `/render` 接口，App Worker 可直接调用 |

新增模块：

| 模块 | 说明 |
|------|------|
| GitHub App 注册与 webhook | 接收 `issue_comment` 事件 |
| Workflow 解析器 | YAML → Drawify AST / DSL |
| R2 存储适配层 | 上传、去重、生命周期 |
| 评论回复器 | 组装 Markdown 并调用 GitHub API |

### 3.2 GitHub App 权限

| 权限 | 用途 |
|------|------|
| `contents: read` | 读取 `.github/workflows/*.yml` |
| `issues: write` | 在 PR/Issue 评论线程回复 |
| `pull_requests: read` | 获取 PR 上下文 |
| `metadata: read` | 基础仓库信息 |

### 3.3 Webhook 事件

| 事件 | 用途 |
|------|------|
| `issue_comment` | 监听 `@drawify-bot` 指令（PR 评论包含在内） |
| `installation` / `installation_repositories` | 记录安装范围 |

### 3.4 为什么不能把 SVG 直接贴进评论

GitHub PR 评论只支持 Markdown。渲染图片需要可访问的 URL（`![alt](url)`）。GitHub 会过滤内联 HTML、`<svg>` 标签和 `data:` URI，因此 **无法跳过托管直接在评论中显示 SVG**。

---

## 4. 存储方案（低成本）

本服务不收费，存储策略以 **短缓存、去重、按需生成** 为核心。

### 4.1 推荐方案：Cloudflare R2

| 项目 | 说明 |
|------|------|
| 免费额度 | 10 GB 存储 / 月 |
| 出站流量 | 通过 Cloudflare 公开访问时无 egress 费用 |
| 超出单价 | 约 $0.015/GB/月 |
| 对象 key | `svg/{content_hash}.svg` |
| 公开 URL | `https://assets.drawify.app/svg/{hash}.svg` |

### 4.2 成本粗算

按平均每张 SVG 80 KB：

| 月新增唯一图 | 30 天滚动存储量 | 月成本 |
|-------------|----------------|--------|
| 1,000 | ~80 MB | $0 |
| 10,000 | ~800 MB | $0 |
| 50,000 | ~4 GB | $0（免费额度内） |

### 4.3 四个省钱策略

1. **内容去重**：以 `sha256(workflow_yaml + drawify_version)` 为 key，相同内容只存一份
2. **短 TTL**：R2 生命周期规则，30 天自动删除；过期后按需重新渲染
3. **缓存命中跳过渲染**：R2 已有则直接返回 URL，省 CPU 与写入
4. **限流防滥用**：
   - 每 repo 每小时 ≤ 10 次
   - 每 installation 每天 ≤ 100 次
   - 同一 PR 同一命令 5 分钟内只响应 1 次

### 4.4 不采用的方案

| 方案 | 不采用原因 |
|------|-----------|
| Mermaid 代码块嵌入 PR | 竞品格式，无法展示 Drawify 能力，与品牌目标冲突 |
| 提交 SVG 到用户仓库 | 产生 bot commit，污染用户历史 |
| AWS S3 裸用 | egress 费用不可控 |
| 永久归档所有 SVG | 免费服务成本不可控 |

---

## 5. 生成什么样的图

### 5.1 MVP：Job 依赖 DAG

- 节点：trigger（`on:`）、job
- 边：`needs` 依赖关系
- 布局：`flowchart` + `sugiyama-v2` + 正交边路由

```text
[on: push/PR] ──► build ──► test ──► deploy
                    └──► lint
```

### 5.2 后续迭代

| 阶段 | 能力 |
|------|------|
| P1 | Job 内 steps 展开 |
| P2 | Reusable workflow（`job.uses`）跨文件引用 → 架构图 |
| P3 | PR 中 workflow 变更的前后 diff 图（结合 `diff.rs`） |

### 5.3 已知限制

- `${{ }}` 表达式无法静态求值，图上标注为「动态条件」
- Matrix 不展开为 N 个节点，折叠为「matrix (N variants)」
- 跨 repo 的 reusable workflow MVP 仅显示引用路径，不递归拉取

---

## 6. 部署与资源

### 6.1 MVP 最小部署

```text
1 台 VPS / Fly.io machine（$5–10/月）
├── github-app（webhook + worker）
├── drawify-server（/render）
└── Redis（Upstash 免费档，webhook 去重 + 限流）

Cloudflare R2（SVG 存储，免费档）
GitHub App 本身（GitHub 侧免费）
```

**预估月成本：$5–15**

### 6.2 计算资源

单次 `@drawify-bot visualize` 耗时约 1–3 秒：

| 步骤 | 耗时 |
|------|------|
| GitHub API 拉取 YAML | 200ms–1s |
| YAML 解析 | <50ms |
| Drawify 渲染 SVG | 100ms–500ms |
| 上传 R2 + 回复评论 | 200ms–1s |

1 vCPU / 1 GB 内存足以支撑早期流量。瓶颈在 GitHub API 配额（每 installation 约 5000 次/小时），不靠缓存命中可支撑约 600–1500 次完整流程/小时。

### 6.3 人力投入

| 阶段 | 人力 |
|------|------|
| MVP | 1 后端 × 2–3 周 |
| Marketplace 上架 | +3–5 天（文档、隐私政策、Logo） |
| 持续运维 | 约 0.5 人天/周 |

---

## 7. 实施路径

### Phase 0：本地验证（3–5 天）

```bash
# 目标：CLI 跑通 YAML → SVG
drawify gha visualize .github/workflows/ci.yml -o ci.svg
```

不建 App，先验证解析质量与图的视觉效果。

### Phase 1：GitHub App MVP（2–3 周）

- 注册 GitHub App，订阅 `issue_comment`
- 支持 `@drawify-bot visualize` 与 `@drawify-bot visualize <path>`
- R2 短缓存 + PR 评论回贴 SVG 链接
- Marketplace 上架

### Phase 2：体验增强

- PR 修改 workflow 时发提示（不自动贴图）
- 公开预览页：`drawify.app/gh/{owner}/{repo}/workflows/{name}`
- README Badge：`![CI Diagram](https://drawify.app/badge/{owner}/{repo})`

### Phase 3：差异化

- Workflow 变更 diff 图
- Steps 展开、reusable workflow 架构图
- 与 GitHub Action（`.dfy` → SVG）联动

---

## 8. 品牌与增长

### 8.1 每条回复的标准模板

```markdown
@user 这是 `{workflow_path}` 的 pipeline 结构图：

![{workflow_name}](https://assets.drawify.app/svg/{hash}.svg)

---
[在 Drawify 中打开](https://drawify.app/view/{id}) · Powered by [Drawify](https://drawify.app)
```

### 8.2 增长触点

| 触点 | 说明 |
|------|------|
| PR 评论 | 每次 `@` 都是一次品牌曝光 |
| SVG 页面 | 点击「在 Drawify 中打开」引流至 Playground |
| Marketplace | 「Drawify Workflow Visualizer」搜索入口 |
| README Badge | 仓库首页持续展示 |

---

## 9. 命令设计（MVP）

| 用户输入 | 行为 |
|----------|------|
| `@drawify-bot visualize` | 渲染当前上下文中的默认 workflow（或 PR 中变更的 workflow） |
| `@drawify-bot visualize ci.yml` | 渲染指定路径 |
| `@drawify-bot help` | 回复帮助与支持的命令列表 |

`issue_comment` webhook 收到后：

1. 检查 `comment.body` 是否包含 `@drawify-bot` 或 `@drawify[bot]`
2. 用 `comment.id` / `X-GitHub-Delivery` 去重，保证只响应一次
3. 快速返回 200，异步执行渲染与回复

---

## 10. 风险与对策

| 风险 | 对策 |
|------|------|
| 免费服务被滥用 | 限流 + 仅已安装 App 的仓库可触发 |
| 存储成本膨胀 | 30 天 TTL + 内容去重 |
| GitHub API 超限 | commit SHA / content hash 缓存 |
| 简单流水线价值感低 | 主打复杂 CI 与 workflow PR review 场景 |
| 竞品已有 YAML 可视化工具 | 差异化在 Drawify 渲染质量、diff 能力、生态联动 |

---

## 11. 待决事项

- [ ] Workflow 解析器：自研 Rust 解析 vs 封装 `@actions/workflow-parser`
- [ ] 默认渲染哪个 workflow 文件（单文件 / 全部 / PR diff 涉及的文件）
- [ ] 私有仓库 SVG 的访问策略（签名 URL vs 仅 GitHub 评论内嵌）
- [ ] 预览页是否纳入 MVP 还是 Phase 2
- [ ] Marketplace 展示名称与 Logo
