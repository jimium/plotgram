/**
 * Prompt 构建单元测试
 */

import { describe, it, expect } from 'vitest';
import { buildMessages, SYSTEM_PROMPT } from '@agent/prompt';
import { createAgentContext, createUserMessage, createAgentMessage } from '@agent/context';

describe('buildMessages', () => {
  it('包含 system prompt', () => {
    const ctx = createAgentContext();
    const messages = buildMessages('你好', ctx);
    expect(messages[0].role).toBe('system');
    expect(messages[0].content).toBe(SYSTEM_PROMPT);
  });

  it('有初始源码时注入当前 DSL', () => {
    const source = 'diagram flowchart { entity a "A" }';
    const ctx = createAgentContext({ initialSource: source });
    const messages = buildMessages('加个实体', ctx);

    const systemMsg = messages.find(
      (m) => m.role === 'system' && m.content.includes('当前图表的 DSL'),
    );
    expect(systemMsg).toBeDefined();
    expect(systemMsg?.content).toContain(source);
  });

  it('无初始源码时不注入 DSL 消息', () => {
    const ctx = createAgentContext();
    const messages = buildMessages('画个图', ctx);

    const dslMsg = messages.find((m) =>
      m.content.includes('当前图表的 DSL'),
    );
    expect(dslMsg).toBeUndefined();
  });

  it('包含对话历史', () => {
    let ctx = createAgentContext();
    ctx = {
      ...ctx,
      history: [
        createUserMessage('画个流程图'),
        createAgentMessage('已生成'),
        createUserMessage('加个实体'),
      ],
    };

    const messages = buildMessages('再加一个', ctx);

    // system + (无 DSL) + 3 条历史 + 当前用户输入 = 5
    expect(messages).toHaveLength(5);
    expect(messages[messages.length - 1].content).toBe('再加一个');
  });

  it('只保留最近 10 条历史', () => {
    let ctx = createAgentContext();
    const history = Array.from({ length: 15 }, (_, i) =>
      i % 2 === 0 ? createUserMessage(`用户${i}`) : createAgentMessage(`Agent${i}`),
    );
    ctx = { ...ctx, history };

    const messages = buildMessages('最新输入', ctx);

    // system + 10 条历史 + 当前输入 = 12
    expect(messages).toHaveLength(12);
  });
});

describe('SYSTEM_PROMPT', () => {
  it('包含核心能力说明', () => {
    expect(SYSTEM_PROMPT).toContain('生成 Drawify DSL');
    expect(SYSTEM_PROMPT).toContain('增量修改');
    expect(SYSTEM_PROMPT).toContain('自动校验');
  });

  it('包含 DSL 语法要点', () => {
    expect(SYSTEM_PROMPT).toContain('diagram flowchart');
    expect(SYSTEM_PROMPT).toContain('entity id');
    expect(SYSTEM_PROMPT).toContain('semantic');
  });

  it('包含 apply_patch 格式说明', () => {
    expect(SYSTEM_PROMPT).toContain('apply_patch');
    expect(SYSTEM_PROMPT).toContain('Change');
  });
});
