# 开发规范

## 1. 代码风格

### 1.1 语言与注释

- 代码注释使用**中文**(与项目主语言一致)
- 不使用 emoji
- 公共 API 必须有 JSDoc 注释
- 复杂逻辑必须有解释性注释

### 1.2 命名规范

| 类型 | 规范 | 示例 |
|------|------|------|
| 文件 | kebab-case 或 PascalCase(组件) | `AgentLoop.ts`、`ChatPanel.tsx` |
| 变量/函数 | camelCase | `sendMessage`、`currentSource` |
| 类型/接口 | PascalCase | `AgentContext`、`ChatMessage` |
| 常量 | UPPER_SNAKE_CASE | `AGENT_TOOL_SCHEMAS`、`MIN_SCALE` |
| React 组件 | PascalCase | `ChatPanel`、`PreviewCanvas` |
| Hook | use 前缀 + camelCase | `useAgent`、`useWasm` |

### 1.3 TypeScript

- 严格模式(`strict: true`)
- 禁止 `any`,必须显式类型
- 优先使用 `interface` 定义对象类型,`type` 定义联合类型
- 导出类型用 `export type`,避免与值导出混淆

### 1.4 React

- 函数组件 + Hooks,不使用 class 组件
- 组件文件使用 `.tsx`,工具文件使用 `.ts`
- Props 接口以 `Props` 结尾命名,如 `ChatPanelProps`
- 副作用必须清理(在 useEffect 返回清理函数)

## 2. 项目结构规范

### 2.1 目录划分

```
src/
├── agent/      # Agent 核心逻辑(无 UI 依赖)
├── components/ # React 组件(无业务逻辑)
├── hooks/      # React Hooks(连接 agent 与 components)
├── lib/        # 工具库(WASM 桥接、LLM 客户端、导出)
├── styles/     # 全局样式
└── types/      # 全局类型定义(如有)
```

### 2.2 依赖方向

```
components → hooks → agent → lib → (wasm/llm)
```

- `components` 不直接调用 `agent` 或 `lib`,通过 `hooks` 中转
- `agent` 不依赖 React,可独立测试
- `lib` 是纯工具,无状态

### 2.3 导入路径

使用路径别名,避免相对路径层级过深:

```typescript
// 正确
import { useAgent } from '@hooks/useAgent';
import type { ChatMessage } from '@agent/types';

// 错误
import { useAgent } from '../../hooks/useAgent';
```

## 3. Agent 开发规范

### 3.1 Tool 设计原则

- 每个 Tool 对应一个 WASM 函数,职责单一
- Tool 参数必须有清晰的 description,供 LLM 理解
- Tool 返回结构化数据,不返回原始字符串
- Tool 执行失败时返回 `{ error: string }`,不抛异常

### 3.2 Prompt 设计

- System Prompt 明确 Agent 的能力边界
- 包含 DSL 语法要点和常见枚举值
- 指导 Agent 优先使用增量修改(apply_patch)而非重写
- 指导 Agent 每次修改后自检(validate)

### 3.3 上下文管理

- `AgentContext` 是可变对象,AgentLoop 原地修改 `source`
- 对话历史超过 20 条自动压缩
- 消息 ID 用时间戳 + 序号保证唯一

## 4. 测试规范

### 4.1 测试框架

使用 Vitest,配置见 `vitest.config.ts`。

### 4.2 测试覆盖

| 模块 | 测试要求 |
|------|---------|
| agent/context | 必须 100% 覆盖 |
| agent/AgentLoop | 必须 90%+ 覆盖,含错误路径 |
| agent/prompt | 必须 90%+ 覆盖 |
| lib/wasm | 必须 90%+ 覆盖(mock WASM) |
| lib/llm | 必须 90%+ 覆盖(mock fetch) |
| components | 可选,关键交互测试 |
| hooks | 可选,集成测试 |

### 4.3 测试命名

```typescript
describe('模块名', () => {
  it('应描述预期行为', () => {});
  it('对某某场景返回某某', () => {});
  it('某某时不做某某', () => {});
});
```

### 4.4 Mock 规范

- WASM 调用必须 mock,不依赖真实 WASM
- fetch 调用必须 mock,不发起真实网络请求
- Mock 函数用 `vi.fn()`,验证调用参数用 `expect(x).toHaveBeenCalledWith(...)`

## 5. Git 规范

### 5.1 分支

- `main`:稳定分支
- `dev`:开发分支
- `feature/xxx`:功能分支
- `fix/xxx`:修复分支

### 5.2 提交信息

遵循 Conventional Commits:

```
<type>(<scope>): <subject>

type: feat | fix | docs | style | refactor | test | chore
scope: agent | ui | lib | docs | test | config
```

示例:
```
feat(agent): 新增 apply_patch Tool 执行器
fix(ui): 修复对话区滚动到底部时机
docs(api): 补充 Change 类型示例
```

## 6. 性能规范

- WASM 模块全局单例,避免重复加载
- 对话历史压缩,避免上下文无限增长
- 渲染防抖 300ms,避免频繁渲染
- Tool 结果日志截断 500 字符
- 大型 SVG 渲染时考虑虚拟化(后续优化)

## 7. 安全规范

- API Key 仅存 localStorage,不上传任何服务器
- 不在代码中硬编码 API Key
- 生产构建移除 console.log
- DSL 经 drawify-core 校验,防止注入
- LLM 请求通过 HTTPS

## 8. 与 drawify-core 的协作规范

- Studio **不修改** drawify-core 的管线架构
- 新增能力优先通过 WASM 绑定扩展,不改动 core
- 如需 core 新增能力(如 ast_to_source),在 core 实现 + WASM 导出 + Studio 消费
- WASM 绑定的类型签名必须与 core 的 Rust 类型对齐(见 `agent/types.ts`)
