/**
 * Agent 类型定义
 *
 * 与 drawify-core 的 diff/types.rs 对齐,
 * Agent 通过这些类型与 WASM 交互
 */

/** 变更操作类型(对应 Rust ChangeOp) */
export type ChangeOp = 'add' | 'remove' | 'modify';

/** 变更目标类型(对应 Rust ChangeTarget) */
export type ChangeTarget = 'entity' | 'relation' | 'group' | 'attribute';

/** 变更路径(对应 Rust ChangePath) */
export interface ChangePath {
  target: ChangeTarget;
  id: string | null;
  attr_key: string | null;
}

/** 单个变更记录(对应 Rust Change) */
export interface Change {
  op: ChangeOp;
  path: ChangePath;
  old_value?: unknown;
  new_value?: unknown;
  description?: string;
}

/** Diff 统计(对应 Rust DiffStats) */
export interface DiffStats {
  added: number;
  removed: number;
  modified: number;
}

/** Diff 结果(对应 Rust DiffResult) */
export interface DiffResult {
  changes: Change[];
  stats: DiffStats;
}

/** Patch 应用结果 */
export interface PatchResult {
  success: boolean;
  source: string | null;
  applied: number;
  skipped: number;
  errors: string[];
}

/** 渲染格式(与 Rust RenderFormat 对齐) */
export type RenderFormat = 'svg' | 'ascii' | 'json' | 'png' | 'webp';

/** 渲染结果(对应 Rust RenderResult) */
export interface RenderResult {
  success: boolean;
  format: string;
  text: string | null;
  errors: string[];
  warnings: string[];
}

/** 校验结果 */
export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

/** 解析结果 */
export interface ParseResult {
  diagram: unknown | null;
  errors: string[];
  warnings: string[];
}

/** 渲染选项 */
export interface RenderOptions {
  theme_id?: string;
  graphic_style?: string;
  dark_mode?: boolean;
  transparent_background?: boolean;
}

/** 图表类型 */
export type DiagramKind =
  | 'flowchart'
  | 'sequence'
  | 'architecture'
  | 'state'
  | 'er'
  | 'mindmap';

/** LLM Tool Call */
export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

/** LLM Tool 定义 schema */
export interface ToolSchema {
  type: 'function';
  function: {
    name: string;
    description: string;
    parameters: {
      type: 'object';
      properties: Record<string, unknown>;
      required?: string[];
    };
  };
}

/** LLM 消息 */
export interface LLMMessage {
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  tool_calls?: ToolCall[];
  tool_call_id?: string;
}

/** LLM 响应 */
export interface LLMResponse {
  content: string;
  tool_calls?: ToolCall[];
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
  };
}

/** Agent 执行步骤 */
export interface AgentStep {
  type: 'thinking' | 'tool_call' | 'tool_result' | 'response' | 'error';
  content: string;
  toolCall?: ToolCall;
  toolResult?: unknown;
  timestamp: number;
}

/** 对话消息(前端展示用) */
export interface ChatMessage {
  id: string;
  role: 'user' | 'agent' | 'system';
  content: string;
  timestamp: number;
  /** Agent 消息附带的变更差异 */
  diff?: DiffResult;
  /** Agent 消息附带的渲染结果 */
  svg?: string;
  /** Agent 消息附带的 Tool 调用记录 */
  toolCalls?: ToolCall[];
  /** 是否有待确认的变更 */
  pendingChanges?: boolean;
}

/** Agent 执行上下文 */
export interface AgentContext {
  /** 当前生效的 DSL 源码 */
  source: string;
  /** 当前图表类型 */
  diagramType: DiagramKind | null;
  /** 对话历史 */
  history: ChatMessage[];
  /** 最大迭代次数 */
  maxIterations: number;
}

/** Agent 执行结果 */
export interface AgentResult {
  message: string;
  source: string;
  svg?: string;
  diff?: DiffResult;
  toolCalls?: ToolCall[];
}

/** Agent 配置 */
export interface AgentConfig {
  /** LLM 客户端 */
  llm: LLMClient;
  /** Tool 执行器映射 */
  tools: Record<string, ToolExecutor>;
  /** 最大迭代次数 */
  maxIterations: number;
  /** 步骤回调(用于 UI 更新) */
  onStep: (step: AgentStep) => void;
  /** AbortSignal,用于取消 LLM 请求 */
  signal?: AbortSignal;
}

/** LLM 流式 chunk */
export interface LLMStreamChunk {
  /** chunk 类型 */
  type:
    | 'delta' // 文本增量(正常回复)
    | 'thinking' // 思考过程(DeepSeek reasoning_content)
    | 'tool_call_delta' // tool_call 增量(按 index 累积)
    | 'done' // 流结束
    | 'error'; // 错误
  /** 文本内容增量 */
  content?: string;
  /** tool_call 增量 */
  toolCallDelta?: {
    index: number;
    id?: string; // 仅首片有
    name?: string; // 仅首片有
    argumentsDelta?: string; // JSON 片段,需累积
  };
  /** token 用量(仅 done 片可能有) */
  usage?: { prompt_tokens: number; completion_tokens: number };
  /** 错误信息 */
  error?: string;
}

/** LLM 客户端接口 */
export interface LLMClient {
  chat(params: {
    messages: LLMMessage[];
    tools?: ToolSchema[];
    signal?: AbortSignal;
  }): Promise<LLMResponse>;
  /** 流式对话,返回 AsyncIterable 供消费 */
  chatStream(params: {
    messages: LLMMessage[];
    tools?: ToolSchema[];
    signal?: AbortSignal;
  }): AsyncIterable<LLMStreamChunk>;
}

/** Tool 执行器函数类型 */
export type ToolExecutor = (
  args: Record<string, unknown>,
  context: AgentContext,
) => Promise<unknown>;
