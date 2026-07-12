import { useMemo } from 'react';
import type { LintResult, LintViolation, LintAdvice } from '../lib/wasm';
import { IconError, IconWarning } from './Icons';

interface LintViewerProps {
  result: LintResult | null;
  ready: boolean;
}

function severityLabel(s: LintViolation['severity']): string {
  return s === 'error' ? 'Error' : 'Warning';
}

function adviceRankLabel(priority: number): string {
  return `P${priority}`;
}

function truncatePreview(text: string, max = 200): string {
  const trimmed = text.trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max)}…`;
}

function metricLabel(violation: LintViolation): string | null {
  if (violation.metric == null) return null;
  const m = violation.metric;
  if (violation.rule === 'SiblingWidthRatio') {
    return `ratio=${m.toFixed(3)}`;
  }
  if (m >= 1) {
    return `area=${m.toFixed(0)}px²`;
  }
  return m.toFixed(2);
}

interface RuleCount {
  rule: string;
  count: number;
}

export function LintViewer({ result, ready }: LintViewerProps) {
  const { violations, advices, adviceByViolation, ruleCounts } = useMemo(() => {
    const vs = result?.report.violations ?? [];
    const ads = result?.report.advices ?? [];
    const counts = new Map<string, number>();
    for (const v of vs) {
      counts.set(v.rule, (counts.get(v.rule) ?? 0) + 1);
    }
    const ruleCounts: RuleCount[] = Array.from(counts.entries())
      .map(([rule, count]) => ({ rule, count }))
      .sort((a, b) => b.count - a.count);
    const adviceByViolation = new Map<number, LintAdvice[]>();
    for (const a of ads) {
      const list = adviceByViolation.get(a.violation_index) ?? [];
      list.push(a);
      adviceByViolation.set(a.violation_index, list);
    }
    return { violations: vs, advices: ads, adviceByViolation, ruleCounts };
  }, [result]);

  if (!ready) {
    return <div className="empty-hint">WASM 加载中…</div>;
  }

  if (!result) {
    return <div className="empty-hint">无 Lint 数据</div>;
  }

  if (!result.success) {
    const errText = result.errors[0]?.message ?? 'Lint 执行失败';
    return (
      <div className="lint-viewer-root">
        <div className="lint-summary lint-summary--error">
          <IconError size={14} />
          <span>{errText}</span>
        </div>
      </div>
    );
  }

  const errorCount = violations.filter(v => v.severity === 'error').length;
  const warningCount = violations.filter(v => v.severity === 'warning').length;

  return (
    <div className="lint-viewer-root">
      <div className="lint-toolbar">
        <span className="lint-toolbar-info">
          <span className={`lint-badge ${result.acceptable ? 'lint-badge--ok' : 'lint-badge--bad'}`}>
            {result.acceptable ? '可接受' : '需处理'}
          </span>
          <span className="lint-stat lint-stat--error">{errorCount} errors</span>
          <span className="lint-stat lint-stat--warn">{warningCount} warnings</span>
          <span className="lint-stat">{advices.length} advices</span>
        </span>
      </div>

      {ruleCounts.length > 0 && (
        <div className="lint-rule-chips">
          {ruleCounts.map(({ rule, count }) => (
            <span key={rule} className="lint-rule-chip" title={rule}>
              {rule} × {count}
            </span>
          ))}
        </div>
      )}

      {violations.length === 0 ? (
        <div className="empty-hint">当前没有布局违规</div>
      ) : (
        <ul className="lint-list">
          {violations.map((v, i) => {
            const sev = v.severity;
            const metric = metricLabel(v);
            const related = adviceByViolation.get(i) ?? [];
            return (
              <li
                key={`${v.rule}-${i}`}
                className={`lint-item lint-item--${sev}`}
              >
                <div className="lint-item-header">
                  {sev === 'error' ? <IconError size={14} /> : <IconWarning size={14} />}
                  <span className="lint-item-rule">{v.rule}</span>
                  <span className={`lint-item-severity lint-item-severity--${sev}`}>
                    {severityLabel(sev)}
                  </span>
                  {v.edge_index != null && (
                    <span className="lint-item-loc">edge #{v.edge_index}</span>
                  )}
                  {metric && <span className="lint-item-metric">{metric}</span>}
                </div>

                <div className="lint-item-message">{v.message}</div>

                {(v.entity_ids?.length ?? 0) > 0 && (
                  <div className="lint-item-meta">
                    <span className="lint-item-meta-label">entities:</span>
                    <span className="lint-item-meta-value">{v.entity_ids!.join(', ')}</span>
                  </div>
                )}
                {(v.group_ids?.length ?? 0) > 0 && (
                  <div className="lint-item-meta">
                    <span className="lint-item-meta-label">groups:</span>
                    <span className="lint-item-meta-value">{v.group_ids!.join(', ')}</span>
                  </div>
                )}
                {(v.related_edge_indices?.length ?? 0) > 0 && (
                  <div className="lint-item-meta">
                    <span className="lint-item-meta-label">edges:</span>
                    <span className="lint-item-meta-value">{v.related_edge_indices!.join(', ')}</span>
                  </div>
                )}

                {related.length > 0 && (
                  <div className="lint-advice-list">
                    {related.map((a, j) => (
                      <div key={`${i}-${j}`} className="lint-advice">
                        <div className="lint-advice-head">
                          <span className="lint-advice-rank">{adviceRankLabel(a.priority)}</span>
                          <span className={`lint-advice-conf lint-advice-conf--${a.confidence}`}>{a.confidence}</span>
                          {a.fix && <span className="lint-advice-fix-tag">fix: {a.fix.action}</span>}
                        </div>
                        <div className="lint-advice-text">{truncatePreview(a.text)}</div>
                      </div>
                    ))}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
