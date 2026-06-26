import { useMemo, useState, useEffect } from 'react';
import { IconClose } from './Icons';
import {
  EXAMPLES,
  CATEGORY_LABELS,
  KIND_LABELS,
  type ExampleCategory,
  type DiagramKind,
} from '../data/examples';

interface ExampleDrawerProps {
  open: boolean;
  activeId: string;
  onSelect: (id: string) => void;
  onClose: () => void;
}

const CATEGORY_ORDER: ExampleCategory[] = ['basic', 'scenario', 'stress'];

type FilterKind = 'all' | DiagramKind;
const KIND_FILTERS: { value: FilterKind; label: string }[] = [
  { value: 'all', label: '全部' },
  { value: 'flowchart', label: '流程图' },
  { value: 'sequence', label: '时序图' },
  { value: 'architecture', label: '架构图' },
  { value: 'state', label: '状态图' },
  { value: 'er', label: 'ER 图' },
  { value: 'mindmap', label: '思维导图' },
];

export function ExampleDrawer({ open, activeId, onSelect, onClose }: ExampleDrawerProps) {
  const [query, setQuery] = useState('');
  const [kindFilter, setKindFilter] = useState<FilterKind>('all');

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  // 重置搜索状态
  useEffect(() => {
    if (open) {
      setQuery('');
      setKindFilter('all');
    }
  }, [open]);

  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = EXAMPLES.filter((e) => {
      const matchesQuery = !q ||
        e.title.toLowerCase().includes(q) ||
        e.description.toLowerCase().includes(q) ||
        KIND_LABELS[e.kind].includes(q);
      const matchesKind = kindFilter === 'all' || e.kind === kindFilter;
      return matchesQuery && matchesKind;
    });
    return CATEGORY_ORDER.map((cat) => ({
      category: cat,
      items: filtered.filter((e) => e.category === cat),
    })).filter((g) => g.items.length > 0);
  }, [query, kindFilter]);

  if (!open) return null;

  return (
    <>
      {/* 遮罩 */}
      <div className="example-drawer-backdrop" onClick={onClose} />
      {/* 抽屉 */}
      <div className="example-drawer">
        <div className="example-drawer-head">
          <h3>示例库</h3>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="关闭">
            <IconClose />
          </button>
        </div>

        <div className="example-drawer-search">
          <input
            className="search-input"
            type="search"
            placeholder="搜索示例…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            autoFocus
          />
        </div>

        <div className="example-drawer-filters">
          {KIND_FILTERS.map(({ value, label }) => (
            <button
              key={value}
              type="button"
              className={`filter-chip ${kindFilter === value ? 'active' : ''}`}
              onClick={() => setKindFilter(value)}
            >
              {label}
            </button>
          ))}
        </div>

        <div className="example-drawer-body">
          {grouped.length === 0 && <div className="empty-hint">没有匹配的示例</div>}
          {grouped.map(({ category, items }) => (
            <section key={category} className="example-drawer-group">
              <h4 className="example-drawer-group-title">{CATEGORY_LABELS[category]}</h4>
              {items.map((ex) => (
                <button
                  key={ex.id}
                  type="button"
                  className={`example-drawer-card ${ex.id === activeId ? 'active' : ''}`}
                  onClick={() => {
                    onSelect(ex.id);
                    onClose();
                  }}
                >
                  <div className="example-drawer-thumb">
                    <span style={{ fontSize: 10, color: 'var(--text-faint)' }}>
                      {KIND_LABELS[ex.kind].slice(0, 2)}
                    </span>
                  </div>
                  <div className="example-drawer-info">
                    <div className="example-drawer-info-title">{ex.title}</div>
                    <div className="example-drawer-info-desc">{ex.description}</div>
                  </div>
                </button>
              ))}
            </section>
          ))}
        </div>
      </div>
    </>
  );
}
