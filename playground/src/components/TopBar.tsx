import { useEffect, useRef, useState } from 'react';
import {
  IconGallery,
  IconHelp,
  IconShare,
  IconDownload,
  IconSun,
  IconMoon,
  IconChevron,
  IconFile,
  IconFolderOpen,
  IconSave,
  IconCommandPalette,
} from './Icons';

export interface ExportActions {
  downloadSvg: () => void;
  downloadPng: () => void;
  downloadWebp: () => void;
  downloadAscii: () => void;
  downloadJson: () => void;
  downloadDrawio: () => void;
  copySvg: () => void;
  copyPng: () => void;
  copyAscii: () => void;
  copyJson: () => void;
  copyDrawio: () => void;
  openInDrawio: () => void;
}

interface TopBarProps {
  theme: 'light' | 'dark';
  version: string | null;
  filename: string;
  dirty: boolean;
  canExport: boolean;
  renderStatus: 'idle' | 'rendering' | 'success' | 'error' | 'warning';
  errorCount: number;
  warningCount: number;
  renderMs: number | null;
  onOpenExamples: () => void;
  onOpenDocs: () => void;
  onToggleTheme: () => void;
  onShare: () => void;
  onNewFile: () => void;
  onOpenFile: () => void;
  onSaveFile: () => void;
  onSaveAsFile: () => void;
  onOpenCommandPalette: () => void;
  exportActions: ExportActions;
  rasterScale: number;
  onRasterScaleChange: (scale: number) => void;
}

function useClickOutside(ref: React.RefObject<HTMLElement | null>, open: boolean, onClose: () => void) {
  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose();
      }
    };
    window.addEventListener('mousedown', onClick);
    return () => window.removeEventListener('mousedown', onClick);
  }, [ref, open, onClose]);
}

function StatusLight({ status, errorCount, warningCount, renderMs }: {
  status: TopBarProps['renderStatus'];
  errorCount: number;
  warningCount: number;
  renderMs: number | null;
}) {
  const [showSuccess, setShowSuccess] = useState(false);

  useEffect(() => {
    if (status === 'success') {
      setShowSuccess(true);
      const timer = setTimeout(() => setShowSuccess(false), 2000);
      return () => clearTimeout(timer);
    }
  }, [status]);

  let className = 'status-light';
  let text: string;

  switch (status) {
    case 'rendering':
      className += ' rendering';
      text = '渲染中…';
      break;
    case 'error':
      className += ' error';
      text = `${errorCount} 个错误`;
      break;
    case 'warning':
      className += ' warning';
      text = `${warningCount} 个警告`;
      break;
    case 'success':
      className += ' success';
      text = renderMs != null ? `渲染成功 ${renderMs.toFixed(1)}ms` : '渲染成功';
      break;
    default:
      className += ' idle';
      text = '就绪';
  }

  if (status === 'success' && !showSuccess) {
    className = 'status-light idle';
    text = '就绪';
  }

  return (
    <span className="status-indicator">
      <span className={className} />
      <span className="status-text">{text}</span>
    </span>
  );
}

