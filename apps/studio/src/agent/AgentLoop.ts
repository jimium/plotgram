/**
 * Agent 循环引擎
 *
 * 核心循环:THINKING → TOOL_EXEC → EVALUATE → RESPOND
 * 支持多轮 Tool 调用、错误自修复、SSE 流式输出
 */

import type {
  AgentConfig,
  AgentContext,
  AgentResult,
  ToolCall,
  LLMResponse,
} from './types';
import { buildMessages } from './prompt';
import { AGENT_TOOL_SCHEMAS } from './tools';

/** 发给 LLM 的 tool 结果最大长度(避免 token 膨胀) */
const MAX_TOOL_RESULT_LEN = 800;

/**
 * 运行 Agent 循环
 *
 * @param userMessage 用户输入
 * @param context Agent 上下文(会被原地修改 source 字段)
 * @param config Agent 配置
 * @returns Agent 执行结果
 */
export async function runAgentLoop(
  userMessage: string,
  context: AgentContext,
  config: AgentConfig,
): Promise<AgentResult> {
  const messages = buildMessages(userMessage, context);
  const toolCallsRecord: ToolCall[] = [];
  let lastSvg: string | undefined;
  let lastDiff: AgentResult['diff'];

  for (let iteration = 0; iteration < config.maxIterations; iteration++) {
    // 检查是否已中止
    if (config.signal?.aborted) {
      throw new DOMException('Agent 已中止', 'AbortError');
    }

    // 1. 流式调用 LLM,实时推送 thinking/delta
    let response: LLMResponse;
    try {
      response = await streamLLMCall(config, messages);
    } catch (err) {
      // AbortError 直接向上抛,由 useAgent 处理
      if (err instanceof DOMException && err.name === 'AbortError') {
        throw err;
      }
      const errorMsg = err instanceof Error ? err.message : String(err);
      config.onStep({
        type: 'error',
        content: `LLM 调用失败: ${errorMsg}`,
        timestamp: Date.now(),
      });
      return {
        message: `Agent 执行失败: ${errorMsg}`,
        source: context.source,
      };
    }

    // 2. 无 Tool Call,直接回复
    if (!response.tool_calls || response.tool_calls.length === 0) {
      config.onStep({
        type: 'response',
        content: response.content,
        timestamp: Date.now(),
      });
      return {
        message: response.content,
        source: context.source,
        svg: lastSvg,
        diff: lastDiff,
        toolCalls: toolCallsRecord,
      };
    }

    // 3. 一次性 push 包含全部 tool_calls 的 assistant 消息(符合 OpenAI 规范)
    // 同时保留 LLM 返回的 content(思考文本)
    messages.push({
      role: 'assistant',
      content: response.content || '',
      tool_calls: response.tool_calls,
    });

    // 4. 执行 Tool Calls
    let renderFailed = false;
    for (const toolCall of response.tool_calls) {
      // 检查中止
      if (config.signal?.aborted) {
        throw new DOMException('Agent 已中止', 'AbortError');
      }

      config.onStep({
        type: 'tool_call',
        content: `调用工具: ${toolCall.name}`,
        toolCall,
        timestamp: Date.now(),
      });
      toolCallsRecord.push(toolCall);

      const executor = config.tools[toolCall.name];
      let result: unknown;

      if (!executor) {
        result = { success: false, errors: [`未知工具: ${toolCall.name}`] };
        config.onStep({
          type: 'error',
          content: `未知工具: ${toolCall.name}`,
          toolCall,
          timestamp: Date.now(),
        });
      } else {
        try {
          result = await executor(toolCall.arguments, context);
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : String(err);
          result = { success: false, errors: [errorMsg] };
          config.onStep({
            type: 'error',
            content: `工具 ${toolCall.name} 执行失败: ${errorMsg}`,
            toolCall,
            timestamp: Date.now(),
          });
        }
      }

      config.onStep({
        type: 'tool_result',
        content: truncateForLog(result),
        toolCall,
        toolResult: result,
        timestamp: Date.now(),
      });

      // 5. 根据结果更新上下文与状态
      const stateUpdate = updateStateFromResult(toolCall.name, result);
      if (stateUpdate.source) {
        context.source = stateUpdate.source;
      }
      if (stateUpdate.svg) {
        lastSvg = stateUpdate.svg;
      }
      if (stateUpdate.diff) {
        lastDiff = stateUpdate.diff;
      }
      if (stateUpdate.renderFailed) {
        renderFailed = true;
      }

      // 6. 将 Tool 结果加入对话(截断后发给 LLM,避免 token 膨胀)
      messages.push({
        role: 'tool',
        tool_call_id: toolCall.id,
        content: truncateForLLM(result),
      });
    }

    // 7. 渲染失败时自动注入修复提示,引导 LLM 调用 validate 自修复
    if (renderFailed) {
      messages.push({
        role: 'system',
        content:
          '上一次 render 失败。请调用 validate 工具获取详细错误,修正 DSL 后重新调用 render。',
      });
    }
  }

  // 达到最大迭代次数
  config.onStep({
    type: 'error',
    content: `达到最大迭代次数 ${config.maxIterations}`,
    timestamp: Date.now(),
  });

  return {
    message: 'Agent 达到最大迭代次数,请尝试更明确的指令',
    source: context.source,
    svg: lastSvg,
    diff: lastDiff,
    toolCalls: toolCallsRecord,
  };
}

