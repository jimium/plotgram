/**
 * Agent Loop 单元测试
 *
 * 使用 mock LLM 客户端模拟 Agent 行为
 */

import { describe, it, expect, vi } from 'vitest';
import { runAgentLoop, createAgentContext } from '@agent/index';
import type { LLMClient, LLMResponse, LLMStreamChunk, ToolExecutor } from '@agent/types';

/** 创建 mock LLM 客户端,按预设序列返回响应 */
function createMockLLM(responses: LLMResponse[]): LLMClient {
  let callIndex = 0;
  const nextResponse = () => {
    const response = responses[callIndex] ?? { content: '完成' };
    callIndex++;
    return response;
  };
  return {
    chat: vi.fn(async () => nextResponse()),
    // 流式:将 LLMResponse 拆分为 delta + tool_call_delta + done
    async *chatStream(): AsyncIterable<LLMStreamChunk> {
      const response = nextResponse();
      // 文本内容作为 delta 推送
      if (response.content) {
        yield { type: 'delta', content: response.content };
      }
      // tool_calls 作为完整 chunk 推送(流结束时输出)
      if (response.tool_calls) {
        for (let i = 0; i < response.tool_calls.length; i++) {
          const tc = response.tool_calls[i];
          yield {
            type: 'tool_call_delta',
            toolCallDelta: {
              index: i,
              id: tc.id,
              name: tc.name,
              argumentsDelta: JSON.stringify(tc.arguments),
            },
          };
        }
      }
      yield { type: 'done' };
    },
  };
}

/** 创建 mock tool 执行器 */
function createMockTools(executors: Record<string, ToolExecutor>): Record<string, ToolExecutor> {
  return executors;
}

describe('runAgentLoop', () => {
  it('LLM 直接回复(无 tool call)时立即返回', async () => {
    const llm = createMockLLM([
      { content: '你好,我是图表 Agent' },
    ]);
    const tools = createMockTools({});
    const context = createAgentContext();

    const result = await runAgentLoop('你好', context, {
      llm,
      tools,
      maxIterations: 5,
      onStep: () => {},
    });

    expect(result.message).toBe('你好,我是图表 Agent');
    expect(result.source).toBe('');
    expect(result.toolCalls).toEqual([]);
  });

  it('执行 render tool 后返回结果', async () => {
    const llm = createMockLLM([
      {
        content: '',
        tool_calls: [
          {
            id: 'call-1',
            name: 'render',
            arguments: { source: 'diagram flowchart {}', format: 'svg' },
          },
        ],
      },
      { content: '已生成图表' },
    ]);

    const tools = createMockTools({
      render: async () => ({
        success: true,
        format: 'svg',
        text: '<svg></svg>',
        errors: [],
        warnings: [],
      }),
    });

    const context = createAgentContext();
    const result = await runAgentLoop('画个流程图', context, {
      llm,
      tools,
      maxIterations: 5,
      onStep: () => {},
    });

    expect(result.message).toBe('已生成图表');
    expect(result.svg).toBe('<svg></svg>');
    expect(result.toolCalls).toHaveLength(1);
    expect(result.toolCalls?.[0].name).toBe('render');
  });

  it('apply_patch 成功后更新上下文 source', async () => {
    const newSource = 'diagram flowchart { entity a "A" }';
    const llm = createMockLLM([
      {
        content: '',
        tool_calls: [
          {
            id: 'call-1',
            name: 'apply_patch',
            arguments: {
              source: '',
              patch: [{ op: 'add', path: { target: 'entity', id: 'a' } }],
            },
          },
        ],
      },
      { content: '已添加实体' },
    ]);

    const tools = createMockTools({
      apply_patch: async () => ({
        success: true,
        source: newSource,
        applied: 1,
        skipped: 0,
        errors: [],
      }),
    });

    const context = createAgentContext();
    const result = await runAgentLoop('加个实体', context, {
      llm,
      tools,
      maxIterations: 5,
      onStep: () => {},
    });

    expect(result.source).toBe(newSource);
    expect(context.source).toBe(newSource);
  });

  it('达到最大迭代次数时返回提示', async () => {
    // 每次都返回 tool call,永不直接回复
    const llm = createMockLLM(
      Array.from({ length: 10 }, () => ({
        content: '',
        tool_calls: [
          { id: 'call-x', name: 'render', arguments: { source: '', format: 'svg' } },
        ],
      })),
    );

    const tools = createMockTools({
      render: async () => ({
        success: true,
        format: 'svg',
        text: '<svg></svg>',
        errors: [],
        warnings: [],
      }),
    });

    const context = createAgentContext({ maxIterations: 3 });
    const result = await runAgentLoop('测试', context, {
      llm,
      tools,
      maxIterations: 3,
      onStep: () => {},
    });

    expect(result.message).toContain('最大迭代次数');
  });

  it('LLM 调用失败时返回错误', async () => {
    const llm: LLMClient = {
      chat: vi.fn(async () => {
        throw new Error('网络错误');
      }),
      async *chatStream(): AsyncIterable<LLMStreamChunk> {
        throw new Error('网络错误');
      },
    };

    const context = createAgentContext();
    const result = await runAgentLoop('测试', context, {
      llm,
      tools: {},
      maxIterations: 5,
      onStep: () => {},
    });

    expect(result.message).toContain('Agent 执行失败');
    expect(result.message).toContain('网络错误');
  });

  it('未知工具时记录错误并继续', async () => {
    const llm = createMockLLM([
      {
        content: '',
        tool_calls: [{ id: 'call-1', name: 'unknown_tool', arguments: {} }],
      },
      { content: '完成' },
    ]);

    const context = createAgentContext();
    const result = await runAgentLoop('测试', context, {
      llm,
      tools: {},
      maxIterations: 5,
      onStep: () => {},
    });

    expect(result.message).toBe('完成');
  });
});
