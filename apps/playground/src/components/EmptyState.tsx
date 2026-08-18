import { useRef, useState } from 'react';

interface EmptyStateProps {
  onLoadSource: (source: string, filename?: string) => void;
  showcaseHref?: string;
}

export function EmptyState({ onLoadSource, showcaseHref = '/showcase/' }: EmptyStateProps) {
  const [pasteOpen, setPasteOpen] = useState(false);
  const [pasteText, setPasteText] = useState('');
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handlePasteConfirm = () => {
    const text = pasteText.trim();
    if (!text) return;
    onLoadSource(text, '未命名.taut');
    setPasteOpen(false);
    setPasteText('');
  };

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      onLoadSource(text, file.name.endsWith('.taut') ? file.name : `${file.name}.taut`);
    } catch {
      // caller may toast
    }
    e.target.value = '';
  };

  return (
    <div className="empty-state">
      <div className="empty-state-card">
        <h2 className="empty-state-title">Tautcore Editor</h2>
        <p className="empty-state-lead">从 Showcase 选一张图开始调校，或载入你自己的 DSL。</p>

        <div className="empty-state-actions">
          <a className="btn btn-primary empty-state-btn" href={showcaseHref} target="_blank" rel="noopener noreferrer">
            浏览 Showcase 画廊
          </a>
          <button type="button" className="btn btn-soft empty-state-btn" onClick={() => setPasteOpen((o) => !o)}>
            粘贴 DSL
          </button>
          <button type="button" className="btn btn-soft empty-state-btn" onClick={() => fileInputRef.current?.click()}>
            上传 .taut
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".taut,.tautcore,text/plain"
            className="empty-state-file-input"
            onChange={handleFileChange}
          />
        </div>

        {pasteOpen && (
          <div className="empty-state-paste">
            <textarea
              className="empty-state-textarea"
              value={pasteText}
              onChange={(e) => setPasteText(e.target.value)}
              placeholder={'diagram flowchart {\n    ...\n}'}
              rows={8}
              spellCheck={false}
            />
            <div className="empty-state-paste-actions">
              <button type="button" className="btn btn-primary" onClick={handlePasteConfirm} disabled={!pasteText.trim()}>
                载入
              </button>
              <button type="button" className="btn btn-ghost" onClick={() => setPasteOpen(false)}>
                取消
              </button>
            </div>
          </div>
        )}

        <p className="empty-state-hint">右侧 Inspector 可调主题、笔触与布局；展开左侧可编辑 DSL。</p>
      </div>
    </div>
  );
}
