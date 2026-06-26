# Drawify Studio 文档

Drawify Studio 前端应用的文档中心。

## 目录结构

```
studio/docs/
├── README.md              ← 本文件
├── architecture.md        整体架构设计
├── api.md                 API 文档
├── development.md         开发规范
├── deployment.md          部署流程
├── tasks.md               开发任务规划
├── agent/                 Agent 子系统文档
│   ├── README.md
│   └── architecture.html  Agent 架构可视化
├── design/                设计方案
│   ├── README.md
│   └── planner-agent-design.md  Planner Agent 设计方案
└── specs/                 接口规范
    └── README.md
```

## 快速导航

### 入门

| 文档 | 说明 |
|------|------|
| [architecture.md](./architecture.md) | 整体架构设计（Agent Loop 循环引擎、分层架构图） |
| [development.md](./development.md) | 开发规范（代码风格、命名规范、React 约定） |
| [deployment.md](./deployment.md) | 部署流程（WASM 构建、前端构建、静态部署） |

### API 与规范

| 文档 | 说明 |
|------|------|
| [api.md](./api.md) | API 文档（Agent 模块、WASM 桥接层、LLM 客户端） |
| [specs/](./specs/) | Studio 前端接口规范（Tool 接口、LLM 协议、WASM 桥接） |

### Agent 子系统

| 文档 | 说明 |
|------|------|
| [agent/architecture.html](./agent/architecture.html) | Agent 架构可视化页面 |
| [design/planner-agent-design.md](./design/planner-agent-design.md) | Planner Agent 设计方案 |

### 项目管理

| 文档 | 说明 |
|------|------|
| [tasks.md](./tasks.md) | 开发任务规划（P0-P3 阶段、任务分解、验收标准） |

## 与项目根目录 docs/ 的关系

| 位置 | 范围 |
|------|------|
| `docs/specs/` | Drawify DSL 语言规范、AST 规范、Pipeline 规范（核心，跨项目） |
| `docs/architecture/` | drawify-core 渲染引擎架构、算法、WASM 模块（后端） |
| `docs/product/` | 产品愿景、竞品分析、路线图（产品） |
| `studio/docs/` | Drawify Studio 前端应用专属文档（本目录） |
