import { useEffect, useRef, useState, useCallback } from 'react';
import type { ChangeJson } from '../lib/wasm';

interface AuditAnimatorProps {
  svg: string;
  prevSvg?: string;
  changes?: ChangeJson[];
  showHighlights?: boolean;
  duration?: number;
  bgGrid?: boolean;
  highlightOnlyOnComplete?: boolean;
}

const DURATION = 700;
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

function edgeBaseKey(g: SVGGElement): string {
  const from = g.getAttribute('data-dfy-from');
  const to = g.getAttribute('data-dfy-to');
  return `${from}->${to}`;
}

function edgeLabelText(g: SVGGElement): string | null {
  const texts = g.querySelectorAll('text');
  for (const t of Array.from(texts)) {
    const content = t.textContent?.trim();
    if (content) return content;
  }
  return null;
}

function edgeFullKey(g: SVGGElement): string {
  const base = edgeBaseKey(g);
  const label = edgeLabelText(g);
  return label ? `${base}::${label}` : base;
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

type ChangeKind = 'add' | 'remove' | 'modify';

function classifyChanges(changes: ChangeJson[] | undefined): {
  addedEntities: Set<string>;
  removedEntities: Set<string>;
  modifiedEntities: Set<string>;
  addedEdges: Set<string>;
  removedEdges: Set<string>;
  modifiedEdges: Set<string>;
} {
  const addedEntities = new Set<string>();
  const removedEntities = new Set<string>();
  const modifiedEntities = new Set<string>();
  const addedEdges = new Set<string>();
  const removedEdges = new Set<string>();
  const modifiedEdges = new Set<string>();

  if (!changes) return { addedEntities, removedEntities, modifiedEntities, addedEdges, removedEdges, modifiedEdges };

  for (const c of changes) {
    const { op, path } = c;
    if (path.target === 'entity' && path.id) {
      if (op === 'add') addedEntities.add(path.id);
      else if (op === 'remove') removedEntities.add(path.id);
      else {
        if (!addedEntities.has(path.id)) modifiedEntities.add(path.id);
      }
    }
    if (path.target === 'relation' && path.id) {
      const eKey = path.id;
      if (op === 'add') addedEdges.add(eKey);
      else if (op === 'remove') removedEdges.add(eKey);
      else {
        if (!addedEdges.has(eKey)) modifiedEdges.add(eKey);
      }
    }
  }
  return { addedEntities, removedEntities, modifiedEntities, addedEdges, removedEdges, modifiedEdges };
}

function edgeIsChanged(g: SVGGElement, set: Set<string>): boolean {
  const full = edgeFullKey(g);
  if (set.has(full)) return true;
  const base = edgeBaseKey(g);
  if (set.has(base)) return true;
  const from = g.getAttribute('data-dfy-from');
  const to = g.getAttribute('data-dfy-to');
  if (from && to) {
    for (const k of set) {
      if (k.startsWith(`${from}->${to}`)) return true;
    }
  }
  return false;
}

function edgeElements(g: SVGGElement): SVGGeometryElement[] {
  return Array.from(g.querySelectorAll<SVGGeometryElement>('line, path, polyline, polygon'));
}

function applyAuditHighlight(g: SVGGElement, kind: ChangeKind) {
  g.classList.add('audit-change', `audit-${kind}`);
  g.dataset.auditKind = kind;

  if (kind === 'add') {
    edgeElements(g).forEach((s) => {
      s.classList.add('audit-stroke-add');
    });
  } else if (kind === 'remove') {
    edgeElements(g).forEach((s) => {
      s.classList.add('audit-stroke-remove');
    });
  } else {
    edgeElements(g).forEach((s) => {
      s.classList.add('audit-stroke-modify');
    });
  }
}

export function AuditAnimator({
  svg,
  prevSvg,
  changes,
  showHighlights = true,
  duration = DURATION,
  bgGrid = true,
}: AuditAnimatorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const layerIdRef = useRef(0);
  const layersRef = useRef<Layer[]>([]);
  const animatingRef = useRef(false);
  const [view, setView] = useState({ scale: 1, x: 0, y: 0 });

  const classified = useRef(classifyChanges(changes));
  classified.current = classifyChanges(changes);

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
    const prevSvgEl = prevLayer?.svg;

    const hasPrev = !!(prevLayer && prevSvgEl && !animatingRef.current && prevSvg);
    const cls = classified.current;

    stage.appendChild(newWrap);
    layersRef.current.push(newLayer);

    if (showHighlights && !hasPrev) {
      allNewAuditMarks(newSvg, cls);
    }

    if (!hasPrev) {
      fit();
      return;
    }
    animatingRef.current = true;
    newWrap.style.opacity = '0';

    const allNewNodes = Array.from(newSvg.querySelectorAll<SVGGElement>('g[data-dfy-kind="node"]'));
    const allNewEdges = Array.from(newSvg.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge"]'));
    const allNewLabels = Array.from(newSvg.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge-label"]'));

    const newNodeMap = new Map<string, SVGGElement>();
    allNewNodes.forEach((g) => {
      const id = g.getAttribute('data-dfy-id');
      if (id) newNodeMap.set(id, g);
    });

    const prevNodes = Array.from(prevSvgEl!.querySelectorAll<SVGGElement>('g[data-dfy-kind="node"]'));
    const prevEdges = Array.from(prevSvgEl!.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge"]'));

    const prevNodeMap = new Map<string, SVGGElement>();
    prevNodes.forEach((g) => {
      const id = g.getAttribute('data-dfy-id');
      if (id) prevNodeMap.set(id, g);
    });

    const newEdgeShapes: { g: SVGGElement; shapes: SVGGeometryElement[]; lengths: number[] }[] = [];
    allNewEdges.forEach((g) => {
      const shapes = edgeElements(g);
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
    if (newTitle) newTitle.style.opacity = '0.001';
    const newAttr = newSvg.querySelector('g.drawify-attribution') as SVGGElement | null;
    if (newAttr) newAttr.style.opacity = '0.001';

    prevNodes.forEach((g) => {
      const id = g.getAttribute('data-dfy-id');
      const isRemoved = !!(id && cls.removedEntities.has(id));
      if (isRemoved) {
        g.classList.add('audit-change', 'audit-remove');
        edgeElements(g).forEach((s) => s.classList.add('audit-stroke-remove'));
      }
    });
    prevEdges.forEach((g) => {
      const isRemoved = edgeIsChanged(g, cls.removedEdges);
      if (isRemoved) {
        g.classList.add('audit-change', 'audit-remove');
        edgeElements(g).forEach((s) => s.classList.add('audit-stroke-remove'));
      }
    });

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

        const removedDur = duration;
        prevNodes.forEach((g) => {
          const id = g.getAttribute('data-dfy-id');
          const stays = id ? newNodeMap.has(id) : false;
          const isRemoved = !!(id && cls.removedEntities.has(id));
          if (isRemoved) {
            setTransitions(g, ['transform', 'opacity'], removedDur, EASE_IN);
            g.style.transform = 'translate(0, 20px)';
            g.style.opacity = '0';
          } else if (stays) {
            setTransitions(g, ['opacity'], duration, EASE_IN);
            g.style.opacity = '0';
          } else {
            setTransitions(g, ['opacity'], duration, EASE_IN);
            g.style.opacity = '0';
          }
        });
        prevEdges.forEach((g) => {
          setTransitions(g, ['opacity'], duration, EASE_IN);
          g.style.opacity = '0';
        });
        const prevTitle = Array.from(prevSvgEl!.children).find((c) => c.tagName === 'text') as SVGElement | undefined;
        if (prevTitle) { setTransitions(prevTitle, ['opacity'], duration, EASE_IN); prevTitle.style.opacity = '0'; }
        const prevAttrEl = prevSvgEl!.querySelector('g.drawify-attribution') as SVGGElement | null;
        if (prevAttrEl) { setTransitions(prevAttrEl, ['opacity'], duration, EASE_IN); prevAttrEl.style.opacity = '0'; }

        window.setTimeout(() => {
          if (showHighlights) {
            allNewAuditMarks(newSvg, cls);
          }

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
        }, duration + 100);
      });
    });
  }, [svg, prevSvg, changes, showHighlights, duration, fit]);

  useEffect(() => {
    const id = requestAnimationFrame(fit);
    return () => cancelAnimationFrame(id);
  }, [fit, svg]);

  useEffect(() => {
    const top = layersRef.current[layersRef.current.length - 1];
    if (!top) return;
    const cls = classified.current;
    const svgEl = top.svg;

    svgEl.querySelectorAll('.audit-change').forEach((el) => {
      el.classList.remove('audit-change', 'audit-add', 'audit-remove', 'audit-modify', 'audit-highlight-on');
      el.classList.remove('audit-stroke-add', 'audit-stroke-remove', 'audit-stroke-modify');
      (el as SVGGElement).dataset.auditKind = '';
    });
    svgEl.querySelectorAll('.audit-badge').forEach((el) => el.remove());

    if (showHighlights) {
      allNewAuditMarks(svgEl, cls);
    }
  }, [showHighlights]);

  return (
    <div className={`anim-host audit-host${bgGrid ? ' anim-grid-bg' : ''}`} ref={hostRef}>
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

function allNewAuditMarks(svgEl: SVGSVGElement, cls: ReturnType<typeof classifyChanges>) {
  svgEl.querySelectorAll<SVGGElement>('g[data-dfy-kind="node"]').forEach((g) => {
    const id = g.getAttribute('data-dfy-id');
    if (!id) return;
    if (cls.addedEntities.has(id)) applyAuditHighlight(g, 'add');
    else if (cls.modifiedEntities.has(id)) applyAuditHighlight(g, 'modify');
  });
  svgEl.querySelectorAll<SVGGElement>('g[data-dfy-kind="edge"]').forEach((g) => {
    if (edgeIsChanged(g, cls.addedEdges)) applyAuditHighlight(g, 'add');
    else if (edgeIsChanged(g, cls.modifiedEdges)) applyAuditHighlight(g, 'modify');
  });
}
