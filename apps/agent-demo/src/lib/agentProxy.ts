/**
 * Agent Proxy LLM 客户端
 *
 * 通过服务器中转 `/agent/chat` 调用 DeepSeek，API Key 不下发客户端。
 * session_id 由前端生成（UUID v4），存 sessionStorage，整页生命周期复用。
 * 复用 OpenAI 兼容的 SSE 解析逻辑（与 studio streamOpenAICompatible 一致）。
 */

import type {
  LLMClient,
  LLMMessage,
  LLMStreamChunk,
  ToolSchema,
} from '@agent/types';

const PROXY_ENDPOINT = import.meta.env.VITE_AGENT_API ?? '/agent/chat';

/** 获取或创建 session_id（UUID v4） */
function getSessionId(): string {
  let id = sessionStorage.getItem('demo-session');
  if (!id) {
    id = crypto.randomUUID();
    sessionStorage.setItem('demo-session', id);
  }
  return id;
}

/** 代理错误（与后端 ProxyError 对齐） */
interface ProxyErrorBody {
  error: string;
  message: string;
  retry_after?: number;
}

/** 将服务器错误码映射为用户可读提示 */
export function describeProxyError(err: ProxyErrorBody): string {
  switch (err.error) {
    case 'demo_disabled':
      return '演示已关闭，感谢体验！';
    case 'bad_origin':
      return '当前来源不在演示白名单内';
    case 'session_invalid':
      return '会话标识无效，请刷新页面重试';
    case 'rate_limited':
      return err.retry_after
        ? `请求过于频繁，请 ${err.retry_after} 秒后重试`
        : '请求过于频繁，请稍后再试';
    case 'quota_exceeded':
      return err.message || '演示额度已用尽';
    case 'upstream_error':
      return err.message || '上游服务异常，请稍后重试';
    default:
      return err.message || err.error;
  }
}

export function createProxyLLMClient(): LLMClient {
  const sessionId = getSessionId();

  async function* chatStream(params: {
    messages: LLMMessage[];
    tools?: ToolSchema[];
    signal?: AbortSignal;
  }): AsyncIterable<LLMStreamChunk> {
    const res = await fetch(PROXY_ENDPOINT, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'text/event-stream',
        'X-Demo-Session': sessionId,
      },
      body: JSON.stringify({
        session_id: sessionId,
        messages: params.messages,
        tools: params.tools,
        max_tokens: 4096,
        temperature: 0.7,
      }),
      signal: params.signal,
    });

    if (!res.ok || !res.body) {
      let errMsg = `代理请求失败 (${res.status})`;
      try {
        const errBody = (await res.json()) as ProxyErrorBody;
        errMsg = describeProxyError(errBody);
      } catch {
        errMsg = `${errMsg}: ${res.statusText}`;
      }
      throw new Error(errMsg);
    }

    yield* streamOpenAICompatible(res.body);
  }

  return {
    chatStream,
    chat: async () => {
      throw new Error('demo 仅支持流式');
    },
  };
}

// ============ OpenAI 兼容 SSE 解析（从 studio llm.ts 复用精简）============

interface OpenAIStreamChunk {
  choices?: Array<{
    delta?: {
      content?: string;
      reasoning_content?: string;
      tool_calls?: Array<{
        index?: number;
        id?: string;
        function?: { name?: string; arguments?: string };
      }>;
    };
    finish_reason?: string;
  }>;
  usage?: { prompt_tokens: number; completion_tokens: number };
}

async function* streamOpenAICompatible(
  body: ReadableStream<Uint8Array>,
): AsyncIterable<LLMStreamChunk> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  // tool_calls 累积器：按 index 聚合(id/name 仅首片有，arguments 需累积)
  const toolCallAcc = new Map<number, { id: string; name: string; args: string }>();
  let usage: { prompt_tokens: number; completion_tokens: number } | undefined;

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      // SSE 以 \n\n 分隔事件
      const events = buffer.split('\n\n');
      buffer = events.pop() ?? '';

      for (const evt of events) {
        const line = evt.split('\n').find((l) => l.startsWith('data: '));
        if (!line) continue;

        const data = line.slice(6);
        if (data === '[DONE]') {
          // 流结束，输出累积的完整 tool_calls
          for (const [, tc] of toolCallAcc) {
            yield {
              type: 'tool_call_delta',
              toolCallDelta: {
                index: 0,
                id: tc.id,
                name: tc.name,
                argumentsDelta: tc.args,
              },
            };
          }
          yield { type: 'done', usage };
          return;
        }

        let chunk: OpenAIStreamChunk;
        try {
          chunk = JSON.parse(data) as OpenAIStreamChunk;
        } catch {
          continue; // 跳过无法解析的行
        }

        if (chunk.usage) {
          usage = {
            prompt_tokens: chunk.usage.prompt_tokens,
            completion_tokens: chunk.usage.completion_tokens,
          };
        }

        const delta = chunk.choices?.[0]?.delta;
        if (!delta) continue;

        // 文本增量
        if (delta.content) {
          yield { type: 'delta', content: delta.content };
        }

        // DeepSeek 思维链(reasoning_content)
        if (delta.reasoning_content) {
          yield { type: 'thinking', content: delta.reasoning_content };
        }

        // tool_calls 增量(按 index 累积)
        if (delta.tool_calls) {
          for (const tc of delta.tool_calls) {
            const idx = tc.index ?? 0;
            const acc = toolCallAcc.get(idx) ?? { id: '', name: '', args: '' };
            if (tc.id) acc.id = tc.id;
            if (tc.function?.name) acc.name = tc.function.name;
            if (tc.function?.arguments) acc.args += tc.function.arguments;
            toolCallAcc.set(idx, acc);
          }
        }
      }
    }
    // 流自然结束(未收到 [DONE])
    yield { type: 'done', usage };
  } finally {
    reader.releaseLock();
  }
}
