import type { SelectOption } from './layoutOptions';
import {
  DEFAULT_PREVIEW_BACKGROUND,
  previewBackgroundForcesTransparent,
  type PreviewBackground,
} from './previewBackground';

export interface ThemeGroup {
  label: string;
  options: SelectOption[];
}

export const THEME_GROUPS: ThemeGroup[] = [
  {
    label: '通用',
    options: [
      { value: 'common.clean-light', label: 'Clean Light' },
      { value: 'common.clean-dark', label: 'Clean Dark' },
      { value: 'common.floating-cards', label: 'Floating Cards（浮岛）' },
      { value: 'common.paper-ink', label: 'Paper & Ink（纸墨）' },
      { value: 'common.dual-channel', label: 'Dual Channel（双通道）' },
      { value: 'common.blueprint', label: 'Blueprint' },
      { value: 'common.presentation', label: 'Presentation' },
      { value: 'common.github-light', label: 'GitHub Light' },
      { value: 'common.github-dark', label: 'GitHub Dark' },
      { value: 'common.okabe-ito', label: 'Okabe-Ito (色盲友好)' },
    ],
  },
  {
    label: '思维导图',
    options: [
      { value: 'mindmap.vivid-branches', label: 'Vivid Branches' },
      { value: 'mindmap.ink-dark', label: 'Ink Dark' },
    ],
  },
];

/** 扁平主题 ID 列表，供 DSL 补全等使用。 */
export const THEME_IDS = THEME_GROUPS.flatMap((group) => group.options.map((opt) => opt.value));

export const GRAPHIC_STYLES: SelectOption[] = [
  { value: 'auto', label: '自动（跟随图表默认）' },
  { value: 'standard', label: 'Standard' },
  { value: 'excalidraw', label: 'Excalidraw（手绘）' },
  { value: 'cross-hatch', label: 'Cross-hatch' },
  { value: 'blueprint', label: 'Blueprint' },
  { value: 'spatial-clarity', label: 'Spatial Clarity' },
  { value: 'neon-glow', label: 'Neon Glow' },
  { value: 'stipple', label: 'Stipple（点绘）' },
];

export interface AppearanceOptions {
  themeId: string;
  graphicStyle: string;
  darkMode: boolean;
}

export const DEFAULT_APPEARANCE_OPTIONS: AppearanceOptions = {
  themeId: 'auto',
  graphicStyle: 'auto',
  darkMode: false,
};

export interface WasmRenderOptions {
  theme_id?: string;
  graphic_style?: string;
  dark_mode: boolean;
  show_title?: boolean;
  transparent_background?: boolean;
  ascii?: {
    output_encoding?: 'ascii' | 'utf8';
    non_ascii_policy?: 'escape' | 'replace' | 'drop' | 'approximate';
  };
}

/** 兼容 localStorage / 分享链接中的 legacy `styleId` 字段。 */
export function normalizeAppearanceOptions(raw: unknown): AppearanceOptions {
  if (!raw || typeof raw !== 'object') {
    return { ...DEFAULT_APPEARANCE_OPTIONS };
  }

  const value = raw as Partial<AppearanceOptions> & { styleId?: string };
  const themeId =
    typeof value.themeId === 'string'
      ? value.themeId
      : typeof value.styleId === 'string'
        ? value.styleId
        : DEFAULT_APPEARANCE_OPTIONS.themeId;

  return {
    themeId,
    graphicStyle:
      typeof value.graphicStyle === 'string'
        ? value.graphicStyle
        : DEFAULT_APPEARANCE_OPTIONS.graphicStyle,
    darkMode:
      typeof value.darkMode === 'boolean'
        ? value.darkMode
        : DEFAULT_APPEARANCE_OPTIONS.darkMode,
  };
}

export function buildRenderOptions(
  opts: AppearanceOptions,
  previewBackground: PreviewBackground = DEFAULT_PREVIEW_BACKGROUND,
): WasmRenderOptions {
  return {
    theme_id: opts.themeId === 'auto' ? undefined : opts.themeId,
    graphic_style: opts.graphicStyle === 'auto' ? undefined : opts.graphicStyle,
    dark_mode: Boolean(opts.darkMode),
    transparent_background: previewBackgroundForcesTransparent(previewBackground),
    ascii: {
      output_encoding: 'utf8',
      non_ascii_policy: 'approximate',
    },
  };
}

export function isAppearanceOverridden(opts: AppearanceOptions): boolean {
  return opts.themeId !== 'auto' || opts.graphicStyle !== 'auto' || opts.darkMode;
}

/** 预览用：在「自动主题」下推断实际会选用的内置主题 ID（与 Rust profile 对齐）。 */
export function resolveEffectiveThemeId(
  opts: AppearanceOptions,
  diagramType: string | null,
): string {
  if (opts.themeId !== 'auto') {
    return opts.themeId;
  }
  const isMindmap = diagramType === 'mindmap';
  if (opts.darkMode) {
    return isMindmap ? 'mindmap.ink-dark' : 'common.clean-dark';
  }
  return isMindmap ? 'mindmap.vivid-branches' : 'common.clean-light';
}
