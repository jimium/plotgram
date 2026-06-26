/**
 * Agent 上下文管理单元测试
 */

import { describe, it, expect } from 'vitest';
import {
  detectDiagramType,
  createAgentContext,
  updateContextSource,
  appendMessage,
  compactHistory,
  createUserMessage,
  createAgentMessage,
} from '@agent/context';

describe('detectDiagramType', () => {
  it('应正确识别 flowchart 类型', () => {
    const source = 'diagram flowchart {\n  entity a "A"\n}';
    expect(detectDiagramType(source)).toBe('flowchart');
  });

  it('应正确识别 sequence 类型', () => {
    const source = 'diagram sequence {\n  entity a "A"\n}';
    expect(detectDiagramType(source)).toBe('sequence');
  });

  it('应正确识别 architecture 类型', () => {
    const source = 'diagram architecture {\n  entity a "A"\n}';
    expect(detectDiagramType(source)).toBe('architecture');
  });

  it('对空源码返回 null', () => {
    expect(detectDiagramType('')).toBeNull();
  });

  it('对无 diagram 声明的源码返回 null', () => {
    expect(detectDiagramType('entity a "A"')).toBeNull();
  });

  it('应处理带前导空白的源码', () => {
    const source = '\n\n  diagram state {\n  }';
    expect(detectDiagramType(source)).toBe('state');
  });
});

describe('createAgentContext', () => {
  it('默认创建空上下文', () => {
    const ctx = createAgentContext();
    expect(ctx.source).toBe('');
    expect(ctx.diagramType).toBeNull();
    expect(ctx.history).toEqual([]);
    expect(ctx.maxIterations).toBe(10);
  });

  it('接受初始源码', () => {
    const source = 'diagram flowchart { entity a "A" }';
    const ctx = createAgentContext({ initialSource: source });
    expect(ctx.source).toBe(source);
    expect(ctx.diagramType).toBe('flowchart');
  });

  it('接受自定义最大迭代次数', () => {
    const ctx = createAgentContext({ maxIterations: 5 });
    expect(ctx.maxIterations).toBe(5);
  });
});

describe('updateContextSource', () => {
  it('更新源码并重新检测图表类型', () => {
    const ctx = createAgentContext();
    const newSource = 'diagram er { entity a "A" }';
    const updated = updateContextSource(ctx, newSource);
    expect(updated.source).toBe(newSource);
    expect(updated.diagramType).toBe('er');
  });

  it('不修改原上下文(不可变)', () => {
    const ctx = createAgentContext({ initialSource: 'diagram flowchart {}' });
    updateContextSource(ctx, 'diagram state {}');
    expect(ctx.source).toBe('diagram flowchart {}');
  });
});

describe('appendMessage', () => {
  it('追加消息到历史', () => {
    const ctx = createAgentContext();
    const msg = createUserMessage('你好');
    const updated = appendMessage(ctx, msg);
    expect(updated.history).toHaveLength(1);
    expect(updated.history[0]).toBe(msg);
  });
});

describe('compactHistory', () => {
  it('历史不超过阈值时不变', () => {
    let ctx = createAgentContext();
    for (let i = 0; i < 5; i++) {
      ctx = appendMessage(ctx, createUserMessage(`msg${i}`));
    }
    const compacted = compactHistory(ctx, 10);
    expect(compacted.history).toHaveLength(5);
  });

  it('历史超过阈值时只保留最近 N 条', () => {
    let ctx = createAgentContext();
    for (let i = 0; i < 25; i++) {
      ctx = appendMessage(ctx, createUserMessage(`msg${i}`));
    }
    const compacted = compactHistory(ctx, 10);
    expect(compacted.history).toHaveLength(10);
    expect(compacted.history[0].content).toBe('msg15');
  });
});

describe('消息创建函数', () => {
  it('createUserMessage 创建用户消息', () => {
    const msg = createUserMessage('测试');
    expect(msg.role).toBe('user');
    expect(msg.content).toBe('测试');
    expect(msg.id).toBeTruthy();
    expect(msg.timestamp).toBeGreaterThan(0);
  });

  it('createAgentMessage 创建 Agent 消息并支持附加字段', () => {
    const msg = createAgentMessage('已生成', { pendingChanges: true });
    expect(msg.role).toBe('agent');
    expect(msg.content).toBe('已生成');
    expect(msg.pendingChanges).toBe(true);
  });
});
