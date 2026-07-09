/**
 * 图表导出工具（演示版精简）
 *
 * 只保留 SVG 下载，演示场景不需要 PNG/WebP 栅格化。
 */

export function downloadSvg(svg: string, filename: string = 'plotgram-diagram.svg'): void {
  const blob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
