import LZString from 'lz-string';
import type { LayoutOptions } from '../data/layoutOptions';
import {
  normalizeAppearanceOptions,
  type AppearanceOptions,
} from '../data/appearanceOptions';

export interface SharedState {
  code: string;
  layout: LayoutOptions;
  appearance: AppearanceOptions;
}

const HASH_PREFIX = '#s=';

export function encodeState(state: SharedState): string {
  const json = JSON.stringify(state);
  return LZString.compressToEncodedURIComponent(json);
}

export function decodeState(encoded: string): SharedState | null {
  try {
    const json = LZString.decompressFromEncodedURIComponent(encoded);
    if (!json) return null;
    const parsed = JSON.parse(json) as Partial<SharedState>;
    if (typeof parsed.code !== 'string') return null;
    return {
      code: parsed.code,
      layout: parsed.layout as LayoutOptions,
      appearance: normalizeAppearanceOptions(parsed.appearance),
    };
  } catch {
    return null;
  }
}

/** 从当前 URL hash 读取分享状态（若有）。 */
export function readStateFromUrl(): SharedState | null {
  const hash = window.location.hash;
  if (!hash.startsWith(HASH_PREFIX)) return null;
  return decodeState(hash.slice(HASH_PREFIX.length));
}

/** 生成可分享的完整 URL，并写入地址栏（不刷新页面）。 */
export function buildShareUrl(state: SharedState): string {
  const encoded = encodeState(state);
  const url = `${window.location.origin}${window.location.pathname}${HASH_PREFIX}${encoded}`;
  window.history.replaceState(null, '', url);
  return url;
}
