/**
 * tautcore-wasm 桥接层
 *
 * 复用 playground 的 WASM 接口（最完整版），类型与 tautcore-wasm crate 实际导出对齐。
 * diff_sources / apply_patch / format_source 在 tautcore-wasm 中均已实现。
 */

/** 渲染格式标识，与 Rust 端 RenderFormat::from_str 对齐。 */
export type RenderFormat = 'svg' | 'ascii' | 'json' | 'drawio' | 'md-outline' | 'opml' | 'freemind';

/** 源码位置（行列从 1 开始）。 */
export interface SpanJson {
  start: { line: number; column: number };
  end: { line: number; column: number };
}

/** 结构化诊断错误（对应 Rust 端 DiagnosticError 序列化）。 */
export interface DiagnosticErrorJson {
  code: string;
  severity: 'error' | 'warning';
  category: 'parse' | 'validation' | 'render' | 'patch';
  message: string;
  location: SpanJson;
  context?: Record<string, unknown> | null;
  suggestion?: { text: string; fix?: { action: string; payload: Record<string, unknown> } | null } | null;
}

/** 单格式渲染结果。`text` 携带 SVG / ASCII / JSON 文本输出。 */
export interface RenderResult {
  success: boolean;
  format: string;
  text: string | null;
  errors: DiagnosticErrorJson[];
  warnings: DiagnosticErrorJson[];
}

export interface ValidationResult {
  valid: boolean;
  errors: DiagnosticErrorJson[];
  warnings: DiagnosticErrorJson[];
}

export interface LintViolation {
  rule: string;
  severity: 'error' | 'warning';
  message: string;
  metric?: number | null;
  entity_ids?: string[];
  group_ids?: string[];
  edge_index?: number | null;
  related_edge_indices?: number[];
}

export interface LintAdvice {
  violation_index: number;
  text: string;
  priority: number;
  confidence: 'high' | 'medium' | 'low';
  knobs?: Array<Record<string, unknown>>;
  fix?: { action: string; payload: Record<string, unknown> } | null;
}

export interface LintReport {
  violations: LintViolation[];
  advices?: LintAdvice[];
}

export interface LintResult {
  success: boolean;
  acceptable: boolean;
  report: LintReport;
  errors: DiagnosticErrorJson[];
  warnings: DiagnosticErrorJson[];
}

export interface LintOptions {
  profile?: 'default' | 'strict' | 'ci' | 'verbose' | 'all';
  fail_on_warning?: boolean;
  advice?: boolean;
}

export interface ParseResult {
  diagram: unknown | null;
  errors: DiagnosticErrorJson[];
  warnings: DiagnosticErrorJson[];
}

// ─── Diff / Patch / Format 类型 ────────────────────────────────────

export type ChangeOp = 'add' | 'remove' | 'modify';
export type ChangeTarget = 'diagram' | 'entity' | 'relation' | 'group' | 'style_decl';

export interface ChangePathJson {
  target: ChangeTarget;
  id?: string;
  attr_key?: string;
}

export interface ChangeJson {
  op: ChangeOp;
  path: ChangePathJson;
  old_value?: unknown;
  new_value?: unknown;
}

export interface ChangeSetJson {
  changes: ChangeJson[];
}

export interface DiffResult {
  success: boolean;
  changes?: ChangeSetJson;
  errors?: string[];
}

export interface PatchApplyResult {
  success: boolean;
  text?: string;
  applied: number;
  errors?: string[];
}

/** tautcore-wasm 模块接口 */
export interface TautcoreWasm {
  default: (input?: unknown) => Promise<unknown>;
  version: () => string;
  render: (source: string, format: string) => string;
  render_with_options: (source: string, format: string, optionsJson: string) => string;
  validate: (source: string) => string;
  lint: (source: string) => string;
  lint_with_options: (source: string, optionsJson: string) => string;
  parse_to_json: (source: string) => string;
  layout_catalog: () => string;
  diff_sources: (sourceA: string, sourceB: string) => string;
  apply_patch: (source: string, patchJson: string) => string;
  format_source: (source: string) => string;
}

let modulePromise: Promise<TautcoreWasm> | null = null;

/** 开发调试：强制下次 loadWasm 重新拉取 WASM。 */
export function resetWasmModule(): void {
  modulePromise = null;
}

