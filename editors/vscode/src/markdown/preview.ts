/** Markdown 代码块语言标识（与 package.json / markdownItPlugin 保持一致）。 */
export const MARKDOWN_CODE_BLOCK_LANGUAGE = 'drawify' as const;

export function isDrawifyCodeBlock(language: string | undefined): boolean {
  return language === MARKDOWN_CODE_BLOCK_LANGUAGE;
}
