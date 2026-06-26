import { useMemo, useState, useEffect } from 'react';
import { IconClose } from './Icons';
import {
  EXAMPLES,
  CATEGORY_LABELS,
  KIND_LABELS,
  type ExampleCategory,
} from '../data/examples';

interface ExampleDialogProps {
  open: boolean;
  activeId: string;
  onSelect: (id: string) => void;
  onClose: () => void;
}

const CATEGORY_ORDER: ExampleCategory[] = ['basic', 'scenario', 'stress'];

export function ExampleDialog({ open, activeId, onSelect, onClose }: ExampleDialogProps) {
  const [query, setQuery] = useState('');

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = EXAMPLES.filter(
      (e) =>
        !q ||
        e.title.toLowerCase().includes(q) ||
        e.description.toLowerCase().includes(q) ||
        KIND_LABELS[e.kind].includes(q),
    );
    return CATEGORY_ORDER.map((cat) => ({
      category: cat,
      items: filtered.filter((e) => e.category === cat),
    })).filter((g) => g.items.length > 0);
  }, [query]);

  if (!open) return null;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal example-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>示例库</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="关闭">
            <IconClose />
          </button>
        </div>

        <div className="modal-toolbar">
          <input
            className="search-input"
            type="search"
            placeholder="搜索示例（标题 / 描述 / 类型）…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            autoFocus
          />
        </div>

        <div className="modal-body">
          {grouped.length === 0 && <div className="empty-hint">没有匹配的示例</div>}
          {grouped.map(({ category, items }) => (
            <section key={category} className="example-group">
              <h3 className="example-group-title">{CATEGORY_LABELS[category]}</h3>
              <div className="example-grid">
                {items.map((ex) => (
                  <button
                    key={ex.id}
                    type="button"
                    className={`example-card ${ex.id === activeId ? 'active' : ''}`}
                    onClick={() => {
                      onSelect(ex.id);
                      onClose();
                    }}
                  >
                    <span className="example-kind">{KIND_LABELS[ex.kind]}</span>
                    <span className="example-title">{ex.title}</span>
                    <span className="example-desc">{ex.description}</span>
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
