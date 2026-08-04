import { useMemo, useState } from 'react';
import type { LintResult, LintViolation, LintAdvice } from '../lib/wasm';
import { IconError, IconWarning, IconCheck, IconChevron, IconClose } from './Icons';

interface LintViewerProps {
  result: LintResult | null;
  ready: boolean;
}

type SeverityFilter = 'all' | 'error' | 'warning';

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

function metaChips(v: LintViolation): Array<{ label: string; value: string }> {
  const items: Array<{ label: string; value: string }> = [];
  if (v.entity_ids?.length) items.push({ label: 'entities', value: v.entity_ids.join(', ') });
  if (v.group_ids?.length) items.push({ label: 'groups', value: v.group_ids.join(', ') });
  if (v.related_edge_indices?.length) {
    items.push({ label: 'edges', value: v.related_edge_indices.join(', ') });
  }
  return items;
}

interface RuleCount {
  rule: string;
  count: number;
  errorCount: number;
}

interface IndexedViolation {
  violation: LintViolation;
  index: number;
}

export function LintViewer({ result, ready }: LintViewerProps) {
  const [severityFilter, setSeverityFilter] = useState<SeverityFilter>('all');
  const [ruleFilter, setRuleFilter] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set());

  const { violations, advices, adviceByViolation, ruleCounts, errorCount, warningCount } = useMemo(() => {
    const vs = result?.report.violations ?? [];
    const ads = result?.report.advices ?? [];
    const counts = new Map<string, RuleCount>();
    let errors = 0;
    let warnings = 0;
    for (const v of vs) {
      if (v.severity === 'error') errors += 1;
      else warnings += 1;
      const rc = counts.get(v.rule) ?? { rule: v.rule, count: 0, errorCount: 0 };
      rc.count += 1;
      if (v.severity === 'error') rc.errorCount += 1;
      counts.set(v.rule, rc);
    }
    const ruleList: RuleCount[] = Array.from(counts.values()).sort(
      (a, b) => b.errorCount - a.errorCount || b.count - a.count || a.rule.localeCompare(b.rule),
    );
    const byViolation = new Map<number, LintAdvice[]>();
    for (const a of ads) {
      const list = byViolation.get(a.violation_index) ?? [];
      list.push(a);
      byViolation.set(a.violation_index, list);
    }
    return {
      violations: vs,
      advices: ads,
      adviceByViolation: byViolation,
      ruleCounts: ruleList,
      errorCount: errors,
      warningCount: warnings,
    };
  }, [result]);

  const visible = useMemo(() => {
    const indexed: IndexedViolation[] = violations.map((violation, index) => ({ violation, index }));
    const filtered = indexed.filter(({ violation }) => {
      if (severityFilter !== 'all' && violation.severity !== severityFilter) return false;
      if (ruleFilter && violation.rule !== ruleFilter) return false;
      return true;
    });
    const rank = (s: LintViolation['severity']) => (s === 'error' ? 0 : 1);
    return filtered.sort(
      (a, b) => rank(a.violation.severity) - rank(b.violation.severity) || a.index - b.index,
    );
  }, [violations, severityFilter, ruleFilter]);

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

  const total = violations.length;
  const toggleAdvice = (i: number) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  };

  return (
    <div className="lint-viewer-root">
      <div className="lint-header">
        <span className={`lint-status ${result.acceptable ? 'lint-status--ok' : 'lint-status--bad'}`}>
          {result.acceptable ? <IconCheck size={13} /> : <IconError size={13} />}
          {result.acceptable ? '布局可接受' : '需要处理'}
        </span>
        <div className="lint-seg" role="group" aria-label="按严重度筛选">
          <button
            type="button"
            className={`lint-seg-btn ${severityFilter === 'all' ? 'active' : ''}`}
            onClick={() => setSeverityFilter('all')}
          >
            全部 <span className="lint-seg-count">{total}</span>
          </button>
          <button
            type="button"
            className={`lint-seg-btn lint-seg-btn--error ${severityFilter === 'error' ? 'active' : ''}`}
            onClick={() => setSeverityFilter(severityFilter === 'error' ? 'all' : 'error')}
            disabled={errorCount === 0}
            title="仅看 Error"
          >
            <IconError size={12} /> {errorCount}
          </button>
          <button
            type="button"
            className={`lint-seg-btn lint-seg-btn--warn ${severityFilter === 'warning' ? 'active' : ''}`}
            onClick={() => setSeverityFilter(severityFilter === 'warning' ? 'all' : 'warning')}
            disabled={warningCount === 0}
            title="仅看 Warning"
          >
            <IconWarning size={12} /> {warningCount}
          </button>
        </div>
        {advices.length > 0 && <span className="lint-advice-count">{advices.length} 条建议</span>}
      </div>

      {ruleCounts.length > 0 && (
        <div className="lint-rule-chips">
          {ruleCounts.map(({ rule, count, errorCount: ruleErrors }) => (
            <button
              key={rule}
              type="button"
              className={`lint-rule-chip ${ruleFilter === rule ? 'active' : ''} ${ruleErrors > 0 ? 'lint-rule-chip--has-error' : ''}`}
              title={ruleFilter === rule ? '取消筛选' : `仅看 ${rule}`}
              onClick={() => setRuleFilter(ruleFilter === rule ? null : rule)}
            >
              {rule} <span className="lint-rule-chip-count">{count}</span>
            </button>
          ))}
          {ruleFilter && (
            <button
              type="button"
              className="lint-rule-chip lint-rule-chip--clear"
              onClick={() => setRuleFilter(null)}
            >
              <IconClose size={11} /> 清除
            </button>
          )}
        </div>
      )}

      {total === 0 ? (
        <div className="lint-empty">
          <IconCheck size={30} />
          <p className="lint-empty-title">没有布局违规</p>
          <p className="lint-empty-sub">当前图形通过了所有布局检查</p>
        </div>
      ) : visible.length === 0 ? (
        <div className="empty-hint">没有符合当前筛选条件的违规</div>
      ) : (
        <ul className="lint-list">
          {visible.map(({ violation: v, index: i }) => {
            const sev = v.severity;
            const metric = metricLabel(v);
            const related = adviceByViolation.get(i) ?? [];
            const chips = metaChips(v);
            const isOpen = expanded.has(i);
            return (
              <li key={`${v.rule}-${i}`} className={`lint-item lint-item--${sev}`}>
                <div className="lint-item-header">
                  {sev === 'error' ? <IconError size={14} /> : <IconWarning size={14} />}
                  <button
                    type="button"
                    className="lint-item-rule"
                    onClick={() => setRuleFilter(ruleFilter === v.rule ? null : v.rule)}
                    title="按此规则筛选"
                  >
                    {v.rule}
                  </button>
                  <span className="lint-item-header-spacer" />
                  {v.edge_index != null && <span className="lint-tag lint-tag--loc">edge #{v.edge_index}</span>}
                  {metric && <span className="lint-tag lint-tag--metric">{metric}</span>}
                </div>

                <div className="lint-item-message">{v.message}</div>

                {chips.length > 0 && (
                  <div className="lint-item-meta-row">
                    {chips.map((m) => (
                      <span key={m.label} className="lint-meta-chip" title={`${m.label}: ${m.value}`}>
                        <span className="lint-meta-chip-label">{m.label}</span>
                        <span className="lint-meta-chip-value">{m.value}</span>
                      </span>
                    ))}
                  </div>
                )}

                {related.length > 0 && (
                  <>
                    <button type="button" className="lint-advice-toggle" onClick={() => toggleAdvice(i)}>
                      <IconChevron size={12} className={isOpen ? 'open' : ''} />
                      {related.length} 条修复建议
                    </button>
                    {isOpen && (
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
                  </>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
