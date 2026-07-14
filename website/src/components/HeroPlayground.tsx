import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Compartment, EditorState } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { useWasm } from '../hooks/useWasm';
import { renderSvg, renderMdOutlineSvg, type DiagnosticErrorJson } from '../lib/wasm';
import { plotgram } from '../lib/plotgramLang';
import { markdownOutline } from '../lib/markdownOutlineLang';

const DEFAULT_SOURCE = `diagram architecture {
    title: "三层架构"

    entity[frontend] client "客户端" {
        semantic: browser
    }
    entity[service] api "API 服务"
    entity[database] db "数据库" {
        semantic: postgres
    }

    client -> api "HTTP 请求"
    api -> db "SQL 查询"
    db --> api "查询结果"
    api --> client "JSON 响应"
}`;

interface Preset {
  label: string;
  source: string;
  /** 输入语法：plotgram DSL 或 markdown 大纲（仅 mindmap 支持）。 */
  inputMode?: 'dfy' | 'md-outline';
}

const MINDMAP_OUTLINE_SOURCE = `# AI 学习路线

## 基础

### 数学
### Python

## 机器学习

### 监督学习
### 无监督学习

## 深度学习

### Transformer
### CNN`;

const PRESETS: Preset[] = [
  { label: '架构图', source: DEFAULT_SOURCE },
  {
    label: '流程图',
    source: `diagram flowchart {
    title: "用户认证流程"
    config {
        direction: top-to-bottom
    }

    entity[client] client "移动客户端"
    entity[gateway] gateway "API 网关" {
        status: healthy
    }
    entity[service] auth "认证服务" {
        owner: "安全团队"
    }
    entity[database] db "用户数据库"
    entity[cache] cache "Token 缓存"

    client -> gateway "HTTPS 请求"
    gateway -> auth "转发认证请求"
    auth -> db "查询用户信息"
    db --> auth "返回用户记录"
    auth -> cache "存储 Token"
    cache --> auth "返回缓存结果"
    auth --> gateway "认证结果"
    gateway --> client "响应"
}`,
  },
  {
    label: '思维导图',
    source: `diagram mindmap {
  title: "AI 学习路线"

  entity[root] ai "AI 学习"
  entity[main] base "基础"
  entity[leaf] math "数学"
  entity[leaf] python "Python"
  entity[main] ml "机器学习"
  entity[leaf] supervised "监督学习"
  entity[main] dl "深度学习"
  entity[leaf] transformer "Transformer"

  ai -> base
  base -> math
  base -> python
  ai -> ml
  ml -> supervised
  ai -> dl
  dl -> transformer
}`,
  },
  {
    label: '思维导图(大纲)',
    inputMode: 'md-outline',
    source: MINDMAP_OUTLINE_SOURCE,
  },
  {
    label: '时序图',
    source: `diagram sequence {
  title: "登录流程"

  entity[actor] user "用户"
  entity[boundary] client "Client"
  entity[control] server "Server"
  entity[database] db "Database"

  user -> client "点击登录"
  client -> server "POST /login"
  server -> db "query user"
  db --> server "user record"
  server --> client "200 OK + token"
}`,
  },
];

const MIN_SCALE = 0.1;
const MAX_SCALE = 8;
const PAN_MARGIN = 0.3; // 至少保留 30% 的图在可视区域内

function clamp(v: number, min: number, max: number) {
  return Math.max(min, Math.min(max, v));
}

function formatErrors(errors: DiagnosticErrorJson[]): string {
  if (errors.length === 0) return '';
  const first = errors[0];
  const loc = first.location?.start;
  const locStr = loc ? `[${loc.line}:${loc.column}] ` : '';
  return `${locStr}${first.message}`;
}

const plotgramHighlightStyle = HighlightStyle.define([
  { tag: t.keyword, color: '#c678dd', fontWeight: '600' },
  { tag: t.typeName, color: '#56b6c2' },
  { tag: t.string, color: '#98c379' },
  { tag: t.number, color: '#d19a66' },
  { tag: t.lineComment, color: '#5c6370', fontStyle: 'italic' },
  { tag: t.operator, color: '#56b6c2', fontWeight: '600' },
  { tag: t.propertyName, color: '#d19a66' },
  { tag: t.atom, color: '#e06c75' },
  { tag: t.bracket, color: '#abb2bf' },
  { tag: t.punctuation, color: '#abb2bf' },
  { tag: t.variableName, color: '#61afef' },
]);

