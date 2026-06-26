# API 文档

本文档描述 Drawify Studio 的核心 API,包括 Agent 模块、WASM 桥接层、LLM 客户端。

## 1. Agent 模块

### 1.1 runAgentLoop

运行 Agent 循环,处理用户输入并返回结果。

```typescript
import { runAgentLoop, createAgentContext } from '@agent/index';

const context = createAgentContext({ maxIterations: 10 });
const result = await runAgentLoop(userMessage, context, {
  llm: llmClient,
  tools: toolExecutors,
  maxIterations: 10,
  onStep: (step) => console.log(step),
});
```

**参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| userMessage | string | 用户输入 |
| context | AgentContext | Agent 上下文(会被原地修改 source) |
| config.llm | LLMClient | LLM 客户端 |
| config.tools | Record<string, ToolExecutor> | Tool 执行器映射 |
| config.maxIterations | number | 最大迭代次数 |
| config.onStep | (step: AgentStep) => void | 步骤回调 |

**返回**:Promise<AgentResult>

```typescript
interface AgentResult {
  message: string;       // Agent 回复内容
  source: string;        // 最终 DSL 源码
  svg?: string;          // 最后一次渲染的 SVG
  diff?: DiffResult;     // 最后一次 diff 结果
  toolCalls?: ToolCall[]; // 所有 Tool 调用记录
}
```

### 1.2 createAgentContext

创建初始 Agent 上下文。

```typescript
const context = createAgentContext({
  initialSource: 'diagram flowchart {}',
  maxIterations: 10,
});
```

### 1.3 AgentContext

```typescript
interface AgentContext {
  source: string;                    // 当前 DSL 源码
  diagramType: DiagramKind | null;   // 图表类型
  history: ChatMessage[];            // 对话历史
  maxIterations: number;             // 最大迭代次数
}
```

## 2. Tool 定义

### 2.1 AGENT_TOOL_SCHEMAS

预定义的 6 个 Tool Schema,供 LLM function-calling。

```typescript
import { AGENT_TOOL_SCHEMAS } from '@agent/tools';
// 传给 LLM 客户端的 tools 参数
```

### 2.2 createToolExecutors

创建 Tool 执行器映射。

```typescript
import { createToolExecutors } from '@agent/tools';

const executors = createToolExecutors(async () => wasmModule);
```

### 2.3 Tool 列表

#### render

渲染 DSL 为指定格式。

```typescript
// 参数
{
  source: string;        // DSL 源码
  format: 'svg' | 'ascii' | 'json';
  options?: RenderOptions;
}

// 返回
{
  success: boolean;
  format: string;
  text: string | null;   // 渲染结果
  errors: string[];
  warnings: string[];
}
```

#### validate

校验 DSL 语法和语义。

```typescript
// 参数
{ source: string }

// 返回
{
  valid: boolean;
  errors: string[];      // 含错误码、行号、修复建议
  warnings: string[];
}
```

#### parse

解析 DSL 为 AST JSON。

```typescript
// 参数
{ source: string }

// 返回
{
  diagram: object | null;
  errors: string[];
  warnings: string[];
}
```

#### diff

比较两份 DSL 差异。

```typescript
// 参数
{
  old_source: string;
  new_source: string;
}

// 返回 DiffResult
{
  changes: Change[];
  stats: { added: number; removed: number; modified: number };
}
```

#### apply_patch

应用增量补丁。

```typescript
// 参数
{
  source: string;
  patch: Change[];
}

// 返回
{
  success: boolean;
  source: string | null;  // 修改后的完整 DSL
  applied: number;
  skipped: number;
  errors: string[];
}
```

#### layout_catalog

查询可用布局算法。

```typescript
// 参数:无
// 返回:LayoutCatalog JSON
```

## 3. Change 类型

与 drawify-core 的 `diff/types.rs` 对齐。

```typescript
interface Change {
  op: 'add' | 'remove' | 'modify';
  path: {
    target: 'entity' | 'relation' | 'group' | 'attribute';
    id: string | null;
    attr_key: string | null;
  };
  old_value?: unknown;
  new_value?: unknown;
  description?: string;
}
```

