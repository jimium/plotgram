/**
 * drawify-wasm 测试用 mock
 *
 * 在 vitest 中替代真实的 WASM 模块,避免依赖 wasm-pack 产物
 */

import { vi } from 'vitest';

const mockWasm = {
  default: vi.fn(async () => {}),
  version: vi.fn(() => '0.0.0-mock'),
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
      text: '<svg>mock</svg>',
      errors: [],
      warnings: [],
    }),
  ),
  validate: vi.fn(() =>
    JSON.stringify({ valid: true, errors: [], warnings: [] }),
  ),
  parse_to_json: vi.fn(() =>
    JSON.stringify({ diagram: {}, errors: [], warnings: [] }),
  ),
  layout_catalog: vi.fn(() => '{}'),
  diff_sources: vi.fn(() =>
    JSON.stringify({
      changes: [],
      stats: { added: 0, removed: 0, modified: 0 },
    }),
  ),
  apply_patch: vi.fn(() =>
    JSON.stringify({
      success: true,
      source: 'mock source',
      applied: 1,
      skipped: 0,
      errors: [],
    }),
  ),
  ast_to_source: vi.fn(() =>
    JSON.stringify({ source: 'mock source', errors: [] }),
  ),
};

export default mockWasm.default;
export const version = mockWasm.version;
export const render = mockWasm.render;
export const render_with_options = mockWasm.render_with_options;
export const validate = mockWasm.validate;
export const parse_to_json = mockWasm.parse_to_json;
export const layout_catalog = mockWasm.layout_catalog;
export const diff_sources = mockWasm.diff_sources;
export const apply_patch = mockWasm.apply_patch;
export const ast_to_source = mockWasm.ast_to_source;
