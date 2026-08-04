/**
 * 图表导出工具
 *
 * 复用 playground 的导出逻辑,提供 SVG/PNG/WebP/文本下载
 */

/** 下载 SVG 文件 */
export function downloadSvg(svg: string, filename: string = 'diagram.svg'): void {
  const blob = new Blob([svg], { type: 'image/svg+xml' });
  triggerDownload(blob, filename);
}

/** 下载文本文件(ASCII 等) */
export function downloadText(text: string, filename: string = 'diagram.txt'): void {
  const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
  triggerDownload(blob, filename);
}

/** 下载 JSON 文件 */
export function downloadJson(json: string, filename: string = 'diagram.json'): void {
  const blob = new Blob([json], { type: 'application/json' });
  triggerDownload(blob, filename);
}

/** 下载 PNG(从 SVG 栅格化) */
export async function downloadPng(
  svg: string,
  filename: string = 'diagram.png',
  scale: number = 2,
): Promise<void> {
  const blob = await rasterizeSvg(svg, scale, 'image/png');
  triggerDownload(blob, filename);
}

/** 下载 WebP(从 SVG 栅格化) */
export async function downloadWebp(
  svg: string,
  filename: string = 'diagram.webp',
  scale: number = 2,
): Promise<void> {
  const blob = await rasterizeSvg(svg, scale, 'image/webp');
  triggerDownload(blob, filename);
}

/** 复制文本到剪贴板 */
export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}

/** 复制 PNG 到剪贴板 */
export async function copyPngToClipboard(svg: string, scale: number = 2): Promise<void> {
  const blob = await rasterizeSvg(svg, scale, 'image/png');
  await navigator.clipboard.write([
    new ClipboardItem({ 'image/png': blob }),
  ]);
}

/** 触发浏览器下载 */
function triggerDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/** 将 SVG 栅格化为 Blob */
async function rasterizeSvg(
  svg: string,
  scale: number,
  type: 'image/png' | 'image/webp',
): Promise<Blob> {
  const img = new Image();
  const svgBlob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
  const url = URL.createObjectURL(svgBlob);

  try {
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error('SVG 加载失败'));
      img.src = url;
    });

    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('无法获取 Canvas 2D 上下文');

    canvas.width = img.naturalWidth * scale;
    canvas.height = img.naturalHeight * scale;
    ctx.scale(scale, scale);
    ctx.drawImage(img, 0, 0);

    const blob = await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (b) => (b ? resolve(b) : reject(new Error('Canvas 转 Blob 失败'))),
        type,
        0.92,
      );
    });

    return blob;
  } finally {
    URL.revokeObjectURL(url);
  }
}
