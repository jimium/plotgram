/**
 * useAgent Hook (演示版)
 *
 * 与 studio 同名 hook 的差异：
 *   - LLM 客户端改为 createProxyLLMClient（服务器中转，API Key 不下发）
 *   - 移除 LlmConfig 依赖（无需用户配置，开箱即用）
 *   - 增加 toolCallTrace 状态，供 ToolCallTrace 组件可视化
 *   - 增加 resetSession（切换 session_id 重新开始）
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
  type ToolCall,
} from '@agent/index';
import { createProxyLLMClient } from '@lib/agentProxy';
import { renderSource, type PlotgramWasm, type RenderFormat } from '@lib/wasm';

/** LLM 请求超时时间(毫秒) — DeepSeek 流式可能较慢，给足 90s */
const LLM_TIMEOUT_MS = 90_000;

/** 单轮对话内的 tool call 轨迹（按调用顺序） */
export interface ToolCallTraceItem {
  id: string;
  toolName: string;
  /** 调用参数摘要（截断） */
  argsPreview: string;
  /** 执行结果摘要（截断） */
  resultPreview: string;
  /** 执行状态 */
  status: 'running' | 'success' | 'error';
  /** 时间戳 */
  timestamp: number;
}

interface UseAgentOptions {
  wasm: PlotgramWasm | null;
  ready: boolean;
}

interface UseAgentResult {
  messages: ChatMessage[];
  currentSource: string;
  currentSvg: string;
  lastDiff: DiffResult | null;
  /** 本轮对话的 tool call 轨迹（用于 ToolCallTrace 面板） */
  toolCallTrace: ToolCallTraceItem[];
  isRunning: boolean;
  error: string | null;
  sendMessage: (text: string) => Promise<void>;
  abort: () => void;
  clearError: () => void;
  /** 清空当前对话与图表（session_id 不变，配额仍累加） */
  resetConversation: () => void;
  /** 用新外观选项重新渲染当前 DSL（不触发 Agent 循环） */
  rerenderWithTheme: (optionsJson: string) => void;
  /** 渲染 drawio XML（用于导出/在 draw.io 打开） */
  renderDrawio: (optionsJson: string) => string | null;
}