/** 生产环境 CDN common 路径（三端共用同一份 wasm 产物，靠 ETag 控制缓存）。 */
function wasmCdnBase(): string {
  return 'https://assets.plotgram.cn/tautcore-wasm/';
}

/** tautcore_wasm.js 加载地址（开发走本地 vite 中间件，生产走 CDN common 路径）。 */
function tautcoreWasmJsUrl(): string {
  if (import.meta.env.DEV) {
    return `../tautcore-wasm/tautcore_wasm.js`;
  }
  return `${wasmCdnBase()}tautcore_wasm.js`;
}

/** wasm 二进制加载地址。 */
function tautcoreWasmBinaryUrl(): string {
  if (import.meta.env.DEV) {
    const origin = typeof window !== 'undefined' ? window.location.origin : '';
    return `${origin}/tautcore-wasm/tautcore_wasm_bg.wasm`;
  }
  return `${wasmCdnBase()}tautcore_wasm_bg.wasm`;
}

/** 懒加载并初始化 WASM 模块（全局单例）。 */
export function loadWasm(): Promise<TautcoreWasm> {
  if (!modulePromise) {
    modulePromise = (async () => {
      // WASM 产物由 wasm-pack 生成到 agent-demo/tautcore-wasm/
      const mod = (await import(
        /* @vite-ignore */ // @ts-ignore WASM 产物由 wasm-pack 生成，首次构建前不存在
        /* webpackIgnore: true */
        tautcoreWasmJsUrl()
      )) as unknown as TautcoreWasm;
      await mod.default({ module_or_path: tautcoreWasmBinaryUrl() });
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

function fallbackDiag(message: string, severity: 'error' | 'warning' = 'error'): DiagnosticErrorJson {
  return {
    code: 'E102',
    severity,
    category: 'render',
    message,
    location: { start: { line: 0, column: 0 }, end: { line: 0, column: 0 } },
  };
}

/** 按指定格式渲染，返回单格式结果。 */
export function renderSource(
  wasm: TautcoreWasm,
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
    errors: [fallbackDiag('无法解析渲染结果')],
    warnings: [],
  });
}

export function validateSource(wasm: TautcoreWasm, source: string): ValidationResult {
  const json = wasm.validate(source);
  return safeParse<ValidationResult>(json, {
    valid: false,
    errors: [fallbackDiag('无法解析校验结果')],
    warnings: [],
  });
}

export function lintSource(wasm: TautcoreWasm, source: string, options?: LintOptions): LintResult {
  const json =
    options && typeof wasm.lint_with_options === 'function'
      ? wasm.lint_with_options(source, JSON.stringify(options))
      : wasm.lint(source);
  return safeParse<LintResult>(json, {
    success: false,
    acceptable: false,
    report: { violations: [], advices: [] },
    errors: [fallbackDiag('无法解析 lint 结果')],
    warnings: [],
  });
}

export function parseSource(wasm: TautcoreWasm, source: string): ParseResult {
  const json = wasm.parse_to_json(source);
  return safeParse<ParseResult>(json, {
    diagram: null,
    errors: [fallbackDiag('无法解析 AST')],
    warnings: [],
  });
}

export function diffSources(wasm: TautcoreWasm, sourceA: string, sourceB: string): DiffResult {
  const json = wasm.diff_sources(sourceA, sourceB);
  return safeParse<DiffResult>(json, {
    success: false,
    errors: ['无法解析 diff 结果'],
  });
}

export function applyPatch(
  wasm: TautcoreWasm,
  source: string,
  patch: ChangeSetJson,
): PatchApplyResult {
  const json = wasm.apply_patch(source, JSON.stringify(patch));
  return safeParse<PatchApplyResult>(json, {
    success: false,
    applied: 0,
    errors: ['无法解析 patch 结果'],
  });
}

export function formatSource(wasm: TautcoreWasm, source: string): { success: boolean; text?: string; errors?: string[] } {
  const json = wasm.format_source(source);
  return safeParse(json, { success: false, errors: ['无法解析 format 结果'] });
}

/** 渲染选项（与 Rust 端 WasmRenderOptions 对齐）。 */
export interface RenderOptions {
  theme_id?: string;
  graphic_style?: string;
  dark_mode?: boolean;
  transparent_background?: boolean;
  show_title?: boolean;
}

export function buildRenderOptions(opts: RenderOptions): string {
  return JSON.stringify(opts);
}