export function TopBar({
  theme,
  filename,
  dirty,
  canExport,
  renderStatus,
  errorCount,
  warningCount,
  renderMs,
  onOpenExamples,
  onOpenDocs,
  onToggleTheme,
  onShare,
  onNewFile,
  onOpenFile,
  onSaveFile,
  onSaveAsFile,
  onOpenCommandPalette,
  exportActions,
  rasterScale,
  onRasterScaleChange,
}: TopBarProps) {
  const [fileMenuOpen, setFileMenuOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const fileMenuRef = useRef<HTMLDivElement>(null);
  const exportRef = useRef<HTMLDivElement>(null);

  useClickOutside(fileMenuRef, fileMenuOpen, () => setFileMenuOpen(false));
  useClickOutside(exportRef, exportOpen, () => setExportOpen(false));

  const runFileAction = (fn: () => void) => {
    fn();
    setFileMenuOpen(false);
  };

  const runExport = (fn: () => void) => {
    fn();
    setExportOpen(false);
  };

  return (
    <header className="topbar">
      <div className="topbar-left">
        <div className="brand">
          <img className="brand-logo" src={theme === 'dark' ? '/logo-dark.svg' : '/logo.svg'} alt="Drawify" />
        </div>

        <div className="file-menu-wrap" ref={fileMenuRef}>
          <button
            type="button"
            className="file-menu-trigger"
            onClick={() => setFileMenuOpen((o) => !o)}
          >
            <IconFile size={14} />
            <span>{dirty ? `${filename} •` : filename}</span>
            <IconChevron size={12} className={fileMenuOpen ? 'flip' : ''} />
          </button>
          {fileMenuOpen && (
            <div className="file-menu">
              <button type="button" onClick={() => runFileAction(onNewFile)}>
                <IconFile size={14} />
                <span>新建</span>
              </button>
              <button type="button" onClick={() => runFileAction(onOpenFile)}>
                <IconFolderOpen size={14} />
                <span>打开 .dfy…</span>
              </button>
              <button type="button" onClick={() => runFileAction(onSaveFile)}>
                <IconSave size={14} />
                <span>保存</span>
                <span className="menu-shortcut">⌘S</span>
              </button>
              <button type="button" onClick={() => runFileAction(onSaveAsFile)}>
                <IconSave size={14} />
                <span>另存为…</span>
              </button>
            </div>
          )}
        </div>

        <span className="topbar-divider" />

        <button type="button" className="btn btn-ghost" onClick={onOpenExamples}>
          <IconGallery />
          <span>示例库</span>
        </button>

        <button type="button" className="btn btn-ghost" onClick={onOpenDocs}>
          <IconHelp />
          <span>文档</span>
        </button>
      </div>

      <div className="topbar-center">
        <StatusLight
          status={renderStatus}
          errorCount={errorCount}
          warningCount={warningCount}
          renderMs={renderMs}
        />
      </div>

      <div className="topbar-right">
        <div className="export-wrap" ref={exportRef}>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => setExportOpen((o) => !o)}
            disabled={!canExport}
          >
            <IconDownload />
            <span>导出</span>
            <IconChevron size={14} className={exportOpen ? 'flip' : ''} />
          </button>
          {exportOpen && (
            <div className="export-menu">
              <button type="button" onClick={() => runExport(exportActions.downloadSvg)}>
                下载 SVG
              </button>
              <div className="png-scale-row">
                <button type="button" onClick={() => runExport(exportActions.downloadPng)}>
                  下载 PNG
                </button>
                <span className="png-scale-selector" title="PNG / WebP 导出倍率">
                  {[1, 2, 3].map((s) => (
                    <button
                      key={s}
                      type="button"
                      className={`scale-btn${rasterScale === s ? ' active' : ''}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        onRasterScaleChange(s);
                      }}
                    >
                      {s}x
                    </button>
                  ))}
                </span>
              </div>
              <button type="button" onClick={() => runExport(exportActions.downloadWebp)}>
                下载 WebP
                <span className="export-menu-meta">{rasterScale}x</span>
              </button>
              <div className="menu-divider" />
              <button type="button" onClick={() => runExport(exportActions.downloadAscii)}>
                下载 ASCII
              </button>
              <button type="button" onClick={() => runExport(exportActions.downloadJson)}>
                下载 Scene JSON
              </button>
              <div className="menu-divider" />
              <button type="button" onClick={() => runExport(exportActions.downloadDrawio)}>
                下载 Drawio
              </button>
              <button type="button" onClick={() => runExport(exportActions.openInDrawio)}>
                在 draw.io 中打开
              </button>
              <div className="menu-divider" />
              <button type="button" onClick={() => runExport(exportActions.copySvg)}>
                复制 SVG 源码
              </button>
              <button type="button" onClick={() => runExport(exportActions.copyPng)}>
                复制 PNG 到剪贴板
              </button>
              <button type="button" onClick={() => runExport(exportActions.copyAscii)}>
                复制 ASCII 文本
              </button>
              <button type="button" onClick={() => runExport(exportActions.copyJson)}>
                复制 Scene JSON
              </button>
              <button type="button" onClick={() => runExport(exportActions.copyDrawio)}>
                复制 Drawio XML
              </button>
            </div>
          )}
        </div>

        <button type="button" className="btn btn-ghost" onClick={onShare} disabled={!canExport}>
          <IconShare />
          <span>分享</span>
        </button>

        <button
          type="button"
          className="icon-btn theme-toggle"
          onClick={onToggleTheme}
          title={theme === 'dark' ? '切换到浅色' : '切换到深色'}
        >
          {theme === 'dark' ? <IconSun /> : <IconMoon />}
        </button>

        <button
          type="button"
          className="icon-btn"
          onClick={onOpenCommandPalette}
          title="命令面板"
        >
          <IconCommandPalette />
        </button>
      </div>
    </header>
  );
}
