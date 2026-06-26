import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { loadWasm, renderSource, diffSources, formatSource, type DrawifyWasm, type ChangeSetJson, type ChangeJson } from '../lib/wasm';
import { SvgAnimator } from './SvgAnimator';
import { SCENES, type AnimationScene } from './scenes';

type PlayState = 'idle' | 'playing' | 'paused';

const RENDER_OPTS = JSON.stringify({
  transparent_background: true,
});

const SPEEDS = [0.5, 1, 1.5, 2, 3];
const STEP_DURATION = 1400;

interface FrameInfo {
  dsl: string;
  svg: string;
  diffFromPrev?: ChangeSetJson;
}

function buildFrames(wasm: DrawifyWasm, scene: AnimationScene): FrameInfo[] {
  const frames: FrameInfo[] = [];
  for (let i = 0; i < scene.dsls.length; i++) {
    const rawDsl = scene.dsls[i];
    const fmt = formatSource(wasm, rawDsl);
    const dsl = fmt.success && fmt.text ? fmt.text : rawDsl;
    const r = renderSource(wasm, dsl, 'svg', RENDER_OPTS);
    const svg = r.success && r.text ? r.text : '';
    const diffFromPrev = i > 0 ? diffSources(wasm, frames[i - 1].dsl, dsl).changes : undefined;
    frames.push({ dsl, svg, diffFromPrev });
  }
  return frames;
}

function changeDescription(c: ChangeJson): string {
  const target = c.path.target;
  const id = c.path.id;
  const attr = c.path.attr_key;
  const targetName = {
    diagram: '图表',
    entity: '节点',
    relation: '边',
    group: '分组',
    style_decl: '样式',
  }[target];

  const opVerb = c.op === 'add' ? '添加' : c.op === 'remove' ? '删除' : '修改';

  if (target === 'diagram') {
    if (attr === 'diagram_type') return `${opVerb}图表类型`;
    return `${opVerb}图表属性 ${attr}`;
  }
  if (attr) {
    return `${opVerb} ${targetName}「${id}」的 ${attr}`;
  }
  if (id) {
    return `${opVerb} ${targetName}「${id}」`;
  }
  return `${opVerb} ${targetName}`;
}

function changeColor(op: string): string {
  if (op === 'add') return '#34d399';
  if (op === 'remove') return '#f87171';
  return '#fbbf24';
}

