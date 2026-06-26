export type PreviewBackground = 'light' | 'dark' | 'snap' | 'transparent';

export const PREVIEW_BG_STORAGE_KEY = 'drawify-playground-preview-bg';

export const PREVIEW_BACKGROUNDS: PreviewBackground[] = ['light', 'dark', 'snap', 'transparent'];

export const DEFAULT_PREVIEW_BACKGROUND: PreviewBackground = 'light';

export const PREVIEW_BG_LABELS: Record<PreviewBackground, string> = {
  light: '浅色衬托（主题背景）',
  dark: '深色衬托（主题背景）',
  snap: 'Snap 网格（透明）',
  transparent: '透明背景',
};

/** snap / transparent 模式强制 WASM 省略画布背景 rect。 */
export function previewBackgroundForcesTransparent(bg: PreviewBackground): boolean {
  return bg === 'snap' || bg === 'transparent';
}

export function normalizePreviewBackground(raw: unknown): PreviewBackground {
  if (typeof raw === 'string' && PREVIEW_BACKGROUNDS.includes(raw as PreviewBackground)) {
    return raw as PreviewBackground;
  }
  return DEFAULT_PREVIEW_BACKGROUND;
}
