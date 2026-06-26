/**
 * PreviewCanvas 预览画布
 *
 * 显示 Agent 渲染的 SVG 结果,支持缩放/平移
 * 不提供编辑能力(与 Playground 划清边界)
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { Button, Space, Tooltip, Spin, Empty } from 'antd';
import {
  ZoomInOutlined,
  ZoomOutOutlined,
  CompressOutlined,
} from '@ant-design/icons';
import type { DiffResult } from '@agent/types';
import { DiffSummary } from './DiffSummary';

interface PreviewCanvasProps {
  svg: string;
  ready: boolean;
  isAgentRunning: boolean;
  lastDiff: DiffResult | null;
}

const MIN_SCALE = 0.1;
const MAX_SCALE = 8;

function clamp(v: number, min: number, max: number) {
  return Math.max(min, Math.min(max, v));
}

export function PreviewCanvas({
  svg,
  ready,
  isAgentRunning,
  lastDiff,
}: PreviewCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const [tx, setTx] = useState(0);
  const [ty, setTy] = useState(0);
  // 用 ref 保存最新 scale,避免 fitToView 依赖 scale 导致闭包陷阱
  const scaleRef = useRef(scale);
  scaleRef.current = scale;

  // 适应窗口:只在 svg 变化时调用,不依赖 scale
  const fitToView = useCallback(() => {
    const container = containerRef.current;
    if (!container || !svg) return;
    const svgEl = container.querySelector('svg');
    if (!svgEl) return;

    const cw = container.clientWidth;
    const ch = container.clientHeight;
    // 用 viewBox 优先获取原始尺寸,回退到 getBoundingClientRect / 当前 scale
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

  // SVG 变化时自动适应(只在 svg 变化时触发,不依赖 scale)
  useEffect(() => {
    if (svg) {
      requestAnimationFrame(fitToView);
    }
  }, [svg, fitToView]);

  // 以指定坐标为中心缩放
  const zoomAt = useCallback((centerX: number, centerY: number, factor: number) => {
    setScale((prevScale) => {
      const nextScale = clamp(prevScale * factor, MIN_SCALE, MAX_SCALE);
      if (nextScale === prevScale) return prevScale;
      // 保持鼠标点在内容上的位置不变: tx' = centerX - (centerX - tx) * (nextScale/prevScale)
      setTx(centerX - (centerX - txRef.current) * (nextScale / prevScale));
      setTy(centerY - (centerY - tyRef.current) * (nextScale / prevScale));
      return nextScale;
    });
  }, []);

  // 用 ref 保存最新 tx/ty,供 zoomAt 闭包读取
  const txRef = useRef(tx);
  txRef.current = tx;
  const tyRef = useRef(ty);
  tyRef.current = ty;

  // 滚轮缩放:以鼠标位置为中心
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const listener = (e: WheelEvent) => {
      if (!svg) return;
      e.preventDefault();
      const factor = e.deltaY < 0 ? 1.05 : 1 / 1.05;
      zoomAt(e.offsetX, e.offsetY, factor);
    };
    container.addEventListener('wheel', listener, { passive: false });
    return () => container.removeEventListener('wheel', listener);
  }, [svg, zoomAt]);

  // 按钮缩放:以容器中心为基准
  const zoomByButton = useCallback(
    (factor: number) => {
      const container = containerRef.current;
      if (!container) return;
      zoomAt(container.clientWidth / 2, container.clientHeight / 2, factor);
    },
    [zoomAt],
  );

  return (
    <div className="preview-pane">
      <div ref={containerRef} className="preview-canvas">
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
                    ? '在右侧对话区输入需求,Agent 将为你生成图表'
                    : 'WASM 加载中...'
                }
              />
            </div>
          )
        )}

        {/* 增量编辑时的 loading 指示(已有 svg 时) */}
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
                <Button
                  size="small"
                  type="text"
                  icon={<ZoomOutOutlined />}
                  onClick={() => zoomByButton(1 / 1.1)}
                />
              </Tooltip>
              <span style={{ fontSize: 12, minWidth: 48, textAlign: 'center' }}>
                {Math.round(scale * 100)}%
              </span>
              <Tooltip title="放大">
                <Button
                  size="small"
                  type="text"
                  icon={<ZoomInOutlined />}
                  onClick={() => zoomByButton(1.1)}
                />
              </Tooltip>
              <Tooltip title="适应窗口">
                <Button
                  size="small"
                  type="text"
                  icon={<CompressOutlined />}
                  onClick={fitToView}
                />
              </Tooltip>
            </Space>
          </div>
        )}
      </div>

      {lastDiff && lastDiff.changes.length > 0 && (
        <div className="preview-diff">
          <DiffSummary diff={lastDiff} />
        </div>
      )}
    </div>
  );
}
