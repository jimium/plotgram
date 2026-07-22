import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { IconZoomIn, IconZoomOut, IconFit, IconReset } from './Icons';
import {
  PREVIEW_BACKGROUNDS,
  PREVIEW_BG_LABELS,
  type PreviewBackground,
} from '../data/previewBackground';

interface PreviewProps {
  svg: string;
  ready: boolean;
  errorText: string | null;
  background: PreviewBackground;
  onBackgroundChange: (background: PreviewBackground) => void;
  /** 变化时触发一次"适应窗口"，用于切换示例。 */
  fitSignal: number;
}

const MIN_SCALE = 0.1;
const MAX_SCALE = 8;
const PAN_MARGIN = 0.3;

function clamp(v: number, min: number, max: number) {
  return Math.max(min, Math.min(max, v));
}

/** 从 SVG 源码解析 viewBox 尺寸 */
function readViewBox(svg: string): string | null {
  const match = svg.match(/viewBox="([\d\s.,-]+)"/);
  if (!match) return null;
  const parts = match[1].trim().split(/[\s,]+/);
  if (parts.length === 4) {
    const w = Math.round(Number.parseFloat(parts[2]));
    const h = Math.round(Number.parseFloat(parts[3]));
    return Number.isFinite(w) && Number.isFinite(h) ? `${w}×${h}` : null;
  }
  return null;
}

