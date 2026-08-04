import { useMemo, useState, type ReactNode } from 'react';
import { IconChevron } from './Icons';
import {
  buildLayoutAlgoOptions,
  buildEdgeRoutingOptions,
  layoutProducesEdgeGeometry,
  LAYOUT_AUTO,
  type LayoutOptions,
  type LayoutCatalog,
  type DiagramDefaults,
} from '../data/layoutOptions';
import {
  THEME_GROUPS,
  isAppearanceOverridden,
  resolveEffectiveThemeId,
  type AppearanceOptions,
} from '../data/appearanceOptions';
import type { DiagramKind } from '../data/diagramKinds';
import { KIND_LABELS } from '../data/diagramKinds';

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

/* ─── Inspector ──────────────────────────────────────────── */

interface InspectorProps {
  layoutOptions: LayoutOptions;
  appearanceOptions: AppearanceOptions;
  diagramType: DiagramKind | null;
  layoutCatalog: LayoutCatalog | null;
  diagramDefaults: DiagramDefaults | null;
  onLayoutChange: (key: 'layoutAlgo' | 'edgeRouting', value: string) => void;
  onAppearanceChange: <K extends keyof AppearanceOptions>(key: K, value: AppearanceOptions[K]) => void;
  onReset: () => void;
}

export function Inspector({
  layoutOptions,
  appearanceOptions,
  diagramType,
  layoutCatalog,
  diagramDefaults,
  onLayoutChange,
  onAppearanceChange,
  onReset,
}: InspectorProps) {
  const effectiveThemeId = useMemo(
    () => resolveEffectiveThemeId(appearanceOptions, diagramType),
    [appearanceOptions, diagramType],
  );

  const layoutAlgoOptions = useMemo(
    () => buildLayoutAlgoOptions(layoutCatalog, diagramType, diagramDefaults),
    [layoutCatalog, diagramType, diagramDefaults],
  );
  const edgeRoutingOptions = useMemo(
    () => buildEdgeRoutingOptions(layoutCatalog, diagramType, diagramDefaults),
    [layoutCatalog, diagramType, diagramDefaults],
  );

  // 有效布局算法（用于判断是否需要展示边路由）
  const effectiveLayoutAlgo = layoutOptions.layoutAlgo !== LAYOUT_AUTO
    ? layoutOptions.layoutAlgo
    : diagramDefaults?.layoutAlgo ?? null;
  const layoutProducesEdges = effectiveLayoutAlgo
    ? layoutProducesEdgeGeometry(layoutCatalog, effectiveLayoutAlgo)
    : false;
  const showEdgeRouting = edgeRoutingOptions.length > 0 && !layoutProducesEdges;

  const layoutOverridden = layoutOptions.layoutAlgo !== LAYOUT_AUTO
    || layoutOptions.edgeRouting !== LAYOUT_AUTO;
  const overridden = layoutOverridden || isAppearanceOverridden(appearanceOptions);

  return (
    <aside className="inspector">
      <div className="inspector-scroll">
        {diagramType && (
          <div className="inspector-kind-row">
            <span className="field-label">图表类型</span>
            <span className="tag tag-kind">{KIND_LABELS[diagramType]}</span>
          </div>
        )}

        {/* ── 布局与路由 ────────────────────────────────── */}
        <Section title="布局算法" defaultOpen>
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
          {showEdgeRouting && (
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
          )}
          <p className="hint">
            默认「自动」跟随源码 / 图表默认；选具体算法可在不修改 DSL 的情况下试验布局与路由效果。
          </p>
        </Section>

        {/* ── 主题 ──────────────────────────────────────── */}
        <Section title="主题" defaultOpen>
          <Field label="配色主题">
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
          <p className="hint">
            实际渲染主题：<code>{effectiveThemeId}</code>
          </p>
        </Section>

        {overridden && (
          <div className="inspector-footer">
            <button type="button" className="btn btn-ghost btn-sm" onClick={onReset}>
              恢复默认
            </button>
          </div>
        )}
      </div>
    </aside>
  );
}
