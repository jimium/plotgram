import { StreamLanguage, LanguageSupport, type StreamParser } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { type CompletionContext, type CompletionResult, type Completion } from '@codemirror/autocomplete';
import { THEME_IDS } from '../data/appearanceOptions';
import { diagramContextField, contextAwareCompletions } from './contextCompletion';

const KEYWORDS = new Set([
  'diagram',
  'entity',
  'group',
  'relation',
  'node_style',
  'edge_style',
  'meta',
]);

const DIAGRAM_TYPES = new Set([
  'flowchart',
  'sequence',
  'architecture',
  'state',
  'er',
  'mindmap',
]);

const BOOLEANS = new Set(['true', 'false']);

interface DrawifyState {
  inString: boolean;
}

const parser: StreamParser<DrawifyState> = {
  startState: () => ({ inString: false }),

  token(stream, state) {
    // 续接的多行字符串（理论上少见，作兜底）
    if (state.inString) {
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === '"') {
          state.inString = false;
          break;
        }
      }
      return 'string';
    }

    if (stream.eatSpace()) return null;

    // 注释
    if (stream.match('//')) {
      stream.skipToEnd();
      return 'comment';
    }

    // 字符串
    if (stream.peek() === '"') {
      stream.next();
      let escaped = false;
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === '"' && !escaped) return 'string';
        escaped = ch === '\\' && !escaped;
      }
      state.inString = true;
      return 'string';
    }

    // 箭头（关系）
    if (stream.match('<->') || stream.match('-->') || stream.match('->')) {
      return 'operator';
    }

    // 数字
    if (stream.match(/^-?\d+(\.\d+)?/)) {
      return 'number';
    }

    // 括号
    if (/[{}[\]()]/.test(stream.peek() ?? '')) {
      stream.next();
      return 'bracket';
    }

    // 标点
    if (/[:,]/.test(stream.peek() ?? '')) {
      stream.next();
      return 'punctuation';
    }

    // 标识符 / 关键字
    if (stream.match(/^[A-Za-z_][\w]*/)) {
      const word = stream.current();

      // 属性键：后续（跳过空白）紧跟冒号
      if (stream.match(/^\s*:/, false)) {
        return 'property';
      }
      if (KEYWORDS.has(word)) return 'keyword';
      if (DIAGRAM_TYPES.has(word)) return 'typeName';
      if (BOOLEANS.has(word)) return 'atom';
      return 'variableName';
    }

    stream.next();
    return null;
  },

  languageData: {
    commentTokens: { line: '//' },
  },

  tokenTable: {
    keyword: t.keyword,
    typeName: t.typeName,
    operator: t.operator,
    string: t.string,
    number: t.number,
    comment: t.lineComment,
    property: t.propertyName,
    bracket: t.bracket,
    atom: t.atom,
    punctuation: t.punctuation,
    variableName: t.variableName,
  },
};

export const drawifyStreamLanguage = StreamLanguage.define(parser);

// ─── 补全 ─────────────────────────────────────────────────

const ENTITY_TYPES = [
  // 通用 / 流程图
  'start', 'process', 'decision', 'end', 'service', 'client', 'gateway',
  // 时序图
  'actor', 'boundary', 'control',
  // 架构图
  'external', 'frontend', 'cache', 'queue', 'storage', 'database',
  // 状态图
  'state', 'initial', 'final', 'choice',
  // 思维导图
  'root', 'main', 'branch', 'leaf',
];

const STATUS_VALUES = ['healthy', 'degraded', 'down', 'unknown'];

const LINE_STYLE_VALUES = ['default', 'error', 'warning'];

const LAYOUT_ALGO_VALUES = [
  'flowchart', 'er',
  'sugiyama', 'sugiyama-v2',
  'architecture-v2', 'force-directed', 'circular',
  'mindmap', 'sequence',
];

const EDGE_ROUTING_VALUES = [
  'orthogonal', 'straight', 'bezier', 'spline',
  'circular', 'organic',
];

const LAYOUT_DIR_VALUES = ['top-to-bottom', 'left-to-right'];

const RENDER_STYLE_VALUES = [
  'standard', 'excalidraw', 'cross-hatch', 'blueprint',
  'spatial-clarity', 'neon-glow', 'stipple',
];

const KEYWORD_COMPLETIONS: Completion[] = [
  { label: 'diagram', type: 'keyword', detail: '图表声明' },
  { label: 'entity', type: 'keyword', detail: '实体声明' },
  { label: 'group', type: 'keyword', detail: '分组' },
  { label: 'title', type: 'property', detail: '标题属性' },
  { label: 'direction', type: 'property', detail: '布局方向' },
  { label: 'layout', type: 'property', detail: '布局算法（可带 { options }）' },
  { label: 'edge_routing', type: 'property', detail: '边路由（可带 { options }）' },
  { label: 'snap', type: 'property', detail: '网格吸附（true | false，默认 true）' },
  { label: 'theme', type: 'property', detail: '主题 ID（StyleSheet）' },
  { label: 'render_style', type: 'property', detail: '笔触皮肤' },
  { label: 'type', type: 'property', detail: '实体类型' },
];


function valueCompletions(values: string[]): Completion[] {
  return values.map((v) => ({ label: v, type: 'enum' }));
}

function listResult(from: number, options: Completion[]): CompletionResult {
  return { from, options, validFor: /^[\w.-]*$/ };
}

export function drawifyCompletions(context: CompletionContext): CompletionResult | null {
  const line = context.state.doc.lineAt(context.pos);
  const textBefore = line.text.slice(0, context.pos - line.from);

  const valueMatch = textBefore.match(/(\w+)\s*:\s*([\w.-]*)$/);
  if (valueMatch) {
    const key = valueMatch[1];
    const partial = valueMatch[2];
    const from = context.pos - partial.length;

    switch (key) {
      case 'type':
        return listResult(from, valueCompletions(ENTITY_TYPES));
      case 'status':
        return listResult(from, valueCompletions(STATUS_VALUES));
      case 'line_style':
        return listResult(from, valueCompletions(LINE_STYLE_VALUES));
      case 'layout':
        return listResult(from, valueCompletions(LAYOUT_ALGO_VALUES));
      case 'edge_routing':
        return listResult(from, valueCompletions(EDGE_ROUTING_VALUES));
      case 'direction':
        return listResult(from, valueCompletions(LAYOUT_DIR_VALUES));
      case 'snap':
        return listResult(from, valueCompletions(['true', 'false']));
      case 'render_style':
        return listResult(from, valueCompletions(RENDER_STYLE_VALUES));
      case 'theme':
        return listResult(from, valueCompletions(THEME_IDS));
      default:
        return null;
    }
  }

  const word = context.matchBefore(/[\w-]+/);
  if (!word && !context.explicit) return null;
  const from = word ? word.from : context.pos;
  return listResult(from, KEYWORD_COMPLETIONS);
}

/**
 * 统一补全源：先尝试上下文感知补全（entity ID 等），失败则回退到关键字/属性值补全。
 */
function unifiedCompletions(context: CompletionContext): CompletionResult | null {
  return contextAwareCompletions(context) ?? drawifyCompletions(context);
}

export function drawify(): LanguageSupport {
  return new LanguageSupport(drawifyStreamLanguage, [
    diagramContextField,
    drawifyStreamLanguage.data.of({ autocomplete: unifiedCompletions }),
  ]);
}
