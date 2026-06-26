import { useEffect, useMemo, useRef, useState } from 'react';
import { loadWasm, renderSource, diffSources, formatSource, type DrawifyWasm, type ChangeJson } from '../lib/wasm';
import { AuditAnimator } from './AuditAnimator';

const RENDER_OPTS = JSON.stringify({ transparent_background: true });

interface AuditScenario {
  id: string;
  title: string;
  description: string;
  dsl1: string;
  dsl2: string;
}

const SCENARIOS: AuditScenario[] = [
  {
    id: 'security-audit',
    title: '安全架构升级审计',
    description: '在 API 网关与后端服务之间新增 WAF、鉴权、审计日志三层安全防护',
    dsl1: `diagram architecture {
  title: "线上服务架构 (变更前)"
  entity[frontend] user "用户"
  entity[gateway] api "API 网关"
  entity[service] app "业务服务"
  entity[database] db "主数据库"
  user -> api "HTTPS"
  api -> app "转发"
  app -> db "读写"
}`,
    dsl2: `diagram architecture {
  title: "线上服务架构 (变更后)"
  entity[frontend] user "用户"
  entity[external] cdn "CDN"
  entity[gateway] waf "WAF 防火墙"
  entity[gateway] api "API 网关"
  entity[service] auth "鉴权服务"
  entity[service] audit "审计日志"
  entity[service] app "业务服务"
  entity[database] db "主数据库"
  user -> cdn
  cdn -> waf "HTTPS"
  waf -> api "清洗后"
  api -> auth "校验"
  auth -> api "通过"
  api -> app "转发"
  api -> audit "记录"
  app -> db "读写"
}`,
  },
  {
    id: 'db-sharding',
    title: '数据库分库分表审计',
    description: '单库扩展为读写分离 + 分库分表架构，新增 Proxy 与从库',
    dsl1: `diagram architecture {
  title: "数据层 (变更前)"
  entity[service] app "应用"
  entity[database] db "MySQL"
  app -> db "读写"
}`,
    dsl2: `diagram architecture {
  title: "数据层 (变更后)"
  entity[service] app "应用"
  entity[gateway] proxy "ShardingProxy"
  entity[database] m0 "主库 M0"
  entity[database] s0 "从库 S0"
  entity[database] s1 "从库 S1"
  entity[cache] cache "Redis 缓存"
  app -> proxy "SQL"
  app -> cache "查询"
  proxy -> m0 "写"
  proxy -> s0 "读"
  proxy -> s1 "读"
  m0 -> s0 "同步"
  m0 -> s1 "同步"
}`,
  },
  {
    id: 'ci-cd-hotfix',
    title: 'CI/CD 增加金丝雀发布',
    description: '在生产部署环节新增金丝雀发布、监控观察与自动回滚路径',
    dsl1: `diagram flowchart {
  title: "CI/CD 流水线 (变更前)"
  config { direction: top-to-bottom }
  entity[start] dev "开发者"
  entity[process] ci "CI 构建"
  entity[decision] test "测试"
  entity[process] deploy "部署生产"
  entity[end] prod "生产"
  dev -> ci
  ci -> test
  test -> deploy "通过"
  deploy -> prod
}`,
    dsl2: `diagram flowchart {
  title: "CI/CD 流水线 (变更后)"
  config { direction: top-to-bottom }
  entity[start] dev "开发者"
  entity[process] ci "CI 构建"
  entity[decision] test "自动化测试"
  entity[decision] approval "审批"
  entity[process] canary "金丝雀发布"
  entity[decision] monitor "监控观察"
  entity[process] rollback "自动回滚"
  entity[process] full "全量发布"
  entity[end] prod "生产"
  dev -> ci
  ci -> test
  test -> approval "通过"
  approval -> canary "批准"
  canary -> monitor
  monitor -> full "指标正常"
  monitor -> rollback "异常"
  rollback -> ci
  full -> prod
}`,
  },
];

const ANIM_DURATION = 700;

function describeChange(c: ChangeJson): { desc: string; target: string } {
  const { op, path } = c;
  const targetName: Record<string, string> = {
    diagram: '图表',
    entity: '节点',
    relation: '边',
    group: '分组',
    style_decl: '样式',
  };
  const verb = op === 'add' ? '新增' : op === 'remove' ? '移除' : '修改';
  const tName = targetName[path.target] || path.target;
  let desc = `${verb}${tName}`;
  let target = path.id || '';
  if (path.attr_key) {
    desc += `属性 ${path.attr_key}`;
  }
  return { desc, target };
}

type Phase = 'before' | 'animating' | 'after';

