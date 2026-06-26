import { useMemo, useState, type ReactNode } from 'react';
import { IconChevron, IconDownload, IconCopy, IconSettings, IconReset } from './Icons';
import { IntentPanel } from './IntentPanel';
import {
  buildLayoutAlgoOptions,
  buildEdgeRoutingOptions,
  buildLayoutDirectionOptions,
  activeLayoutOptionSpecs,
  activeEdgeRoutingOptionSpecs,
  optionSliderBounds,
  optionDiscreteChoices,
  resolveEffectiveLayout,
  formatAlgoName,
  isLayoutOverridden,
  layoutAlgoSupportsGridSnap,
  LAYOUT_ORIGIN_LABELS,
  type AlgorithmOptionInfo,
  type AlgorithmConfigValues,
  type EffectiveLayoutField,
  type EffectiveGridSnapField,
  type LayoutOptions,
  type LayoutCatalog,
  type DiagramDefaults,
} from '../data/layoutOptions';
import {
  GRAPHIC_STYLES,
  THEME_GROUPS,
  isAppearanceOverridden,
  type AppearanceOptions,
} from '../data/appearanceOptions';
import type { IntentDrafts } from '../data/intentOptions';
import type { DiagramKind } from '../data/examples';
import { KIND_LABELS } from '../data/examples';
import type { ExportReport } from '../lib/wasm';

/* ─── 通用子组件 ─────────────────────────────────────────── */

interface SectionProps {
  title: string;
  badge?: ReactNode;
  defaultOpen?: boolean;
  children: ReactNode;
}

function Section({ title, badge, defaultOpen = true, children }: SectionProps) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className={`inspector-section ${open ? 'open' : ''}`}>
      <button type="button" className="inspector-section-head" onClick={() => setOpen((o) => !o)}>
        <IconChevron size={14} className="section-chevron" />
        <span className="section-title">{title}</span>
        {badge}
      </button>
      {open && <div className="inspector-section-body">{children}</div>}
    </div>
  );
}

interface FieldProps {
  label: string;
  children: ReactNode;
}

function Field({ label, children }: FieldProps) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
    </label>
  );
}

interface SliderFieldProps {
  spec: AlgorithmOptionInfo;
  value: number | null;
  onChange: (value: number | null) => void;
}

function SliderField({ spec, value, onChange }: SliderFieldProps) {
  const bounds = optionSliderBounds(spec);
  const choices = optionDiscreteChoices(spec);
  const current = value ?? spec.default;
  const isOverridden = value !== null && value !== undefined && value !== spec.default;

  const formatValue = (v: number) => {
    if (bounds.step >= 1) return String(Math.round(v));
    return v.toFixed(2).replace(/\.?0+$/, '');
  };

  if (choices) {
    return (
      <div className="param-field">
        <div className="param-field-head">
          <span className="param-label">{spec.key}</span>
          <div className="param-field-actions">
            {isOverridden && (
              <button
                type="button"
                className="param-reset-btn"
                title="恢复默认"
                onClick={() => onChange(null)}
              >
                <IconReset size={12} />
              </button>
            )}
          </div>
        </div>
        <select
          className="select"
          value={String(current)}
          onChange={(e) => {
            const parsed = Number(e.target.value);
            onChange(Number.isFinite(parsed) && parsed !== spec.default ? parsed : null);
          }}
        >
          {choices.map((c) => (
            <option key={c.value} value={c.value}>{c.label}</option>
          ))}
        </select>
        <span className="hint">{spec.description}</span>
      </div>
    );
  }

  return (
    <div className="param-field">
      <div className="param-field-head">
        <span className="param-label">{spec.key}</span>
        <div className="param-field-actions">
          <span className={`param-value ${isOverridden ? 'overridden' : ''}`}>
            {formatValue(current)}
          </span>
          {isOverridden && (
            <button
              type="button"
              className="param-reset-btn"
              title="恢复默认"
              onClick={() => onChange(null)}
            >
              <IconReset size={12} />
            </button>
          )}
        </div>
      </div>
      <div className="param-slider-row">
        <input
          type="range"
          className="param-slider"
          min={bounds.min}
          max={bounds.max}
          step={bounds.step}
          value={current}
          onChange={(e) => {
            const parsed = Number(e.target.value);
            onChange(Number.isFinite(parsed) ? parsed : null);
          }}
          onDoubleClick={() => onChange(null)}
        />
      </div>
      <span className="hint">{spec.description}</span>
    </div>
  );
}

