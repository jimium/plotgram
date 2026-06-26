import { useEffect, useRef, useState, useCallback } from 'react';

interface AnimCanvasProps {
  svg: string;
  duration?: number;
  bgGrid?: boolean;
}

const DURATION = 550;
const EASE_OUT = 'cubic-bezier(0.22, 1, 0.36, 1)';
const EASE_IN = 'cubic-bezier(0.55, 0, 0.68, 0.5)';
const EASE_STD = 'cubic-bezier(0.4, 0, 0.2, 1)';

function readViewBox(svgEl: SVGSVGElement): { w: number; h: number } | null {
  const vb = svgEl.getAttribute('viewBox');
  if (!vb) return null;
  const parts = vb.split(/[\s,]+/).map(Number);
  if (parts.length === 4 && Number.isFinite(parts[2]) && Number.isFinite(parts[3])) {
    return { w: parts[2], h: parts[3] };
  }
  return null;
}

function parseSvg(svgStr: string): SVGSVGElement | null {
  const parser = new DOMParser();
  const doc = parser.parseFromString(svgStr, 'image/svg+xml');
  return doc.querySelector('svg') as SVGSVGElement | null;
}

function svgCenter(el: SVGGraphicsElement): { x: number; y: number } {
  const bbox = el.getBBox();
  const ctm = el.getCTM();
  const lx = bbox.x + bbox.width / 2;
  const ly = bbox.y + bbox.height / 2;
  if (!ctm) return { x: lx, y: ly };
  return {
    x: ctm.a * lx + ctm.c * ly + ctm.e,
    y: ctm.b * lx + ctm.d * ly + ctm.f,
  };
}

function edgeKey(g: SVGGElement): string {
  const from = g.getAttribute('data-dfy-from');
  const to = g.getAttribute('data-dfy-to');
  const idx = g.getAttribute('data-dfy-index') || '0';
  return `${from}->${to}#${idx}`;
}

function setTransitions(el: SVGElement | HTMLElement, props: string[], dur: number, ease: string) {
  (el as HTMLElement).style.transition = props
    .map((p) => `${p} ${dur}ms ${ease}`)
    .join(', ');
}

function clearTransitions(el: SVGElement | HTMLElement) {
  (el as HTMLElement).style.transition = '';
}

interface Layer {
  id: number;
  wrap: HTMLDivElement;
  svg: SVGSVGElement;
}

