/**
 * 图表主题与渲染选项（从 playground appearanceOptions 精简移植）
 */

export interface ThemeOption {
  value: string;
  label: string;
}

export interface ThemeGroup {
  label: string;
  options: ThemeOption[];
}

export const THEME_GROUPS: ThemeGroup[] = [
  {
    label: '通用',
    options: [
      { value: 'auto', label: '自动（跟随图表默认）' },
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

export const GRAPHIC_STYLES: ThemeOption[] = [
  { value: 'auto', label: '自动' },
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

export const DEFAULT_APPEARANCE: AppearanceOptions = {
  themeId: 'auto',
  graphicStyle: 'auto',
  darkMode: false,
};

export interface WasmRenderOptions {
  theme_id?: string;
  graphic_style?: string;
  dark_mode?: boolean;
  transparent_background?: boolean;
  show_title?: boolean;
}

export function buildRenderOptions(opts: AppearanceOptions): WasmRenderOptions {
  return {
    theme_id: opts.themeId === 'auto' ? undefined : opts.themeId,
    graphic_style: opts.graphicStyle === 'auto' ? undefined : opts.graphicStyle,
    dark_mode: Boolean(opts.darkMode),
  };
}
