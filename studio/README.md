# Drawify Studio

**LLM 驱动的 Agent 绘图工作台**

Drawify Studio 是基于 [drawify](../) 构建的智能图表创作工具。用户用自然语言描述需求,LLM Agent 通过 Tool-Calling 操控 drawify-wasm 生成与迭代图表,无需手写 DSL。

## 与 Playground 的区别

| 能力 | Playground | Studio |
|------|-----------|--------|
| 手写 DSL 编辑 | 有 | 无 |
| 布局/外观参数面板 | 有 | 无 |
| 示例库浏览 | 有 | 无 |
| LLM 对话驱动 | 无 | 核心 |
| Agent 多步迭代 | 无 | 核心 |
| 变更 Diff 预览 | 无 | 核心 |
| Agent Tool-Calling | 无 | 核心 |

Studio 不重复 Playground 的工作,专注 LLM 驱动与 Agent 绘图。

## 核心特性

- **自然语言驱动**:用户描述需求,Agent 生成图表
- **多轮迭代**:基于上下文增量修改,支持"加个缓存""改个标签"等指令
- **变更可视化**:每次修改展示 Diff 摘要,用户可接受或拒绝
- **错误自修复**:Agent 利用 drawify 的结构化诊断自动修复 DSL 错误
- **多 LLM 支持**:OpenAI / Anthropic / Ollama / 自定义 OpenAI 兼容服务
- **本地运行**:WASM 渲染,API Key 仅存本地,不上传第三方

## 快速开始

### 环境要求

- Node.js 18+
- npm 或 pnpm
- Rust 1.75+(用于构建 drawify-wasm,见根目录说明)

### 安装

```bash
cd studio
npm install
```

### 构建 WASM 产物

Studio 依赖 drawify-wasm,需先在仓库根目录构建:

```bash
cd ../crates/drawify-wasm
wasm-pack build --target web --out-dir ../../studio/drawify-wasm
```

### 配置 LLM

复制环境变量模板并填入 API Key:

```bash
cp .env.example .env.local
```

编辑 `.env.local`:

```env
VITE_LLM_PROVIDER=openai
VITE_LLM_API_KEY=sk-...
VITE_LLM_MODEL=gpt-4o
```

### 启动开发服务器

```bash
npm run dev
```

打开 http://localhost:3100

## 使用方式

1. 在右侧对话区输入需求,例如"画一个微服务架构图"
2. Agent 调用 render 工具生成 SVG,左侧预览区显示结果
3. 继续对话迭代,例如"给订单服务加一个 Redis 缓存"
4. Agent 用 apply_patch 增量修改,展示变更摘要
5. 接受或拒绝变更,导出 SVG

## 项目结构

```
studio/
├── src/
│   ├── agent/              # Agent 核心(重点)
│   │   ├── types.ts        # 类型定义(与 drawify-core 对齐)
│   │   ├── prompt.ts       # System Prompt 模板
│   │   ├── tools.ts        # Tool 定义与 WASM 桥接
│   │   ├── context.ts      # 对话上下文管理
│   │   ├── AgentLoop.ts    # Agent 循环引擎
│   │   └── index.ts
│   ├── components/         # UI 组件
│   │   ├── ChatPanel.tsx   # 对话面板
│   │   ├── ChatMessage.tsx # 单条消息
│   │   ├── PreviewCanvas.tsx # SVG 预览
│   │   ├── DiffSummary.tsx # 变更摘要
│   │   ├── DslViewer.tsx   # DSL 只读查看
│   │   ├── TopBar.tsx
│   │   └── ExportActions.tsx
│   ├── hooks/              # React Hooks
│   │   ├── useAgent.ts     # Agent 状态管理
│   │   ├── useWasm.ts      # WASM 加载
│   │   └── useDiagram.ts   # 图表渲染
│   ├── lib/                # 工具库
│   │   ├── wasm.ts         # WASM 桥接(扩展 diff/patch)
│   │   ├── llm.ts          # LLM 客户端封装
│   │   └── exportImage.ts  # 导出工具
│   ├── styles/             # 样式
│   ├── App.tsx
│   └── main.tsx
├── tests/                  # 单元测试
│   ├── agent/
│   └── lib/
├── docs/                   # 文档
│   ├── architecture.md
│   ├── api.md
│   ├── development.md
│   └── deployment.md
├── package.json
├── tsconfig.json
├── vite.config.ts
└── vitest.config.ts
```

## 文档

- [架构设计](docs/architecture.md)
- [API 文档](docs/api.md)
- [开发规范](docs/development.md)
- [部署流程](docs/deployment.md)
- [开发任务规划](docs/tasks.md)

## 技术栈

- React 19 + TypeScript
- Vite 8
- Vitest(测试)
- drawify-wasm(图表渲染引擎)
- OpenAI 兼容 API(LLM)

## 许可证

MIT
