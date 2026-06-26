import { useEffect, useRef, useState } from 'react';

interface SequenceAnimatorProps {
  svg: string;
  step: number;
  totalSteps: number;
  duration?: number;
  onReady?: (info: { steps: StepInfo[] }) => void;
}

export interface StepInfo {
  index: number;
  from: string;
  to: string;
  label: string;
  arrow: string;
}

interface MessageInfo {
  index: number;
  edgeG: SVGGElement;
  labelG: SVGGElement | null;
  from: string;
  to: string;
  arrow: string;
  labelText: string;
  length: number;
  shape: SVGLineElement | SVGPolylineElement;
  points: Array<{ x: number; y: number }>;
  markerUrl: string;
}

function parsePoints(shape: SVGLineElement | SVGPolylineElement): Array<{ x: number; y: number }> {
  if (shape instanceof SVGLineElement) {
    return [
      { x: parseFloat(shape.getAttribute('x1') || '0'), y: parseFloat(shape.getAttribute('y1') || '0') },
      { x: parseFloat(shape.getAttribute('x2') || '0'), y: parseFloat(shape.getAttribute('y2') || '0') },
    ];
  }
  const pts = shape.getAttribute('points');
  if (!pts) return [];
  const nums = pts.trim().split(/[\s,]+/).map(Number);
  const out: Array<{ x: number; y: number }> = [];
  for (let i = 0; i < nums.length - 1; i += 2) {
    out.push({ x: nums[i], y: nums[i + 1] });
  }
  return out;
}

function pathLength(points: Array<{ x: number; y: number }>): number {
  let total = 0;
  for (let i = 1; i < points.length; i++) {
    total += Math.hypot(points[i].x - points[i - 1].x, points[i].y - points[i - 1].y);
  }
  return total;
}

function pointAlongPath(points: Array<{ x: number; y: number }>, t: number): { x: number; y: number } {
  const total = pathLength(points);
  if (total === 0 || points.length === 0) return { x: 0, y: 0 };
  let target = t * total;
  for (let i = 1; i < points.length; i++) {
    const seg = Math.hypot(points[i].x - points[i - 1].x, points[i].y - points[i - 1].y);
    if (target <= seg || i === points.length - 1) {
      const lt = seg === 0 ? 0 : Math.min(1, target / seg);
      return {
        x: points[i - 1].x + (points[i].x - points[i - 1].x) * lt,
        y: points[i - 1].y + (points[i].y - points[i - 1].y) * lt,
      };
    }
    target -= seg;
  }
  return points[points.length - 1];
}

const SIGNAL_COLOR = '#6366f1';

function ensureSignalDefs(svgEl: SVGSVGElement) {
  let defs = svgEl.querySelector('defs') as SVGDefsElement | null;
  if (!defs) {
    defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');
    svgEl.insertBefore(defs, svgEl.firstChild);
  }
  if (!defs.querySelector('#seq-signal-filter')) {
    defs.insertAdjacentHTML(
      'beforeend',
      `<filter id="seq-signal-filter" x="-100%" y="-100%" width="300%" height="300%">
        <feGaussianBlur in="SourceGraphic" stdDeviation="2" result="blur"/>
        <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
      </filter>
      <radialGradient id="seq-signal-gradient">
        <stop offset="0%" stop-color="#fff" stop-opacity="1"/>
        <stop offset="40%" stop-color="${SIGNAL_COLOR}" stop-opacity="1"/>
        <stop offset="100%" stop-color="${SIGNAL_COLOR}" stop-opacity="0"/>
      </radialGradient>`
    );
  }
}