export function SvgAnimator({ svg, duration = DURATION, bgGrid = true }: AnimCanvasProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const layerIdRef = useRef(0);
  const layersRef = useRef<Layer[]>([]);
  const animatingRef = useRef(false);
  const [view, setView] = useState({ scale: 1, x: 0, y: 0 });

  const fit = useCallback(() => {
    const host = hostRef.current;
    if (!host) return;
    const top = layersRef.current[layersRef.current.length - 1];
    if (!top) return;
    const vb = readViewBox(top.svg);
    if (!vb) return;
    const cw = host.clientWidth;
    const ch = host.clientHeight;
    const pad = 32;
    const s = Math.min((cw - pad * 2) / vb.w, (ch - pad * 2) / vb.h);
    const ox = (cw - vb.w * s) / 2;
    const oy = (ch - vb.h * s) / 2;
    setView({ scale: s, x: ox, y: oy });
  }, []);

  useEffect(() => {
    const onResize = () => requestAnimationFrame(fit);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [fit]);

  useEffect(() => {
    const host = hostRef.current;
    const stage = stageRef.current;
    if (!host || !stage || !svg) return;

    const newSvg = parseSvg(svg);
    if (!newSvg) return;

    const vb = readViewBox(newSvg);
    if (vb) {
      newSvg.setAttribute('width', String(vb.w));
      newSvg.setAttribute('height', String(vb.h));
    }

    const newWrap = document.createElement('div');
    newWrap.style.cssText = 'position:absolute;left:0;top:0;';
    newWrap.appendChild(newSvg);

    const newId = ++layerIdRef.current;
    const newLayer: Layer = { id: newId, wrap: newWrap, svg: newSvg };
    const prevLayer = layersRef.current[layersRef.current.length - 1];
    const prevSvg = prevLayer?.svg;
    const willAnimate = !!(prevLayer && prevSvg && !animatingRef.current);

    const allNewNodes = Array.from(newSvg.querySelectorAll<SVGGElement>('g[data-dfy-kind="node"]'));
    const allNewEdges = Array.from(newSvg.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge"]'));
    const allNewLabels = Array.from(newSvg.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge-label"]'));

    const newNodeMap = new Map<string, SVGGElement>();
    allNewNodes.forEach((g) => {
      const id = g.getAttribute('data-dfy-id');
      if (id) newNodeMap.set(id, g);
    });
    const newEdgeMap = new Map<string, SVGGElement>();
    allNewEdges.forEach((g) => {
      if (g.getAttribute('data-dfy-from')) newEdgeMap.set(edgeKey(g), g);
    });

    stage.appendChild(newWrap);
    layersRef.current.push(newLayer);

    if (!willAnimate) {
      fit();
      return;
    }
    animatingRef.current = true;
    newWrap.style.opacity = '0';

    const prevNodes = Array.from(prevSvg!.querySelectorAll<SVGGElement>('g[data-dfy-kind="node"]'));
    const prevEdges = Array.from(prevSvg!.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge"]'));

    const prevNodeMap = new Map<string, SVGGElement>();
    prevNodes.forEach((g) => {
      const id = g.getAttribute('data-dfy-id');
      if (id) prevNodeMap.set(id, g);
    });
    const prevEdgeMap = new Map<string, SVGGElement>();
    prevEdges.forEach((g) => {
      if (g.getAttribute('data-dfy-from')) prevEdgeMap.set(edgeKey(g), g);
    });

    const newEdgeShapes: { g: SVGGElement; shapes: SVGGeometryElement[]; lengths: number[] }[] = [];
    allNewEdges.forEach((g) => {
      const shapes = Array.from(
        g.querySelectorAll<SVGGeometryElement>('line, path, polyline, polygon, rect, circle, ellipse')
      );
      const lengths = shapes.map((s) => {
        if ('getTotalLength' in s && typeof (s as SVGGeometryElement).getTotalLength === 'function') {
          try { return (s as SVGGeometryElement).getTotalLength(); } catch { /* ignore */ }
        }
        if (s instanceof SVGLineElement) {
          return Math.hypot(
            (Number(s.getAttribute('x2')) || 0) - (Number(s.getAttribute('x1')) || 0),
            (Number(s.getAttribute('y2')) || 0) - (Number(s.getAttribute('y1')) || 0)
          );
        }
        return 200;
      });
      newEdgeShapes.push({ g, shapes, lengths });
    });

    allNewNodes.forEach((g) => {
      const id = g.getAttribute('data-dfy-id')!;
      const prev = prevNodeMap.get(id);
      if (prev) {
        const pc = svgCenter(prev);
        const nc = svgCenter(g);
        const dx = pc.x - nc.x;
        const dy = pc.y - nc.y;
        g.style.opacity = '0.001';
        g.style.transform = `translate(${dx}px, ${dy}px)`;
      } else {
        g.style.opacity = '0.001';
        g.style.transform = 'translate(0, -20px)';
      }
    });

    allNewLabels.forEach((g) => {
      g.style.opacity = '0.001';
    });

    newEdgeShapes.forEach(({ g, shapes, lengths }) => {
      g.style.opacity = '0.001';
      shapes.forEach((s, i) => {
        const st = s.style as CSSStyleDeclaration;
        st.strokeDasharray = `${lengths[i]}`;
        st.strokeDashoffset = `${lengths[i]}`;
      });
    });

    const newTitle = Array.from(newSvg.children).find((c) => c.tagName === 'text') as SVGElement | undefined;
    if (newTitle) { newTitle.style.opacity = '0.001'; }
    const newAttr = newSvg.querySelector('g.drawify-attribution') as SVGGElement | null;
    if (newAttr) { newAttr.style.opacity = '0.001'; }

    requestAnimationFrame(() => {
      newWrap.style.opacity = '1';
      requestAnimationFrame(() => {
        allNewNodes.forEach((g) => {
          setTransitions(g, ['transform', 'opacity'], duration, EASE_OUT);
          g.style.transform = 'translate(0, 0)';
          g.style.opacity = '1';
        });
        allNewLabels.forEach((g) => {
          setTransitions(g, ['opacity'], duration, EASE_STD);
          g.style.opacity = '1';
        });
        newEdgeShapes.forEach(({ g, shapes }) => {
          setTransitions(g, ['opacity'], duration, EASE_STD);
          g.style.opacity = '1';
          shapes.forEach((s) => {
            const st = s.style as CSSStyleDeclaration;
            st.transition = `stroke-dashoffset ${duration}ms ${EASE_OUT}, opacity ${duration}ms ${EASE_STD}`;
            st.strokeDashoffset = '0';
          });
        });
        if (newTitle) { setTransitions(newTitle, ['opacity'], duration, EASE_STD); newTitle.style.opacity = '1'; }
        if (newAttr) { setTransitions(newAttr, ['opacity'], duration, EASE_STD); newAttr.style.opacity = '1'; }

        prevNodes.forEach((g) => {
          const id = g.getAttribute('data-dfy-id');
          const stays = id ? newNodeMap.has(id) : false;
          if (stays) {
            setTransitions(g, ['opacity'], duration, EASE_IN);
            g.style.opacity = '0';
          } else {
            setTransitions(g, ['transform', 'opacity'], duration, EASE_IN);
            g.style.transform = 'translate(0, 20px)';
            g.style.opacity = '0';
          }
        });
        prevEdges.forEach((g) => {
          setTransitions(g, ['opacity'], duration, EASE_IN);
          g.style.opacity = '0';
        });
        const prevTitle = Array.from(prevSvg!.children).find((c) => c.tagName === 'text') as SVGElement | undefined;
        if (prevTitle) { setTransitions(prevTitle, ['opacity'], duration, EASE_IN); prevTitle.style.opacity = '0'; }
        const prevAttr = prevSvg!.querySelector('g.drawify-attribution') as SVGGElement | null;
        if (prevAttr) { setTransitions(prevAttr, ['opacity'], duration, EASE_IN); prevAttr.style.opacity = '0'; }

        window.setTimeout(() => {
          [prevLayer].forEach((l) => {
            if (l.id !== newId) l.wrap.remove();
          });
          layersRef.current = layersRef.current.filter((l) => l.id === newId);

          allNewNodes.forEach((g) => { clearTransitions(g); g.style.transform = ''; g.style.opacity = ''; });
          allNewLabels.forEach((g) => { clearTransitions(g); g.style.opacity = ''; });
          newEdgeShapes.forEach(({ g, shapes }) => {
            clearTransitions(g);
            g.style.opacity = '';
            shapes.forEach((s) => {
              const st = s.style as CSSStyleDeclaration;
              st.transition = '';
              st.strokeDasharray = '';
              st.strokeDashoffset = '';
            });
          });
          if (newTitle) { clearTransitions(newTitle); newTitle.style.opacity = ''; }
          if (newAttr) { clearTransitions(newAttr); newAttr.style.opacity = ''; }
          animatingRef.current = false;
          fit();
        }, duration + 60);
      });
    });
  }, [svg, duration, fit]);

  useEffect(() => {
    const id = requestAnimationFrame(fit);
    return () => cancelAnimationFrame(id);
  }, [fit, svg]);

  return (
    <div className={`anim-host${bgGrid ? ' anim-grid-bg' : ''}`} ref={hostRef}>
      <div
        ref={stageRef}
        className="anim-stage"
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
