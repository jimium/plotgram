import type { DiagramKind } from './diagramKinds';

export interface SelectOption {
  value: string;
  label: string;
}

/** WASM `layout_catalog()` 返回的结构（与 plotgram-core 对齐） */
export interface AlgorithmOptionInfo {
  key: string;
  kind: 'non_negative_number' | 'positive_number' | 'number';
  default: number;
  description: string;
  min?: number;
  max?: number;
  exclude_min?: boolean;
}

export interface LayoutAlgoInfo {
  name: string;
  produces_edge_geometry: boolean;
  options: AlgorithmOptionInfo[];
  /** 该算法支持的方向列表。空数组表示不消费 diagram 级 `direction`。 */
  supported_directions?: string[];
}

export interface EdgeRoutingAlgoInfo {
  name: string;
  options: AlgorithmOptionInfo[];
}

export interface DiagramTypeCatalog {
  name: string;
  implemented: boolean;
  default_layout: string;
  default_edge_routing: string;
  uses_edge_routing: boolean;
  layouts: string[];
  edge_routings: string[];
  /** 用户未在 DSL 中声明 `direction` 时的默认值。undefined 表示不参与 direction 体系。 */
  default_direction?: string;
}

export interface LayoutCatalog {
  layouts: LayoutAlgoInfo[];
  edge_routings: EdgeRoutingAlgoInfo[];
  diagram_types: DiagramTypeCatalog[];
}

/** `null` 表示使用算法默认值，不写入 DSL 配置块 */
export type AlgorithmConfigValues = Record<string, number | null>;

/** `align` 属性模式；`default` 表示使用布局算法默认，不写入 DSL */
export type AlignMode = 'default' | 'off' | 'rank' | 'layer' | 'full';

export const ALIGN_MODE_LABELS: Record<AlignMode, string> = {
  default: '默认（算法）',
  off: '关闭',
  rank: 'rank — 流向轴',
  layer: 'layer — 同层分布',
  full: 'full — rank + layer',
};

export const ALIGN_MODE_OPTIONS: SelectOption[] = (
  Object.keys(ALIGN_MODE_LABELS) as AlignMode[]
).map((value) => ({ value, label: ALIGN_MODE_LABELS[value] }));

export function normalizeAlignMode(raw: unknown): AlignMode {
  if (raw === false || raw === 'off' || raw === 'false') return 'off';
  if (raw === 'rank' || raw === 'layer' || raw === 'full') return raw;
  return 'default';
}

function parseAlignFromSource(token: string): AlignMode {
  switch (token.toLowerCase()) {
    case 'false':
    case 'off':
    case 'none':
      return 'off';
    case 'true':
    case 'default':
      return 'default';
    case 'rank':
    case 'layer':
      return token.toLowerCase() as 'rank' | 'layer';
    case 'full':
    case 'all':
    case 'both':
      return 'full';
    default:
      return 'default';
  }
}

export interface LayoutOptions {
  layoutAlgo: string;
  edgeRouting: string;
  layoutDirection: string;
  /** 边像素量化（`snap` 属性）；默认 true（flowchart / er / sugiyama-v2 / architecture-v2 生效） */
  gridSnap: boolean;
  /** 节点结构对齐（`align` 属性）；`default` 使用算法默认策略 */
  gridAlign: AlignMode;
  layoutConfig: AlgorithmConfigValues;
  edgeRoutingConfig: AlgorithmConfigValues;
}

export interface DiagramDefaults {
  layoutAlgo: string;
  edgeRouting?: string;
}

/** 布局方向未指定时的内部值（不写入 DSL） */
export const LAYOUT_DIRECTION_UNSPECIFIED = 'auto';

/**
 * 布局 / 边路由选择器的「自动」哨兵值。
 * 表示「不在预览时覆盖」——渲染时保持源码 / 图表默认原样。
 */
export const LAYOUT_AUTO = 'auto';