/**
 * 流式调用 LLM,实时推送 thinking/delta,返回完整响应
 */
async function streamLLMCall(
  config: AgentConfig,
  messages: Parameters<AgentConfig['llm']['chatStream']>[0]['messages'],
): Promise<LLMResponse> {
  let content = '';
  const toolCallAcc = new Map<number, ToolCall>();
  let hasToolCalls = false;

  for await (const chunk of config.llm.chatStream({
    messages,
    tools: AGENT_TOOL_SCHEMAS,
    signal: config.signal,
  })) {
    switch (chunk.type) {
      case 'delta':
        content += chunk.content ?? '';
        // 实时推送文本增量(打字机效果)
        config.onStep({
          type: 'thinking',
          content: chunk.content ?? '',
          timestamp: Date.now(),
        });
        break;
      case 'thinking':
        // DeepSeek reasoning_content,推送为思考过程
        config.onStep({
          type: 'thinking',
          content: chunk.content ?? '',
          timestamp: Date.now(),
        });
        break;
      case 'tool_call_delta':
        hasToolCalls = true;
        if (chunk.toolCallDelta) {
          const { index, id, name, argumentsDelta } = chunk.toolCallDelta;
          const acc = toolCallAcc.get(index) ?? {
            id: '',
            name: '',
            arguments: {},
          };
          if (id) acc.id = id;
          if (name) acc.name = name;
          // argumentsDelta 是累积后的完整 JSON 字符串(在流结束时输出)
          if (argumentsDelta) {
            try {
              acc.arguments = JSON.parse(argumentsDelta) as Record<string, unknown>;
            } catch {
              acc.arguments = { _raw: argumentsDelta };
            }
          }
          toolCallAcc.set(index, acc);
        }
        break;
      case 'done':
        // 流结束
        break;
      case 'error':
        throw new Error(chunk.error ?? 'LLM 流式错误');
    }
  }

  return {
    content,
    tool_calls: hasToolCalls ? [...toolCallAcc.values()] : undefined,
  };
}

/** 从 Tool 结果中提取状态更新 */
interface StateUpdate {
  source?: string;
  svg?: string;
  diff?: AgentResult['diff'];
  renderFailed?: boolean;
}

function updateStateFromResult(toolName: string, result: unknown): StateUpdate {
  if (!result || typeof result !== 'object') return {};

  const update: StateUpdate = {};

  if (toolName === 'render') {
    const r = result as { success: boolean; text?: string };
    if (r.success && r.text) {
      update.svg = r.text;
    } else if (!r.success) {
      update.renderFailed = true;
    }
  }

  if (toolName === 'apply_patch') {
    const r = result as { success: boolean; source?: string };
    if (r.success && r.source) {
      update.source = r.source;
    }
  }

  if (toolName === 'diff') {
    update.diff = result as AgentResult['diff'];
  }

  return update;
}

/** 截断 Tool 结果用于日志展示 */
function truncateForLog(result: unknown, maxLength = 500): string {
  const str = typeof result === 'string' ? result : JSON.stringify(result);
  if (str.length <= maxLength) return str;
  return str.slice(0, maxLength) + '...(已截断)';
}

/** 截断 Tool 结果发给 LLM(避免大型 SVG 消耗 token) */
function truncateForLLM(result: unknown): string {
  const obj = result as Record<string, unknown> | null;
  if (!obj || typeof obj !== 'object') {
    return JSON.stringify(result);
  }

  // render 结果:只返回摘要,不返回完整 SVG
  if (obj.success === true && typeof obj.text === 'string') {
    const text = obj.text as string;
    const summary = {
      success: true,
      format: obj.format,
      length: text.length,
      preview: text.slice(0, 200),
    };
    return JSON.stringify(summary);
  }

  // 其他结果:截断超长内容
  const str = JSON.stringify(result);
  if (str.length <= MAX_TOOL_RESULT_LEN) return str;
  return str.slice(0, MAX_TOOL_RESULT_LEN) + '...(已截断)';
}
