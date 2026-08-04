import { useState } from 'react';
import { IconError, IconWarning, IconCheck, IconChevron } from './Icons';
import type { Diagnostic } from '../lib/errorParse';

interface DiagnosticsProps {
  diagnostics: Diagnostic[];
  ready: boolean;
  success: boolean;
  renderMs: number | null;
  onGoto: (line: number) => void;
}

export function Diagnostics({ diagnostics, ready, success, renderMs, onGoto }: DiagnosticsProps) {
  const [expanded, setExpanded] = useState(false);

  const errors = diagnostics.filter((d) => d.severity === 'error');
  const warnings = diagnostics.filter((d) => d.severity === 'warning');
  const hasItems = diagnostics.length > 0;

  const statusNode = !ready ? (
    <span className="diag-status diag-loading">加载中…</span>
  ) : errors.length > 0 ? (
    <span className="diag-status diag-error">
      <IconError size={14} /> {errors.length} 个错误
    </span>
  ) : (
    <span className="diag-status diag-ok">
      <IconCheck size={14} /> {success ? '渲染成功' : '已就绪'}
    </span>
  );

  return (
    <div className={`diagnostics ${expanded && hasItems ? 'expanded' : ''}`}>
      <button
        type="button"
        className="diag-bar"
        onClick={() => hasItems && setExpanded((e) => !e)}
        style={{ cursor: hasItems ? 'pointer' : 'default' }}
      >
        {statusNode}

        {warnings.length > 0 && (
          <span className="diag-status diag-warn">
            <IconWarning size={14} /> {warnings.length} 个警告
          </span>
        )}

        {renderMs != null && errors.length === 0 && (
          <span className="diag-meta">{renderMs.toFixed(1)} ms</span>
        )}

        <span className="diag-spacer" />

        {hasItems && (
          <span className="diag-toggle">
            {expanded ? '收起' : '展开详情'}
            <IconChevron size={14} className={expanded ? 'flip' : ''} />
          </span>
        )}
      </button>

      {expanded && hasItems && (
        <ul className="diag-list">
          {diagnostics.map((d, i) => (
            <li
              key={`${d.raw}-${i}`}
              className={`diag-item diag-item-${d.severity}`}
              onClick={() => d.line && onGoto(d.line)}
              style={{ cursor: d.line ? 'pointer' : 'default' }}
            >
              {d.severity === 'error' ? <IconError size={14} /> : <IconWarning size={14} />}
              {d.code && <span className="diag-code">{d.code}</span>}
              {d.line && <span className="diag-loc">L{d.line}{d.column ? `:${d.column}` : ''}</span>}
              <span className="diag-msg">{d.message}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