/** 面板未做任何覆盖的初始状态：布局与边路由均跟随源码 / 图表默认。 */
export const AUTO_LAYOUT_OPTIONS: LayoutOptions = {
  layoutAlgo: LAYOUT_AUTO,
  edgeRouting: LAYOUT_AUTO,
  layoutDirection: LAYOUT_DIRECTION_UNSPECIFIED,
  gridSnap: true,
  gridAlign: 'default',
  layoutConfig: {},
  edgeRoutingConfig: {},
};

export function layoutOptionsFromDefaults(defaults: DiagramDefaults): LayoutOptions {
  return {
    layoutAlgo: defaults.layoutAlgo,
    edgeRouting: defaults.edgeRouting ?? '',
    layoutDirection: LAYOUT_DIRECTION_UNSPECIFIED,
    gridSnap: true,
    gridAlign: 'default',
    layoutConfig: {},
    edgeRoutingConfig: {},
  };
}

/** localStorage 初始占位；默认跟随源码 / 图表默认。 */
export const EMPTY_LAYOUT_OPTIONS: LayoutOptions = AUTO_LAYOUT_OPTIONS;

/** @deprecated 使用 AUTO_LAYOUT_OPTIONS 或 layoutOptionsFromDefaults */
export const DEFAULT_LAYOUT_OPTIONS = EMPTY_LAYOUT_OPTIONS;

export function normalizeLayoutOptions(
  raw: Partial<LayoutOptions> | null | undefined,
  _defaults: DiagramDefaults | null = null,
): LayoutOptions {
  const layoutAlgo = raw?.layoutAlgo ? raw.layoutAlgo : LAYOUT_AUTO;
  const edgeRouting = raw?.edgeRouting ? raw.edgeRouting : LAYOUT_AUTO;
  const layoutDirection = raw?.layoutDirection ?? LAYOUT_DIRECTION_UNSPECIFIED;

  return {
    layoutAlgo,
    edgeRouting,
    layoutDirection,
    gridSnap: raw?.gridSnap ?? true,
    gridAlign: normalizeAlignMode(raw?.gridAlign),
    layoutConfig: raw?.layoutConfig ?? {},
    edgeRoutingConfig: raw?.edgeRoutingConfig ?? {},
  };
}

/** 展示用标签（算法名来自 WASM catalog，标签留在 UI 层） */
const LAYOUT_LABELS: Record<string, string> = {
  'flowchart': 'flowchart — 流程图分层布局',
  'er': 'er — ER 图分层布局',
  'sugiyama': 'sugiyama — Sugiyama 分层（旧版）',
  'sugiyama-v2': 'sugiyama-v2 — Sugiyama 分层（新版）',
  'sequence': 'sequence — 时序图专属',
  'architecture-v2': 'architecture-v2 — 架构图分组分层',
  'force-directed': 'force-directed — 分组感知力导向',
  'circular': 'circular — 自适应圆形',
  'mindmap': 'mindmap — 思维导图',
};

const EDGE_ROUTING_LABELS: Record<string, string> = {
  'orthogonal': 'orthogonal — 正交折线',
  'straight': 'straight — 直线',
  'bezier': 'bezier — 自适应贝塞尔曲线',
  'spline': 'spline — 多段样条（避障）',
  'circular': 'circular — 弧形边（圆形布局）',
  'organic': 'organic — 有机贝塞尔曲线',
};

export type LayoutValueOrigin = 'source' | 'diagram-default' | 'panel' | 'unspecified';

export const LAYOUT_ORIGIN_LABELS: Record<LayoutValueOrigin, string> = {
  source: '源码',
  'diagram-default': '图表默认',
  panel: '面板覆盖',
  unspecified: '未指定',
};

export interface ParsedSourceLayout {
  layoutAlgo?: string;
  edgeRouting?: string;
  layoutDirection?: string;
  /** 源码中的 snap（边像素量化）；缺省表示 true */
  gridSnap?: boolean;
  /** 源码中的 align（节点结构对齐）；缺省表示算法默认 */
  gridAlign?: AlignMode;
}