function collectMessages(svgEl: SVGSVGElement): MessageInfo[] {
  const edgeGs = Array.from(svgEl.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge"]'));
  const labelGs = Array.from(svgEl.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge-label"]'));

  const labelByKey = new Map<string, SVGGElement>();
  for (const lg of labelGs) {
    const idx = lg.getAttribute('data-dfy-index');
    const from = lg.getAttribute('data-dfy-from') || '';
    const to = lg.getAttribute('data-dfy-to') || '';
    labelByKey.set(`${idx}|${from}|${to}`, lg);
  }

  const messages: MessageInfo[] = [];
  for (const g of edgeGs) {
    const idxStr = g.getAttribute('data-dfy-index');
    const from = g.getAttribute('data-dfy-from') || '';
    const to = g.getAttribute('data-dfy-to') || '';
    const arrow = g.getAttribute('data-dfy-arrow') || '';
    if (idxStr == null) continue;
    const index = parseInt(idxStr, 10);
    const labelG = labelByKey.get(`${idxStr}|${from}|${to}`) || null;
    const labelText = extractLabelText(labelG);
    const shape = g.querySelector<SVGLineElement | SVGPolylineElement>('line, polyline');
    if (!shape) continue;
    const points = parsePoints(shape);
    const length = pathLength(points);
    const markerUrl = shape.getAttribute('marker-end') || '';
    messages.push({ index, edgeG: g, labelG, from, to, arrow, labelText, length, shape, points, markerUrl });
  }
  messages.sort((a, b) => a.index - b.index);
  return messages;
}

function extractLabelText(labelG: SVGGElement | null): string {
  if (!labelG) return '';
  const texts = labelG.querySelectorAll('text');
  const parts: string[] = [];
  texts.forEach((t) => {
    const c = t.textContent;
    if (c) parts.push(c);
  });
  return parts.join(' ').trim();
}

export function SequenceAnimator({ svg, step, totalSteps, duration = 700, onReady }: SequenceAnimatorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ scale: 1, x: 0, y: 0 });
  const messagesRef = useRef<MessageInfo[]>([]);
  const signalCircleRef = useRef<SVGCircleElement | null>(null);
  const signalPulseRef = useRef<SVGCircleElement | null>(null);
  const prevStepRef = useRef(-1);
  const animFrameRef = useRef<number | null>(null);
  const animStartRef = useRef(0);
  const currentAnimTargetRef = useRef<number>(-1);

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
    let vw = 800, vh = 600;
    if (vb) {
      const parts = vb.split(/[\s,]+/).map(Number);
      if (parts.length === 4) { vw = parts[2]; vh = parts[3]; }
    }
    svgEl.setAttribute('width', String(vw));
    svgEl.setAttribute('height', String(vh));
    svgEl.style.position = 'absolute';
    svgEl.style.left = '0';
    svgEl.style.top = '0';
    stage.appendChild(svgEl);

    const cw = host.clientWidth;
    const ch = host.clientHeight;
    const pad = 32;
    const s = Math.min((cw - pad * 2) / vw, (ch - pad * 2) / vh);
    const ox = (cw - vw * s) / 2;
    const oy = (ch - vh * s) / 2;
    setView({ scale: s, x: ox, y: oy });

    ensureSignalDefs(svgEl);

    const signal = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    signal.setAttribute('r', '7');
    signal.setAttribute('fill', 'url(#seq-signal-gradient)');
    signal.setAttribute('filter', 'url(#seq-signal-filter)');
    signal.style.opacity = '0';
    signal.style.pointerEvents = 'none';
    svgEl.appendChild(signal);
    signalCircleRef.current = signal;

    const pulse = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    pulse.setAttribute('r', '12');
    pulse.setAttribute('fill', 'none');
    pulse.setAttribute('stroke', SIGNAL_COLOR);
    pulse.setAttribute('stroke-width', '2');
    pulse.style.opacity = '0';
    pulse.style.pointerEvents = 'none';
    svgEl.appendChild(pulse);
    signalPulseRef.current = pulse;

    const messages = collectMessages(svgEl);
    messagesRef.current = messages;

    messages.forEach((m) => initMessage(m));

    if (onReady) {
      onReady({
        steps: messages.map((m) => ({ index: m.index, from: m.from, to: m.to, label: m.labelText, arrow: m.arrow })),
      });
    }

    prevStepRef.current = -1;
  }, [svg]);

  useEffect(() => {
    const messages = messagesRef.current;
    if (messages.length === 0) return;

    if (animFrameRef.current != null) {
      cancelAnimationFrame(animFrameRef.current);
      animFrameRef.current = null;
    }

    const prev = prevStepRef.current;
    prevStepRef.current = step;

    const forward = step > prev;

    messages.forEach((m, i) => {
      const state = getMsgState(i, step, totalSteps);
      applyMsgVisual(m, state, forward && i === step - 1 ? duration : 250);
    });

    if (forward && step > 0 && step <= messages.length) {
      animateSignal(messages[step - 1], duration);
    } else {
      const sig = signalCircleRef.current;
      const pul = signalPulseRef.current;
      if (sig) sig.style.opacity = '0';
      if (pul) pul.style.opacity = '0';
    }
  }, [step, totalSteps, duration]);

  function animateSignal(m: MessageInfo, dur: number) {
    const sig = signalCircleRef.current;
    const pul = signalPulseRef.current;
    if (!sig) return;
    currentAnimTargetRef.current = m.index;
    animStartRef.current = performance.now();

    const tick = (now: number) => {
      if (currentAnimTargetRef.current !== m.index) return;
      const elapsed = now - animStartRef.current;
      const t = Math.min(1, elapsed / dur);
      const ease = 1 - Math.pow(1 - t, 3);
      const pos = pointAlongPath(m.points, ease);
      sig.setAttribute('cx', String(pos.x));
      sig.setAttribute('cy', String(pos.y));
      if (t <= 0.1) {
        sig.style.opacity = String(t / 0.1);
      } else if (t > 0.85) {
        sig.style.opacity = String((1 - t) / 0.15);
      } else {
        sig.style.opacity = '1';
      }
      if (pul) {
        pul.setAttribute('cx', String(pos.x));
        pul.setAttribute('cy', String(pos.y));
        if (t > 0.8) {
          const pt = (t - 0.8) / 0.2;
          pul.style.opacity = String(1 - pt);
          pul.setAttribute('r', String(12 + pt * 18));
        } else {
          pul.style.opacity = '0';
          pul.setAttribute('r', '12');
        }
      }
      if (t < 1) {
        animFrameRef.current = requestAnimationFrame(tick);
      } else {
        sig.style.opacity = '0';
        if (pul) pul.style.opacity = '0';
      }
    };
    animFrameRef.current = requestAnimationFrame(tick);
  }

  return (
    <div className="anim-host" ref={hostRef} style={{ position: 'absolute', inset: 0 }}>
      <div
        ref={stageRef}
        className="seq-stage"
        style={{
          position: 'absolute',
          left: 0,
          top: 0,
          transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
          transformOrigin: 'top left',
          willChange: 'transform',
        }}
      />
    </div>
  );
}

