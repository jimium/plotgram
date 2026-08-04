/**
 * ExportActions 导出操作组件
 */

import { downloadSvg, downloadPng, downloadText, downloadJson, copyText } from '@lib/exportImage';

interface ExportActionsProps {
  svg: string;
  ascii?: string;
  sceneJson?: string;
}

export function ExportActions({ svg, ascii, sceneJson }: ExportActionsProps) {
  return (
    <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
      {svg && (
        <>
          <button type="button" className="btn btn-ghost" onClick={() => downloadSvg(svg)}>
            下载 SVG
          </button>
          <button type="button" className="btn btn-ghost" onClick={() => copyText(svg)}>
            复制 SVG
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => downloadPng(svg)}
          >
            下载 PNG
          </button>
        </>
      )}
      {ascii && (
        <button type="button" className="btn btn-ghost" onClick={() => downloadText(ascii)}>
          下载 ASCII
        </button>
      )}
      {sceneJson && (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => downloadJson(sceneJson)}
        >
          下载 Scene JSON
        </button>
      )}
    </div>
  );
}