export interface EffectiveLayoutField {
  value: string | null;
  origin: LayoutValueOrigin;
}

export interface EffectiveAlignField {
  mode: AlignMode;
  applicable: boolean;
  origin: LayoutValueOrigin;
}

export interface EffectiveLayout {
  layoutAlgo: EffectiveLayoutField;
  edgeRouting: EffectiveLayoutField | null;
  layoutDirection: EffectiveLayoutField;
  gridSnap: EffectiveToggleField;
  gridAlign: EffectiveAlignField;
}

/** 通用布尔开关生效字段（snap / align 共用） */
export interface EffectiveToggleField {
  enabled: boolean;
  applicable: boolean;
  origin: LayoutValueOrigin;
}

const LAYOUT_ATTR_PATTERN = /^\s*(layout|edge_routing|direction|snap|align)\s*:/;

function countBraceDelta(line: string): number {
  let delta = 0;
  for (const ch of line) {
    if (ch === '{') delta += 1;
    if (ch === '}') delta -= 1;
  }
  return delta;
}

function buildAlgorithmAttribute(
  attr: 'layout' | 'edge_routing',
  algo: string,
  specs: AlgorithmOptionInfo[],
  values: AlgorithmConfigValues,
): string {
  const overrides = specs.filter((spec) => {
    const value = values[spec.key];
    return value !== null && value !== undefined && value !== spec.default;
  });
  if (overrides.length === 0) {
    return `    ${attr}: ${algo}`;
  }
  const optionLines = overrides
    .map((spec) => `        ${spec.key}: ${values[spec.key]}`)
    .join('\n');
  return `    ${attr}: ${algo} {\n${optionLines}\n    }`;
}

export function resolveEffectiveLayoutAlgo(
  opts: LayoutOptions,
  _defaults: DiagramDefaults | null = null,
): string | null {
  return opts.layoutAlgo || null;
}

export function resolveEffectiveEdgeRouting(
  opts: LayoutOptions,
  _defaults: DiagramDefaults | null = null,
): string | null {
  return opts.edgeRouting || null;
}

export function activeLayoutOptionSpecs(
  catalog: LayoutCatalog | null,
  opts: LayoutOptions,
  defaults: DiagramDefaults | null,
): AlgorithmOptionInfo[] {
  const algo = resolveEffectiveLayoutAlgo(opts, defaults);
  if (!algo) return [];
  return layoutAlgoInfo(catalog, algo)?.options ?? [];
}

export function activeEdgeRoutingOptionSpecs(
  catalog: LayoutCatalog | null,
  opts: LayoutOptions,
  defaults: DiagramDefaults | null,
): AlgorithmOptionInfo[] {
  const algo = resolveEffectiveEdgeRouting(opts, defaults);
  if (!algo) return [];
  return edgeRoutingAlgoInfo(catalog, algo)?.options ?? [];
}

export interface SliderBounds {
  min: number;
  max: number;
  step: number;
}

export interface DiscreteChoice {
  value: number;
  label: string;
}

const KEY_SLIDER_MAX: Record<string, number> = {
  group_padding: 200,
  padding: 300,
  component_gap: 400,
  level_gap: 500,
  branch_gap: 200,
  node_gap: 100,
  center_gap: 300,
  node_spacing: 250,
  message_spacing: 200,
};

const KEY_STEP: Record<string, number> = {
  tension: 0.05,
  shoulder_ratio: 0.01,
  depth_decay: 0.01,
  port_distribution: 0.01,
};