export default function App() {
  const [wasm, setWasm] = useState<DrawifyWasm | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [sceneIdx, setSceneIdx] = useState(0);
  const [frameIdx, setFrameIdx] = useState(0);
  const [frames, setFrames] = useState<FrameInfo[]>([]);
  const [playState, setPlayState] = useState<PlayState>('idle');
  const [speedIdx, setSpeedIdx] = useState(1);
  const [showDsl, setShowDsl] = useState(true);
  const [showChanges, setShowChanges] = useState(true);
  const timerRef = useRef<number | null>(null);
  const isBuildingRef = useRef(false);

  const scene = SCENES[sceneIdx];
  const currentFrame = frames[frameIdx];
  const speed = SPEEDS[speedIdx];

  useEffect(() => {
    let cancelled = false;
    loadWasm()
      .then((w) => {
        if (cancelled) return;
        setWasm(w);
      })
      .catch((err) => {
        if (cancelled) return;
        setLoadError(err instanceof Error ? err.message : String(err));
      });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (!wasm || isBuildingRef.current) return;
    isBuildingRef.current = true;
    setFrames([]);
    setFrameIdx(0);
    setPlayState('idle');
    setTimeout(() => {
      const built = buildFrames(wasm, scene);
      setFrames(built);
      isBuildingRef.current = false;
    }, 30);
  }, [wasm, scene]);

  const clearTimer = () => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const goTo = useCallback((idx: number) => {
    if (idx < 0 || idx >= frames.length || !frames.length) return;
    setFrameIdx(idx);
  }, [frames]);

  const next = useCallback(() => {
    setFrameIdx((i) => {
      if (i >= frames.length - 1) {
        setPlayState('idle');
        return i;
      }
      return i + 1;
    });
  }, [frames.length]);

  useEffect(() => {
    clearTimer();
    if (playState !== 'playing') return;
    const duration = STEP_DURATION / speed;
    timerRef.current = window.setTimeout(() => {
      next();
    }, duration);
    return clearTimer;
  }, [playState, frameIdx, speed, next]);

  useEffect(() => () => clearTimer(), []);

  const handlePlay = () => {
    if (frameIdx >= frames.length - 1) setFrameIdx(0);
    setPlayState('playing');
  };
  const handlePause = () => setPlayState('paused');
  const handleReset = () => {
    clearTimer();
    setPlayState('idle');
    setFrameIdx(0);
  };
  const handleStepForward = () => { clearTimer(); setPlayState('paused'); next(); };
  const handleStepBack = () => {
    clearTimer();
    setPlayState('paused');
    setFrameIdx((i) => Math.max(0, i - 1));
  };

  const handleSelectScene = (idx: number) => {
    clearTimer();
    setPlayState('idle');
    setSceneIdx(idx);
  };

  const diffSummary = useMemo(() => {
    if (!currentFrame?.diffFromPrev) return null;
    const changes = currentFrame.diffFromPrev.changes;
    const adds = changes.filter((c) => c.op === 'add').length;
    const removes = changes.filter((c) => c.op === 'remove').length;
    const modifies = changes.filter((c) => c.op === 'modify').length;
    return { total: changes.length, adds, removes, modifies, changes };
  }, [currentFrame]);

  return (
    <div className="demo-root">
      <header className="demo-header">
        <div className="demo-header-left">
          <div className="demo-logo">
            <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
              <rect x="2" y="2" width="10" height="10" rx="2" fill="#6366f1"/>
              <rect x="16" y="2" width="10" height="10" rx="2" fill="#22d3ee" opacity="0.8"/>
              <rect x="2" y="16" width="10" height="10" rx="2" fill="#f472b6" opacity="0.7"/>
              <rect x="16" y="16" width="10" height="10" rx="2" fill="#34d399" opacity="0.8"/>
            </svg>
            <div>
              <h1>DSL + Patch <span className="demo-badge">动画演示</span></h1>
              <p>增量语义变更驱动的图表平滑演化</p>
            </div>
          </div>
        </div>
        <div className="demo-header-right">
          <a href="/sequence.html" className="demo-link">时序演示 →</a>
          <a href="/audit.html" className="demo-link">审计演示 →</a>
          <a href="/" className="demo-link">← 返回 Playground</a>
        </div>
      </header>

      <div className="demo-body">
        <aside className="demo-sidebar">
          <div className="demo-panel">
            <h2>场景选择</h2>
            <div className="scene-list">
              {SCENES.map((s, i) => (
                <button
                  key={s.id}
                  className={`scene-item${sceneIdx === i ? ' active' : ''}${!frames.length && sceneIdx === i ? ' loading' : ''}`}
                  onClick={() => handleSelectScene(i)}
                >
                  <div className="scene-title">{s.title}</div>
                  <div className="scene-desc">{s.description}</div>
                  <div className="scene-meta">{s.dsls.length} 个关键帧</div>
                </button>
              ))}
            </div>
          </div>

          <div className="demo-panel">
            <h2>播放控制</h2>
            <div className="controls">
              <div className="controls-row">
                <button className="ctrl-btn" onClick={handleReset} title="重置">
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M3 3v4h4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/><path d="M3 7a5 5 0 1 1 1.5 3.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
                </button>
                <button className="ctrl-btn" onClick={handleStepBack} disabled={frameIdx === 0} title="上一步">
                  <svg width="16" height="16" viewBox="0 0 16 16"><path d="M10 3L5 8l5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" fill="none"/></svg>
                </button>
                {playState === 'playing' ? (
                  <button className="ctrl-btn ctrl-primary" onClick={handlePause} title="暂停">
                    <svg width="16" height="16" viewBox="0 0 16 16"><rect x="4" y="3" width="3" height="10" fill="currentColor" rx="0.5"/><rect x="9" y="3" width="3" height="10" fill="currentColor" rx="0.5"/></svg>
                  </button>
                ) : (
                  <button className="ctrl-btn ctrl-primary" onClick={handlePlay} title="播放" disabled={!frames.length}>
                    <svg width="16" height="16" viewBox="0 0 16 16"><path d="M4 3l9 5-9 5V3z" fill="currentColor"/></svg>
                  </button>
                )}
                <button className="ctrl-btn" onClick={handleStepForward} disabled={frameIdx >= frames.length - 1} title="下一步">
                  <svg width="16" height="16" viewBox="0 0 16 16"><path d="M6 3l5 5-5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" fill="none"/></svg>
                </button>
              </div>
              <div className="controls-row speed-row">
                <span className="ctrl-label">速度</span>
                <div className="speed-chips">
                  {SPEEDS.map((s, i) => (
                    <button
                      key={s}
                      className={`speed-chip${speedIdx === i ? ' active' : ''}`}
                      onClick={() => setSpeedIdx(i)}
                    >
                      {s}x
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <div className="progress-section">
              <div className="progress-header">
                <span>
                  帧 {frames.length ? frameIdx + 1 : 0} / {frames.length}
                </span>
                <span className="frame-label">
                  {frames.length && scene.frameLabels[frameIdx]}
                </span>
              </div>
              <div className="progress-track" onClick={(e) => {
                if (!frames.length) return;
                const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
                const ratio = (e.clientX - rect.left) / rect.width;
                goTo(Math.min(frames.length - 1, Math.max(0, Math.floor(ratio * frames.length))));
                setPlayState('paused');
              }}>
                <div className="progress-fill" style={{ width: frames.length ? `${((frameIdx + 1) / frames.length) * 100}%` : '0%' }} />
                {frames.map((_, i) => (
                  <div
                    key={i}
                    className={`progress-dot${i === frameIdx ? ' current' : ''}${i < frameIdx ? ' passed' : ''}`}
                    style={{ left: `${(i / (frames.length - 1 || 1)) * 100}%` }}
                    onClick={(e) => { e.stopPropagation(); goTo(i); setPlayState('paused'); }}
                  />
                ))}
              </div>
              <div className="frame-ticks">
                {frames.map((_, i) => (
                  <button
                    key={i}
                    className={`frame-tick${i === frameIdx ? ' current' : ''}`}
                    onClick={() => { goTo(i); setPlayState('paused'); }}
                    title={scene.frameLabels[i]}
                  >{i + 1}</button>
                ))}
              </div>
            </div>
          </div>

          <div className="demo-panel toggle-panel">
            <label className="toggle-row">
              <input type="checkbox" checked={showDsl} onChange={(e) => setShowDsl(e.target.checked)} />
              <span>显示 DSL 源码</span>
            </label>
            <label className="toggle-row">
              <input type="checkbox" checked={showChanges} onChange={(e) => setShowChanges(e.target.checked)} />
              <span>显示 Patch 变更</span>
            </label>
          </div>
        </aside>

        <main className="demo-main">
          {loadError && (
            <div className="error-banner">WASM 加载失败: {loadError}</div>
          )}
          {!wasm && !loadError && (
            <div className="loading-overlay">
              <div className="spinner" />
              <p>正在加载 WASM 模块...</p>
            </div>
          )}

          <div className="canvas-toolbar">
            <div className="canvas-title">
              <span className="title-accent" />
              <h2>{scene.title}</h2>
              <span className="frame-pill">{scene.frameLabels[frameIdx]}</span>
            </div>
            {diffSummary && frameIdx > 0 && (
              <div className="diff-summary">
                <span className="diff-stat diff-add">+{diffSummary.adds}</span>
                <span className="diff-stat diff-remove">-{diffSummary.removes}</span>
                <span className="diff-stat diff-modify">~{diffSummary.modifies}</span>
                <span className="diff-total">{diffSummary.total} 处变更</span>
              </div>
            )}
          </div>

          <div className="canvas-container">
            <SvgAnimator svg={currentFrame?.svg || ''} duration={550} />
          </div>

          <div className="bottom-panels">
            {showChanges && diffSummary && (
              <div className="changes-panel">
                <div className="panel-header">
                  <h3>Patch — 语义变更（第 {frameIdx + 1} 帧）</h3>
                  <p className="panel-sub">DSL 增量补丁驱动图表演化</p>
                </div>
                <div className="changes-list">
                  {diffSummary.changes.map((c, i) => (
                    <div key={i} className="change-item" style={{ borderLeftColor: changeColor(c.op) }}>
                      <span className="change-op-badge" style={{ backgroundColor: changeColor(c.op) + '22', color: changeColor(c.op), borderColor: changeColor(c.op) + '55' }}>
                        {c.op === 'add' ? '+ add' : c.op === 'remove' ? '− remove' : '~ modify'}
                      </span>
                      <span className="change-desc">{changeDescription(c)}</span>
                      {c.path.target !== 'diagram' && c.path.id && (
                        <code className="change-id">{c.path.id}</code>
                      )}
                      {c.path.attr_key && (
                        <code className="change-attr">.{c.path.attr_key}</code>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {showDsl && currentFrame && (
              <div className="dsl-panel">
                <div className="panel-header">
                  <h3>DSL 源码</h3>
                  <p className="panel-sub">当前帧对应的 Drawify DSL</p>
                </div>
                <pre className="dsl-code"><code>{currentFrame.dsl}</code></pre>
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