function StaticSvg({ svg, label, labelClass }: { svg: string; label: string; labelClass: string }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ scale: 1, x: 0, y: 0 });

  useEffect(() => {
    const host = hostRef.current;
    const stage = stageRef.current;
    if (!host || !stage || !svg) return;
    const parser = new DOMParser();
    const doc = parser.parseFromString(svg, 'image/svg+xml');
    const svgEl = doc.querySelector('svg') as SVGSVGElement | null;
    if (!svgEl) return;

    stage.innerHTML = '';
    const vb = svgEl.getAttribute('viewBox');
    if (vb) {
      const parts = vb.split(/[\s,]+/).map(Number);
      if (parts.length === 4) {
        const w = parts[2], h = parts[3];
        svgEl.setAttribute('width', String(w));
        svgEl.setAttribute('height', String(h));
        const cw = host.clientWidth;
        const ch = host.clientHeight;
        const pad = 24;
        const s = Math.min((cw - pad * 2) / w, (ch - pad * 2) / h);
        const ox = (cw - w * s) / 2;
        const oy = (ch - h * s) / 2;
        setView({ scale: s, x: ox, y: oy });
      }
    }
    svgEl.style.position = 'absolute';
    svgEl.style.left = '0';
    svgEl.style.top = '0';
    stage.appendChild(svgEl);
  }, [svg]);

  return (
    <div className="anim-host" ref={hostRef} style={{ position: 'absolute', inset: 0 }}>
      <div
        ref={stageRef}
        style={{
          position: 'absolute',
          left: 0, top: 0,
          transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
          transformOrigin: 'top left',
          willChange: 'transform',
        }}
      />
      <span className={`audit-canvas-label ${labelClass}`}>{label}</span>
    </div>
  );
}

