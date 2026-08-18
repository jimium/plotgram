const ALLOWED_PREFIXES = [
  'flowchart/',
  'sequence/',
  'architecture/',
  'state/',
  'er/',
  'mindmap/',
] as const;

/** 校验 showcase 相对路径（仅指针，非 DSL 正文）。 */
export function validateTautPath(path: string): string | null {
  const trimmed = path.trim();
  if (!trimmed || trimmed.includes('..') || trimmed.includes('\\')) {
    return null;
  }
  if (!trimmed.endsWith('.taut')) {
    return null;
  }
  const allowed = ALLOWED_PREFIXES.some((prefix) => trimmed.startsWith(prefix));
  return allowed ? trimmed : null;
}

export function readTautQuery(): string | null {
  const raw = new URLSearchParams(window.location.search).get('taut');
  if (!raw) return null;
  return validateTautPath(raw);
}

export function filenameFromTautPath(path: string): string {
  const parts = path.split('/');
  return parts[parts.length - 1] || '未命名.taut';
}

/** 从站点根路径 fetch showcase 下的 .taut 文件。 */
export async function fetchShowcaseTaut(path: string): Promise<string> {
  const safe = validateTautPath(path);
  if (!safe) {
    throw new Error('非法的样例路径');
  }
  const url = `/showcase/${safe}`;
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }
  const text = await res.text();
  if (!text.trim()) {
    throw new Error('文件为空');
  }
  return text;
}

/** Showcase → Editor 启动参数；载入后从地址栏清除，避免残留。 */
const SHOWCASE_LAUNCH_QUERY_KEYS = ['taut', 'scale', 'zoom', 'zoomScale', 'fit'] as const;

export function clearTautQuery(): void {
  const url = new URL(window.location.href);
  let changed = false;
  for (const key of SHOWCASE_LAUNCH_QUERY_KEYS) {
    if (!url.searchParams.has(key)) continue;
    url.searchParams.delete(key);
    changed = true;
  }
  if (!changed) return;
  window.history.replaceState(null, '', `${url.pathname}${url.search}${url.hash}`);
}
