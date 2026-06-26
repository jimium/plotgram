# Drawify 成功路线图：必须做对的五件事

> 基于竞品分析与项目现状，识别出决定 Drawify 成败的五个关键领域。

## 0. 现状概览

Drawify 是一个为 AI Agent 设计的图表 DSL 语言和渲染引擎，核心差异化：

- **固定语法**（3 种箭头、显式结构）— LLM 生成更准确
- **语义实体**（`type: database`）— 渲染器自动选择形状和图标
- **AST 一等公民** — JSON 导出、语义 diff/patch
- **自动布局** — 引擎推断布局，无需指定坐标
- **多种笔触皮肤** — Excalidraw / Blueprint / Neon Glow 等
- **Rust + WASM** — 高性能，浏览器可直接运行

技术引擎已相当完善（6 种图表类型、5 种导出格式、7 种笔触皮肤、多布局算法、CLI/Server/WASM 全渠道）。核心风险不在技术，而在**证明差异化价值**和**让足够多的人体验到**。

---

## 1. 让 LLM 真的能写好 Drawify

**为什么关键**："为 AI Agent 设计"是核心卖点，但目前只是 claim，没有数据支撑。如果 LLM 生成 Drawify 的正确率不比 Mermaid 高，卖点就不成立。

### 1.1 LLM 语法正确率基准测试

- 用 GPT-4 / Claude / Gemini 对比生成 Mermaid vs Drawify
- 测试维度：语法正确率、语义正确率、首次生成可用率、修复轮次
- 场景覆盖：6 种图表类型 × 简单/中等/复杂
- 目标：Drawify 首次正确率 > Mermaid 20%+

### 1.2 LLM Prompt Template / System Prompt

- 提供开箱即用的 system prompt，让任何 LLM 都能高质量生成 Drawify DSL
- 这是 Mermaid/D2 都没做好的——它们只提供了语法文档
- 包含：语法速查表、常见模式模板、错误修复指引
- 发布为可复制的 markdown 文件 + API 可消费的 JSON 格式

### 1.3 结构化错误 → 自动修复闭环

- 当前 `validate` 返回的错误信息已比 Mermaid 好（有位置、有修复建议）
- 但还缺一步：把错误信息喂回 LLM 让它自动修复
- 需要设计一个 `fix` 接口或示例 workflow：
  ```
  用户描述 → LLM 生成 DSL → validate 报错 → 错误信息喂回 LLM → 修复后 DSL → 通过
  ```
- 这才是"为 AI 设计"的完整体验

---

## 2. Playground 必须让人 30 秒内 "Wow"

**为什么关键**：第一印象决定一切。没有可体验的产品就没有用户。

### 2.1 Live Playground 上线

- 部署到 drawify.dev 或类似域名
- 打开即用，零安装
- WASM 驱动，全部在浏览器运行

### 2.2 "AI 生成"按钮

- 用户输入自然语言描述 → 调用 LLM → 生成 Drawify DSL → 实时渲染
- 这是 Eraser.io 的核心体验，Drawify 必须做到
- 可以先用客户端 API Key 模式（用户自带 Key），降低成本

### 2.3 示例库要惊艳

- 6 种图表类型各放 2-3 个高质量示例
- 一打开就觉得"这比 Mermaid 好看"
- 示例要展示 Drawify 独有能力：语义实体、图标库、笔触皮肤

### 2.4 笔触皮肤一键切换

- Excalidraw / Blueprint / Neon Glow 等皮肤是 Drawify 独有的杀手级特性
- Mermaid 和 D2 都没有
- 必须在首页就能看到切换效果
- 一个按钮从 Standard → Excalidraw → Blueprint，视觉冲击力极强

---

## 3. 抢占 GitHub / 开发者生态位

**为什么关键**：Mermaid 统治这个领域的核心原因是 GitHub 原生渲染。Drawify 无法复制这一点，但可以找到侧翼入口。

### 3.1 GitHub Action（P1）

- 提交 `.dfy` 文件 → CI 自动渲染 SVG/PNG → 附加到 PR comment
- 这是最低成本的生态切入点
- 用户不需要 GitHub 支持 Drawify，只需要一个 Action

### 3.2 VS Code 插件（P1）

- 语法高亮 + 实时预览
- Mermaid 和 D2 都有，Drawify 也必须有
- 基于 WASM，可以在插件内直接渲染，无需外部服务

### 3.3 Kroki 集成（P2）

- Kroki 是通用图表渲染 API，支持 20+ 语言
- 让 Kroki 支持 Drawify 格式，就能进入所有使用 Kroki 的平台
- 需要向 Kroki 提交 PR 或提供独立渲染服务

### 3.4 Markdown 代码块渲染（P2）

