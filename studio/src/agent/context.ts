/**
 * Agent 上下文管理
 *
 * 维护对话状态、当前 DSL、图表类型等信息
 */

import type { AgentContext, ChatMessage, DiagramKind } from './types';

/** 检测 DSL 源码中的图表类型 */
export function detectDiagramType(source: string): DiagramKind | null {
  const match = source.match(/^\s*diagram\s+(flowchart|sequence|architecture|state|er|mindmap)\b/m);
  return (match?.[1] as DiagramKind) ?? null;
}

/** 创建初始 Agent 上下文 */
export function createAgentContext(options?: {
  initialSource?: string;
  maxIterations?: number;
}): AgentContext {
  const source = options?.initialSource ?? '';
  return {
    source,
    diagramType: detectDiagramType(source),
    history: [],
    maxIterations: options?.maxIterations ?? 10,
  };
}

/** 更新上下文中的 DSL 源码 */
export function updateContextSource(
  context: AgentContext,
  newSource: string,
): AgentContext {
  return {
    ...context,
    source: newSource,
    diagramType: detectDiagramType(newSource),
  };
}

/** 追加对话消息到上下文历史 */
export function appendMessage(
  context: AgentContext,
  message: ChatMessage,
): AgentContext {
  return {
    ...context,
    history: [...context.history, message],
  };
}

/** 压缩对话历史(保留最近 N 条,避免上下文过长) */
export function compactHistory(
  context: AgentContext,
  keepLast: number = 20,
): AgentContext {
  if (context.history.length <= keepLast) {
    return context;
  }
  return {
    ...context,
    history: context.history.slice(-keepLast),
  };
}

/** 生成唯一消息 ID */
let messageSeq = 0;
export function createMessageId(): string {
  return `msg-${Date.now()}-${++messageSeq}`;
}

/** 创建用户消息 */
export function createUserMessage(content: string): ChatMessage {
  return {
    id: createMessageId(),
    role: 'user',
    content,
    timestamp: Date.now(),
  };
}

/** 创建 Agent 消息 */
export function createAgentMessage(
  content: string,
  extras?: Partial<ChatMessage>,
): ChatMessage {
  return {
    id: createMessageId(),
    role: 'agent',
    content,
    timestamp: Date.now(),
    ...extras,
  };
}

/** 创建系统消息 */
export function createSystemMessage(content: string): ChatMessage {
  return {
    id: createMessageId(),
    role: 'system',
    content,
    timestamp: Date.now(),
  };
}