export function optionSliderBounds(spec: AlgorithmOptionInfo): SliderBounds {
  if (spec.kind === 'number' && spec.min !== undefined && spec.max !== undefined) {
    const range = spec.max - spec.min;
    const step = KEY_STEP[spec.key]
      ?? (Number.isInteger(spec.default) && range > 2 ? 1 : (range <= 2 ? 0.05 : 1));
    return {
      min: spec.min,
      max: spec.max,
      step,
    };
  }

  const hardMax = KEY_SLIDER_MAX[spec.key] ?? Math.max(spec.default * 4, 100);
  const isFractional = !Number.isInteger(spec.default);
  return {
    min: spec.kind === 'positive_number' ? 0.001 : 0,
    max: hardMax,
    step: isFractional ? 0.5 : 1,
  };
}

export function optionInputBounds(spec: AlgorithmOptionInfo): { min?: number; max?: number; step?: number } {
  return optionSliderBounds(spec);
}

export function optionDiscreteChoices(spec: AlgorithmOptionInfo): DiscreteChoice[] | null {
  if (spec.kind !== 'number') return null;
  if (spec.min === undefined || spec.max === undefined) return null;
  if (spec.min !== 0) return null;
  if (spec.max > 5) return null;
  if ((spec.max - spec.min) % 1 !== 0) return null;

  const choiceRe = /(?<![.\d])(\d+)\s*=\s*([^,，0-9][^,，]*?)(?:[,，]|$)/g;
  const matches = [...spec.description.matchAll(choiceRe)];
  if (matches.length < 2) return null;

  const choices: DiscreteChoice[] = [];
  for (const m of matches) {
    const v = Number(m[1]);
    if (Number.isFinite(v) && v >= spec.min && v <= spec.max) {
      choices.push({ value: v, label: m[2].trim() });
    }
  }
  return choices.length >= 2 ? choices : null;
}

export function hasAlgorithmConfigOverrides(
  specs: AlgorithmOptionInfo[],
  values: AlgorithmConfigValues,
): boolean {
  return specs.some((spec) => {
    const value = values[spec.key];
    return value !== null && value !== undefined && value !== spec.default;
  });
}

export function parseLayoutCatalog(json: string): LayoutCatalog | null {
  try {
    return JSON.parse(json) as LayoutCatalog;
  } catch {
    return null;
  }
}

export function catalogForDiagramType(
  catalog: LayoutCatalog | null,
  type: DiagramKind | null,
): DiagramTypeCatalog | null {
  if (!catalog || !type) return null;
  return catalog.diagram_types.find((d) => d.name === type) ?? null;
}

export function layoutAlgoInfo(
  catalog: LayoutCatalog | null,
  name: string,
): LayoutAlgoInfo | undefined {
  return catalog?.layouts.find((l) => l.name === name);
}

export function edgeRoutingAlgoInfo(
  catalog: LayoutCatalog | null,
  name: string,
): EdgeRoutingAlgoInfo | undefined {
  return catalog?.edge_routings.find((r) => r.name === name);
}

export function allEdgeRoutingNames(catalog: LayoutCatalog | null): string[] {
  return catalog?.edge_routings.map((r) => r.name) ?? [];
}

export function layoutAlgoSupportsGridSnap(layoutAlgo: string | null | undefined): boolean {
  return layoutAlgo === 'flowchart'
    || layoutAlgo === 'er'
    || layoutAlgo === 'sugiyama-v2'
    || layoutAlgo === 'architecture-v2';
}

export function layoutProducesEdgeGeometry(
  catalog: LayoutCatalog | null,
  layoutName: string,
): boolean {
  if (!catalog || !layoutName) return false;
  return catalog.layouts.find((l) => l.name === layoutName)?.produces_edge_geometry ?? false;
}

export function getDiagramDefaults(
  catalog: LayoutCatalog | null,
  type: DiagramKind | null,
): DiagramDefaults | null {
  const entry = catalogForDiagramType(catalog, type);
  if (!entry) return null;
  const defaults: DiagramDefaults = { layoutAlgo: entry.default_layout };
  if (entry.uses_edge_routing && entry.default_edge_routing) {
    defaults.edgeRouting = entry.default_edge_routing;
  }
  return defaults;
}

