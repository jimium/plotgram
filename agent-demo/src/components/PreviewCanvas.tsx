/**
 * PreviewCanvas 预览画布
 *
 * 三栏布局左侧：SVG 预览 + DSL 源码切换 + 主题/导出工具栏。
 * 工具栏支持：主题切换、暗色模式、导出 SVG/PNG、在 draw.io 打开。
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { Button, Space, Tooltip, Spin, Empty, Segmented, Select, message } from 'antd';
import {
  ZoomInOutlined,
  ZoomOutOutlined,
  CompressOutlined,
  DownloadOutlined,
  BgColorsOutlined,
  MoonOutlined,
  SunOutlined,
  CopyOutlined,
} from '@ant-design/icons';
import { DslViewer } from './DslViewer';
import { RenderStructureViewer } from './RenderStructureViewer';
import {
  THEME_GROUPS,
  DEFAULT_APPEARANCE,
  buildRenderOptions,
  type AppearanceOptions,
} from '@lib/themes';
import { downloadSvg, downloadPng, openInDrawio, copyText } from '@lib/exportImage';
import type { PlotgramWasm } from '@lib/wasm';

interface PreviewCanvasProps {
  svg: string;
  source: string;
  wasm: PlotgramWasm | null;
  ready: boolean;
  isAgentRunning: boolean;
  onRerenderTheme: (optionsJson: string) => void;
  onRenderDrawio: (optionsJson: string) => string | null;
}

const MIN_SCALE = 0.1;
const MAX_SCALE = 8;

function clamp(v: number, min: number, max: number) {
  return Math.max(min, Math.min(max, v));
}

export function PreviewCanvas({
  svg,
  source,
  wasm,
  ready,
  isAgentRunning,
  onRerenderTheme,
  onRenderDrawio,
}: PreviewCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const [tx, setTx] = useState(0);
  const [ty, setTy] = useState(0);
  const [view, setView] = useState<'preview' | 'source' | 'structure'>('preview');
  const [appearance, setAppearance] = useState<AppearanceOptions>(DEFAULT_APPEARANCE);
  const [isDragging, setIsDragging] = useState(false);
  const scaleRef = useRef(scale);
  scaleRef.current = scale;
  const txRef = useRef(tx);
  txRef.current = tx;
  const tyRef = useRef(ty);
  tyRef.current = ty;
  const dragStateRef = useRef<{ startX: number; startY: number; startTx: number; startTy: number } | null>(null);

  // 主题变化时重新渲染
  const applyAppearance = useCallback(
    (opts: AppearanceOptions) => {
      setAppearance(opts);
      onRerenderTheme(JSON.stringify(buildRenderOptions(opts)));
    },
    [onRerenderTheme],
  );

  const fitToView = useCallback(() => {
    const container = containerRef.current;
    if (!container || !svg) return;
    const svgEl = container.querySelector('svg');
    if (!svgEl) return;

    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const viewBox = svgEl.viewBox.baseVal;
    let naturalW: number;
    let naturalH: number;
    if (viewBox && viewBox.width > 0 && viewBox.height > 0) {
      naturalW = viewBox.width;
      naturalH = viewBox.height;
    } else {
      const bbox = svgEl.getBoundingClientRect();
      const curScale = scaleRef.current || 1;
      naturalW = bbox.width / curScale;
      naturalH = bbox.height / curScale;
    }
    if (naturalW === 0 || naturalH === 0) return;

    const padding = 48;
    const nextScale = clamp(
      Math.min((cw - padding) / naturalW, (ch - padding) / naturalH),
      MIN_SCALE,
      MAX_SCALE,
    );
    setScale(nextScale);
    setTx((cw - naturalW * nextScale) / 2);
    setTy((ch - naturalH * nextScale) / 2);
  }, [svg]);

  useEffect(() => {
    if (svg && view === 'preview') {
      requestAnimationFrame(fitToView);
    }
  }, [svg, fitToView, view]);

  const zoomAt = useCallback((centerX: number, centerY: number, factor: number) => {
    setScale((prevScale) => {
      const nextScale = clamp(prevScale * factor, MIN_SCALE, MAX_SCALE);
      if (nextScale === prevScale) return prevScale;
      setTx(centerX - (centerX - txRef.current) * (nextScale / prevScale));
      setTy(centerY - (centerY - tyRef.current) * (nextScale / prevScale));
      return nextScale;
    });
  }, []);

  // 滚轮 / 双指手势：
  //   - ctrlKey=true（pinch zoom 捏合）→ 缩放
  //   - 普通滚动（双指上下/鼠标滚轮）→ 平移画面
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const listener = (e: WheelEvent) => {
      if (!svg || view !== 'preview') return;
      e.preventDefault();
      if (e.ctrlKey) {
        // pinch zoom：以光标为中心缩放
        const rect = container.getBoundingClientRect();
        const cx = e.clientX - rect.left;
        const cy = e.clientY - rect.top;
        const factor = Math.exp(-e.deltaY * 0.01);
        zoomAt(cx, cy, factor);
      } else {
        // 普通滚动：平移画面
        setTx((prev) => prev - e.deltaX);
        setTy((prev) => prev - e.deltaY);
      }
    };
    container.addEventListener('wheel', listener, { passive: false });
    return () => container.removeEventListener('wheel', listener);
  }, [svg, zoomAt, view]);

  // 拖拽平移：mousedown 记录起点，window mousemove/up 实时更新
  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!svg || view !== 'preview') return;
      if (e.button !== 0) return; // 只响应左键
      dragStateRef.current = {
        startX: e.clientX,
        startY: e.clientY,
        startTx: txRef.current,
        startTy: tyRef.current,
      };
      setIsDragging(true);
    },
    [svg, view],
  );

  useEffect(() => {
    if (!isDragging) return;
    const onMove = (e: MouseEvent) => {
      const s = dragStateRef.current;
      if (!s) return;
      setTx(s.startTx + (e.clientX - s.startX));
      setTy(s.startTy + (e.clientY - s.startY));
    };
    const onUp = () => {
      dragStateRef.current = null;
      setIsDragging(false);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [isDragging]);

  const zoomByButton = useCallback(
    (factor: number) => {
      const container = containerRef.current;
      if (!container) return;
      zoomAt(container.clientWidth / 2, container.clientHeight / 2, factor);
    },
    [zoomAt],
  );

  const resetTo100 = useCallback(() => {
    const container = containerRef.current;
    if (!container || !svg) return;
    const svgEl = container.querySelector('svg');
    if (!svgEl) return;
    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const viewBox = svgEl.viewBox.baseVal;
    let naturalW: number;
    let naturalH: number;
    if (viewBox && viewBox.width > 0 && viewBox.height > 0) {
      naturalW = viewBox.width;
      naturalH = viewBox.height;
    } else {
      const bbox = svgEl.getBoundingClientRect();
      naturalW = bbox.width / (scaleRef.current || 1);
      naturalH = bbox.height / (scaleRef.current || 1);
    }
    if (naturalW === 0 || naturalH === 0) return;
    setScale(1);
    setTx((cw - naturalW) / 2);
    setTy((ch - naturalH) / 2);
  }, [svg]);

  // 导出操作
  const handleExportSvg = () => {
    if (svg) {
      downloadSvg(svg);
      message.success('SVG 已导出');
    }
  };

  const handleExportPng = () => {
    if (svg) {
      downloadPng(svg, 'diagram.png', 2)
        .then(() => message.success('PNG 已导出 (2x)'))
        .catch(() => message.error('PNG 导出失败'));
    }
  };

  const handleDrawioOpen = () => {
    const xml = onRenderDrawio(JSON.stringify(buildRenderOptions(appearance)));
    if (xml) {
      openInDrawio(xml);
    } else {
      message.error('Drawio 生成失败');
    }
  };

  const handleCopyDsl = () => {
    if (source) {
      copyText(source)
        .then(() => message.success('DSL 已复制'))
        .catch(() => message.error('复制失败'));
    }
  };

  // 主题选项
  const themeOptions = THEME_GROUPS.flatMap((g) => [
    { label: g.label, options: g.options.map((o) => ({ label: o.label, value: o.value })) },
  ]);

  return (
    <div className="preview-pane">
      {/* 工具栏 */}
      <div className="preview-toolbar-top">
        <Segmented
          size="small"
          value={view}
          onChange={(v) => setView(v as 'preview' | 'source' | 'structure')}
          options={[
            { label: '预览', value: 'preview' },
            { label: 'DSL 源码', value: 'source' },
            { label: '渲染结构', value: 'structure' },
          ]}
        />
        <Space size={8} className="preview-toolbar-right">
          {view === 'preview' && svg && (
            <div className="preview-toolbar-group">
              <span className="preview-toolbar-label">
                <BgColorsOutlined />
                主题
              </span>
              <Select
                size="small"
                style={{ width: 130 }}
                value={appearance.themeId}
                onChange={(v) => applyAppearance({ ...appearance, themeId: v })}
                options={themeOptions}
              />
              <Tooltip title={appearance.darkMode ? '切换亮色' : '切换暗色'}>
                <Button
                  size="small"
                  type={appearance.darkMode ? 'primary' : 'text'}
                  icon={appearance.darkMode ? <SunOutlined /> : <MoonOutlined />}
                  onClick={() => applyAppearance({ ...appearance, darkMode: !appearance.darkMode })}
                />
              </Tooltip>
            </div>
          )}
          <Space size={4} className="preview-export-group">
            <Tooltip title="导出 SVG 文件">
              <Button size="small" icon={<DownloadOutlined />} onClick={handleExportSvg} disabled={!svg}>
                SVG
              </Button>
            </Tooltip>
            <Tooltip title="导出 PNG 图片 (2x)">
              <Button size="small" onClick={handleExportPng} disabled={!svg}>
                PNG
              </Button>
            </Tooltip>
            <Tooltip title="在 draw.io 中打开">
              <Button size="small" onClick={handleDrawioOpen} disabled={!svg}>
                draw.io
              </Button>
            </Tooltip>
            <Tooltip title="复制 DSL 源码">
              <Button size="small" icon={<CopyOutlined />} onClick={handleCopyDsl} disabled={!source}>
              </Button>
            </Tooltip>
          </Space>
        </Space>
      </div>

      {/* 内容区 */}
      {view === 'preview' ? (
        <div
          ref={containerRef}
          className="preview-canvas"
          onMouseDown={handleMouseDown}
          style={{
            cursor: isDragging
              ? 'grabbing'
              : svg
                ? 'grab'
                : 'default',
          }}
        >
          {isAgentRunning && !svg && (
            <div className="preview-loading">
              <Spin tip="Agent 正在生成图表...">
                <div style={{ minHeight: 80 }} />
              </Spin>
            </div>
          )}

          {svg ? (
            <div
              className="preview-content"
              style={{ transform: `translate(${tx}px, ${ty}px) scale(${scale})` }}
              dangerouslySetInnerHTML={{ __html: svg }}
            />
          ) : (
            !isAgentRunning && (
              <div className="preview-empty">
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description={
                    ready
                      ? '在右侧对话区输入需求，Agent 将为你生成图表'
                      : 'WASM 加载中...'
                  }
                />
              </div>
            )
          )}

          {isAgentRunning && svg && (
            <div className="preview-busy-badge">
              <Spin size="small" />
              <span style={{ marginLeft: 6, fontSize: 12 }}>Agent 执行中...</span>
            </div>
          )}

          {svg && (
            <div className="preview-toolbar">
              <Space size={4}>
                <Tooltip title="缩小">
                  <Button size="small" type="text" icon={<ZoomOutOutlined />} onClick={() => zoomByButton(1 / 1.1)} />
                </Tooltip>
                <Tooltip title="重置为 100%">
                  <Button
                    size="small"
                    type="text"
                    onClick={resetTo100}
                    style={{ fontSize: 12, minWidth: 48, textAlign: 'center', padding: '0 4px' }}
                  >
                    {Math.round(scale * 100)}%
                  </Button>
                </Tooltip>
                <Tooltip title="放大">
                  <Button size="small" type="text" icon={<ZoomInOutlined />} onClick={() => zoomByButton(1.1)} />
                </Tooltip>
                <Tooltip title="适应窗口">
                  <Button size="small" type="text" icon={<CompressOutlined />} onClick={fitToView} />
                </Tooltip>
              </Space>
            </div>
          )}
        </div>
      ) : view === 'source' ? (
        <div className="preview-source">
          <DslViewer source={source} />
        </div>
      ) : (
        <div className="preview-structure">
          <RenderStructureViewer source={source} wasm={wasm} ready={ready} />
        </div>
      )}
    </div>
  );
}
