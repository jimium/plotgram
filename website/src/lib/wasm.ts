// website 端极简 WASM 加载器：从 CDN 加载 plotgram-wasm（与 playground 共享产物）。

/** 单格式渲染结果。 */
export interface RenderResult {
  success: boolean;
  format: string;
  text: string | null;
  errors: DiagnosticErrorJson[];
  warnings: DiagnosticErrorJson[];
}

export interface SpanJson {
  start: { line: number; column: number };
  end: { line: number; column: number };
}

export interface DiagnosticErrorJson {
  code: string;
  severity: 'error' | 'warning';
  category: 'parse' | 'validation' | 'render' | 'patch';
  message: string;
  location: SpanJson;
  context?: Record<string, unknown> | null;
  suggestion?: { text: string; fix?: unknown } | null;
}

export interface PlotgramWasm {
  default: (input?: unknown) => Promise<unknown>;
  version: () => string;
  render: (source: string, format: string) => string;
  render_with_options: (source: string, format: string, optionsJson: string) => string;
}

let modulePromise: Promise<PlotgramWasm> | null = null;

/** CDN 上 plotgram-wasm 的 common 根目录（website / playground / agent 三端共用）。 */
function wasmCdnBase(): string {
  const fromEnv = import.meta.env.VITE_WASM_CDN_BASE as string | undefined;
  if (fromEnv) return fromEnv.endsWith('/') ? fromEnv : `${fromEnv}/`;
  return 'https://assets.pg.agcli.cn/plotgram-wasm/';
}

function plotgramWasmJsUrl(): string {
  return `${wasmCdnBase()}plotgram_wasm.js`;
}

function plotgramWasmBinaryUrl(): string {
  return `${wasmCdnBase()}plotgram_wasm_bg.wasm`;
}

/** 懒加载并初始化 WASM 模块（全局单例）。 */
export function loadWasm(): Promise<PlotgramWasm> {
  if (!modulePromise) {
    modulePromise = (async () => {
      const mod = (await import(
        /* @vite-ignore */ // @ts-ignore WASM 产物由 wasm-pack 生成
        plotgramWasmJsUrl()
      )) as unknown as PlotgramWasm;
      await mod.default({ module_or_path: plotgramWasmBinaryUrl() });
      return mod;
    })();
  }
  return modulePromise;
}

function safeParse<T>(json: string, fallback: T): T {
  try {
    return JSON.parse(json) as T;
  } catch {
    return fallback;
  }
}

function fallbackDiag(message: string): DiagnosticErrorJson {
  return {
    code: 'E102',
    severity: 'error',
    category: 'render',
    message,
    location: { start: { line: 0, column: 0 }, end: { line: 0, column: 0 } },
  };
}

export interface WasmRenderOptions {
  theme_id?: string;
  graphic_style?: string;
  dark_mode?: boolean;
  transparent_background?: boolean;
  show_title?: boolean;
}

/** 渲染 SVG，返回单格式结果。 */
export function renderSvg(wasm: PlotgramWasm, source: string, options?: WasmRenderOptions): RenderResult {
  const json = options
    ? wasm.render_with_options(source, 'svg', JSON.stringify(options))
    : wasm.render(source, 'svg');
  return safeParse<RenderResult>(json, {
    success: false,
    format: 'svg',
    text: null,
    errors: [fallbackDiag('无法解析渲染结果')],
    warnings: [],
  });
}
