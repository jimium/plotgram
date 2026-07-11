import { useEffect, useMemo, useRef, useState } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { useWasm } from '../hooks/useWasm';
import { renderSvg, type DiagnosticErrorJson } from '../lib/wasm';
import { plotgram } from '../lib/plotgramLang';

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
}

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

  const editorHostRef = useRef<HTMLDivElement>(null);
  const editorViewRef = useRef<EditorView | null>(null);
  const sourceRef = useRef(source);
  sourceRef.current = source;

  useEffect(() => {
    if (!editorHostRef.current) return;
    const state = EditorState.create({
      doc: sourceRef.current,
      extensions: [
        history(),
        bracketMatching(),
        plotgram(),
        syntaxHighlighting(plotgramHighlightStyle),
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
      const result = renderSvg(wasm, source, { transparent_background: true });
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
  }, [source, wasm]);

  const status = useMemo(() => {
    if (wasmError) return { kind: 'error' as const, text: `WASM 加载失败：${wasmError}` };
    if (!ready) return { kind: 'loading' as const, text: '正在加载渲染引擎…' };
    if (renderError) return { kind: 'dsl-error' as const, text: renderError };
    return { kind: 'ok' as const, text: '实时渲染中 · 修改代码即时预览' };
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
          <div className="hero-preview-canvas">
            {svg ? (
              <div className="hero-svg-host" dangerouslySetInnerHTML={{ __html: svg }} />
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
