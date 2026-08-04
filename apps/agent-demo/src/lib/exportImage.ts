/**
 * 导出工具（从 playground 精简移植）
 * 支持 SVG / PNG 下载、drawio 下载与在线打开、剪贴板复制
 */

function triggerDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

export function downloadSvg(svg: string, filename = 'diagram.svg'): void {
  triggerDownload(new Blob([svg], { type: 'image/svg+xml;charset=utf-8' }), filename);
}

export function downloadDrawio(xml: string, filename = 'diagram.drawio'): void {
  triggerDownload(new Blob([xml], { type: 'application/xml;charset=utf-8' }), filename);
}

/** 在 draw.io (diagrams.net) 中直接打开 XML */
export function openInDrawio(xml: string): void {
  const encoded = encodeURIComponent(xml);
  const url = `https://app.diagrams.net/?mode=device#R${encoded}`;
  window.open(url, '_blank', 'noopener');
}

function readSvgSize(svg: string): { width: number; height: number } {
  const doc = new DOMParser().parseFromString(svg, 'image/svg+xml');
  const el = doc.documentElement;
  const parseLen = (v: string | null): number | null => {
    if (!v) return null;
    const n = Number.parseFloat(v);
    return Number.isFinite(n) ? n : null;
  };
  let width = parseLen(el.getAttribute('width'));
  let height = parseLen(el.getAttribute('height'));
  if (!width || !height) {
    const viewBox = el.getAttribute('viewBox');
    if (viewBox) {
      const parts = viewBox.split(/[\s,]+/).map(Number);
      if (parts.length === 4) {
        width = width || parts[2];
        height = height || parts[3];
      }
    }
  }
  return { width: width || 800, height: height || 600 };
}

function svgToImage(svg: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const blob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const img = new Image();
    img.onload = () => {
      URL.revokeObjectURL(url);
      resolve(img);
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error('SVG 转图片失败'));
    };
    img.src = url;
  });
}

export async function downloadPng(svg: string, filename = 'diagram.png', scale = 2): Promise<void> {
  const { width, height } = readSvgSize(svg);
  const img = await svgToImage(svg);
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(width * scale));
  canvas.height = Math.max(1, Math.round(height * scale));
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('无法创建画布上下文');
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  await new Promise<void>((resolve, reject) => {
    canvas.toBlob(
      (blob) => (blob ? (triggerDownload(blob, filename), resolve()) : reject(new Error('PNG 编码失败'))),
      'image/png',
    );
  });
}

export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}
