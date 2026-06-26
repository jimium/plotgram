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

export function downloadText(text: string, filename = 'diagram.txt'): void {
  triggerDownload(new Blob([text], { type: 'text/plain;charset=utf-8' }), filename);
}

export function downloadJson(json: string, filename = 'diagram.json'): void {
  triggerDownload(new Blob([json], { type: 'application/json;charset=utf-8' }), filename);
}

/** 下载 Drawio (.drawio) XML 文件 */
export function downloadDrawio(xml: string, filename = 'diagram.drawio'): void {
  triggerDownload(new Blob([xml], { type: 'application/xml;charset=utf-8' }), filename);
}

/** 在 draw.io (diagrams.net) 中直接打开 XML。
 *  使用 URL hash scheme: https://app.diagrams.net/#R{encodedXml} */
export function openInDrawio(xml: string): void {
  const encoded = encodeURIComponent(xml);
  const url = `https://app.diagrams.net/?mode=device#R${encoded}`;
  window.open(url, '_blank', 'noopener');
}

/** 从 SVG 源码解析出渲染像素尺寸（优先 width/height，其次 viewBox）。 */
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

async function svgToRasterBlob(
  svg: string,
  mimeType: 'image/png' | 'image/webp',
  scale = 2,
  quality?: number,
): Promise<Blob> {
  const { width, height } = readSvgSize(svg);
  const img = await svgToImage(svg);

  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(width * scale));
  canvas.height = Math.max(1, Math.round(height * scale));

  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('无法创建画布上下文');
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

  const label = mimeType === 'image/png' ? 'PNG' : 'WebP';

  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (blob) resolve(blob);
        else reject(new Error(`${label} 编码失败（当前浏览器可能不支持该格式）`));
      },
      mimeType,
      quality,
    );
  });
}

export async function svgToPngBlob(svg: string, scale = 2): Promise<Blob> {
  return svgToRasterBlob(svg, 'image/png', scale);
}

export async function svgToWebpBlob(svg: string, scale = 2, quality = 0.92): Promise<Blob> {
  return svgToRasterBlob(svg, 'image/webp', scale, quality);
}

export async function downloadPng(svg: string, filename = 'diagram.png', scale = 2): Promise<void> {
  const blob = await svgToPngBlob(svg, scale);
  triggerDownload(blob, filename);
}

export async function downloadWebp(
  svg: string,
  filename = 'diagram.webp',
  scale = 2,
  quality = 0.92,
): Promise<void> {
  const blob = await svgToWebpBlob(svg, scale, quality);
  triggerDownload(blob, filename);
}

export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}

export async function copyPngToClipboard(svg: string, scale = 2): Promise<void> {
  const blob = await svgToPngBlob(svg, scale);
  const item = new ClipboardItem({ 'image/png': blob });
  await navigator.clipboard.write([item]);
}
