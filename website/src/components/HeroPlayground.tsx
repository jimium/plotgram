import { useEffect, useMemo, useState } from 'react';
import { useWasm } from '../hooks/useWasm';
import { renderSvg, type DiagnosticErrorJson } from '../lib/wasm';

const DEFAULT_SOURCE = `// 经典三层架构：Client → API → DB
// Mermaid 对照: graph LR 三层结构
diagram architecture {
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
  title: "订单处理流程"
  config { direction: top-to-bottom }

  entity[start] start "开始"
  entity[process] order "用户下单"
  entity[process] pay "支付"
  entity[process] ship "发货"
  entity[end] done "完成"

  start -> order
  order -> pay
  pay -> ship
  ship -> done
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

export default function HeroPlayground() {
  const { wasm, ready, error: wasmError } = useWasm();
  const [source, setSource] = useState(DEFAULT_SOURCE);
  const [svg, setSvg] = useState<string>('');
  const [renderError, setRenderError] = useState<string>('');
  const [activePreset, setActivePreset] = useState(0);

  // 防抖渲染
  useEffect(() => {
    if (!wasm) return;

    const timer = setTimeout(() => {
      const result = renderSvg(wasm, source);
      if (result.success && result.text) {
        setSvg(result.text);
        setRenderError('');
      } else {
        setRenderError(formatErrors(result.errors));
      }
    }, 150);

    return () => clearTimeout(timer);
  }, [source, wasm]);

  const status = useMemo(() => {
    if (wasmError) return { kind: 'error' as const, text: `WASM 加载失败：${wasmError}` };
    if (!ready) return { kind: 'loading' as const, text: '正在加载渲染引擎…' };
    if (renderError) return { kind: 'dsl-error' as const, text: renderError };
    return { kind: 'ok' as const, text: '实时渲染中 · 修改左侧代码自动更新' };
  }, [wasmError, ready, renderError]);

  return (
    <div className="hero-visual">
      <div className="hero-visual-header">
        <span className="hero-visual-dot red" />
        <span className="hero-visual-dot yellow" />
        <span className="hero-visual-dot green" />
        <span className="hero-playground-title">live-demo.pgm</span>
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
          <div className="hero-editor-label">DSL</div>
          <textarea
            className="hero-editor"
            value={source}
            spellCheck={false}
            onChange={(e) => setSource(e.target.value)}
          />
        </div>
        <div className="hero-preview">
          <div className="hero-preview-label">
            {status.kind === 'ok' && <span className="hero-status-dot ok" />}
            {status.kind === 'loading' && <span className="hero-status-dot loading" />}
            {status.kind === 'error' && <span className="hero-status-dot error" />}
            {status.kind === 'dsl-error' && <span className="hero-status-dot error" />}
            <span className={`hero-status-text ${status.kind}`}>{status.text}</span>
          </div>
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
    </div>
  );
}
