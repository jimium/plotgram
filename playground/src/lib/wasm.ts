import type { RefinementReport } from '../data/intentOptions';

/** drawio 导出降级报告。 */
export interface ExportWarning {
  code: string;
  entity_id?: string | null;
  edge_index?: number | null;
  tier: string;
  message: string;
}

export interface ExportStats {
  node_count: number;
  edge_count: number;
  group_count: number;
  bezier_count: number;
  title_count: number;
}

export interface ExportReport {
  format: string;
  export_version: string;
  diagram_type: string;
  global_degrade: string;
  warnings: ExportWarning[];
  stats: ExportStats;
}

/** 源码位置（行列从 1 开始）。 */
export interface SpanJson {
  start: { line: number; column: number };
  end: { line: number; column: number };
}

/** Fix Action 自动修复动作。 */
export interface FixActionJson {
  action: string;
  payload: Record<string, unknown>;
}

/** 修复建议。 */
export interface SuggestionJson {
  text: string;
  fix?: FixActionJson | null;
}

/** 结构化诊断错误（对应 Rust 端 DiagnosticError 序列化）。 */
export interface DiagnosticErrorJson {
  code: string;
  severity: 'error' | 'warning';
  category: 'parse' | 'validation' | 'render' | 'patch';
  message: string;
  location: SpanJson;
  context?: Record<string, unknown> | null;
  suggestion?: SuggestionJson | null;
}

/** 单格式渲染结果。`text` 携带 SVG / ASCII / JSON 文本输出。 */
export interface RenderResult {
  success: boolean;
  format: string;
  text: string | null;
  errors: DiagnosticErrorJson[];
  warnings: DiagnosticErrorJson[];
  /** 布局意图修正报告（仅当请求携带 `layout_intents` 时存在）。 */
  refinement_report?: RefinementReport | null;
  /** drawio 导出降级报告（仅 drawio 格式时存在）。 */
  export_report?: ExportReport | null;
}

export interface ValidationResult {
  valid: boolean;
  errors: DiagnosticErrorJson[];
  warnings: DiagnosticErrorJson[];
}

export interface ParseResult {
  diagram: DiagramJson | null;
  errors: DiagnosticErrorJson[];
  warnings: DiagnosticErrorJson[];
}

export interface DiagramJson {
  diagram_type: string;
  attributes: DiagramAttributeJson[];
  entities: EntityJson[];
  relations: RelationJson[];
  groups: GroupJson[];
  style_decls: StyleDeclJson[];
  source_info: { file: string | null; line_count: number };
}

export interface DiagramAttributeJson {
  key: string;
  value: string | number | boolean | { $enum: string };
  span: { start: { line: number; column: number }; end: { line: number; column: number } };
}

export interface EntityJson {
  id: string;
  label: string;
  group_id: string | null;
  span: { start: { line: number; column: number }; end: { line: number; column: number } };
}

export interface RelationJson {
  from: string;
  to: string;
  arrow: string;
  label: string | null;
  span: { start: { line: number; column: number }; end: { line: number; column: number } };
}

export interface GroupJson {
  id: string;
  label: string;
  parent_id: string | null;
  entity_ids: string[];
  child_group_ids: string[];
  span: { start: { line: number; column: number }; end: { line: number; column: number } };
}

export interface StyleDeclJson {
  kind: string;
  target: string;
}

export interface DrawifyWasm {
  default: (input?: unknown) => Promise<unknown>;
  version: () => string;
  render: (source: string, format: string) => string;
  render_with_options: (source: string, format: string, optionsJson: string) => string;
  validate: (source: string) => string;
  parse_to_json: (source: string) => string;
  layout_catalog: () => string;
  diff_sources: (sourceA: string, sourceB: string) => string;
  apply_patch: (source: string, patchJson: string) => string;
  format_source: (source: string) => string;
}

let modulePromise: Promise<DrawifyWasm> | null = null;

/** 懒加载并初始化 WASM 模块（全局单例）。 */
export function loadWasm(): Promise<DrawifyWasm> {
  if (!modulePromise) {
    modulePromise = (async () => {
      const mod = (await import(
        /* @vite-ignore */ '../../drawify-wasm/drawify_wasm.js'
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

/** 渲染格式标识,与 Rust 端 RenderFormat::from_str 对齐。 */
export type RenderFormat = 'svg' | 'ascii' | 'json' | 'png' | 'webp' | 'drawio' | 'md-outline' | 'opml' | 'freemind';

/** 构造一个最小 fallback DiagnosticErrorJson。 */
function fallbackDiag(message: string, severity: 'error' | 'warning' = 'error'): DiagnosticErrorJson {
  return {
    code: 'E102',
    severity,
    category: 'render',
    message,
    location: { start: { line: 0, column: 0 }, end: { line: 0, column: 0 } },
  };
}

/** 按指定格式渲染,返回单格式结果。 */
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
    errors: [fallbackDiag('无法解析渲染结果')],
    warnings: [],
  });
}

export function validateSource(wasm: DrawifyWasm, source: string): ValidationResult {
  const json = wasm.validate(source);
  return safeParse<ValidationResult>(json, {
    valid: false,
    errors: [fallbackDiag('无法解析校验结果')],
    warnings: [],
  });
}

export function parseSource(wasm: DrawifyWasm, source: string): ParseResult {
  const json = wasm.parse_to_json(source);
  return safeParse<ParseResult>(json, {
    diagram: null,
    errors: [fallbackDiag('无法解析 AST')],
    warnings: [],
  });
}

// ─── Diff / Patch / Format 类型与包装 ───────────────────────────────

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

export interface FormatResult {
  success: boolean;
  text?: string;
  errors?: string[];
}

export function diffSources(wasm: DrawifyWasm, sourceA: string, sourceB: string): DiffResult {
  const json = wasm.diff_sources(sourceA, sourceB);
  return safeParse<DiffResult>(json, {
    success: false,
    errors: ['无法解析 diff 结果'],
  });
}

export function applyPatch(wasm: DrawifyWasm, source: string, patch: ChangeSetJson): PatchApplyResult {
  const json = wasm.apply_patch(source, JSON.stringify(patch));
  return safeParse<PatchApplyResult>(json, {
    success: false,
    applied: 0,
    errors: ['无法解析 patch 结果'],
  });
}

export function formatSource(wasm: DrawifyWasm, source: string): FormatResult {
  const json = wasm.format_source(source);
  return safeParse<FormatResult>(json, {
    success: false,
    errors: ['无法解析 format 结果'],
  });
}
