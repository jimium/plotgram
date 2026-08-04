/**
 * LLM 客户端封装
 *
 * 支持 OpenAI / Anthropic / DeepSeek / Ollama / Custom 五种 provider
 * 统一为 OpenAI 兼容的 chat completions 接口(Anthropic 走原生 messages 接口)
 * 支持 SSE 流式输出
 */

import type {
  LLMClient,
  LLMMessage,
  LLMResponse,
  LLMStreamChunk,
  ToolSchema,
} from '@agent/types';

/** LLM Provider 类型 */
export type LLMProvider = 'openai' | 'anthropic' | 'deepseek' | 'ollama' | 'custom';

/** LLM 配置 */
export interface LLMConfig {
  provider: LLMProvider;
  apiKey: string;
  model: string;
  baseUrl: string;
  maxTokens: number;
  temperature: number;
}

/** 从环境变量读取默认配置 */
export function loadLLMConfigFromEnv(): LLMConfig {
  return {
    provider: (import.meta.env.VITE_LLM_PROVIDER ?? 'openai') as LLMProvider,
    apiKey: import.meta.env.VITE_LLM_API_KEY ?? '',
    model: import.meta.env.VITE_LLM_MODEL ?? 'gpt-4o',
    baseUrl: import.meta.env.VITE_LLM_BASE_URL ?? 'https://api.openai.com/v1',
    maxTokens: 4096,
    temperature: 0.7,
  };
}

/** chat 请求参数 */
interface ChatParams {
  messages: LLMMessage[];
  tools?: ToolSchema[];
  signal?: AbortSignal;
}

/** 创建 LLM 客户端 */
export function createLLMClient(config: LLMConfig): LLMClient {
  return {
    /** 非流式对话(保留作为 fallback) */
    async chat(params: ChatParams): Promise<LLMResponse> {
      const headers = buildHeaders(config);
      const body = buildRequestBody(config, params.messages, params.tools);
      const endpoint = getChatEndpoint(config);

      const res = await fetch(endpoint, {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
        signal: params.signal,
      });

      if (!res.ok) {
        const errText = await res.text();
        throw new Error(`LLM 请求失败 (${res.status}): ${errText}`);
      }

      const data = await res.json();
      return parseResponse(config.provider, data);
    },

    /** 流式对话(SSE) */
    async *chatStream(params: ChatParams): AsyncIterable<LLMStreamChunk> {
      const headers = buildHeaders(config);
      const body = buildRequestBody(config, params.messages, params.tools, true);
      const endpoint = getChatEndpoint(config);

      const res = await fetch(endpoint, {
        method: 'POST',
        headers: { ...headers, Accept: 'text/event-stream' },
        body: JSON.stringify(body),
        signal: params.signal,
      });

      if (!res.ok || !res.body) {
        const errText = await res.text().catch(() => '');
        throw new Error(`LLM 流式请求失败 (${res.status}): ${errText}`);
      }

      if (config.provider === 'anthropic') {
        yield* streamAnthropic(res.body);
      } else {
        yield* streamOpenAICompatible(res.body);
      }
    },
  };
}

/** 构建请求头 */
function buildHeaders(config: LLMConfig): Record<string, string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (config.provider === 'anthropic') {
    headers['x-api-key'] = config.apiKey;
    headers['anthropic-version'] = '2023-06-01';
  } else {
    headers['Authorization'] = `Bearer ${config.apiKey}`;
  }
  return headers;
}

/** 构建 LLM 请求体(OpenAI 兼容格式) */
function buildRequestBody(
  config: LLMConfig,
  messages: LLMMessage[],
  tools?: ToolSchema[],
  stream = false,
): Record<string, unknown> {
  // 转换内部消息格式为 API 兼容格式
  // 内部 ToolCall 是扁平结构 {id, name, arguments:object}
  // API 要求嵌套结构 {id, type:'function', function:{name, arguments:string}}
  const apiMessages = messages.map((msg) => {
    if (msg.role === 'assistant' && msg.tool_calls && msg.tool_calls.length > 0) {
      return {
        role: 'assistant',
        content: msg.content || null,
        tool_calls: msg.tool_calls.map((tc) => ({
          id: tc.id,
          type: 'function' as const,
          function: {
            name: tc.name,
            arguments:
              typeof tc.arguments === 'string'
                ? tc.arguments
                : JSON.stringify(tc.arguments ?? {}),
          },
        })),
      };
    }
    return msg;
  });

  const body: Record<string, unknown> = {
    model: config.model,
    messages: apiMessages,
    max_tokens: config.maxTokens,
    temperature: config.temperature,
  };

  if (stream) {
    body.stream = true;
  }

  if (tools && tools.length > 0) {
    body.tools = tools;
    body.tool_choice = 'auto';
  }

  return body;
}

/** 获取 chat completions 端点 */
function getChatEndpoint(config: LLMConfig): string {
  const base = config.baseUrl.replace(/\/$/, '');
  if (config.provider === 'anthropic') {
    return `${base}/messages`;
  }
  return `${base}/chat/completions`;
}

// ============ 非流式响应解析 ============

/** 解析 LLM 响应(统一为 LLMResponse) */
function parseResponse(provider: LLMProvider, data: unknown): LLMResponse {
  if (provider === 'anthropic') {
    return parseAnthropicResponse(data as AnthropicResponse);
  }
  return parseOpenAIResponse(data as OpenAIResponse);
}

interface OpenAIResponse {
  choices: Array<{
    message: {
      content: string | null;
      tool_calls?: Array<{
        id: string;
        function: { name: string; arguments: string };
      }>;
    };
  }>;
  usage?: { prompt_tokens: number; completion_tokens: number };
}

