import { isNodeWasmAvailable, renderSource } from '../wasm/node';
import { MARKDOWN_CODE_BLOCK_LANGUAGE } from './preview';

// markdown-it 由 VS Code 内置提供，此处仅使用 duck typing
interface MarkdownIt {
  renderer: {
    rules: {
      fence?: (
        tokens: MarkdownItToken[],
        idx: number,
        options: unknown,
        env: unknown,
        self: MarkdownItRenderer,
      ) => string;
    };
  };
}

interface MarkdownItToken {
  info: string;
  content: string;
}

interface MarkdownItRenderer {
  renderToken(
    tokens: MarkdownItToken[],
    idx: number,
    options: unknown,
    env: unknown,
    self: MarkdownItRenderer,
  ): string;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function fenceLanguage(info: string): string {
  return info.trim().split(/\s+/)[0]?.toLowerCase() ?? '';
}

export function extendDrawifyMarkdownIt(md: MarkdownIt, extensionPath: string): MarkdownIt {
  const defaultFence =
    md.renderer.rules.fence ??
    ((tokens, idx, options, env, self) => self.renderToken(tokens, idx, options, env, self));

  md.renderer.rules.fence = (tokens, idx, options, env, self) => {
    const token = tokens[idx];
    if (fenceLanguage(token.info) !== MARKDOWN_CODE_BLOCK_LANGUAGE) {
      return defaultFence(tokens, idx, options, env, self);
    }

    if (!isNodeWasmAvailable(extensionPath)) {
      return (
        '<pre class="drawify-markdown-error">' +
        'Drawify WASM 未构建，请在 editors/vscode 执行 npm run build:wasm' +
        '</pre>\n'
      );
    }

    try {
      const result = renderSource(extensionPath, token.content);
      if (result.success && result.svg) {
        return `<div class="drawify-markdown-diagram">\n${result.svg}\n</div>\n`;
      }

      const message = escapeHtml(result.errors.join('\n') || 'Drawify 渲染失败');
      return `<pre class="drawify-markdown-error">${message}</pre>\n`;
    } catch (error) {
      const message = escapeHtml(error instanceof Error ? error.message : String(error));
      return `<pre class="drawify-markdown-error">${message}</pre>\n`;
    }
  };

  return md;
}