export function Preview({
  svg,
  ready,
  errorText,
  background,
  onBackgroundChange,
  fitSignal,
}: PreviewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const [tx, setTx] = useState(0);
  const [ty, setTy] = useState(0);
  const [glowKey, setGlowKey] = useState(0);
  const panState = useRef<{ x: number; y: number; tx: number; ty: number } | null>(null);
  const svgNaturalSizeRef = useRef<{ w: number; h: number }>({ w: 0, h: 0 });

  const viewBoxLabel = svg ? readViewBox(svg) : null;

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

  // 用 ref 保持最新的 fitToView，避免将其放入 effect 依赖引发"缩放→fitToView 覆盖"的循环
  const fitToViewRef = useRef<() => void>(() => {});

  const fitToView = useCallback(() => {
    const container = containerRef.current;
    const content = contentRef.current;
    if (!container || !content) return;
    const svgEl = content.querySelector('svg');
    if (!svgEl) return;

    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const bbox = svgEl.getBoundingClientRect();
    const naturalW = bbox.width / scale;
    const naturalH = bbox.height / scale;
    if (naturalW === 0 || naturalH === 0) return;

    const padding = 48;
    // 适应窗口时上限锁定 100%：小图保持原始尺寸，避免被放大导致模糊/失真
    const fitScale = Math.min((cw - padding) / naturalW, (ch - padding) / naturalH);
    const nextScale = clamp(Math.min(fitScale, 1), MIN_SCALE, MAX_SCALE);

    setScale(nextScale);
    setTx((cw - naturalW * nextScale) / 2);
    setTy((ch - naturalH * nextScale) / 2);
  }, [scale]);
  fitToViewRef.current = fitToView;

  const resetView = useCallback(() => {
    setScale(1);
    const container = containerRef.current;
    const content = contentRef.current;
    if (container && content) {
      const svgEl = content.querySelector('svg');
      if (svgEl) {
        const naturalW = svgEl.getBoundingClientRect().width / scale;
        setTx((container.clientWidth - naturalW) / 2);
        setTy(24);
        return;
      }
    }
    setTx(0);
    setTy(0);
  }, [scale]);

  // 首次出图：适应窗口
  const prevHasSvg = useRef(false);
  useEffect(() => {
    const hasSvg = Boolean(svg);
    if (hasSvg && !prevHasSvg.current) {
      requestAnimationFrame(() => {
        updateSvgNaturalSize();
        fitToViewRef.current();
      });
      // 触发成功发光动画
      setGlowKey((k) => k + 1);
    }
    prevHasSvg.current = hasSvg;
  }, [svg, updateSvgNaturalSize]);

  useEffect(() => {
    if (!svg) return;
    requestAnimationFrame(() => fitToViewRef.current());
  }, [fitSignal, svg]);

  const zoomBy = useCallback(
    (factor: number, originX?: number, originY?: number) => {
      const container = containerRef.current;
      if (!container) return;
      const rect = container.getBoundingClientRect();
      const px = originX ?? rect.width / 2;
      const py = originY ?? rect.height / 2;

      setScale((prev) => {
        const next = clamp(prev * factor, MIN_SCALE, MAX_SCALE);
        const ratio = next / prev;
        const nextTx = px - (px - tx) * ratio;
        const nextTy = py - (py - ty) * ratio;
        const clamped = clampPan(nextTx, nextTy, next);
        setTx(clamped.tx);
        setTy(clamped.ty);
        return next;
      });
    },
    [tx, ty, clampPan],
  );

  // 滚轮 / 双指手势：
  //   - ctrlKey=true（pinch zoom 捏合）→ 缩放
  //   - 普通滚动（双指上下/鼠标滚轮）→ 平移画面
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const listener = (e: WheelEvent) => {
      if (!svg) return;
      e.preventDefault();
      if (e.ctrlKey) {
        // pinch zoom：以光标为中心缩放
        const rect = container.getBoundingClientRect();
        const cx = e.clientX - rect.left;
        const cy = e.clientY - rect.top;
        const factor = Math.exp(-e.deltaY * 0.01);
        zoomBy(factor, cx, cy);
      } else {
        // 普通滚动：平移画面
        const nextTx = tx - e.deltaX;
        const nextTy = ty - e.deltaY;
        const clamped = clampPan(nextTx, nextTy, scale);
        setTx(clamped.tx);
        setTy(clamped.ty);
      }
    };
    container.addEventListener('wheel', listener, { passive: false });
    return () => container.removeEventListener('wheel', listener);
  }, [svg, zoomBy, tx, ty, scale, clampPan]);

  /** 阻止浮动工具栏的指针事件冒泡到画布，防止触发平移 */
  const stopProp = (e: ReactPointerEvent | React.MouseEvent) => e.stopPropagation();

  const handlePointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!svg) return;
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    panState.current = { x: e.clientX, y: e.clientY, tx, ty };
  };

  const handlePointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const pan = panState.current;
    if (!pan) return;
    const nextTx = pan.tx + (e.clientX - pan.x);
    const nextTy = pan.ty + (e.clientY - pan.y);
    const clamped = clampPan(nextTx, nextTy, scale);
    setTx(clamped.tx);
    setTy(clamped.ty);
  };

  const endPan = (e: ReactPointerEvent<HTMLDivElement>) => {
    panState.current = null;
    (e.target as HTMLElement).releasePointerCapture?.(e.pointerId);
  };

  const bgClass = `preview-canvas preview-bg-${background}`;

  return (
    <div className="preview-pane">
      <div
        ref={containerRef}
        className={`${bgClass}${glowKey > 0 && svg ? ' glow-success' : ''}`}
        key={glowKey > 0 && svg ? `glow-${glowKey}` : undefined}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={endPan}
        onPointerLeave={endPan}
        style={{ cursor: panState.current ? 'grabbing' : svg ? 'grab' : 'default' }}
      >
        {errorText ? (
          <div className="preview-message preview-error">
            <strong>渲染失败</strong>
            <span>{errorText}</span>
          </div>
        ) : svg ? (
          <div
            ref={contentRef}
            className="preview-content"
            style={{ transform: `translate(${tx}px, ${ty}px) scale(${scale})` }}
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        ) : (
          <div className="preview-message">
            {ready ? '在左侧输入 Plotgram，将在这里实时渲染。' : 'WASM 加载中…'}
          </div>
        )}

        {/* 背景切换（左上角浮动） */}
        {svg && (
          <div className="bg-switch-toolbar" onPointerDown={stopProp} onClick={stopProp}>
            {PREVIEW_BACKGROUNDS.map((b) => (
              <button
                key={b}
                type="button"
                className={`bg-chip bg-chip-${b} ${background === b ? 'active' : ''}`}
                onClick={() => onBackgroundChange(b)}
                title={PREVIEW_BG_LABELS[b]}
              />
            ))}
          </div>
        )}

        {/* viewBox 尺寸标签（右上角） */}
        {svg && viewBoxLabel && (
          <div className="viewbox-label" onPointerDown={stopProp}>{viewBoxLabel}</div>
        )}

        {/* 缩放工具栏（左下角浮动，Figma 风格） */}
        {svg && (
          <div className="zoom-toolbar" onPointerDown={stopProp} onClick={stopProp}>
            <button type="button" className="icon-btn" onClick={() => zoomBy(1 / 1.1)} title="缩小">
              <IconZoomOut />
            </button>
            <span className="zoom-label" onDoubleClick={resetView} title="双击重置为 100%">
              {Math.round(scale * 100)}%
            </span>
            <button type="button" className="icon-btn" onClick={() => zoomBy(1.1)} title="放大">
              <IconZoomIn />
            </button>
            <button type="button" className="icon-btn" onClick={fitToView} title="适应窗口">
              <IconFit />
            </button>
            <button type="button" className="icon-btn" onClick={resetView} title="重置视图">
              <IconReset />
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
