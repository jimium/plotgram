/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_CDN_BASE?: string;
  /** wasm 内容 md5，用于 ?v= 缓存破坏（start.sh / deploy 注入） */
  readonly VITE_WASM_BUILD_STAMP?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
