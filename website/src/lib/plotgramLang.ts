import { StreamLanguage, LanguageSupport, type StreamParser } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

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

interface PlotgramState {
  inString: boolean;
}

const parser: StreamParser<PlotgramState> = {
  startState: () => ({ inString: false }),

  token(stream, state) {
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

    if (stream.match('//')) {
      stream.skipToEnd();
      return 'comment';
    }

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

    if (stream.match('<->') || stream.match('-->') || stream.match('->')) {
      return 'operator';
    }

    if (stream.match(/^-?\d+(\.\d+)?/)) {
      return 'number';
    }

    if (/[{}[\]()]/.test(stream.peek() ?? '')) {
      stream.next();
      return 'bracket';
    }

    if (/[:,]/.test(stream.peek() ?? '')) {
      stream.next();
      return 'punctuation';
    }

    if (stream.match(/^[A-Za-z_][\w]*/)) {
      const word = stream.current();
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

export const plotgramStreamLanguage = StreamLanguage.define(parser);

export function plotgram(): LanguageSupport {
  return new LanguageSupport(plotgramStreamLanguage);
}
