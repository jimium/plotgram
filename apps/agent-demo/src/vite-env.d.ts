/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Agent 中转 API 端点，默认走 Vite proxy 到本地 tautcore-server */
  readonly VITE_AGENT_API?: string;
  /** 部署子路径，如 /agent/ */
  readonly VITE_BASE_PATH?: string;
  /** 静态资源 CDN 根路径，如 https://assets.plotgram.cn/agent/ */
  readonly VITE_CDN_BASE?: string;
  /** WASM 构建戳（md5），用于 CDN 缓存失效 */
  readonly VITE_WASM_BUILD_STAMP?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