export function useAgent(options: UseAgentOptions): UseAgentResult {
  const { wasm, ready } = options;
  const contextRef = useRef<AgentContext>(createAgentContext());
  const abortControllerRef = useRef<AbortController | null>(null);

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [currentSource, setCurrentSource] = useState('');
  const [currentSvg, setCurrentSvg] = useState('');
  const [lastDiff, setLastDiff] = useState<DiffResult | null>(null);
  const [toolCallTrace, setToolCallTrace] = useState<ToolCallTraceItem[]>([]);
  const [isRunning, setIsRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const currentSourceRef = useRef(currentSource);
  currentSourceRef.current = currentSource;

  // 代理 LLM 客户端：服务器持有 DeepSeek Key，session_id 存 sessionStorage
  const llmClient = useMemo(() => createProxyLLMClient(), []);

  const toolExecutors = useMemo(() => {
    if (!wasm) return null;
    return createToolExecutors(async () => wasm);
  }, [wasm]);

  const sendMessage = useCallback(
    async (text: string) => {
      if (!wasm || !ready || !toolExecutors) {
        setError('WASM 未就绪，无法发送消息');
        return;
      }
      if (isRunning) {
        setError('Agent 正在执行中，请等待完成或中止');
        return;
      }

      const controller = new AbortController();
      abortControllerRef.current = controller;
      const timeoutId = setTimeout(() => controller.abort(), LLM_TIMEOUT_MS);

      setIsRunning(true);
      setError(null);
      setToolCallTrace([]); // 每轮新对话重置 trace

      const userMsg = createUserMessage(text);
      contextRef.current = appendMessage(contextRef.current, userMsg);
      setMessages((prev) => [...prev, userMsg]);

      // 流式 agent 消息占位
      const streamingMsg = createAgentMessage('');
      const streamingId = streamingMsg.id;
      setMessages((prev) => [...prev, streamingMsg]);

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

          // thinking/delta: 打字机效果
          if (step.type === 'thinking' && step.content) {
            streamBuffer.content += step.content;
            const snapshot = streamBuffer.content;
            setMessages((prev) =>
              prev.map((m) => (m.id === streamingId ? { ...m, content: snapshot } : m)),
            );
          }

          // tool_call: 追加 trace 项 + 流式提示
          if (step.type === 'tool_call' && step.toolCall) {
            const tc = step.toolCall;
            const argsPreview = summarizeArgs(tc);
            const traceItem: ToolCallTraceItem = {
              id: tc.id || `${tc.name}-${Date.now()}`,
              toolName: tc.name,
              argsPreview,
              resultPreview: '',
              status: 'running',
              timestamp: Date.now(),
            };
            setToolCallTrace((prev) => [...prev, traceItem]);

            const hint = `\n\n🔧 **调用工具** \`${tc.name}\``;
            streamBuffer.content += hint;
            const snapshot = streamBuffer.content;
            setMessages((prev) =>
              prev.map((m) => (m.id === streamingId ? { ...m, content: snapshot } : m)),
            );
          }

          // tool_result: 更新对应 trace 项的状态
          if (step.type === 'tool_result' && step.toolCall) {
            const tcId = step.toolCall.id || `${step.toolCall.name}-${Date.now()}`;
            const resultPreview = summarizeResult(step.toolResult);
            const isSuccess = !(
              step.toolResult &&
              typeof step.toolResult === 'object' &&
              'success' in step.toolResult &&
              step.toolResult.success === false
            );
            setToolCallTrace((prev) =>
              prev.map((item) =>
                item.id === tcId
                  ? { ...item, resultPreview, status: isSuccess ? 'success' : 'error' }
                  : item,
              ),
            );
          }

          // render_update: 增量推送，立即更新预览 / DSL / diff，不必等 Agent 文字回复完成
          if (step.type === 'render_update' && step.renderUpdate) {
            const { source: newSource, svg: newSvg, diff: newDiff } = step.renderUpdate;
            if (newSource && newSource !== contextRef.current.source) {
              contextRef.current = { ...contextRef.current, source: newSource };
              currentSourceRef.current = newSource;
              setCurrentSource(newSource);
            }
            if (newSvg) {
              setCurrentSvg(newSvg);
            }
            if (newDiff) {
              setLastDiff(newDiff);
            }
          }
        },
      };

      try {
        const result: AgentResult = await runAgentLoop(text, contextRef.current, config);

        if (controller.signal.aborted) {
          const abortMsg = createSystemMessage('Agent 执行已中止');
          contextRef.current = appendMessage(contextRef.current, abortMsg);
          contextRef.current = compactHistory(contextRef.current);
          setMessages((prev) => [...prev.filter((m) => m.id !== streamingId), abortMsg]);
        } else {
          if (contextRef.current.source && contextRef.current.source !== currentSourceRef.current) {
            setCurrentSource(contextRef.current.source);
          }
          if (result.svg) setCurrentSvg(result.svg);
          if (result.diff) setLastDiff(result.diff);

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

          const agentMsg = createAgentMessage(finalContent, {
            svg: result.svg,
            diff: result.diff,
            toolCalls: result.toolCalls,
          });
          contextRef.current = appendMessage(contextRef.current, agentMsg);
          contextRef.current = compactHistory(contextRef.current);
        }
      } catch (err) {
        if (err instanceof DOMException && err.name === 'AbortError') {
          const abortMsg = createSystemMessage('Agent 执行已中止');
          contextRef.current = appendMessage(contextRef.current, abortMsg);
          contextRef.current = compactHistory(contextRef.current);
          setMessages((prev) => [...prev.filter((m) => m.id !== streamingId), abortMsg]);
        } else {
          const errMsg = err instanceof Error ? err.message : String(err);
          setError(errMsg);
          const errorMsg = createSystemMessage(`Agent 执行失败: ${errMsg}`);
          setMessages((prev) => [...prev.filter((m) => m.id !== streamingId), errorMsg]);
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

  const clearError = useCallback(() => setError(null), []);

  const resetConversation = useCallback(() => {
    if (isRunning) {
      abortControllerRef.current?.abort();
    }
    contextRef.current = createAgentContext();
    setMessages([]);
    setCurrentSource('');
    setCurrentSvg('');
    setLastDiff(null);
    setToolCallTrace([]);
    setError(null);
  }, [isRunning]);

  /** 用新外观选项重新渲染当前 DSL（不触发 Agent 循环，仅更新 SVG） */
  const rerenderWithTheme = useCallback(
    (optionsJson: string) => {
      if (!wasm || !contextRef.current.source) return;
      const result = renderSource(wasm, contextRef.current.source, 'svg', optionsJson);
      if (result.success && result.text) {
        setCurrentSvg(result.text);
      }
    },
    [wasm],
  );

  /** 渲染 drawio XML（用于导出/在 draw.io 打开） */
  const renderDrawio = useCallback(
    (optionsJson: string): string | null => {
      if (!wasm || !contextRef.current.source) return null;
      const result = renderSource(wasm, contextRef.current.source, 'drawio' as RenderFormat, optionsJson);
      return result.success && result.text ? result.text : null;
    },
    [wasm],
  );

  return {
    messages,
    currentSource,
    currentSvg,
    lastDiff,
    toolCallTrace,
    isRunning,
    error,
    sendMessage,
    abort,
    clearError,
    resetConversation,
    rerenderWithTheme,
    renderDrawio,
  };
}

// ─── 工具函数 ────────────────────────────────────────────

function summarizeArgs(tc: ToolCall): string {
  const args = tc.arguments || {};
  if (tc.name === 'lint') {
    const profile = typeof args.profile === 'string' ? args.profile : 'default';
    const advice = typeof args.advice === 'boolean' ? args.advice : true;
    return `lint profile=${profile} advice=${String(advice)}`;
  }
  // render/validate 的 source 太长，只保留前 80 字符 + 长度
  if (typeof args.source === 'string') {
    const s = args.source as string;
    return s.length > 80 ? `source (${s.length} chars): ${s.slice(0, 80)}...` : `source: ${s}`;
  }
  if (tc.name === 'diff') {
    return `old: ${(args.old_source as string)?.length ?? 0} chars → new: ${(args.new_source as string)?.length ?? 0} chars`;
  }
  if (tc.name === 'apply_patch') {
    const patchLen = Array.isArray(args.patch) ? args.patch.length : 0;
    return `patch: ${patchLen} changes`;
  }
  if (tc.name === 'layout_catalog') return '(无参数)';
  const json = JSON.stringify(args);
  return json.length > 120 ? json.slice(0, 120) + '...' : json;
}

function summarizeResult(result: unknown): string {
  if (!result || typeof result !== 'object') return String(result);
  const r = result as Record<string, unknown>;
  if (r.success === true) {
    if (r.report && typeof r.report === 'object') {
      const report = r.report as { violations?: unknown[]; advices?: unknown[] };
      const violationCount = Array.isArray(report.violations) ? report.violations.length : 0;
      const adviceCount = Array.isArray(report.advices) ? report.advices.length : 0;
      return `lint 完成 (${violationCount} violations, ${adviceCount} advices)`;
    }
    if (typeof r.text === 'string') {
      return `渲染成功 (${(r.text as string).length} chars)`;
    }
    if (typeof r.source === 'string') {
      return `patch 应用成功 (applied=${r.applied ?? 0})`;
    }
    if (Array.isArray(r.changes)) {
      return `diff 完成 (${r.changes.length} changes)`;
    }
    if (r.valid === true) return '校验通过';
    return '成功';
  }
  if (r.success === false) {
    const errs = Array.isArray(r.errors) ? r.errors : [];
    const firstErr = errs[0];
    if (firstErr && typeof firstErr === 'object' && 'message' in firstErr) {
      return `失败: ${String((firstErr as { message: unknown }).message).slice(0, 100)}`;
    }
    if (typeof firstErr === 'string') return `失败: ${firstErr.slice(0, 100)}`;
    return '失败';
  }
  const json = JSON.stringify(result);
  return json.length > 120 ? json.slice(0, 120) + '...' : json;
}
