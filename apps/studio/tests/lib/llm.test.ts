/**
 * LLM 客户端单元测试
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createLLMClient, loadLLMConfigFromEnv, type LLMConfig } from '@lib/llm';

describe('loadLLMConfigFromEnv', () => {
  it('返回默认配置', () => {
    const config = loadLLMConfigFromEnv();
    expect(config.provider).toBeDefined();
    expect(config.model).toBeDefined();
    expect(config.baseUrl).toBeDefined();
    expect(config.maxTokens).toBe(4096);
    expect(config.temperature).toBe(0.7);
  });
});

describe('createLLMClient', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn();
    global.fetch = fetchMock as unknown as typeof global.fetch;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const baseConfig: LLMConfig = {
    provider: 'openai',
    apiKey: 'test-key',
    model: 'gpt-4o',
    baseUrl: 'https://api.openai.com/v1',
    maxTokens: 1000,
    temperature: 0.5,
  };

  it('OpenAI 格式:使用 Bearer token', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        choices: [
          {
            message: {
              content: '你好',
              tool_calls: undefined,
            },
          },
        ],
        usage: { prompt_tokens: 10, completion_tokens: 5 },
      }),
    });

    const client = createLLMClient(baseConfig);
    const result = await client.chat({
      messages: [{ role: 'user', content: '你好' }],
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, options] = fetchMock.mock.calls[0];
    expect(url).toBe('https://api.openai.com/v1/chat/completions');
    expect(options.method).toBe('POST');
    expect(options.headers['Authorization']).toBe('Bearer test-key');

    expect(result.content).toBe('你好');
    expect(result.usage?.prompt_tokens).toBe(10);
  });

  it('Anthropic 格式:使用 x-api-key header', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        content: [{ type: 'text', text: '你好' }],
        usage: { input_tokens: 10, output_tokens: 5 },
      }),
    });

    const client = createLLMClient({ ...baseConfig, provider: 'anthropic' });
    const result = await client.chat({
      messages: [{ role: 'user', content: '你好' }],
    });

    const [, options] = fetchMock.mock.calls[0];
    expect(options.headers['x-api-key']).toBe('test-key');
    expect(options.headers['anthropic-version']).toBe('2023-06-01');
    expect(result.content).toBe('你好');
  });

  it('请求失败时抛出错误', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 401,
      text: async () => 'Unauthorized',
    });

    const client = createLLMClient(baseConfig);
    await expect(
      client.chat({ messages: [{ role: 'user', content: '你好' }] }),
    ).rejects.toThrow('LLM 请求失败 (401)');
  });

  it('解析 OpenAI tool_calls', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        choices: [
          {
            message: {
              content: null,
              tool_calls: [
                {
                  id: 'call-1',
                  function: {
                    name: 'render',
                    arguments: '{"source":"diagram flowchart {}","format":"svg"}',
                  },
                },
              ],
            },
          },
        ],
      }),
    });

    const client = createLLMClient(baseConfig);
    const result = await client.chat({
      messages: [{ role: 'user', content: '画图' }],
    });

    expect(result.tool_calls).toHaveLength(1);
    expect(result.tool_calls?.[0].name).toBe('render');
    expect(result.tool_calls?.[0].arguments.source).toBe('diagram flowchart {}');
  });

  it('解析 Anthropic tool_use', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        content: [
          { type: 'text', text: '正在生成' },
          {
            type: 'tool_use',
            id: 'call-1',
            name: 'render',
            input: { source: 'diagram flowchart {}', format: 'svg' },
          },
        ],
      }),
    });

    const client = createLLMClient({ ...baseConfig, provider: 'anthropic' });
    const result = await client.chat({
      messages: [{ role: 'user', content: '画图' }],
    });

    expect(result.content).toBe('正在生成');
    expect(result.tool_calls).toHaveLength(1);
    expect(result.tool_calls?.[0].name).toBe('render');
  });

  it('assistant tool_calls 转换为 API 兼容嵌套格式', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        choices: [{ message: { content: '完成', tool_calls: undefined } }],
      }),
    });

    const client = createLLMClient(baseConfig);
    await client.chat({
      messages: [
        { role: 'user', content: '画图' },
        {
          role: 'assistant',
          content: '',
          tool_calls: [
            {
              id: 'call-1',
              name: 'render',
              arguments: { source: 'diagram flowchart {}', format: 'svg' },
            },
          ],
        },
        { role: 'tool', tool_call_id: 'call-1', content: '{"success":true}' },
      ],
    });

    const [, options] = fetchMock.mock.calls[0];
    const body = JSON.parse(options.body);

    // assistant message 的 tool_calls 应转换为嵌套格式
    const assistantMsg = body.messages[1];
    expect(assistantMsg.role).toBe('assistant');
    expect(assistantMsg.content).toBeNull(); // 空字符串转为 null
    expect(assistantMsg.tool_calls[0]).toEqual({
      id: 'call-1',
      type: 'function',
      function: {
        name: 'render',
        arguments: '{"source":"diagram flowchart {}","format":"svg"}',
      },
    });

    // tool message 保持原样
    const toolMsg = body.messages[2];
    expect(toolMsg.role).toBe('tool');
    expect(toolMsg.tool_call_id).toBe('call-1');
  });
});
