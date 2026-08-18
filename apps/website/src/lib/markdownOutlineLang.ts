import { StreamLanguage, LanguageSupport, type StreamParser } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

// 轻量 Markdown 大纲语法高亮：仅识别 ATX 标题 / 列表项 / HTML 注释 / 行内格式，
// 与 tautcore 的 md-outline 导入解析器（ATX 标题模式）保持一致。

interface MdOutlineState {
  inString: boolean;
}

const parser: StreamParser<MdOutlineState> = {
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

    // ATX 标题：1-6 个 # 后跟空格
    if (stream.match(/^#{1,6}(?=\s)/)) {
      stream.eatSpace();
      stream.skipToEnd();
      return 'heading';
    }

    // HTML 注释（tautcore:entity-id 等元信息）
    if (stream.match(/^<!--/)) {
      while (!stream.eol()) {
        if (stream.match('-->')) break;
        stream.next();
      }
      return 'comment';
    }

    // 无序列表项
    if (stream.match(/^[-*+]\s+/)) {
      return 'operator';
    }

    // 行内代码 `code`
    if (stream.peek() === '`') {
      stream.next();
      while (!stream.eol()) {
        if (stream.next() === '`') break;
      }
      return 'string';
    }

    // 粗体/斜体
    if (stream.match(/^\*\*([^*]+)\*\*/)) return 'strong';
    if (stream.match(/^\*([^*]+)\*/)) return 'emphasis';

    // 链接 [text](url)
    if (stream.match(/^\[([^\]]*)\]\([^)]*\)/)) return 'url';

    stream.skipToEnd();
    return null;
  },

  languageData: {
    commentTokens: { block: { open: '<!--', close: '-->' } },
  },

  tokenTable: {
    heading: t.heading1,
    comment: t.comment,
    operator: t.list,
    string: t.string,
    strong: t.strong,
    emphasis: t.emphasis,
    url: t.url,
  },
};

export const markdownOutlineStreamLanguage = StreamLanguage.define(parser);

export function markdownOutline(): LanguageSupport {
  return new LanguageSupport(markdownOutlineStreamLanguage);
}
