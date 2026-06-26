import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { loadWasm, renderSource, type DrawifyWasm } from '../lib/wasm';
import { SequenceAnimator, type StepInfo } from './SequenceAnimator';

const RENDER_OPTS = JSON.stringify({ transparent_background: true });
const STEP_DURATION = 900;

interface SequenceScene {
  id: string;
  title: string;
  description: string;
  dsl: string;
}

const SCENES: SequenceScene[] = [
  {
    id: 'oauth-login',
    title: 'OAuth 授权码登录',
    description: '经典的第三方登录流程：用户 → 浏览器 → 认证服务 → 资源服务',
    dsl: `diagram sequence {
    title: "OAuth 授权码登录"

    entity user "用户" { type: actor }
    entity browser "浏览器" { type: boundary }
    entity auth "认证服务" { type: control }
    entity resource "资源服务" { type: control }

    user -> browser "点击登录"
    browser -> auth "重定向到授权页"
    user -> auth "输入凭证"
    auth --> browser "返回授权码"
    browser -> auth "用授权码换 Token"
    auth --> browser "返回 Access Token"
    browser -> resource "携带 Token 请求资源"
    resource --> browser "返回受保护数据"
}`,
  },
  {
    id: 'microservice-checkout',
    title: '微服务结账流程',
    description: '电商下单：订单、库存、支付、消息队列、通知服务多服务协作',
    dsl: `diagram sequence {
    title: "微服务结账流程"

    entity user "用户" { type: actor }
    entity web "Web 前端" { type: boundary }
    entity gateway "API 网关" { type: boundary }
    entity order "订单服务" { type: control }
    entity inventory "库存服务" { type: control }
    entity payment "支付服务" { type: control }
    entity mq "消息队列" { type: control }
    entity notify "通知服务" { type: control }

    user -> web "提交订单"
    web -> gateway "POST /checkout"
    gateway -> order "创建订单"
    order -> inventory "锁定库存"
    inventory --> order "锁定成功"
    order -> payment "发起支付"
    payment --> order "支付成功"
    order -> mq "发布订单完成事件"
    mq --> notify "消费事件"
    notify --> user "发送确认邮件"
    order --> gateway "订单结果"
    gateway --> web "结账完成"
    web --> user "显示成功页"
}`,
  },
  {
    id: 'simple-request',
    title: '简单请求-响应',
    description: '最基础的时序：客户端发请求，服务端返回响应',
    dsl: `diagram sequence {
    title: "HTTP 请求响应"

    entity client "客户端" { type: boundary }
    entity server "服务器" { type: control }
    entity db "数据库" { type: database }

    client -> server "HTTP Request"
    server -> db "SELECT * FROM users"
    db --> server "rows"
    server --> client "HTTP 200 + JSON"
}`,
  },
];

function ArrowSym({ arrow }: { arrow: string }) {
  if (arrow === 'passive') return <span className="seq-arrow-sym">⇢</span>;
  return <span className="seq-arrow-sym">→</span>;
}