function parseOpenAIResponse(data: OpenAIResponse): LLMResponse {
  const choice = data.choices?.[0];
  if (!choice) {
    return { content: '' };
  }

  const toolCalls = choice.message.tool_calls?.map((tc) => ({
    id: tc.id,
    name: tc.function.name,
    arguments: safeParseArgs(tc.function.arguments),
  }));

  return {
    content: choice.message.content ?? '',
    tool_calls: toolCalls,
    usage: data.usage,
  };
}

interface AnthropicResponse {
  content: Array<
    | { type: 'text'; text: string }
    | {
        type: 'tool_use';
        id: string;
        name: string;
        input: Record<string, unknown>;
      }
  >;
  usage?: { input_tokens: number; output_tokens: number };
}

function parseAnthropicResponse(data: AnthropicResponse): LLMResponse {
  let content = '';
  const toolCalls: LLMResponse['tool_calls'] = [];

  for (const block of data.content ?? []) {
    if (block.type === 'text') {
      content += block.text;
    } else if (block.type === 'tool_use') {
      toolCalls?.push({
        id: block.id,
        name: block.name,
        arguments: block.input,
      });
    }
  }

  return {
    content,
    tool_calls: toolCalls,
    usage: data.usage
      ? {
          prompt_tokens: data.usage.input_tokens,
          completion_tokens: data.usage.output_tokens,
        }
      : undefined,
  };
}

// ============ SSE 流式解析 ============

/**
 * 解析 OpenAI 兼容的 SSE 流(OpenAI/DeepSeek/Ollama/Custom)
 *
 * 格式:
 *   data: {"choices":[{"delta":{"content":"..."}}]}\n\n
 *   data: {"choices":[{"delta":{"tool_calls":[...]}}]}\n\n
 *   data: [DONE]\n\n
 *
 * DeepSeek 特有:delta.reasoning_content(思维链)
 */
async function* streamOpenAICompatible(body: ReadableStream<Uint8Array>): AsyncIterable<LLMStreamChunk> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  // tool_calls 累积器:按 index 聚合(id/name 仅首片有,arguments 需累积)
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
        const line = evt
          .split('\n')
          .find((l) => l.startsWith('data: '));
        if (!line) continue;

        const data = line.slice(6);
        if (data === '[DONE]') {
          // 流结束,输出累积的完整 tool_calls
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

        // usage(部分 provider 在最后一个 chunk 返回)
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

/**
 * 解析 Anthropic SSE 流
 *
 * 事件类型:
 *   message_start - 消息开始(含 input_tokens)
 *   content_block_start - 内容块开始(tool_use 含 id/name)
 *   content_block_delta - 内容块增量(text_delta / input_json_delta)
 *   content_block_stop - 内容块结束
 *   message_delta - 消息增量(含 output_tokens)
 *   message_stop - 消息结束
 */
async function* streamAnthropic(body: ReadableStream<Uint8Array>): AsyncIterable<LLMStreamChunk> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  // tool_use 累积器:按 content_block_index 聚合
  const toolAcc = new Map<number, { id: string; name: string; args: string }>();
  let inputTokens = 0;
  let outputTokens = 0;

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      const events = buffer.split('\n\n');
      buffer = events.pop() ?? '';

      for (const evt of events) {
        const lines = evt.split('\n');
        const eventLine = lines.find((l) => l.startsWith('event: '));
        const dataLine = lines.find((l) => l.startsWith('data: '));
        if (!eventLine || !dataLine) continue;

        const eventType = eventLine.slice(7);
        const data = dataLine.slice(6);

        let payload: Record<string, unknown>;
        try {
          payload = JSON.parse(data) as Record<string, unknown>;
        } catch {
          continue;
        }

        switch (eventType) {
          case 'message_start': {
            const msg = payload.message as { usage?: { input_tokens: number } };
            if (msg?.usage?.input_tokens) {
              inputTokens = msg.usage.input_tokens;
            }
            break;
          }
          case 'content_block_start': {
            const block = payload as {
              index: number;
              content_block: { type: string; id?: string; name?: string };
            };
            if (block.content_block?.type === 'tool_use') {
              toolAcc.set(block.index, {
                id: block.content_block.id ?? '',
                name: block.content_block.name ?? '',
                args: '',
              });
            }
            break;
          }
          case 'content_block_delta': {
            const delta = payload as {
              index: number;
              delta: { type: string; text?: string; partial_json?: string };
            };
            if (delta.delta?.type === 'text_delta' && delta.delta.text) {
              yield { type: 'delta', content: delta.delta.text };
            } else if (delta.delta?.type === 'input_json_delta' && delta.delta.partial_json) {
              const acc = toolAcc.get(delta.index);
              if (acc) {
                acc.args += delta.delta.partial_json;
              }
            }
            break;
          }
          case 'content_block_stop': {
            const block = payload as { index: number };
            const acc = toolAcc.get(block.index);
            if (acc) {
              yield {
                type: 'tool_call_delta',
                toolCallDelta: {
                  index: block.index,
                  id: acc.id,
                  name: acc.name,
                  argumentsDelta: acc.args,
                },
              };
            }
            break;
          }
          case 'message_delta': {
            const delta = payload as { usage?: { output_tokens: number } };
            if (delta.usage?.output_tokens) {
              outputTokens = delta.usage.output_tokens;
            }
            break;
          }
          case 'message_stop': {
            yield {
              type: 'done',
              usage: { prompt_tokens: inputTokens, completion_tokens: outputTokens },
            };
            return;
          }
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}

/** 安全解析 tool arguments(JSON 字符串) */
function safeParseArgs(args: string): Record<string, unknown> {
  try {
    return JSON.parse(args) as Record<string, unknown>;
  } catch {
    return {};
  }
}
