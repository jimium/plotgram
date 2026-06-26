/**
 * drawify-wasm 桥接层
 *
 * 复用 playground 已有的 WASM 接口,并新增 Studio 所需的 diff/apply_patch 绑定
 *
 * 注意:diff_sources / apply_patch / ast_to_source 需要在 drawify-wasm crate
 * 中新增对应导出后才能生效(见 docs/architecture.md 的 WASM 扩展章节)
 */

import type {
  RenderResult,
  ValidationResult,
  ParseResult,
  DiffResult,
  PatchResult,
  RenderFormat,
  RenderOptions,
  Change,
} from '@agent/types';

/** drawify-wasm 模块接口 */
export interface DrawifyWasm {
  default: (input?: unknown) => Promise<unknown>;
  version: () => string;
  render: (source: string, format: string) => string;
  render_with_options: (source: string, format: string, optionsJson: string) => string;
  validate: (source: string) => string;
  parse_to_json: (source: string) => string;
  layout_catalog: () => string;
  // 以下为 Studio 需要新增的绑定
  diff_sources?: (oldSource: string, newSource: string) => string;
  apply_patch?: (source: string, patchJson: string) => string;
  ast_to_source?: (astJson: string) => string;
}

let modulePromise: Promise<DrawifyWasm> | null = null;

/** 懒加载并初始化 WASM 模块(全局单例) */
export function loadWasm(): Promise<DrawifyWasm> {
  if (!modulePromise) {
    modulePromise = (async () => {
      // WASM 产物由仓库根目录的 wasm-pack 生成到 studio/drawify-wasm/
      // 该路径在构建前可能不存在,用动态 import 并忽略类型检查
      const mod = (await import(
        /* @vite-ignore */ /* @ts-expect-error WASM 产物由 wasm-pack 生成,构建前不存在 */
        '../drawify-wasm/drawify_wasm.js'
      )) as unknown as DrawifyWasm;
      await mod.default();
      return mod;
    })();
  }
  return modulePromise;
}

function safeParse<T>(json: string, fallback: T): T {
  try {
    return JSON.parse(json) as T;
  } catch {
    return fallback;
  }
}

/** 按指定格式渲染,返回单格式结果 */
export function renderSource(
  wasm: DrawifyWasm,
  source: string,
  format: RenderFormat,
  optionsJson?: string,
): RenderResult {
  const json =
    optionsJson && typeof wasm.render_with_options === 'function'
      ? wasm.render_with_options(source, format, optionsJson)
      : wasm.render(source, format);

  return safeParse<RenderResult>(json, {
    success: false,
    format,
    text: null,
    errors: ['无法解析渲染结果'],
    warnings: [],
  });
}

/** 校验 DSL 源码 */
export function validateSource(wasm: DrawifyWasm, source: string): ValidationResult {
  const json = wasm.validate(source);
  return safeParse<ValidationResult>(json, {
    valid: false,
    errors: ['无法解析校验结果'],
    warnings: [],
  });
}

/** 解析 DSL 为 AST JSON */
export function parseSource(wasm: DrawifyWasm, source: string): ParseResult {
  const json = wasm.parse_to_json(source);
  return safeParse<ParseResult>(json, {
    diagram: null,
    errors: ['无法解析 AST'],
    warnings: [],
  });
}

/**
 * 比较两份 DSL 源码的差异
 *
 * 依赖 drawify-wasm 新增的 diff_sources 绑定
 */
export function diffSources(
  wasm: DrawifyWasm,
  oldSource: string,
  newSource: string,
): DiffResult {
  if (typeof wasm.diff_sources !== 'function') {
    return {
      changes: [],
      stats: { added: 0, removed: 0, modified: 0 },
    };
  }
  const json = wasm.diff_sources(oldSource, newSource);
  return safeParse<DiffResult>(json, {
    changes: [],
    stats: { added: 0, removed: 0, modified: 0 },
  });
}

/**
 * 对 DSL 应用增量补丁
 *
 * 依赖 drawify-wasm 新增的 apply_patch 绑定
 */
export function applyPatch(
  wasm: DrawifyWasm,
  source: string,
  patch: unknown[],
): PatchResult {
  if (typeof wasm.apply_patch !== 'function') {
    return {
      success: false,
      source: null,
      applied: 0,
      skipped: patch.length,
      errors: ['当前 WASM 版本不支持 apply_patch,请重新构建 drawify-wasm'],
    };
  }
  const json = wasm.apply_patch(source, JSON.stringify(patch));
  return safeParse<PatchResult>(json, {
    success: false,
    source: null,
    applied: 0,
    skipped: 0,
    errors: ['无法解析 patch 结果'],
  });
}

/** 检查 WASM 是否支持 Studio 所需的全部能力 */
export function checkStudioCapabilities(wasm: DrawifyWasm): {
  diff: boolean;
  applyPatch: boolean;
  astToSource: boolean;
} {
  return {
    diff: typeof wasm.diff_sources === 'function',
    applyPatch: typeof wasm.apply_patch === 'function',
    astToSource: typeof wasm.ast_to_source === 'function',
  };
}

/** 渲染选项构建 */
export function buildRenderOptions(opts: RenderOptions): string {
  return JSON.stringify(opts);
}

export type { Change };
