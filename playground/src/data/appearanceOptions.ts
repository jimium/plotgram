import type { SelectOption } from './layoutOptions';
import type { LayoutIntentOverlay } from './intentOptions';
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
      { value: 'common.blueprint', label: 'Blueprint' },
      { value: 'common.presentation', label: 'Presentation' },
      { value: 'common.minimal-gray', label: 'Minimal Gray' },
      { value: 'common.brand-vivid', label: 'Brand Vivid' },
      { value: 'common.dracula', label: 'Dracula' },
      { value: 'common.nord', label: 'Nord' },
      { value: 'common.tokyo-night', label: 'Tokyo Night' },
      { value: 'common.catppuccin-mocha', label: 'Catppuccin Mocha' },
      { value: 'common.solarized-light', label: 'Solarized Light' },
      { value: 'common.gruvbox-dark', label: 'Gruvbox Dark' },
      { value: 'common.one-dark', label: 'One Dark' },
      { value: 'common.rose-pine', label: 'Rosé Pine' },
      { value: 'common.github-light', label: 'GitHub Light' },
      { value: 'common.github-dark', label: 'GitHub Dark' },
      { value: 'common.catppuccin-latte', label: 'Catppuccin Latte' },
      { value: 'common.monokai', label: 'Monokai' },
      { value: 'common.ibm-carbon', label: 'IBM Carbon Accessible' },
      { value: 'common.okabe-ito', label: 'Okabe-Ito' },
      { value: 'common.tol-bright', label: 'Tol Bright' },
      { value: 'common.tol-high-contrast', label: 'Tol High Contrast' },
    ],
  },
  {
    label: '思维导图',
    options: [
      { value: 'mindmap.vivid-branches', label: 'Vivid Branches' },
      { value: 'mindmap.pastel-soft', label: 'Pastel Soft' },
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
  transparent_background?: boolean;
  ascii?: {
    output_encoding?: 'ascii' | 'utf8';
    non_ascii_policy?: 'escape' | 'replace' | 'drop' | 'approximate';
  };
  /** 布局意图叠加层（可选）。透传至 Rust 端 `RenderRequest::layout_overlay`。 */
  layout_intents?: LayoutIntentOverlay | null;
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
  layoutIntents?: LayoutIntentOverlay | null,
  previewBackground: PreviewBackground = DEFAULT_PREVIEW_BACKGROUND,
): WasmRenderOptions {
  const options: WasmRenderOptions = {
    theme_id: opts.themeId === 'auto' ? undefined : opts.themeId,
    graphic_style: opts.graphicStyle === 'auto' ? undefined : opts.graphicStyle,
    dark_mode: Boolean(opts.darkMode),
    transparent_background: previewBackgroundForcesTransparent(previewBackground),
    ascii: {
      output_encoding: 'utf8',
      non_ascii_policy: 'approximate',
    },
  };
  if (layoutIntents) {
    options.layout_intents = layoutIntents;
  }
  return options;
}

export function isAppearanceOverridden(opts: AppearanceOptions): boolean {
  return opts.themeId !== 'auto' || opts.graphicStyle !== 'auto' || opts.darkMode;
}