type MsgState = 'ready' | 'future' | 'active' | 'visited' | 'done';

function getMsgState(i: number, step: number, totalSteps: number): MsgState {
  if (step === 0) return 'ready';
  if (step >= totalSteps) return 'done';
  const activeIdx = step - 1;
  if (i === activeIdx) return 'active';
  if (i < activeIdx) return 'visited';
  return 'future';
}

function initMessage(m: MessageInfo) {
  const { edgeG, labelG, shape } = m;
  edgeG.classList.add('seq-msg');
  edgeG.style.transition = 'none';
  shape.style.transition = 'none';

  if (labelG) {
    labelG.classList.add('seq-label');
    labelG.style.transition = 'none';
    labelG.style.transformOrigin = 'center';
  }

  applyMsgVisual(m, 'ready', 0);
}

function applyMsgVisual(m: MessageInfo, state: MsgState, dur: number) {
  const { edgeG, labelG, shape } = m;
  edgeG.classList.remove('seq-active', 'seq-visited', 'seq-future', 'seq-done', 'seq-ready');
  edgeG.classList.add(`seq-${state}`);

  const trans = `opacity ${dur}ms ease, filter ${dur}ms ease`;
  const shapeTrans = `stroke ${dur}ms ease, stroke-width ${dur}ms ease`;
  const labelTrans = `opacity ${dur}ms ease, transform ${dur}ms cubic-bezier(0.34, 1.56, 0.64, 1)`;

  edgeG.style.transition = trans;
  shape.style.transition = shapeTrans;

  switch (state) {
    case 'ready':
    case 'done':
      edgeG.style.opacity = '1';
      (shape as SVGElement).style.filter = 'none';
      shape.style.strokeWidth = (parseFloat(shape.getAttribute('stroke-width') || '2')).toString();
      break;
    case 'future':
      edgeG.style.opacity = '0.3';
      (shape as SVGElement).style.filter = 'grayscale(0.6)';
      shape.style.strokeWidth = (parseFloat(shape.getAttribute('stroke-width') || '2')).toString();
      break;
    case 'active':
      edgeG.style.opacity = '1';
      shape.style.strokeWidth = String(Math.max(2.5, parseFloat(shape.getAttribute('stroke-width') || '2') + 0.8));
      (shape as SVGElement).style.filter = 'drop-shadow(0 0 6px rgba(99,102,241,0.8))';
      break;
    case 'visited':
      edgeG.style.opacity = '0.85';
      (shape as SVGElement).style.filter = 'none';
      shape.style.strokeWidth = (parseFloat(shape.getAttribute('stroke-width') || '2')).toString();
      break;
  }

  if (labelG) {
    labelG.style.transition = labelTrans;
    switch (state) {
      case 'ready':
      case 'done':
      case 'visited':
        labelG.style.opacity = '1';
        labelG.style.transform = 'scale(1)';
        break;
      case 'future':
        labelG.style.opacity = '0.35';
        labelG.style.transform = 'scale(0.95)';
        break;
      case 'active':
        labelG.style.opacity = '1';
        labelG.style.transform = 'scale(1.08)';
        break;
    }
  }
}