interface AlgorithmOptionFieldsProps {
  title: string;
  specs: AlgorithmOptionInfo[];
  values: AlgorithmConfigValues;
  onChange: (key: string, value: number | null) => void;
}

function AlgorithmOptionFields({ title, specs, values, onChange }: AlgorithmOptionFieldsProps) {
  const [open, setOpen] = useState(false);

  if (specs.length === 0) return null;

  const hasOverrides = specs.some(
    (spec) => values[spec.key] !== null && values[spec.key] !== undefined && values[spec.key] !== spec.default,
  );

  return (
    <div className={`algo-options-panel ${open ? 'open' : ''}`}>
      <button
        type="button"
        className="algo-options-toggle"
        onClick={() => setOpen((o) => !o)}
      >
        <IconSettings size={13} className="algo-options-toggle-icon" />
        <span>{title}</span>
        {hasOverrides && <span className="param-dot" title="已修改" />}
        <IconChevron size={12} className="algo-options-chevron" />
      </button>
      {open && (
        <div className="algo-options-body">
          {specs.map((spec) => (
            <SliderField
              key={spec.key}
              spec={spec}
              value={values[spec.key] ?? null}
              onChange={(v) => onChange(spec.key, v)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function EffectiveLayoutRow({ label, field }: { label: string; field: EffectiveLayoutField }) {
  return (
    <div className="field-row effective-layout-row">
      <span className="field-label">{label}</span>
      <div className="effective-layout-value">
        <code>{field.value ? formatAlgoName(field.value) : '—'}</code>
        <span className={`tag tag-origin tag-origin-${field.origin}`}>
          {LAYOUT_ORIGIN_LABELS[field.origin]}
        </span>
      </div>
    </div>
  );
}

interface EffectiveLayoutSummaryProps {
  effective: ReturnType<typeof resolveEffectiveLayout>;
  showEdgeRouting: boolean;
}

function EffectiveLayoutSummary({ effective, showEdgeRouting }: EffectiveLayoutSummaryProps) {
  return (
    <div className="effective-layout">
      <span className="effective-layout-title">当前生效</span>
      <EffectiveLayoutRow label="节点布局" field={effective.layoutAlgo} />
      {showEdgeRouting && effective.edgeRouting && (
        <EffectiveLayoutRow label="边路由" field={effective.edgeRouting} />
      )}
      <EffectiveLayoutRow label="布局方向" field={effective.layoutDirection} />
      {effective.gridSnap.applicable && (
        <EffectiveGridSnapRow field={effective.gridSnap} />
      )}
    </div>
  );
}

function EffectiveGridSnapRow({ field }: { field: EffectiveGridSnapField }) {
  return (
    <div className="field-row effective-layout-row">
      <span className="field-label">网格吸附</span>
      <div className="effective-layout-value">
        <code>{field.enabled ? '开启' : '关闭'}</code>
        <span className={`tag tag-origin tag-origin-${field.origin}`}>
          {LAYOUT_ORIGIN_LABELS[field.origin]}
        </span>
      </div>
    </div>
  );
}

/* ─── Inspector 2.0 ──────────────────────────────────────── */

interface InspectorProps {
  sourceCode: string;
  layoutOptions: LayoutOptions;
  appearanceOptions: AppearanceOptions;
  diagramType: DiagramKind | null;
  layoutCatalog: LayoutCatalog | null;
  diagramDefaults: DiagramDefaults | null;
  layoutSource: 'source' | 'panel';
  onLayoutChange: (key: 'layoutAlgo' | 'edgeRouting' | 'layoutDirection' | 'gridSnap', value: string | boolean) => void;
  onLayoutConfigChange: (key: string, value: number | null) => void;
  onEdgeRoutingConfigChange: (key: string, value: number | null) => void;
  onAppearanceChange: <K extends keyof AppearanceOptions>(key: K, value: AppearanceOptions[K]) => void;
  onResetLayout: () => void;
  onReset: () => void;
  onLayoutSourceChange: (source: 'source' | 'panel') => void;
  /** 布局意图草稿 */
  intentDrafts: IntentDrafts;
  onIntentChange: (drafts: IntentDrafts) => void;
  /** 当前 diagram 的实体 ID 列表，供意图面板选择节点 */
  entityIds: string[];
  /** 导出回调 */
  onExportSvg?: () => void;
  onExportPng?: () => void;
  onExportWebp?: () => void;
  onExportAscii?: () => void;
  onExportJson?: () => void;
  onExportDrawio?: () => void;
  onCopySvg?: () => void;
  onCopyAscii?: () => void;
  onCopyJson?: () => void;
  onCopyDrawio?: () => void;
  /** 在 draw.io 中打开 */
  onOpenInDrawio?: () => void;
  /** drawio 导出降级报告 */
  drawioExportReport?: ExportReport | null;
  rasterScale?: number;
  onRasterScaleChange?: (scale: number) => void;
  canExport?: boolean;
}

export function Inspector({
  sourceCode,
  layoutOptions,
  appearanceOptions,
  diagramType,
  layoutCatalog,
  diagramDefaults,
  layoutSource,
  onLayoutChange,
  onLayoutConfigChange,
  onEdgeRoutingConfigChange,
  onAppearanceChange,
  onResetLayout,
  onReset,
  onLayoutSourceChange,
  intentDrafts,
  onIntentChange,
  entityIds,
  onExportSvg,
  onExportPng,
  onExportWebp,
  onExportAscii,
  onExportJson,
  onExportDrawio,
  onCopySvg,
  onCopyAscii,
  onCopyJson,
  onCopyDrawio,
  onOpenInDrawio,
  drawioExportReport,
  rasterScale = 2,
  onRasterScaleChange,
  canExport,
}: InspectorProps) {
  const layoutOverridden = isLayoutOverridden(layoutOptions, layoutCatalog, diagramDefaults);
  const appearanceOverridden = isAppearanceOverridden(appearanceOptions);
  const [exportFormat, setExportFormat] = useState<'svg' | 'png' | 'webp' | 'ascii' | 'json' | 'drawio'>('svg');

  const effectiveLayout = useMemo(
    () => resolveEffectiveLayout(sourceCode, layoutOptions, layoutSource, layoutCatalog, diagramDefaults),
    [sourceCode, layoutOptions, layoutSource, layoutCatalog, diagramDefaults],
  );

  const layoutAlgoOptions = useMemo(
    () => buildLayoutAlgoOptions(layoutCatalog, diagramType, diagramDefaults),
    [layoutCatalog, diagramType, diagramDefaults],
  );
  const edgeRoutingOptions = useMemo(
    () => buildEdgeRoutingOptions(layoutCatalog, diagramType, diagramDefaults),
    [layoutCatalog, diagramType, diagramDefaults],
  );
  const layoutDirectionOptions = useMemo(
    () => buildLayoutDirectionOptions(layoutCatalog, layoutOptions.layoutAlgo),
    [layoutCatalog, layoutOptions.layoutAlgo],
  );
  const layoutOptionSpecs = useMemo(
    () => activeLayoutOptionSpecs(layoutCatalog, layoutOptions, diagramDefaults),
    [layoutCatalog, layoutOptions, diagramDefaults],
  );
  const edgeRoutingOptionSpecs = useMemo(
    () => activeEdgeRoutingOptionSpecs(layoutCatalog, layoutOptions, diagramDefaults),
    [layoutCatalog, layoutOptions, diagramDefaults],
  );

  const showEdgeRouting = edgeRoutingOptions.length > 0
    && effectiveLayout.edgeRouting !== null;

  const showGridSnap = layoutAlgoSupportsGridSnap(effectiveLayout.layoutAlgo.value);

  return (
    <aside className="inspector">
      <div className="inspector-scroll">
        {/* ── 第一层：图表信息 ──────────────────────────── */}
        <Section title="图表信息" defaultOpen={true}>
          <div className="field-row" style={{ justifyContent: 'space-between' }}>
            <span className="field-label">类型</span>
            {diagramType ? (
              <span className="tag tag-kind">{KIND_LABELS[diagramType]}</span>
            ) : (
              <span style={{ color: 'var(--text-faint)', fontSize: 12 }}>—</span>
            )}
          </div>
          <div className="field-row" style={{ justifyContent: 'space-between' }}>
            <span className="field-label">布局来源</span>
            <div className="layout-source-toggle">
              <input
                type="radio"
                id="ls-source"
                name="layoutSource"
                checked={layoutSource === 'source'}
                onChange={() => onLayoutSourceChange('source')}
              />
              <label htmlFor="ls-source">跟随源码</label>
              <input
                type="radio"
                id="ls-panel"
                name="layoutSource"
                checked={layoutSource === 'panel'}
                onChange={() => onLayoutSourceChange('panel')}
              />
              <label htmlFor="ls-panel">面板覆盖</label>
            </div>
          </div>
          {(layoutOverridden || appearanceOverridden) && (
            <button type="button" className="link-btn" onClick={onReset} style={{ alignSelf: 'flex-end' }}>
              重置全部
            </button>
          )}
        </Section>

        {/* ── 第二层：布局 ──────────────────────────────── */}
        <Section
          title="布局"
          badge={layoutSource === 'panel' && layoutOverridden ? <span className="tag tag-override">已覆盖</span> : null}
          defaultOpen={layoutSource === 'panel'}
        >
          <EffectiveLayoutSummary effective={effectiveLayout} showEdgeRouting={showEdgeRouting} />

          {layoutSource === 'panel' ? (
            <>
              <Field label="节点布局">
                <select
                  className="select"
                  value={layoutOptions.layoutAlgo}
                  onChange={(e) => onLayoutChange('layoutAlgo', e.target.value)}
                >
                  {layoutAlgoOptions.map(({ value, label }) => (
                    <option key={value} value={value}>{label}</option>
                  ))}
                </select>
              </Field>
              <AlgorithmOptionFields
                title="布局参数"
                specs={layoutOptionSpecs}
                values={layoutOptions.layoutConfig}
                onChange={onLayoutConfigChange}
              />
              {showEdgeRouting && (
                <>
                  <Field label="边路由">
                    <select
                      className="select"
                      value={layoutOptions.edgeRouting}
                      onChange={(e) => onLayoutChange('edgeRouting', e.target.value)}
                    >
                      {edgeRoutingOptions.map(({ value, label }) => (
                        <option key={value} value={value}>{label}</option>
                      ))}
                    </select>
                  </Field>
                  <AlgorithmOptionFields
                    title="边路由参数"
                    specs={edgeRoutingOptionSpecs}
                    values={layoutOptions.edgeRoutingConfig}
                    onChange={onEdgeRoutingConfigChange}
                  />
                </>
              )}
              <Field label="布局方向">
                <select
                  className="select"
                  value={layoutOptions.layoutDirection}
                  onChange={(e) => onLayoutChange('layoutDirection', e.target.value)}
                >
                  {layoutDirectionOptions.map(({ value, label }) => (
                    <option key={value} value={value}>{label}</option>
                  ))}
                </select>
              </Field>

              {showGridSnap && (
                <label className="toggle">
                  <input
                    type="checkbox"
                    checked={layoutOptions.gridSnap}
                    onChange={(e) => onLayoutChange('gridSnap', e.target.checked)}
                  />
                  <span>网格吸附（Grid Snap）</span>
                </label>
              )}

              {layoutOverridden && (
                <button type="button" className="link-btn" onClick={onResetLayout}>
                  恢复布局默认
                </button>
              )}
            </>
          ) : (
            <>
              {showGridSnap && (
                <label className="toggle">
                  <input
                    type="checkbox"
                    checked={effectiveLayout.gridSnap.enabled}
                    disabled
                  />
                  <span>网格吸附（Grid Snap）</span>
                </label>
              )}
              <p className="hint">
                布局参数由源码中的 <code>layout</code> / <code>edge_routing</code> / <code>direction</code> / <code>snap</code> 决定；未写明的项使用图表默认。切换为「面板覆盖」可在不修改源码的情况下试验算法。
              </p>
            </>
          )}
        </Section>

        {/* ── 布局意图 ──────────────────────────────────── */}
        <Section
          title="布局意图"
          badge={intentDrafts.enabled ? <span className="tag tag-override">已启用</span> : null}
          defaultOpen={false}
        >
          <IntentPanel drafts={intentDrafts} onChange={onIntentChange} entityIds={entityIds} />
        </Section>

        {/* ── 第三层：外观 ──────────────────────────────── */}
        <Section
          title="外观"
          badge={appearanceOverridden ? <span className="tag tag-override">已覆盖</span> : null}
          defaultOpen={false}
        >
          <Field label="主题">
            <select
              className="select"
              value={appearanceOptions.themeId}
              onChange={(e) => onAppearanceChange('themeId', e.target.value)}
            >
              <option value="auto">自动（跟随图表默认）</option>
              {THEME_GROUPS.map(({ label, options }) => (
                <optgroup key={label} label={label}>
                  {options.map(({ value, label: optionLabel }) => (
                    <option key={value} value={value}>{optionLabel}</option>
                  ))}
                </optgroup>
              ))}
            </select>
          </Field>
          <Field label="图形风格">
            <select
              className="select"
              value={appearanceOptions.graphicStyle}
              onChange={(e) => onAppearanceChange('graphicStyle', e.target.value)}
            >
              {GRAPHIC_STYLES.map(({ value, label }) => (
                <option key={value} value={value}>{label}</option>
              ))}
            </select>
          </Field>
          <label className="toggle">
            <input
              type="checkbox"
              checked={appearanceOptions.darkMode}
              onChange={(e) => onAppearanceChange('darkMode', e.target.checked)}
            />
            <span>启用深色模式偏好</span>
          </label>
          <p className="hint">
            主题控制配色与视觉 token，图形风格控制画法；深色模式偏好仅在「自动主题」时参与默认主题选择。
          </p>
        </Section>

        {/* ── 第四层：导出 ──────────────────────────────── */}
        <Section title="导出" defaultOpen={false}>
          <Field label="格式">
            <div className="export-format-group">
              {(['svg', 'png', 'webp', 'ascii', 'json', 'drawio'] as const).map((fmt) => (
                <button
                  key={fmt}
                  type="button"
                  className={`export-format-btn ${exportFormat === fmt ? 'active' : ''}`}
                  onClick={() => setExportFormat(fmt)}
                >
                  {fmt.toUpperCase()}
                </button>
              ))}
            </div>
          </Field>
          {(exportFormat === 'png' || exportFormat === 'webp') && (
            <Field label="导出倍率">
              <div className="export-scale-group">
                {[1, 2, 3].map((s) => (
                  <button
                    key={s}
                    type="button"
                    className={`scale-btn${rasterScale === s ? ' active' : ''}`}
                    onClick={() => onRasterScaleChange?.(s)}
                  >
                    {s}x
                  </button>
                ))}
              </div>
            </Field>
          )}
          <p className="hint">
            JSON 为 Scene JSON（<code>drawify.export_scene</code>），含布局与样式，供 Agent / CI / 自定义前端消费。
          </p>
          <div className="export-actions">
            <button
              type="button"
              className="btn btn-soft btn-sm"
              disabled={!canExport}
              onClick={() => {
                if (exportFormat === 'svg') onExportSvg?.();
                else if (exportFormat === 'png') onExportPng?.();
                else if (exportFormat === 'webp') onExportWebp?.();
                else if (exportFormat === 'ascii') onExportAscii?.();
                else if (exportFormat === 'json') onExportJson?.();
                else onExportDrawio?.();
              }}
            >
              <IconDownload size={14} />
              下载
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              disabled={!canExport || exportFormat === 'png' || exportFormat === 'webp'}
              onClick={() => {
                if (exportFormat === 'ascii') onCopyAscii?.();
                else if (exportFormat === 'json') onCopyJson?.();
                else if (exportFormat === 'drawio') onCopyDrawio?.();
                else onCopySvg?.();
              }}
            >
              <IconCopy size={14} />
              复制
            </button>
            {exportFormat === 'drawio' && (
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                disabled={!canExport}
                onClick={() => onOpenInDrawio?.()}
                title="在 diagrams.net 中打开编辑"
              >
                在 draw.io 中打开
              </button>
            )}
          </div>
          {exportFormat === 'drawio' && drawioExportReport && drawioExportReport.warnings.length > 0 && (
            <div className="drawio-warnings" style={{ marginTop: 8, padding: '6px 8px', background: 'var(--surface-sunken, #f5f5f5)', borderRadius: 6, fontSize: 12 }}>
              <span style={{ fontWeight: 500 }}>
                {drawioExportReport.warnings.length} 项降级
              </span>
              <ul style={{ margin: '4px 0 0', paddingLeft: 16 }}>
                {drawioExportReport.warnings.slice(0, 5).map((w, i) => (
                  <li key={i} style={{ color: 'var(--text-secondary, #666)' }}>{w.message}</li>
                ))}
                {drawioExportReport.warnings.length > 5 && (
                  <li style={{ color: 'var(--text-faint, #999)' }}>
                    ...还有 {drawioExportReport.warnings.length - 5} 项
                  </li>
                )}
              </ul>
            </div>
          )}
        </Section>
      </div>
    </aside>
  );
}