export default function AuditApp() {
  const [wasm, setWasm] = useState<DrawifyWasm | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [scenarioIdx, setScenarioIdx] = useState(0);
  const [phase, setPhase] = useState<Phase>('before');
  const [showHighlights, setShowHighlights] = useState(true);
  const [built, setBuilt] = useState<{
    dsl1: string; svg1: string;
    dsl2: string; svg2: string;
    changes: ChangeJson[];
  } | null>(null);

  useEffect(() => {
    let cancelled = false;
    loadWasm()
      .then((w) => { if (!cancelled) setWasm(w); })
      .catch((err) => { if (!cancelled) setLoadError(err instanceof Error ? err.message : String(err)); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (!wasm) return;
    setPhase('before');
    const s = SCENARIOS[scenarioIdx];
    const f1 = formatSource(wasm, s.dsl1);
    const f2 = formatSource(wasm, s.dsl2);
    const dsl1 = f1.success && f1.text ? f1.text : s.dsl1;
    const dsl2 = f2.success && f2.text ? f2.text : s.dsl2;
    const r1 = renderSource(wasm, dsl1, 'svg', RENDER_OPTS);
    const r2 = renderSource(wasm, dsl2, 'svg', RENDER_OPTS);
    const diff = diffSources(wasm, dsl1, dsl2);
    setBuilt({
      dsl1,
      svg1: r1.success && r1.text ? r1.text : '',
      dsl2,
      svg2: r2.success && r2.text ? r2.text : '',
      changes: diff.changes?.changes || [],
    });
  }, [wasm, scenarioIdx]);

  const summary = useMemo(() => {
    if (!built) return null;
    const adds = built.changes.filter((c) => c.op === 'add').length;
    const removes = built.changes.filter((c) => c.op === 'remove').length;
    const modifies = built.changes.filter((c) => c.op === 'modify').length;
    return { total: built.changes.length, adds, removes, modifies };
  }, [built]);

  const scenario = SCENARIOS[scenarioIdx];

  const handleApply = () => {
    setPhase('animating');
    setTimeout(() => setPhase('after'), ANIM_DURATION + 150);
  };
  const handleReset = () => setPhase('before');

  return (
    <div className="audit-root">
      <header className="audit-header">
        <div className="audit-title-row">
          <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
            <rect x="2" y="2" width="10" height="10" rx="2" fill="#6366f1"/>
            <rect x="16" y="2" width="10" height="10" rx="2" fill="#10b981" opacity="0.9"/>
            <rect x="2" y="16" width="10" height="10" rx="2" fill="#ef4444" opacity="0.8"/>
            <rect x="16" y="16" width="10" height="10" rx="2" fill="#f59e0b" opacity="0.9"/>
          </svg>
          <div>
            <h1 style={{ margin: 0, fontSize: 16, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 8 }}>
              DSL + Patch <span className="audit-badge-pro">审计演示</span>
            </h1>
            <p style={{ margin: '2px 0 0 0', fontSize: 12, color: 'var(--text-muted)' }}>
              通过语义 Diff 标记变更内容，用于架构审计与变更评审
            </p>
          </div>
        </div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <select
            value={scenarioIdx}
            onChange={(e) => setScenarioIdx(Number(e.target.value))}
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
            {SCENARIOS.map((s, i) => (
              <option key={s.id} value={i}>{s.title}</option>
            ))}
          </select>
          <a href="/animation.html" className="demo-link">← 动画演示</a>
          <a href="/" className="demo-link">← 返回 Playground</a>
        </div>
      </header>

      <div className="audit-body">
        <div className="audit-main">
          {loadError && <div className="error-banner">WASM 加载失败: {loadError}</div>}
          {!wasm && !loadError && (
            <div className="loading-overlay">
              <div className="spinner" />
              <p>正在加载 WASM 模块...</p>
            </div>
          )}

          <div style={{
            padding: '12px 24px',
            borderBottom: '1px solid var(--border)',
            background: 'var(--panel)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            flexShrink: 0,
          }}>
            <div>
              <div className="audit-scenario-name">{scenario.title}</div>
              <div className="audit-scenario-desc">{scenario.description}</div>
            </div>
            {summary && (
              <div className="audit-summary">
                <span className="audit-stat add">+{summary.adds} 新增</span>
                <span className="audit-stat remove">-{summary.removes} 移除</span>
                <span className="audit-stat modify">~{summary.modifies} 修改</span>
              </div>
            )}
          </div>

          <div className="audit-canvases">
            <div className="audit-canvas-wrap">
              <StaticSvg svg={built?.svg1 || ''} label="DSL (变更前)" labelClass="before" />
            </div>
            <div className="audit-arrow">
              <div className="plus">+</div>
            </div>
            <div className="audit-canvas-wrap" style={{ position: 'relative' }}>
              {phase === 'before' ? (
                <StaticSvg svg={built?.svg1 || ''} label="点 Patch 查看变更" labelClass="before" />
              ) : (
                <AuditAnimator
                  svg={built?.svg2 || ''}
                  prevSvg={built?.svg1}
                  changes={built?.changes}
                  showHighlights={showHighlights}
                  duration={ANIM_DURATION}
                />
              )}
              {phase === 'after' && (
                <span className="audit-canvas-label after">DSL2 (变更后)</span>
              )}
            </div>
          </div>

          <div style={{
            padding: '12px 24px',
            borderTop: '1px solid var(--border)',
            background: 'var(--panel)',
            display: 'flex',
            gap: 12,
            alignItems: 'center',
            flexShrink: 0,
          }}>
            <div style={{ flex: 1, fontFamily: 'var(--mono)', fontSize: 12, color: 'var(--text-muted)' }}>
              <code style={{ color: 'var(--text)' }}>dsl2 = apply_patch(dsl, patch)</code>
              <span style={{ margin: '0 8px' }}>·</span>
              {phase === 'before' && '点击「应用 Patch」观看变更动画'}
              {phase === 'animating' && '正在应用语义补丁...'}
              {phase === 'after' && '已应用变更，图表中高亮显示所有变化点'}
            </div>
            <label className="audit-toggle" style={{ margin: 0 }}>
              <input
                type="checkbox"
                checked={showHighlights}
                onChange={(e) => setShowHighlights(e.target.checked)}
              />
              <span>高亮变更</span>
            </label>
            {phase === 'before' ? (
              <button className="audit-btn audit-btn-primary" onClick={handleApply} disabled={!built}>
                ▶ 应用 Patch
              </button>
            ) : (
              <button className="audit-btn" onClick={handleReset}>
                ↺ 重置
              </button>
            )}
          </div>
        </div>

        <aside className="audit-sidebar">
          <div className="audit-side-panel">
            <h2>变更清单 (Patch)</h2>
            <p style={{ margin: '0 0 10px 0', fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.5 }}>
              由 <code style={{ fontFamily: 'var(--mono)' }}>diff_sources(dsl, dsl2)</code> 计算得出的语义级变更
            </p>
            <div className="audit-legend">
              <div className="audit-legend-item">
                <span className="audit-legend-dot add" />
                <span>新增元素（绿色发光脉冲）</span>
              </div>
              <div className="audit-legend-item">
                <span className="audit-legend-dot remove" />
                <span>移除元素（红色虚线淡出）</span>
              </div>
              <div className="audit-legend-item">
                <span className="audit-legend-dot modify" />
                <span>修改元素（橙色发光脉冲）</span>
              </div>
            </div>
          </div>

          <div className="audit-side-panel audit-changes-panel">
            <h2>语义变更 ({built?.changes.length || 0})</h2>
            {built?.changes.map((c, i) => {
              const { desc, target } = describeChange(c);
              return (
                <div key={i} className={`audit-change-item ${c.op}`}>
                  <span className="audit-op-badge">
                    {c.op === 'add' ? '+ add' : c.op === 'remove' ? '− remove' : '~ modify'}
                  </span>
                  <span className="audit-change-desc">{desc}</span>
                  {target && <code className="audit-change-target">{target}</code>}
                </div>
              );
            })}
          </div>
        </aside>
      </div>
    </div>
  );
}
