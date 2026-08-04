import { useCallback, useEffect, useRef, useState } from 'react';
import { useWasm } from '../hooks/useWasm';
import { renderSvg } from '../lib/wasm';

const USER_MSG_1 = '画一个订单状态机图：待支付 → 已支付 → 待发货 → 已发货 → 已签收。分支：支付超时取消、签收后退货退款。';

const DSL_1 = `diagram state {
    title: "订单状态机"

    entity[initial] init ""
    entity[state] pending "待支付"
    entity[state] paid "已支付"
    entity[state] to_ship "待发货"
    entity[state] shipped "已发货"
    entity[state] signed "已签收"
    entity[final] cancelled "已取消"
    entity[final] returned "已退货退款"
    entity[choice] pay_timeout "支付超时?"

    init -> pending
    pending -> pay_timeout
    pay_timeout -> paid "支付成功"
    pay_timeout -> cancelled "支付超时"
    paid -> to_ship "审核通过"
    to_ship -> shipped "仓库发货"
    shipped -> signed "用户签收"
    signed -> returned "申请退货"
}`;

// Line 13 is the one that changes: entity[state] to_ship "待发货" → "准备发货"
const CHANGED_LINE = 13;

const DSL_2 = `diagram state {
    title: "订单状态机"

    entity[initial] init ""
    entity[state] pending "待支付"
    entity[state] paid "已支付"
    entity[state] to_ship "准备发货"
    entity[state] shipped "已发货"
    entity[state] signed "已签收"
    entity[final] cancelled "已取消"
    entity[final] returned "已退货退款"
    entity[choice] pay_timeout "支付超时?"

    init -> pending
    pending -> pay_timeout
    pay_timeout -> paid "支付成功"
    pay_timeout -> cancelled "支付超时"
    paid -> to_ship "审核通过"
    to_ship -> shipped "仓库发货"
    shipped -> signed "用户签收"
    signed -> returned "申请退货"
}`;

const USER_MSG_2 = '把"待发货"改成"准备发货"';

type RoundPhase = 'idle' | 'user-typing' | 'thinking' | 'agent-coding' | 'rendering' | 'done';

const USER_TYPE_SPEED = 20;
const AGENT_TYPE_SPEED = 6;
const THINKING_DURATION = 700;
const USER_PAUSE = 350;
const BETWEEN_ROUNDS = 1500;

