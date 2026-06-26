import type { DiagnosticErrorJson } from './wasm';

export type DiagnosticSeverity = 'error' | 'warning';

/**
 * 前端消费的诊断类型。
 *
 * 直接从 WASM 返回的 `DiagnosticErrorJson` 映射而来，
 * 保留完整的 code / context / suggestion / fix 结构，
 * 同时提供 `line` / `column` 便捷字段供编辑器与问题列表使用。
 */
export interface Diagnostic {
  severity: DiagnosticSeverity;
  code: string;
  line: number;
  column: number;
  message: string;
  category: string;
  context: Record<string, unknown> | null;
  suggestion: { text: string; fix?: { action: string; payload: Record<string, unknown> } | null } | null;
  /** 原始结构化对象，供需要完整 location end 的场景使用。 */
  raw: DiagnosticErrorJson;
}

/** 将 WASM 返回的结构化诊断映射为前端 Diagnostic。 */
export function fromDiagnosticJson(err: DiagnosticErrorJson, severity: DiagnosticSeverity): Diagnostic {
  return {
    severity,
    code: err.code,
    line: err.location.start.line,
    column: err.location.start.column,
    message: err.message,
    category: err.category,
    context: err.context ?? null,
    suggestion: err.suggestion
      ? {
          text: err.suggestion.text,
          fix: err.suggestion.fix ?? null,
        }
      : null,
    raw: err,
  };
}

/** 合并 errors 与 warnings 为前端 Diagnostic 列表。 */
export function parseDiagnostics(
  errors: DiagnosticErrorJson[],
  warnings: DiagnosticErrorJson[],
): Diagnostic[] {
  const parsedErrors = errors.map((e) => fromDiagnosticJson(e, 'error'));
  const parsedWarnings = warnings.map((w) => fromDiagnosticJson(w, 'warning'));
  return [...parsedErrors, ...parsedWarnings];
}

// ─── context 格式化辅助 ──────────────────────────────────────────

/** 列表型 context 字段标签映射。 */
const LIST_FIELDS: Array<[string, string]> = [
  ['available_entities', '可用实体'],
  ['valid_values', '合法值'],
  ['valid_attributes', '合法属性'],
  ['expected', '期望'],
];

/** 标量型 context 字段标签映射。 */
const SCALAR_FIELDS: Array<[string, string]> = [
  ['referenced_entity', '引用的实体'],
  ['invalid_attribute', '无效属性'],
  ['invalid_value', '无效值'],
  ['duplicate_id', '重复的 ID'],
  ['invalid_id', '无效的 ID'],
  ['entity_id', '实体'],
  ['group_id', '分组'],
  ['selector', '选择器'],
  ['unexpected', '实际遇到'],
  ['attribute', '属性'],
  ['expected_type', '期望类型'],
  ['actual_type', '实际类型'],
  ['path', '路径'],
  ['detail', '详情'],
];

/** 将 context JSON 格式化为人类可读的行列表（与 Rust 端 format_context_lines 对齐）。 */
export function formatContextLines(ctx: Record<string, unknown> | null): string[] {
  if (!ctx) return [];
  const lines: string[] = [];

  for (const [key, label] of LIST_FIELDS) {
    const arr = ctx[key];
    if (Array.isArray(arr) && arr.length > 0) {
      const items = arr.map((v) => (typeof v === 'string' ? v : JSON.stringify(v)));
      lines.push(`${label}: ${items.join(', ')}`);
    }
  }

  for (const [key, label] of SCALAR_FIELDS) {
    const val = ctx[key];
    if (val !== undefined && val !== null) {
      const valStr = typeof val === 'string' ? val : JSON.stringify(val);
      lines.push(`${label}: ${valStr}`);
    }
  }

  return lines;
}