export default function SequenceApp() {
  const [wasm, setWasm] = useState<DrawifyWasm | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [sceneIdx, setSceneIdx] = useState(0);
  const [step, setStep] = useState(0);
  const [autoPlay, setAutoPlay] = useState(false);
  const [svg, setSvg] = useState('');
  const [steps, setSteps] = useState<StepInfo[]>([]);
  const [showComplete, setShowComplete] = useState(false);
  const autoTimerRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    loadWasm()
      .then((w) => { if (!cancelled) setWasm(w); })
      .catch((err) => { if (!cancelled) setLoadError(err instanceof Error ? err.message : String(err)); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (!wasm) return;
    setStep(0);
    setShowComplete(false);
    const s = SCENES[sceneIdx];
    const r = renderSource(wasm, s.dsl, 'svg', RENDER_OPTS);
    setSvg(r.success && r.text ? r.text : '');
  }, [wasm, sceneIdx]);

  const total = steps.length;
  const isComplete = step >= total && total > 0;

  useEffect(() => {
    if (isComplete) {
      setShowComplete(true);
      if (autoTimerRef.current != null) {
        clearTimeout(autoTimerRef.current);
        autoTimerRef.current = null;
      }
      setAutoPlay(false);
    }
  }, [isComplete]);

  useEffect(() => {
    if (!autoPlay || isComplete) {
      if (autoTimerRef.current != null) {
        clearTimeout(autoTimerRef.current);
        autoTimerRef.current = null;
      }
      return;
    }
    autoTimerRef.current = window.setTimeout(() => {
      setStep((s) => Math.min(s + 1, total));
    }, STEP_DURATION + 400);
    return () => {
      if (autoTimerRef.current != null) {
        clearTimeout(autoTimerRef.current);
        autoTimerRef.current = null;
      }
    };
  }, [autoPlay, step, total, isComplete]);

  const handleReady = useCallback((info: { steps: StepInfo[] }) => {
    setSteps(info.steps);
  }, []);

  const handleNext = () => {
    if (step < total) setStep(step + 1);
  };
  const handlePrev = () => {
    if (isComplete) setShowComplete(false);
    setStep((s) => Math.max(0, s - 1));
  };
  const handleReset = () => {
    setShowComplete(false);
    setStep(0);
    setAutoPlay(false);
  };
  const handleAuto = () => {
    if (autoPlay) {
      setAutoPlay(false);
    } else {
      if (isComplete) {
        setShowComplete(false);
        setStep(0);
      }
      setAutoPlay(true);
    }
  };

  const currentStep = step > 0 && step <= total ? steps[step - 1] : null;

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
      if (e.key === 'ArrowRight' || e.key === 'n' || e.key === 'N') {
        e.preventDefault();
        if (step < total) setStep((s) => s + 1);
      } else if (e.key === 'ArrowLeft' || e.key === 'p' || e.key === 'P') {
        e.preventDefault();
        if (isComplete) setShowComplete(false);
        setStep((s) => Math.max(0, s - 1));
      } else if (e.key === ' ') {
        e.preventDefault();
        handleAuto();
      } else if (e.key === 'r' || e.key === 'R') {
        e.preventDefault();
        handleReset();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [step, total, isComplete, autoPlay]);

  const overlayText = useMemo(() => {
    if (total === 0) return '加载中...';
    if (step === 0) return '点击 NEXT 开始讲解';
    if (isComplete) return '流程完成 ✓';
    if (currentStep) {
      return `${currentStep.from} → ${currentStep.to}: ${currentStep.label}`;
    }
    return '';
  }, [step, total, isComplete, currentStep]);

  const scene = SCENES[sceneIdx];

  return (
    <div className="seq-root">
      <header className="seq-header">
        <div className="seq-title-row">
          <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
            <line x1="6" y1="4" x2="6" y2="24" stroke="#6366f1" strokeWidth={1.5} strokeDasharray="3 3"/>
            <line x1="14" y1="4" x2="14" y2="24" stroke="#8b5cf6" strokeWidth={1.5} strokeDasharray="3 3"/>
            <line x1="22" y1="4" x2="22" y2="24" stroke="#ec4899" strokeWidth={1.5} strokeDasharray="3 3"/>
            <rect x="3" y="2" width="6" height="5" rx="1" fill="#6366f1"/>
            <rect x="11" y="2" width="6" height="5" rx="1" fill="#8b5cf6"/>
            <rect x="19" y="2" width="6" height="5" rx="1" fill="#ec4899"/>
            <path d="M6 11 L13 11 L13 9 L16 12 L13 15 L13 13 L6 13 Z" fill="#6366f1"/>
          </svg>
          <div>
            <h1 style={{ margin: 0, fontSize: 16, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 8 }}>
              时序图走读 <span className="seq-badge-demo">Walkthrough</span>
            </h1>
            <p style={{ margin: '2px 0 0 0', fontSize: 12, color: 'var(--text-muted)' }}>
              逐帧走查消息交互流程，适合演示与讲解
            </p>
          </div>
        </div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <select
            value={sceneIdx}
            onChange={(e) => setSceneIdx(Number(e.target.value))}
            style={{
              padding: '6px 10px',
              background: 'var(--panel-3)',
              border: '1px solid var(--border)',
              borderRadius: 'var(--radius-sm)',
              color: 'var(--text)',
              fontSize: 12,
              fontFamily: 'inherit',
              cursor: 'pointer',
            }}
          >
            {SCENES.map((s, i) => (
              <option key={s.id} value={i}>{s.title}</option>
            ))}
          </select>
          <a href="/animation.html" className="demo-link">动画演示</a>
          <a href="/audit.html" className="demo-link">审计演示</a>
          <a href="/" className="demo-link">← Playground</a>
        </div>
      </header>

      <div className="seq-body">
        {loadError && <div className="error-banner">WASM 加载失败: {loadError}</div>}
        {!wasm && !loadError && (
          <div className="loading-overlay">
            <div className="spinner" />
            <p>正在加载 WASM 模块...</p>
          </div>
        )}

        <div className="seq-canvas-wrap" style={{ padding: '60px 0 0 0' }}>
          <div className="seq-step-overlay">
            {step > 0 && !isComplete && <span className="seq-step-num">STEP {step}/{total}</span>}
            {isComplete && <span className="seq-step-num" style={{ color: '#34d399' }}>✓ 完成</span>}
            {step === 0 && <span className="seq-step-num" style={{ color: '#a5b4fc' }}>准备</span>}
            <span className="seq-step-text">{overlayText}</span>
          </div>
          {svg && (
            <SequenceAnimator
              svg={svg}
              step={step}
              totalSteps={total}
              duration={STEP_DURATION}
              onReady={handleReady}
            />
          )}
          {showComplete && (
            <div className={`seq-complete-banner ${showComplete ? 'show' : ''}`}>
              <div className="seq-complete-title">✓ 流程完成</div>
              <div className="seq-complete-sub">{total} 条消息，点击 RESET 重新讲解</div>
            </div>
          )}
        </div>

        <aside className="seq-sidebar">
          <div className="seq-side-panel">
            <div className="seq-scenario-name">{scene.title}</div>
            <div className="seq-scenario-desc">{scene.description}</div>
            <div className="seq-progress-wrap">
              <div className="seq-progress-bar">
                <div
                  className="seq-progress-fill"
                  style={{ width: `${total === 0 ? 0 : (step / total) * 100}%` }}
                />
              </div>
              <div className="seq-progress-text">
                <span>{step === 0 ? '未开始' : isComplete ? '已完成' : `走读中`}</span>
                <span>{Math.min(step, total)} / {total}</span>
              </div>
            </div>
          </div>

          <div className="seq-side-panel">
            <h2>控制</h2>
            <div className="seq-controls">
              <button className="seq-btn" onClick={handleReset} disabled={step === 0}>↺ 重置</button>
              <button className="seq-btn" onClick={handlePrev} disabled={step === 0}>◀ 上一步</button>
            </div>
            <div style={{ height: 8 }} />
            <div className="seq-controls">
              <button
                className={`seq-btn ${autoPlay ? '' : 'seq-btn-primary'}`}
                onClick={handleAuto}
                disabled={isComplete && !autoPlay}
              >
                {autoPlay ? '⏸ 暂停' : isComplete ? '✓ 已完成' : '▶ 自动播放'}
              </button>
              <button
                className="seq-btn seq-btn-primary"
                onClick={handleNext}
                disabled={isComplete}
              >
                NEXT →
              </button>
            </div>
            <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: '10px 0 0 0', lineHeight: 1.5 }}>
              快捷键：<kbd style={kbdStyle}>→</kbd> 下一步 · <kbd style={kbdStyle}>←</kbd> 上一步 · <kbd style={kbdStyle}>Space</kbd> 自动播放
            </p>
          </div>

          <div className="seq-side-panel seq-steps-panel">
            <h2>消息列表 ({total})</h2>
            {steps.map((s, i) => {
              const isCur = i === step - 1;
              const isDone = i < step - 1 || isComplete;
              const cls = isCur ? 'seq-step-current' : isDone ? 'seq-step-done' : 'seq-step-pending';
              return (
                <div key={i} className={`seq-step-item ${cls}`}>
                  <span className="seq-step-num-badge">{i + 1}</span>
                  <div className="seq-step-body">
                    <div className="seq-step-from-to">
                      {s.from} <ArrowSym arrow={s.arrow} /> {s.to}
                    </div>
                    <div className="seq-step-msg">{s.label || '(无标签)'}</div>
                  </div>
                </div>
              );
            })}
          </div>
        </aside>
      </div>
    </div>
  );
}

const kbdStyle: CSSProperties = {
  padding: '1px 5px',
  background: 'var(--panel-3)',
  border: '1px solid var(--border)',
  borderRadius: 3,
  fontFamily: 'var(--mono)',
  fontSize: 10,
};
