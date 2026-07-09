/** Vite `base` 配置值，部署在子路径时用于拼接静态资源与页面链接。 */
export const baseUrl = import.meta.env.BASE_URL;

/** 静态资源 CDN 根路径（如 https://assets.pg.agcli.cn/playground/）。 */
export const cdnBase: string = import.meta.env.VITE_CDN_BASE || '';

export function withBase(path: string): string {
  const normalized = path.startsWith('/') ? path.slice(1) : path;
  return `${baseUrl}${normalized}`;
}

/** 大体积静态资源优先走 CDN，否则回退到同源 base。 */
export function assetBase(): string {
  return cdnBase || baseUrl;
}
