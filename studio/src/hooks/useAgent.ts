/**
 * useAgent Hook
 *
 * 管理 Agent 对话状态,封装 AgentLoop 调用
 */

import { useCallback, useMemo, useRef, useState } from 'react';
import {
  runAgentLoop,
  createAgentContext,
  appendMessage,
  compactHistory,
  createUserMessage,
  createAgentMessage,
  createSystemMessage,
  createToolExecutors,
  type AgentContext,
  type AgentConfig,
  type AgentResult,
  type AgentStep,
  type ChatMessage,
  type DiffResult,
} from '@agent/index';
import { createLLMClient } from '@lib/llm';
import type { LLMConfig } from '@lib/llm';
import type { DrawifyWasm } from '@lib/wasm';

/** LLM 请求超时时间(毫秒) */
const LLM_TIMEOUT_MS = 60_000;

interface UseAgentOptions {
  wasm: DrawifyWasm | null;
  ready: boolean;
  llmConfig: LLMConfig;
}

interface UseAgentResult {
  /** 对话消息列表 */
  messages: ChatMessage[];
  /** 当前生效的 DSL 源码 */
  currentSource: string;
  /** 当前渲染的 SVG */
  currentSvg: string;
  /** 最近一次变更差异 */
  lastDiff: DiffResult | null;
  /** Agent 是否正在执行 */
  isRunning: boolean;
  /** 错误信息 */
  error: string | null;
  /** 发送用户消息 */
  sendMessage: (text: string) => Promise<void>;
  /** 中止当前 Agent 执行 */
  abort: () => void;
  /** 清除错误信息 */
  clearError: () => void;
}

export function useAgent(options: UseAgentOptions): UseAgentResult {
  const { wasm, ready, llmConfig } = options;
  const contextRef = useRef<AgentContext>(createAgentContext());
  // AbortController 用于真正取消 LLM 请求
  const abortControllerRef = useRef<AbortController | null>(null);

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [currentSource, setCurrentSource] = useState('');
  const [currentSvg, setCurrentSvg] = useState('');
  const [lastDiff, setLastDiff] = useState<DiffResult | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 用 ref 保存 currentSource 最新值,避免 sendMessage 依赖 currentSource 导致频繁重建
  const currentSourceRef = useRef(currentSource);
  currentSourceRef.current = currentSource;

  const llmClient = useMemo(() => {
    return createLLMClient(llmConfig);
  }, [llmConfig]);

  const toolExecutors = useMemo(() => {
    if (!wasm) return null;
    return createToolExecutors(async () => wasm);
  }, [wasm]);

  const sendMessage = useCallback(
    async (text: string) => {
      if (!wasm || !ready || !toolExecutors) {
        setError('WASM 未就绪,无法发送消息');
        return;
      }
      if (isRunning) {
        setError('Agent 正在执行中,请等待完成或中止');
        return;
      }

      // 创建本次请求的 AbortController(带超时)
      const controller = new AbortController();
      abortControllerRef.current = controller;
      const timeoutId = setTimeout(() => controller.abort(), LLM_TIMEOUT_MS);

      setIsRunning(true);
      setError(null);

      // 追加用户消息
      const userMsg = createUserMessage(text);
      contextRef.current = appendMessage(contextRef.current, userMsg);
      setMessages((prev) => [...prev, userMsg]);

      // 创建流式 agent 消息(占位),onStep 实时更新 content(打字机效果)
      const streamingMsg = createAgentMessage('', { pendingChanges: false });
      const streamingId = streamingMsg.id;
      setMessages((prev) => [...prev, streamingMsg]);

      // 流式内容累积器(用 ref 避免 setMessages 闭包陈旧)
      const streamBuffer = { content: '' };

      const config: AgentConfig = {
        llm: llmClient,
        tools: toolExecutors,
        maxIterations: contextRef.current.maxIterations,
        signal: controller.signal,
        onStep: (step: AgentStep) => {
          if (step.type === 'error') {
            console.warn('[Agent]', step.content);
            return;
          }
          // thinking/delta: 实时追加 content(打字机效果)
          if (step.type === 'thinking' && step.content) {
            streamBuffer.content += step.content;
            const snapshot = streamBuffer.content;
            setMessages((prev) =>
              prev.map((m) =>
                m.id === streamingId ? { ...m, content: snapshot } : m,
              ),
            );
          }
          // tool_call: 追加工具调用提示
          if (step.type === 'tool_call' && step.toolCall) {
            const hint = `\n\n🔧 调用工具: ${step.toolCall.name}\n`;
            streamBuffer.content += hint;
            const snapshot = streamBuffer.content;
            setMessages((prev) =>
              prev.map((m) =>
                m.id === streamingId ? { ...m, content: snapshot } : m,
              ),
            );
          }
        },
      };

      try {
        const result: AgentResult = await runAgentLoop(
          text,
          contextRef.current,
          config,
        );

        // 检查是否已被中止(可能是超时或用户主动 abort)
        if (controller.signal.aborted) {
          const abortMsg = createSystemMessage('Agent 执行已中止');
          contextRef.current = appendMessage(contextRef.current, abortMsg);
          contextRef.current = compactHistory(contextRef.current);
          // 移除流式占位消息,追加中止消息
          setMessages((prev) => [
            ...prev.filter((m) => m.id !== streamingId),
            abortMsg,
          ]);
        } else {
          // 更新上下文与状态
          if (contextRef.current.source && contextRef.current.source !== currentSourceRef.current) {
            setCurrentSource(contextRef.current.source);
          }
          if (result.svg) {
            setCurrentSvg(result.svg);
          }
          if (result.diff) {
            setLastDiff(result.diff);
          }

          // 用最终结果更新流式消息(替换为完整回复)
          const finalContent = result.message || streamBuffer.content;
          setMessages((prev) =>
            prev.map((m) =>
              m.id === streamingId
                ? {
                    ...m,
                    content: finalContent,
                    svg: result.svg,
                    diff: result.diff,
                    toolCalls: result.toolCalls,
                  }
                : m,
            ),
          );

          // 更新 context history(用最终内容)
          const agentMsg = createAgentMessage(finalContent, {
            svg: result.svg,
            diff: result.diff,
            toolCalls: result.toolCalls,
            pendingChanges: false,
          });
          contextRef.current = appendMessage(contextRef.current, agentMsg);
          contextRef.current = compactHistory(contextRef.current);
        }
      } catch (err) {
        // AbortError 不作为错误显示(用户主动中止)
        if (err instanceof DOMException && err.name === 'AbortError') {
          const abortMsg = createSystemMessage('Agent 执行已中止');
          contextRef.current = appendMessage(contextRef.current, abortMsg);
          contextRef.current = compactHistory(contextRef.current);
          setMessages((prev) => [
            ...prev.filter((m) => m.id !== streamingId),
            abortMsg,
          ]);
        } else {
          const errMsg = err instanceof Error ? err.message : String(err);
          setError(errMsg);
          const errorMsg = createSystemMessage(`Agent 执行失败: ${errMsg}`);
          // 移除流式占位消息,追加错误消息
          setMessages((prev) => [
            ...prev.filter((m) => m.id !== streamingId),
            errorMsg,
          ]);
        }
      } finally {
        clearTimeout(timeoutId);
        abortControllerRef.current = null;
        setIsRunning(false);
      }
    },
    [wasm, ready, toolExecutors, llmClient, isRunning],
  );

  const abort = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
  }, []);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  return {
    messages,
    currentSource,
    currentSvg,
    lastDiff,
    isRunning,
    error,
    sendMessage,
    abort,
    clearError,
  };
}
