/**
 * WASM 桥接层单元测试
 *
 * 使用 mock WASM 模块测试桥接函数
 */

import { describe, it, expect, vi } from 'vitest';
import {
  renderSource,
  validateSource,
  parseSource,
  diffSources,
  applyPatch,
  checkStudioCapabilities,
} from '@lib/wasm';
import type { DrawifyWasm } from '@lib/wasm';

/** 创建 mock WASM 模块 */
function createMockWasm(overrides?: Partial<DrawifyWasm>): DrawifyWasm {
  return {
    default: vi.fn(async () => {}),
    version: vi.fn(() => '0.1.0-test'),
    render: vi.fn(() =>
      JSON.stringify({
        success: true,
        format: 'svg',
        text: '<svg>mock</svg>',
        errors: [],
        warnings: [],
      }),
    ),
    render_with_options: vi.fn(() =>
      JSON.stringify({
        success: true,
        format: 'svg',
        text: '<svg>mock-with-options</svg>',
        errors: [],
        warnings: [],
      }),
    ),
    validate: vi.fn(() =>
      JSON.stringify({
        valid: true,
        errors: [],
        warnings: [],
      }),
    ),
    parse_to_json: vi.fn(() =>
      JSON.stringify({
        diagram: { diagram_type: 'flowchart' },
        errors: [],
        warnings: [],
      }),
    ),
    layout_catalog: vi.fn(() => '{}'),
    ...overrides,
  };
}

describe('renderSource', () => {
  it('调用 render_with_options 当提供 optionsJson', () => {
    const wasm = createMockWasm();
    const result = renderSource(wasm, 'source', 'svg', '{"theme_id":"auto"}');

    expect(wasm.render_with_options).toHaveBeenCalledWith(
      'source',
      'svg',
      '{"theme_id":"auto"}',
    );
    expect(result.success).toBe(true);
    expect(result.text).toBe('<svg>mock-with-options</svg>');
  });

  it('调用 render 当未提供 optionsJson', () => {
    const wasm = createMockWasm();
    const result = renderSource(wasm, 'source', 'svg');

    expect(wasm.render).toHaveBeenCalledWith('source', 'svg');
    expect(result.success).toBe(true);
  });

  it('返回结果解析失败时使用 fallback', () => {
    const wasm = createMockWasm({
      render: vi.fn(() => 'invalid json'),
    });
    const result = renderSource(wasm, 'source', 'svg');

    expect(result.success).toBe(false);
    expect(result.errors).toContain('无法解析渲染结果');
  });
});

describe('validateSource', () => {
  it('返回校验结果', () => {
    const wasm = createMockWasm();
    const result = validateSource(wasm, 'source');

    expect(wasm.validate).toHaveBeenCalledWith('source');
    expect(result.valid).toBe(true);
  });
});

describe('parseSource', () => {
  it('返回解析结果', () => {
    const wasm = createMockWasm();
    const result = parseSource(wasm, 'source');

    expect(wasm.parse_to_json).toHaveBeenCalledWith('source');
    expect(result.diagram).toEqual({ diagram_type: 'flowchart' });
  });
});

describe('diffSources', () => {
  it('WASM 支持 diff_sources 时返回结果', () => {
    const wasm = createMockWasm({
      diff_sources: vi.fn(() =>
        JSON.stringify({
          changes: [{ op: 'add', path: { target: 'entity', id: 'a' } }],
          stats: { added: 1, removed: 0, modified: 0 },
        }),
      ),
    });

    const result = diffSources(wasm, 'old', 'new');
    expect(wasm.diff_sources).toHaveBeenCalledWith('old', 'new');
    expect(result.changes).toHaveLength(1);
    expect(result.stats.added).toBe(1);
  });

  it('WASM 不支持 diff_sources 时返回空结果', () => {
    const wasm = createMockWasm();
    const result = diffSources(wasm, 'old', 'new');

    expect(result.changes).toEqual([]);
    expect(result.stats).toEqual({ added: 0, removed: 0, modified: 0 });
  });
});

describe('applyPatch', () => {
  it('WASM 支持 apply_patch 时返回结果', () => {
    const wasm = createMockWasm({
      apply_patch: vi.fn(() =>
        JSON.stringify({
          success: true,
          source: 'new source',
          applied: 1,
          skipped: 0,
          errors: [],
        }),
      ),
    });

    const result = applyPatch(wasm, 'old source', [{ op: 'add' }]);
    expect(wasm.apply_patch).toHaveBeenCalledWith(
      'old source',
      JSON.stringify([{ op: 'add' }]),
    );
    expect(result.success).toBe(true);
    expect(result.source).toBe('new source');
  });

  it('WASM 不支持 apply_patch 时返回错误', () => {
    const wasm = createMockWasm();
    const result = applyPatch(wasm, 'source', [{ op: 'add' }]);

    expect(result.success).toBe(false);
    expect(result.errors[0]).toContain('不支持 apply_patch');
  });
});

describe('checkStudioCapabilities', () => {
  it('检测全部能力可用', () => {
    const wasm = createMockWasm({
      diff_sources: vi.fn(),
      apply_patch: vi.fn(),
      ast_to_source: vi.fn(),
    });

    const caps = checkStudioCapabilities(wasm);
    expect(caps.diff).toBe(true);
    expect(caps.applyPatch).toBe(true);
    expect(caps.astToSource).toBe(true);
  });

  it('检测能力缺失', () => {
    const wasm = createMockWasm();
    const caps = checkStudioCapabilities(wasm);
    expect(caps.diff).toBe(false);
    expect(caps.applyPatch).toBe(false);
    expect(caps.astToSource).toBe(false);
  });
});