export function buildLayoutAlgoOptions(
  catalog: LayoutCatalog | null,
  type: DiagramKind | null,
  defaults: DiagramDefaults | null = null,
): SelectOption[] {
  const entry = catalogForDiagramType(catalog, type);
  const names = entry?.layouts ?? catalog?.layouts.map((l) => l.name) ?? [];
  const autoLabel = defaults?.layoutAlgo
    ? `自动（${formatAlgoName(defaults.layoutAlgo)}）`
    : '自动（跟随源码）';
  const options: SelectOption[] = [{ value: LAYOUT_AUTO, label: autoLabel }];
  for (const value of names) {
    options.push({ value, label: LAYOUT_LABELS[value] ?? value });
  }
  return options;
}

export function buildEdgeRoutingOptions(
  catalog: LayoutCatalog | null,
  type: DiagramKind | null,
  defaults: DiagramDefaults | null = null,
): SelectOption[] {
  const entry = catalogForDiagramType(catalog, type);
  if (entry && !entry.uses_edge_routing) {
    return [];
  }
  const names = entry?.edge_routings ?? allEdgeRoutingNames(catalog);
  if (names.length === 0) return [];
  const autoLabel = defaults?.edgeRouting
    ? `自动（${formatAlgoName(defaults.edgeRouting)}）`
    : '自动（跟随源码）';
  const options: SelectOption[] = [{ value: LAYOUT_AUTO, label: autoLabel }];
  for (const value of names) {
    options.push({ value, label: EDGE_ROUTING_LABELS[value] ?? value });
  }
  return options;
}

export function buildLayoutDirectionOptions(
  catalog: LayoutCatalog | null,
  layoutAlgo: string | null | undefined,
): SelectOption[] {
  // 查找当前 layout 算法支持的方向
  const supported = layoutAlgo && catalog
    ? catalog.layouts.find((l) => l.name === layoutAlgo)?.supported_directions
    : undefined;

  // 如果算法不支持 direction，返回空列表
  if (supported && supported.length === 0) {
    return [];
  }

  const allDirections: SelectOption[] = [
    { value: LAYOUT_DIRECTION_UNSPECIFIED, label: '不指定' },
    { value: 'top-to-bottom', label: 'top-to-bottom — 自上而下' },
    { value: 'left-to-right', label: 'left-to-right — 自左而右' },
    { value: 'radial', label: 'radial — 径向' },
  ];

  // 如果算法声明了 supported_directions，按其过滤
  if (supported) {
    const allowed = new Set(supported);
    return allDirections.filter(
      (opt) => opt.value === LAYOUT_DIRECTION_UNSPECIFIED || allowed.has(opt.value),
    );
  }

  // 无 catalog 信息时，返回不含 radial 的默认列表
  return allDirections.filter((opt) => opt.value !== 'radial');
}

export function reconcileLayoutOptionsWithDefaults(
  opts: LayoutOptions,
  catalog: LayoutCatalog | null,
  type: DiagramKind | null,
  _defaults: DiagramDefaults,
): LayoutOptions {
  const layoutNames = buildLayoutAlgoOptions(catalog, type).map((o) => o.value);
  const routingNames = buildEdgeRoutingOptions(catalog, type).map((o) => o.value);

  const layoutAlgo = layoutNames.includes(opts.layoutAlgo)
    ? opts.layoutAlgo
    : LAYOUT_AUTO;

  const edgeRouting = routingNames.length === 0
    ? ''
    : routingNames.includes(opts.edgeRouting)
      ? opts.edgeRouting
      : LAYOUT_AUTO;

  return {
    ...opts,
    layoutAlgo,
    edgeRouting,
  };
}