- 至少支持 ````drawify` 在常见静态站点生成器中渲染
- Docusaurus 插件、MkDocs 插件
- Remark/rehype 插件（适用于 Next.js 等框架）

---

## 4. Diff/Patch 是护城河，但需要场景验证

**为什么关键**：AST 级别的 diff/patch 是 Drawify 独有的能力，但目前看不到真实使用场景。如果场景跑不通，就是过度设计。

### 4.1 "图表即代码审查"场景

- PR 中 `.dfy` 文件的 diff 不只是文本 diff，而是语义 diff
- 示例："新增了 2 个节点（Redis、Kafka），删除了 1 条边（API → DB），修改了 1 个实体标签"
- 做一个 GitHub Action demo 展示这个体验
- 与文本 diff 对比，语义 diff 的信息密度高一个量级

### 4.2 AI Agent 增量编辑场景

- 展示 LLM 如何通过 patch 而非重写来修改图表
- 示例："把流程图中的审批步骤改为并行审批" → AI 只需生成 patch（添加节点 + 修改边），而非重写整个图表
- 优势：保留用户的手动调整、减少 LLM token 消耗、变更可审查

### 4.3 诚实评估

- 如果这两个场景跑不通，diff/patch 就是过度设计
- 需要尽早验证，避免在低价值特性上投入过多

---

## 5. 开源社区运营

**为什么关键**：Mermaid 有 70k+ GitHub stars，D2 有 18k+。Drawify 要被看见。

### 5.1 开源发布准备

- 确保 repo 干净、README 完善、贡献指南清晰
- LICENSE 已是 MIT，无需调整
- 添加 CODE_OF_CONDUCT.md、CONTRIBUTING.md
- 确保所有公开 API 文档化

### 5.2 技术博客 / 内容营销

重点文章方向（数据驱动，不是自嗨）：

1. **"为什么 Mermaid 的语法对 LLM 不友好"** — 用基准测试数据说话
2. **"Drawify vs Mermaid vs D2：LLM 生成正确率对比"** — 核心差异化证明
3. **"用 AI Agent 自动维护架构图"** — 场景化展示
4. **"Diagram-as-Code 的下一个十年：AI-Native DSL"** — 愿景叙事

### 5.3 发布渠道

- Hacker News：准备好 demo 和对比数据
- Reddit：r/programming、r/rust、r/LocalLLaMA
- V2EX / 掘金 / 知乎：中文开发者社区
- Twitter/X：@drawify 官方账号

### 5.4 AI 编程工具集成

- **MCP Server**：让 Cursor / Claude Code / Copilot 等 AI 编程工具的 agent 能直接生成 Drawify DSL
- 这是最直接的方式——AI 工具是 Drawify 的天然分发渠道
- 提供 `drawify-mcp` crate，暴露 render/validate/diff 能力

---

## 优先级矩阵

| 优先级 | 事项 | 依赖 | 预期影响 |
|--------|------|------|----------|
| **P0** | LLM 正确率基准测试 + Prompt Template | 无 | 核心卖点需要数据支撑 |
| **P0** | Playground 上线 + AI 生成按钮 | WASM 已就绪 | 没有可体验的产品就没有用户 |
| **P1** | VS Code 插件 | WASM 已就绪 | 开发者日常触点 |
| **P1** | GitHub Action | CLI + Server 已就绪 | 代码审查场景入口 |
| **P1** | 开源发布 + 社区推广 | P0 完成 | 被看见才能被采用 |
| **P2** | Diff/Patch 场景验证与 demo | Diff 模块已就绪 | 护城河需要场景验证 |
| **P2** | MCP Server | Server 已就绪 | AI 工具生态入口 |
| **P2** | Kroki / Markdown 插件集成 | 核心稳定后 | 平台覆盖面 |

---

## 竞品参照

| 项目 | 定位 | GitHub Stars | Drawify 的差异化 |
|------|------|-------------|-----------------|
| Mermaid | 文本图表，GitHub 原生渲染 | ~70k | Mermaid 为人类手写设计，语法歧义多，LLM 生成错误率高 |
| PlantUML | UML 体系最全，企业级 | ~10k | Java 依赖，无 WASM，无笔触皮肤 |
| D2 | 现代语法，默认美观 | ~18k | 无语义实体、无 AST diff/patch、无笔触皮肤 |
| Graphviz | 老牌布局引擎 | ~13k | 语法晦涩，无 AI 友好设计 |
| Eraser.io | AI + diagram-as-code | 商业产品 | 闭源，无 AST 操作，无笔触皮肤 |
| Structurizr | C4 模型专用 | ~2k | 仅限 C4，无通用图表 |

**关键洞察**：市场上没有其他项目同时做到"为 AI 生成优化 + AST 级别操作 + 笔触皮肤 + 自动布局"这个组合。Drawify 的窗口期在于**在 Mermaid/D2 意识到 AI-native 需求之前**建立先发优势。

---

## 一句话总结

核心风险不是技术（引擎已经相当完善），而是**证明"为 AI 设计"不是空话**，以及**让足够多的人体验到这个差异**。P0 只有两条：用数据证明 LLM 友好性，用 Playground 让人亲眼看到。
