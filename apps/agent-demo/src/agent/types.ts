/**
 * Agent 类型定义
 *
 * 与 tautcore-wasm 的 diff/patch/render 类型对齐，
 * DiffResult 归一化为带 stats 的展示格式（供 DiffSummary 使用）。
 */

import type {
  RenderResult,
  ValidationResult,
  ParseResult,
  RenderFormat,
  RenderOptions,
  ChangeOp,
  ChangeTarget,
  ChangeJson,
} from '@lib/wasm';

// 重新导出 WASM 层基础类型，供 agent 模块统一引用
export type { RenderResult, ValidationResult, ParseResult, RenderFormat, RenderOptions };

/** 变更路径（展示用，与 wasm ChangePathJson 一致）。 */
export interface ChangePath {
  target: ChangeTarget;
  id?: string;
  attr_key?: string;
}

/** 单条变更（展示用）。 */
export interface Change {
  op: ChangeOp;
  path: ChangePath;
  old_value?: unknown;
  new_value?: unknown;
}

/** Diff 统计。 */
export interface DiffStats {
  added: number;
  removed: number;
  modified: number;
}

/** Diff 结果（归一化展示格式，由 tools.ts 从 wasm DiffResult 转换）。 */
export interface DiffResult {
  success: boolean;
  changes: Change[];
  stats: DiffStats;
  errors: string[];
}

/** Patch 应用结果（展示格式）。 */
export interface PatchResult {
  success: boolean;
  source: string | null;
  applied: number;
  skipped: number;
  errors: string[];
}

/** 图表类型 */
export type DiagramKind =
  | 'flowchart'
  | 'sequence'
  | 'architecture'
  | 'state'
  | 'er'
  | 'mindmap';

/** LLM Tool Call（前端内部使用，扁平格式便于执行） */
export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

/** OpenAI/DeepSeek 兼容的 tool_call 格式（发给 LLM 时用） */
export interface LLMToolCall {
  id: string;
  type: 'function';
  function: {
    name: string;
    arguments: string; // JSON 字符串
  };
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
  /** DeepSeek 要求 tool 角色消息带此字段 */
  type?: string;
  tool_calls?: LLMToolCall[];
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
  type: 'thinking' | 'tool_call' | 'tool_result' | 'response' | 'error' | 'render_update';
  content: string;
  toolCall?: ToolCall;
  toolResult?: unknown;
  /** render_update 事件携带的最新渲染产物 */
  renderUpdate?: {
    source?: string;
    svg?: string;
    diff?: DiffResult;
  };
  timestamp: number;
}

/** 对话消息（前端展示用） */
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
  /** 步骤回调（用于 UI 更新） */
  onStep: (step: AgentStep) => void;
  /** AbortSignal，用于取消 LLM 请求 */
  signal?: AbortSignal;
}

/** LLM 流式 chunk */
export interface LLMStreamChunk {
  type:
    | 'delta' // 文本增量（正常回复）
    | 'thinking' // 思考过程（DeepSeek reasoning_content）
    | 'tool_call_delta' // tool_call 增量（按 index 累积）
    | 'done' // 流结束
    | 'error'; // 错误
  content?: string;
  toolCallDelta?: {
    index: number;
    id?: string;
    name?: string;
    argumentsDelta?: string;
  };
  usage?: { prompt_tokens: number; completion_tokens: number };
  error?: string;
}

/** LLM 客户端接口 */
export interface LLMClient {
  chat(params: {
    messages: LLMMessage[];
    tools?: ToolSchema[];
    signal?: AbortSignal;
  }): Promise<LLMResponse>;
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

export type { ChangeJson };