export function detectDiagramType(source: string): DiagramKind | null {
  const match = source.match(/diagram\s+(\w+)\s*\{/);
  const kind = match?.[1];
  const known: DiagramKind[] = ['flowchart', 'sequence', 'architecture', 'state', 'er', 'mindmap'];
  return kind && (known as string[]).includes(kind) ? (kind as DiagramKind) : null;
}

/** 布局算法内部值 → 展示名 */
export function formatAlgoName(value: string): string {
  return value.replace(/_/g, '-');
}

function readDiagramTopLevelAttr(line: string, attr: 'layout' | 'edge_routing'): string | undefined {
  const match = line.match(new RegExp(`^\\s*${attr}\\s*:\\s*([\\w-]+)`));
  return match?.[1];
}

/** 从 diagram 块顶层解析布局相关属性（不含配置块内部字段） */
export function parseLayoutFromSource(source: string): ParsedSourceLayout {
  const result: ParsedSourceLayout = {};
  const lines = source.split('\n');
  let inDiagram = false;
  let depth = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (!inDiagram && /diagram\s+\w+\s*\{/.test(line)) {
      inDiagram = true;
      depth = countBraceDelta(line);
      continue;
    }

    if (!inDiagram) continue;

    if (depth > 0 && LAYOUT_ATTR_PATTERN.test(line)) {
      const layoutAlgo = readDiagramTopLevelAttr(line, 'layout');
      if (layoutAlgo) result.layoutAlgo = layoutAlgo;

      const edgeRouting = readDiagramTopLevelAttr(line, 'edge_routing');
      if (edgeRouting) result.edgeRouting = edgeRouting;

      const layoutMatch = line.match(/^\s*direction\s*:\s*(top-to-bottom|left-to-right|radial)/);
      if (layoutMatch) result.layoutDirection = layoutMatch[1];

      const snapMatch = line.match(/^\s*snap\s*:\s*(true|false)/);
      if (snapMatch) result.gridSnap = snapMatch[1] === 'true';

      const alignMatch = line.match(/^\s*align\s*:\s*([\w-]+)/);
      if (alignMatch) result.gridAlign = parseAlignFromSource(alignMatch[1]);

      let attrBalance = countBraceDelta(line);
      depth += attrBalance;
      while (attrBalance > 0 && i + 1 < lines.length) {
        i += 1;
        const nextDelta = countBraceDelta(lines[i]);
        attrBalance += nextDelta;
        depth += nextDelta;
      }
      if (depth <= 0) inDiagram = false;
      continue;
    }

    depth += countBraceDelta(line);
    if (depth <= 0) inDiagram = false;
  }

  return result;
}

function resolveEffectiveField(
  panelValue: string,
  sourceValue: string | undefined,
  defaultValue: string | null | undefined,
  layoutSource: 'source' | 'panel',
): EffectiveLayoutField {
  if (layoutSource === 'panel' && panelValue) {
    return { value: panelValue, origin: 'panel' };
  }
  if (sourceValue) {
    return { value: sourceValue, origin: 'source' };
  }
  if (defaultValue) {
    return { value: defaultValue, origin: 'diagram-default' };
  }
  return { value: null, origin: 'unspecified' };
}

function resolveEffectiveDirection(
  panelValue: string,
  sourceValue: string | undefined,
  layoutSource: 'source' | 'panel',
): EffectiveLayoutField {
  if (layoutSource === 'panel' && panelValue !== LAYOUT_DIRECTION_UNSPECIFIED) {
    return { value: panelValue, origin: 'panel' };
  }
  if (sourceValue) {
    return { value: sourceValue, origin: 'source' };
  }
  return { value: null, origin: 'unspecified' };
}

function resolveEffectiveAlign(
  panelMode: AlignMode,
  sourceMode: AlignMode | undefined,
  layoutSource: 'source' | 'panel',
  layoutAlgo: string | null,
): EffectiveAlignField {
  const applicable = layoutAlgoSupportsGridSnap(layoutAlgo);
  if (!applicable) {
    return { mode: 'default', applicable: false, origin: 'unspecified' };
  }
  if (layoutSource === 'panel') {
    return { mode: panelMode, applicable: true, origin: 'panel' };
  }
  if (sourceMode !== undefined) {
    return { mode: sourceMode, applicable: true, origin: 'source' };
  }
  return { mode: 'default', applicable: true, origin: 'diagram-default' };
}

/** 通用布尔开关 resolve（snap 专用）：算法不支持 → 不适用；面板覆盖 → 面板值；源码声明 → 源码值；否则 → 默认开启 */
function resolveEffectiveToggle(
  panelEnabled: boolean,
  sourceValue: boolean | undefined,
  layoutSource: 'source' | 'panel',
  layoutAlgo: string | null,
): EffectiveToggleField {
  const applicable = layoutAlgoSupportsGridSnap(layoutAlgo);
  if (!applicable) {
    return { enabled: false, applicable: false, origin: 'unspecified' };
  }
  if (layoutSource === 'panel') {
    return { enabled: panelEnabled, applicable: true, origin: 'panel' };
  }
  if (sourceValue !== undefined) {
    return { enabled: sourceValue, applicable: true, origin: 'source' };
  }
  return { enabled: true, applicable: true, origin: 'diagram-default' };
}

/** 合并源码、图表默认与面板选项，得到预览中实际生效的布局信息 */
export function resolveEffectiveLayout(
  source: string,
  opts: LayoutOptions,
  layoutSource: 'source' | 'panel',
  catalog: LayoutCatalog | null,
  defaults: DiagramDefaults | null,
): EffectiveLayout {
  const parsed = parseLayoutFromSource(source);
  const diagramType = detectDiagramType(source);
  const entry = catalogForDiagramType(catalog, diagramType);
  const layoutAlgo = resolveEffectiveField(
    opts.layoutAlgo,
    parsed.layoutAlgo,
    defaults?.layoutAlgo ?? null,
    layoutSource,
  );

  const usesEdgeRouting = entry?.uses_edge_routing ?? true;
  const producesEdges = layoutAlgo.value !== null
    && layoutProducesEdgeGeometry(catalog, layoutAlgo.value);

  const edgeRouting = usesEdgeRouting && !producesEdges
    ? resolveEffectiveField(
      opts.edgeRouting,
      parsed.edgeRouting,
      defaults?.edgeRouting ?? null,
      layoutSource,
    )
    : null;

  const layoutDirection = resolveEffectiveDirection(
    opts.layoutDirection,
    parsed.layoutDirection,
    layoutSource,
  );

  const gridSnap = resolveEffectiveToggle(
    opts.gridSnap,
    parsed.gridSnap,
    layoutSource,
    layoutAlgo.value,
  );

  const gridAlign = resolveEffectiveAlign(
    opts.gridAlign,
    parsed.gridAlign,
    layoutSource,
    layoutAlgo.value,
  );

  return { layoutAlgo, edgeRouting, layoutDirection, gridSnap, gridAlign };
}

/** 面板选项是否与图表类型默认一致（含参数未改） */
export function isLayoutAtDefaults(
  opts: LayoutOptions,
  defaults: DiagramDefaults | null,
): boolean {
  if (!defaults) return false;
  if (opts.layoutAlgo !== defaults.layoutAlgo) return false;
  if (defaults.edgeRouting !== undefined && opts.edgeRouting !== defaults.edgeRouting) return false;
  if (opts.layoutDirection !== LAYOUT_DIRECTION_UNSPECIFIED) return false;
  if (opts.gridSnap === false) return false;
  if (opts.gridAlign !== 'default') return false;
  return true;
}

/** 移除 diagram 块顶层指定属性（含 `{ }` 配置块），便于预览时注入覆盖值。
 * `attrs` 限定只剥离那些属性；缺省时剥离全部布局相关属性。 */
export function stripLayoutAttributes(
  source: string,
  attrs: readonly string[] = ['layout', 'edge_routing', 'direction', 'snap', 'align'],
): string {
  const pattern = new RegExp(`^\\s*(${attrs.join('|')})\\s*:`);
  const lines = source.split('\n');
  let inDiagram = false;
  let depth = 0;
  const result: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (!inDiagram && /diagram\s+\w+\s*\{/.test(line)) {
      inDiagram = true;
      depth = countBraceDelta(line);
      result.push(line);
      continue;
    }

    if (inDiagram) {
      if (depth > 0 && pattern.test(line)) {
        let attrBalance = countBraceDelta(line);
        depth += attrBalance;
        while (attrBalance > 0 && i + 1 < lines.length) {
          i += 1;
          const nextDelta = countBraceDelta(lines[i]);
          attrBalance += nextDelta;
          depth += nextDelta;
        }
        if (depth <= 0) inDiagram = false;
        continue;
      }

      depth += countBraceDelta(line);
      result.push(line);
      if (depth <= 0) inDiagram = false;
      continue;
    }

    result.push(line);
  }

  return result.join('\n');
}

/**
 * 在渲染前注入布局选项（仅影响预览，不修改编辑器中的源码）。
 *
 * 只覆盖用户显式选定的项：`layout` / `edge_routing` 为 `auto` 时保留源码原样。
 */
export function applyLayoutOptions(
  source: string,
  opts: LayoutOptions,
  catalog: LayoutCatalog | null = null,
  defaults: DiagramDefaults | null = null,
): string {
  const layoutOverride =
    opts.layoutAlgo && opts.layoutAlgo !== LAYOUT_AUTO ? opts.layoutAlgo : null;
  const routingOverride =
    opts.edgeRouting && opts.edgeRouting !== LAYOUT_AUTO ? opts.edgeRouting : null;

  // 没有任何显式覆盖 → 保持源码原样
  if (!layoutOverride && !routingOverride) return source;

  const injections: string[] = [];
  const strip: string[] = [];

  if (layoutOverride) {
    const layoutSpecs = activeLayoutOptionSpecs(catalog, opts, defaults);
    injections.push(
      buildAlgorithmAttribute('layout', layoutOverride, layoutSpecs, opts.layoutConfig),
    );
    strip.push('layout');
  }

  if (routingOverride) {
    const diagramType = detectDiagramType(source);
    const entry = catalogForDiagramType(catalog, diagramType);
    const usesEdgeRouting = entry?.uses_edge_routing ?? true;
    // 若有效布局自己生成边几何，则边路由无意义
    const effectiveLayout = layoutOverride ?? defaults?.layoutAlgo ?? null;
    const producesEdges = effectiveLayout
      ? layoutProducesEdgeGeometry(catalog, effectiveLayout)
      : false;
    if (usesEdgeRouting && !producesEdges) {
      const routingSpecs = activeEdgeRoutingOptionSpecs(catalog, opts, defaults);
      injections.push(
        buildAlgorithmAttribute('edge_routing', routingOverride, routingSpecs, opts.edgeRoutingConfig),
      );
      strip.push('edge_routing');
    }
  }

  if (injections.length === 0) return source;

  const stripped = stripLayoutAttributes(source, strip);
  const replaced = stripped.replace(
    /(diagram\s+\w+\s*\{)\s*\n/,
    `$1\n${injections.join('\n')}\n`,
  );

  return replaced === stripped ? stripped : replaced;
}

export function isLayoutOverridden(
  opts: LayoutOptions,
  catalog: LayoutCatalog | null = null,
  defaults: DiagramDefaults | null = null,
): boolean {
  if (defaults && !isLayoutAtDefaults(opts, defaults)) {
    return true;
  }
  return (
    hasAlgorithmConfigOverrides(activeLayoutOptionSpecs(catalog, opts, defaults), opts.layoutConfig)
    || hasAlgorithmConfigOverrides(
      activeEdgeRoutingOptionSpecs(catalog, opts, defaults),
      opts.edgeRoutingConfig,
    )
    || opts.gridSnap === false
    || opts.gridAlign !== 'default'
  );
}