export default function AgentConversationDemo() {
  const { wasm, ready } = useWasm();
  const [round, setRound] = useState<1 | 2>(1);
  const [phase, setPhase] = useState<RoundPhase>('idle');
  const [r1UserText, setR1UserText] = useState('');
  const [r1CodeText, setR1CodeText] = useState('');
  const [r1Svg, setR1Svg] = useState('');
  const [r2UserText, setR2UserText] = useState('');
  const [r2Svg, setR2Svg] = useState('');
  const [r2SvgVisible, setR2SvgVisible] = useState(false);
  const [hasPlayed, setHasPlayed] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const codeRef = useRef<HTMLPreElement>(null);
  const timersRef = useRef<ReturnType<typeof setTimeout>[]>([]);

  const clearAllTimers = useCallback(() => {
    timersRef.current.forEach(clearTimeout);
    timersRef.current = [];
  }, []);

  const reset = useCallback(() => {
    clearAllTimers();
    setRound(1);
    setPhase('idle');
    setR1UserText('');
    setR1CodeText('');
    setR1Svg('');
    setR2UserText('');
    setR2Svg('');
    setR2SvgVisible(false);
  }, [clearAllTimers]);

  const typeText = useCallback(
    (
      text: string,
      setter: (v: string) => void,
      speed: number,
      onDone: () => void,
    ) => {
      let i = 0;
      const interval = setInterval(() => {
        i++;
        setter(text.slice(0, i));
        if (i >= text.length) {
          clearInterval(interval);
          onDone();
        }
      }, speed);
      timersRef.current.push(interval as unknown as ReturnType<typeof setTimeout>);
    },
    [],
  );

  const startAnimation = useCallback(() => {
    if (!ready) return;
    reset();

    // ── Round 1: User asks → Agent generates DSL → SVG renders ──
    setRound(1);
    setPhase('user-typing');
    typeText(USER_MSG_1, setR1UserText, USER_TYPE_SPEED, () => {
      timersRef.current.push(
        setTimeout(() => {
          setPhase('thinking');
          timersRef.current.push(
            setTimeout(() => {
              setPhase('agent-coding');
              typeText(DSL_1, setR1CodeText, AGENT_TYPE_SPEED, () => {
                timersRef.current.push(
                  setTimeout(() => {
                    setPhase('rendering');
                    if (wasm) {
                      const result = renderSvg(wasm, DSL_1, { transparent_background: true });
                      if (result.success && result.text) {
                        setR1Svg(result.text);
                      }
                    }
                    timersRef.current.push(
                      setTimeout(() => {
                        setPhase('done');
                        // Start round 2 after pause
                        timersRef.current.push(
                          setTimeout(() => {
                            // ── Round 2: User asks to modify → Agent applies patch ──
                            setRound(2);
                            setPhase('user-typing');
                            typeText(USER_MSG_2, setR2UserText, USER_TYPE_SPEED, () => {
                              timersRef.current.push(
                                setTimeout(() => {
                                  setPhase('thinking');
                                  timersRef.current.push(
                                    setTimeout(() => {
                                      // Simulate patch apply + render
                                      setPhase('rendering');
                                      if (wasm) {
                                        const result = renderSvg(wasm, DSL_2, { transparent_background: true });
                                        if (result.success && result.text) {
                                          setR2Svg(result.text);
                                        }
                                      }
                                      timersRef.current.push(
                                        setTimeout(() => {
                                          setR2SvgVisible(true);
                                          setPhase('done');
                                        }, 300),
                                      );
                                    }, THINKING_DURATION),
                                  );
                                }, USER_PAUSE),
                              );
                            });
                          }, BETWEEN_ROUNDS),
                        );
                      }, 300),
                    );
                  }, 200),
                );
              });
            }, THINKING_DURATION),
          );
        }, USER_PAUSE),
      );
    });
  }, [ready, reset, typeText, wasm]);

  // Auto-play when in view
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting && !hasPlayed && ready) {
          setHasPlayed(true);
          startAnimation();
        }
      },
      { threshold: 0.3 },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [hasPlayed, ready, startAnimation]);

  useEffect(() => {
    return () => clearAllTimers();
  }, [clearAllTimers]);

  const handleReplay = () => {
    setHasPlayed(true);
    startAnimation();
  };

  const isTyping = phase === 'user-typing' || phase === 'agent-coding';
  const isActive = phase !== 'idle';
  const currentSvg = r2SvgVisible ? r2Svg : r1Svg;

  // Split DSL_1 into lines for highlighting
  const dslLines = DSL_1.split('\n');

  const timelineSteps = [
    { label: '输入需求', active: phase !== 'idle' },
    { label: '生成 DSL', active: phase === 'agent-coding' || phase === 'rendering' || phase === 'done' || round === 2 },
    { label: '渲染', active: phase === 'rendering' || phase === 'done' || round === 2 },
    { label: 'Patch 修改', active: round === 2 },
    { label: '完成', active: round === 2 && phase === 'done' },
  ];

  return (
    <div className="agent-demo" ref={containerRef}>
      <div className="agent-demo-header">
        <div className="agent-demo-dots">
          <span className="agent-demo-dot red" />
          <span className="agent-demo-dot yellow" />
          <span className="agent-demo-dot green" />
        </div>
        <span className="agent-demo-title">Agent Demo — 对话 + 增量修改</span>
        <button
          className="agent-demo-replay"
          onClick={handleReplay}
          disabled={!ready || isTyping}
          title="重新播放"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="23 4 23 10 17 10" />
            <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
          </svg>
          重播
        </button>
      </div>

      <div className="agent-demo-body">
        {/* Conversation panel */}
        <div className="agent-demo-chat">
          {/* Round 1: User */}
          <div className={`agent-chat-bubble agent-chat-user ${isActive ? 'active' : ''}`}>
            <div className="agent-chat-avatar user">U</div>
            <div className="agent-chat-content">
              <div className="agent-chat-role">你</div>
              <div className="agent-chat-text">
                {r1UserText || ''}
                {round === 1 && phase === 'user-typing' && <span className="agent-cursor" />}
              </div>
            </div>
          </div>

          {/* Round 1: Agent generates DSL */}
          {(phase !== 'idle' && (round === 1 || round === 2)) && (
            <div className="agent-chat-bubble agent-chat-agent">
              <div className="agent-chat-avatar agent">A</div>
              <div className="agent-chat-content">
                <div className="agent-chat-role">Agent</div>
                {round === 1 && phase === 'thinking' ? (
                  <div className="agent-thinking">
                    <span>正在生成 DSL</span>
                    <span className="agent-thinking-dots">
                      <i>.</i><i>.</i><i>.</i>
                    </span>
                  </div>
                ) : (
                  <div className="agent-chat-text">
                    <pre className="agent-dsl-code" ref={codeRef}>
                      <code>
                        {round === 2 && r2SvgVisible
                          ? dslLines.map((line, i) => (
                              <span
                                key={i}
                                className={i === CHANGED_LINE - 1 ? 'agent-dsl-line-changed' : ''}
                              >
                                {line}{'\n'}
                              </span>
                            ))
                          : r1CodeText}
                      </code>
                      {(round === 1 && phase === 'agent-coding') && <span className="agent-cursor" />}
                    </pre>
                    {round === 1 && phase === 'done' && (
                      <div className="agent-done-hint">
                        <span className="agent-check">✓</span>
                        渲染完成 · 一次生成，无需修改
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Round 2: User modification request */}
          {round === 2 && (
            <div className="agent-chat-bubble agent-chat-user active">
              <div className="agent-chat-avatar user">U</div>
              <div className="agent-chat-content">
                <div className="agent-chat-role">你</div>
                <div className="agent-chat-text">
                  {r2UserText || ''}
                  {round === 2 && phase === 'user-typing' && <span className="agent-cursor" />}
                </div>
              </div>
            </div>
          )}

          {/* Round 2: Agent applies patch (no DSL regeneration!) */}
          {round === 2 && phase !== 'user-typing' && (
            <div className="agent-chat-bubble agent-chat-agent">
              <div className="agent-chat-avatar agent">A</div>
              <div className="agent-chat-content">
                <div className="agent-chat-role">Agent</div>
                {round === 2 && phase === 'thinking' ? (
                  <div className="agent-thinking">
                    <span>正在分析变更</span>
                    <span className="agent-thinking-dots">
                      <i>.</i><i>.</i><i>.</i>
                    </span>
                  </div>
                ) : (
                  <div className="agent-chat-text">
                    {/* Patch card — shows the incremental change */}
                    <div className="agent-patch-card">
                      <div className="agent-patch-card-header">
                        <span className="agent-patch-icon">🔧</span>
                        <span>apply_patch</span>
                        <span className="agent-patch-badge">1 change</span>
                      </div>
                      <div className="agent-patch-body">
                        <div className="agent-patch-row">
                          <span className="agent-patch-op modify">modify</span>
                          <span className="agent-patch-target">entity[to_ship].label</span>
                        </div>
                        <div className="agent-patch-diff">
                          <span className="agent-patch-old">"待发货"</span>
                          <span className="agent-patch-arrow">→</span>
                          <span className="agent-patch-new">"准备发货"</span>
                        </div>
                      </div>
                    </div>
                    {round === 2 && phase === 'done' && (
                      <div className="agent-done-hint">
                        <span className="agent-check">✓</span>
                        增量修改完成 · 只 Patch 一个属性，不重生成整张图
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* SVG Preview panel */}
        <div className="agent-demo-preview">
          <div className="agent-demo-preview-label">
            <span className="agent-preview-dot" data-active={phase === 'rendering' || phase === 'done'} />
            实时预览
          </div>
          <div className="agent-demo-preview-canvas">
            {currentSvg ? (
              <div
                className="agent-svg-host"
                key={currentSvg.slice(0, 40)}
                dangerouslySetInnerHTML={{ __html: currentSvg }}
              />
            ) : (
              <div className="agent-preview-placeholder">
                {!ready ? (
                  <>
                    <span className="agent-spinner" />
                    <span>加载渲染引擎…</span>
                  </>
                ) : phase === 'idle' ? (
                  <span>等待对话开始…</span>
                ) : phase === 'user-typing' ? (
                  <span>等待 Agent 响应…</span>
                ) : phase === 'thinking' ? (
                  <span>Agent 思考中…</span>
                ) : phase === 'agent-coding' ? (
                  <span>正在生成代码…</span>
                ) : (
                  <span>渲染中…</span>
                )}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Timeline bar */}
      <div className="agent-demo-timeline">
        {timelineSteps.map((step, i) => (
          <div
            key={step.label}
            className={`agent-timeline-step ${step.active ? 'past' : ''}`}
          >
            <div className="agent-timeline-dot" />
            <span className="agent-timeline-label">{step.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}