const markdownOutlineHighlightStyle = HighlightStyle.define([
  { tag: t.heading1, color: '#c678dd', fontWeight: '700' },
  { tag: t.heading2, color: '#c678dd', fontWeight: '700' },
  { tag: t.heading3, color: '#d19a66', fontWeight: '600' },
  { tag: t.heading4, color: '#d19a66', fontWeight: '600' },
  { tag: t.heading5, color: '#61afef', fontWeight: '600' },
  { tag: t.heading6, color: '#61afef', fontWeight: '600' },
  { tag: t.comment, color: '#5c6370', fontStyle: 'italic' },
  { tag: t.list, color: '#56b6c2', fontWeight: '600' },
  { tag: t.string, color: '#98c379' },
  { tag: t.strong, color: '#e06c75', fontWeight: '700' },
  { tag: t.emphasis, color: '#e06c75', fontStyle: 'italic' },
  { tag: t.url, color: '#61afef' },
]);

const editorTheme = EditorView.theme({
  '&': {
    height: '100%',
    fontSize: '13.5px',
    backgroundColor: 'transparent',
    color: '#abb2bf',
  },
  '.cm-scroller': {
    fontFamily: "'JetBrains Mono', 'SF Mono', 'Monaco', 'Menlo', monospace",
    lineHeight: '1.65',
    overflow: 'auto',
  },
  '.cm-content': {
    padding: '18px 20px',
    caretColor: '#c678dd',
  },
  '.cm-line': {
    padding: '0 2px',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '.cm-cursor': {
    borderLeftColor: '#c678dd',
    borderLeftWidth: '2px',
  },
  '.cm-selectionBackground, ::selection': {
    background: 'rgba(124, 58, 237, 0.25)',
  },
  '.cm-matchingBracket': {
    backgroundColor: 'rgba(124, 58, 237, 0.2)',
    borderRadius: '3px',
  },
  '.cm-gutters': {
    display: 'none',
  },
});

export default function HeroPlayground() {
  const { wasm, ready, error: wasmError } = useWasm();
  const [source, setSource] = useState(DEFAULT_SOURCE);
  const [svg, setSvg] = useState<string>('');
  const [renderError, setRenderError] = useState<string>('');
  const [renderMs, setRenderMs] = useState<number | null>(null);
  const [activePreset, setActivePreset] = useState(0);
  const [inputMode, setInputMode] = useState<'dfy' | 'md-outline'>('dfy');

  const editorHostRef = useRef<HTMLDivElement>(null);
  const editorViewRef = useRef<EditorView | null>(null);
  const languageCompartmentRef = useRef(new Compartment());
  const highlightCompartmentRef = useRef(new Compartment());
  const sourceRef = useRef(source);
  sourceRef.current = source;
  const inputModeRef = useRef(inputMode);
  inputModeRef.current = inputMode;

  // 预览缩放/平移
  const containerRef = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const [tx, setTx] = useState(0);
  const [ty, setTy] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const scaleRef = useRef(scale);
  scaleRef.current = scale;
  const txRef = useRef(tx);
  txRef.current = tx;
  const tyRef = useRef(ty);
  tyRef.current = ty;
  const dragStateRef = useRef<{ startX: number; startY: number; startTx: number; startTy: number } | null>(null);
  const svgNaturalSizeRef = useRef<{ w: number; h: number }>({ w: 0, h: 0 });

  // 读取 SVG 自然尺寸
  const updateSvgNaturalSize = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;
    const svgEl = container.querySelector('svg');
    if (!svgEl) return;
    const viewBox = svgEl.viewBox.baseVal;
    if (viewBox && viewBox.width > 0 && viewBox.height > 0) {
      svgNaturalSizeRef.current = { w: viewBox.width, h: viewBox.height };
    } else {
      const bbox = svgEl.getBBox();
      svgNaturalSizeRef.current = { w: bbox.width, h: bbox.height };
    }
  }, []);

  // 限制 tx/ty，确保至少 PAN_MARGIN 比例的图在可视区域内
  const clampPan = useCallback((newTx: number, newTy: number, s: number) => {
    const container = containerRef.current;
    if (!container) return { tx: newTx, ty: newTy };
    const { w: nw, h: nh } = svgNaturalSizeRef.current;
    if (nw === 0 || nh === 0) return { tx: newTx, ty: newTy };
    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const sw = nw * s;
    const sh = nh * s;
    const margin = PAN_MARGIN;
    const minTx = -(sw - cw * margin);
    const maxTx = cw * margin;
    const minTy = -(sh - ch * margin);
    const maxTy = ch * margin;
    return {
      tx: clamp(newTx, Math.min(minTx, maxTx), Math.max(minTx, maxTx)),
      ty: clamp(newTy, Math.min(minTy, maxTy), Math.max(minTy, maxTy)),
    };
  }, []);

  useEffect(() => {
    if (!editorHostRef.current) return;
    const initialMode = inputModeRef.current;
    const state = EditorState.create({
      doc: sourceRef.current,
      extensions: [
        history(),
        bracketMatching(),
        languageCompartmentRef.current.of(
          initialMode === 'md-outline' ? markdownOutline() : plotgram(),
        ),
        highlightCompartmentRef.current.of(
          syntaxHighlighting(
            initialMode === 'md-outline' ? markdownOutlineHighlightStyle : plotgramHighlightStyle,
          ),
        ),
        editorTheme,
        EditorView.lineWrapping,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) setSource(u.state.doc.toString());
        }),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
      ],
    });
    const view = new EditorView({ state, parent: editorHostRef.current });
    editorViewRef.current = view;
    return () => {
      view.destroy();
      editorViewRef.current = null;
    };
  }, []);

  // 切换 inputMode 时，热替换编辑器语言扩展
  useEffect(() => {
    const view = editorViewRef.current;
    if (!view) return;
    view.dispatch({
      effects: [
        languageCompartmentRef.current.reconfigure(
          inputMode === 'md-outline' ? markdownOutline() : plotgram(),
        ),
        highlightCompartmentRef.current.reconfigure(
          syntaxHighlighting(
            inputMode === 'md-outline' ? markdownOutlineHighlightStyle : plotgramHighlightStyle,
          ),
        ),
      ],
    });
  }, [inputMode]);

  useEffect(() => {
    const view = editorViewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== source) {
      view.dispatch({ changes: { from: 0, to: current.length, insert: source } });
    }
  }, [source]);

  useEffect(() => {
    if (!wasm) return;

    const timer = setTimeout(() => {
      const t0 = performance.now();
      const result =
        inputMode === 'md-outline'
          ? renderMdOutlineSvg(wasm, source, { transparent_background: true })
          : renderSvg(wasm, source, { transparent_background: true });
      const elapsed = performance.now() - t0;
      if (result.success && result.text) {
        setSvg(result.text);
        setRenderError('');
        setRenderMs(elapsed);
      } else {
        setRenderError(formatErrors(result.errors));
        setRenderMs(elapsed);
      }
    }, 150);

    return () => clearTimeout(timer);
  }, [source, wasm, inputMode]);

  // 缩放/平移逻辑（与 AGENT 预览区行为一致）
  const zoomAt = useCallback((centerX: number, centerY: number, factor: number) => {
    setScale((prevScale) => {
      const nextScale = clamp(prevScale * factor, MIN_SCALE, MAX_SCALE);
      if (nextScale === prevScale) return prevScale;
      const nextTx = centerX - (centerX - txRef.current) * (nextScale / prevScale);
      const nextTy = centerY - (centerY - tyRef.current) * (nextScale / prevScale);
      const clamped = clampPan(nextTx, nextTy, nextScale);
      setTx(clamped.tx);
      setTy(clamped.ty);
      return nextScale;
    });
  }, [clampPan]);

  const fitToView = useCallback(() => {
    const container = containerRef.current;
    if (!container || !svg) return;
    const svgEl = container.querySelector('svg');
    if (!svgEl) return;
    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const viewBox = svgEl.viewBox.baseVal;
    let naturalW: number;
    let naturalH: number;
    if (viewBox && viewBox.width > 0 && viewBox.height > 0) {
      naturalW = viewBox.width;
      naturalH = viewBox.height;
    } else {
      const bbox = svgEl.getBoundingClientRect();
      const curScale = scaleRef.current || 1;
      naturalW = bbox.width / curScale;
      naturalH = bbox.height / curScale;
    }
    if (naturalW === 0 || naturalH === 0) return;
    const padding = 32;
    const nextScale = clamp(
      Math.min((cw - padding) / naturalW, (ch - padding) / naturalH),
      MIN_SCALE,
      MAX_SCALE,
    );
    setScale(nextScale);
    setTx((cw - naturalW * nextScale) / 2);
    setTy((ch - naturalH * nextScale) / 2);
  }, [svg]);

  const resetTo100 = useCallback(() => {
    const container = containerRef.current;
    if (!container || !svg) return;
    const svgEl = container.querySelector('svg');
    if (!svgEl) return;
    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const viewBox = svgEl.viewBox.baseVal;
    let naturalW: number;
    let naturalH: number;
    if (viewBox && viewBox.width > 0 && viewBox.height > 0) {
      naturalW = viewBox.width;
      naturalH = viewBox.height;
    } else {
      const bbox = svgEl.getBoundingClientRect();
      naturalW = bbox.width / (scaleRef.current || 1);
      naturalH = bbox.height / (scaleRef.current || 1);
    }
    if (naturalW === 0 || naturalH === 0) return;
    setScale(1);
    setTx((cw - naturalW) / 2);
    setTy((ch - naturalH) / 2);
  }, [svg]);

  const zoomByButton = useCallback(
    (factor: number) => {
      const container = containerRef.current;
      if (!container) return;
      zoomAt(container.clientWidth / 2, container.clientHeight / 2, factor);
    },
    [zoomAt],
  );

  // SVG 变化时自适应并更新自然尺寸
  useEffect(() => {
    if (svg) {
      requestAnimationFrame(() => {
        updateSvgNaturalSize();
        fitToView();
      });
    }
  }, [svg, fitToView, updateSvgNaturalSize]);

  // 滚轮：ctrlKey=缩放，普通滚动=平移
  // 始终阻止预览区内的 pinch zoom 冒泡到浏览器，避免整页缩放
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const listener = (e: WheelEvent) => {
      // ctrlKey(pinch zoom) 始终阻止默认行为,避免浏览器缩放整页
      if (e.ctrlKey) e.preventDefault();
      if (!svg) return;
      if (!e.ctrlKey) e.preventDefault();
      if (e.ctrlKey) {
        const rect = container.getBoundingClientRect();
        const cx = e.clientX - rect.left;
        const cy = e.clientY - rect.top;
        const factor = Math.exp(-e.deltaY * 0.01);
        zoomAt(cx, cy, factor);
      } else {
        setTx((prev) => {
          const next = prev - e.deltaX;
          return clampPan(next, tyRef.current, scaleRef.current).tx;
        });
        setTy((prev) => {
          const next = prev - e.deltaY;
          return clampPan(txRef.current, next, scaleRef.current).ty;
        });
      }
    };
    // Safari 的 pinch 通过 gesturestart/gesturechange 触发
    const gestureHandler = (e: Event) => e.preventDefault();
    container.addEventListener('wheel', listener, { passive: false });
    container.addEventListener('gesturestart', gestureHandler, { passive: false });
    container.addEventListener('gesturechange', gestureHandler, { passive: false });
    return () => {
      container.removeEventListener('wheel', listener);
      container.removeEventListener('gesturestart', gestureHandler);
      container.removeEventListener('gesturechange', gestureHandler);
    };
  }, [svg, zoomAt, clampPan]);

  // 拖拽平移
  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!svg) return;
      if (e.button !== 0) return;
      dragStateRef.current = {
        startX: e.clientX,
        startY: e.clientY,
        startTx: txRef.current,
        startTy: tyRef.current,
      };
      setIsDragging(true);
    },
    [svg],
  );

  useEffect(() => {
    if (!isDragging) return;
    const onMove = (e: MouseEvent) => {
      const s = dragStateRef.current;
      if (!s) return;
      const nextTx = s.startTx + (e.clientX - s.startX);
      const nextTy = s.startTy + (e.clientY - s.startY);
      const clamped = clampPan(nextTx, nextTy, scaleRef.current);
      setTx(clamped.tx);
      setTy(clamped.ty);
    };
    const onUp = () => {
      dragStateRef.current = null;
      setIsDragging(false);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [isDragging, clampPan]);

  const status = useMemo(() => {
    if (wasmError) return { kind: 'error' as const, text: `WASM 加载失败：${wasmError}` };
    if (!ready) return { kind: 'loading' as const, text: '正在加载渲染引擎…' };
    if (renderError) return { kind: 'dsl-error' as const, text: renderError };
    return { kind: 'ok' as const, text: '实时渲染 · 修改代码即时预览' };
  }, [wasmError, ready, renderError]);

  return (
    <div className="hero-visual">
      <div className="hero-visual-header">
        <div className="hero-window-dots">
          <span className="hero-visual-dot red" />
          <span className="hero-visual-dot yellow" />
          <span className="hero-visual-dot green" />
        </div>
        <span className="hero-playground-title">plotgram demo</span>
        <div className="hero-playground-presets">
          {PRESETS.map((p, i) => (
            <button
              key={p.label}
              className={`hero-preset-btn ${i === activePreset ? 'active' : ''}`}
              onClick={() => {
                setActivePreset(i);
                setSource(p.source);
                setInputMode(p.inputMode ?? 'dfy');
              }}
            >
              {p.label}
            </button>
          ))}
        </div>
      </div>
      <div className="hero-visual-body">
        <div className="hero-editor-wrap">
          <div className="hero-editor" ref={editorHostRef} />
        </div>
        <div className="hero-preview">
          <div
            ref={containerRef}
            className="hero-preview-canvas"
            onMouseDown={handleMouseDown}
            style={{
              cursor: isDragging ? 'grabbing' : svg ? 'grab' : 'default',
            }}
          >
            {svg ? (
              <div
                className="hero-svg-host"
                style={{ transform: `translate(${tx}px, ${ty}px) scale(${scale})` }}
                dangerouslySetInnerHTML={{ __html: svg }}
              />
            ) : (
              <div className="hero-preview-placeholder">
                {status.kind === 'loading' ? (
                  <span className="hero-spinner" />
                ) : status.kind === 'error' ? (
                  <span>渲染引擎加载失败</span>
                ) : (
                  <span>等待渲染…</span>
                )}
              </div>
            )}
            {svg && (
              <div className="hero-preview-toolbar">
                <button onClick={() => zoomByButton(1 / 1.1)} aria-label="缩小">−</button>
                <button onClick={resetTo100} aria-label="重置为 100%">
                  {Math.round(scale * 100)}%
                </button>
                <button onClick={() => zoomByButton(1.1)} aria-label="放大">+</button>
                <button onClick={fitToView} aria-label="适应窗口">适应</button>
              </div>
            )}
          </div>
        </div>
      </div>
      <div className="hero-visual-footer">
        <div className="hero-footer-status">
          {status.kind === 'ok' && <span className="hero-status-dot ok" />}
          {status.kind === 'loading' && <span className="hero-status-dot loading" />}
          {status.kind === 'error' && <span className="hero-status-dot error" />}
          {status.kind === 'dsl-error' && <span className="hero-status-dot error" />}
          <span className={`hero-status-text ${status.kind}`}>{status.text}</span>
        </div>
        {renderMs !== null && status.kind !== 'loading' && (
          <span className="hero-footer-render-time">
            渲染 {renderMs.toFixed(1)} ms
          </span>
        )}
      </div>
    </div>
  );
}