### 3.1 新增实体示例

```json
{
  "op": "add",
  "path": { "target": "entity", "id": "redis", "attr_key": null },
  "new_value": {
    "id": "redis",
    "label": "Redis 缓存",
    "standard": {
      "type": { "$enum": "cache" },
      "semantic": { "$enum": "redis" }
    }
  }
}
```

### 3.2 修改属性示例

```json
{
  "op": "modify",
  "path": {
    "target": "entity",
    "id": "order_svc",
    "attr_key": "style/fill"
  },
  "old_value": "#ffffff",
  "new_value": "#ff6b6b"
}
```

### 3.3 删除关系示例

```json
{
  "op": "remove",
  "path": { "target": "relation", "id": "order_svc->redis", "attr_key": null },
  "old_value": { "from": "order_svc", "to": "redis" }
}
```

## 4. WASM 桥接层

### 4.1 loadWasm

懒加载 WASM 模块(全局单例)。

```typescript
import { loadWasm } from '@lib/wasm';

const wasm = await loadWasm();
```

### 4.2 renderSource

渲染 DSL。

```typescript
import { renderSource } from '@lib/wasm';

const result = renderSource(wasm, source, 'svg', optionsJson?);
```

### 4.3 diffSources

比较两份 DSL(需 WASM 支持 diff_sources 绑定)。

```typescript
import { diffSources } from '@lib/wasm';

const diff = diffSources(wasm, oldSource, newSource);
```

### 4.4 applyPatch

应用补丁(需 WASM 支持 apply_patch 绑定)。

```typescript
import { applyPatch } from '@lib/wasm';

const result = applyPatch(wasm, source, patchArray);
```

### 4.5 checkStudioCapabilities

检查 WASM 是否支持 Studio 所需能力。

```typescript
import { checkStudioCapabilities } from '@lib/wasm';

const caps = checkStudioCapabilities(wasm);
// { diff: boolean, applyPatch: boolean, astToSource: boolean }
```

## 5. LLM 客户端

### 5.1 createLLMClient

创建 LLM 客户端。

```typescript
import { createLLMClient, loadLLMConfigFromEnv } from '@lib/llm';

const config = loadLLMConfigFromEnv();
const client = createLLMClient(config);
```

### 5.2 LLMConfig

```typescript
interface LLMConfig {
  provider: 'openai' | 'anthropic' | 'ollama' | 'custom';
  apiKey: string;
  model: string;
  baseUrl: string;
  maxTokens: number;
  temperature: number;
}
```

### 5.3 LLMClient

```typescript
interface LLMClient {
  chat(params: {
    messages: LLMMessage[];
    tools?: ToolSchema[];
  }): Promise<LLMResponse>;
}
```

## 6. Hooks

### 6.1 useAgent

管理 Agent 对话状态。

```typescript
import { useAgent } from '@hooks/useAgent';

const {
  messages,          // ChatMessage[]
  currentSource,     // string
  currentSvg,        // string
  lastDiff,          // DiffResult | null
  isRunning,         // boolean
  error,             // string | null
  sendMessage,       // (text: string) => Promise<void>
  abort,             // () => void
  acceptChanges,     // () => void
  rejectChanges,     // () => void
} = useAgent({ wasm, ready });
```

### 6.2 useWasm

加载 WASM 模块。

```typescript
import { useWasm } from '@hooks/useWasm';

const { wasm, ready, error, version, capabilities } = useWasm();
```

### 6.3 useDiagram

根据 DSL 自动渲染(防抖)。

```typescript
import { useDiagram } from '@hooks/useDiagram';

const { svg, validationResult, isLoading } = useDiagram({
  wasm, ready, source, debounceMs: 300,
});
```

## 7. 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| VITE_LLM_PROVIDER | openai | LLM Provider |
| VITE_LLM_API_KEY | (空) | LLM API Key |
| VITE_LLM_MODEL | gpt-4o | 模型名 |
| VITE_LLM_BASE_URL | https://api.openai.com/v1 | API 端点 |
| VITE_AGENT_MAX_ITERATIONS | 10 | Agent 最大迭代次数 |
