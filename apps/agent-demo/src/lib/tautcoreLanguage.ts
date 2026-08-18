/**
 * Tautcore DSL 语法高亮（CodeMirror StreamLanguage）
 *
 * 简易分词器，识别关键字/图表类型/字符串/注释/箭头/数字。
 */
import { StreamLanguage } from '@codemirror/language';

const KEYWORDS = new Set([
  'diagram', 'entity', 'group', 'config', 'node_style', 'edge_style', 'meta', 'true', 'false',
]);

const DIAGRAM_TYPES = new Set([
  'flowchart', 'sequence', 'architecture', 'state', 'er', 'mindmap',
]);

export const tautcoreLanguage = StreamLanguage.define<{
  inString: boolean;
  inComment: boolean;
}>({
  name: 'tautcore',
  startState: () => ({ inString: false, inComment: false }),
  token(stream) {
    // 字符串
    if (stream.match('"')) {
      let escaped = false;
      while (!stream.eol()) {
        const ch = stream.next()!;
        if (escaped) { escaped = false; continue; }
        if (ch === '\\') { escaped = true; continue; }
        if (ch === '"') break;
      }
      return 'string';
    }

    // 行注释
    if (stream.match('//')) {
      stream.skipToEnd();
      return 'comment';
    }

    // 箭头
    if (stream.match('<->') || stream.match('-->') || stream.match('->')) {
      return 'operator';
    }

    // 标点
    if (stream.match(/[{}[\]:]/)) {
      return 'punctuation';
    }

    // 数字
    if (stream.match(/[0-9]+(\.[0-9]+)?/)) {
      return 'number';
    }

    // 标识符/关键字
    if (stream.match(/[a-z][a-z0-9_.-]*/i)) {
      const w = stream.current();
      if (KEYWORDS.has(w)) return 'keyword';
      if (DIAGRAM_TYPES.has(w)) return 'atom';
      // 属性值中的 atom（如 service, redis, sugiyama-v2）
      // 带点号的（如 common.clean-light）也视为 atom
      if (w.includes('.') || w.includes('-')) return 'atom';
      return 'variableName';
    }

    stream.next();
    return null;
  },
  languageData: {
    commentTokens: { line: '//' },
  },
});
