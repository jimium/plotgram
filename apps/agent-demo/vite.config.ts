import { createReadStream, existsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';

// Plotgram Agent Demo 开发服务器配置
// WASM 产物由仓库根目录的 plotgram-wasm crate 提供，通过相对路径引用
// 开发时 /agent/chat 代理到本地 plotgram-server (6080)
// 生产构建走 CDN：assets/* 由 VITE_CDN_BASE 重写，wasm 走 plotgram-wasm/ 子目录
const rootDir = fileURLToPath(new URL('.', import.meta.url));
const wasmDir = resolve(rootDir, 'plotgram-wasm');
const cdnBase = process.env.VITE_CDN_BASE || '';

/**
 * 开发/预览时从 `agent-demo/plotgram-wasm/` 提供 `/plotgram-wasm/*`。
 * 与 playground 同款中间件，保证开发时 wasm 二进制可被 fetch。
 */
function servePlotgramWasm(): Plugin {
  return {
    name: 'serve-plotgram-wasm',
    configureServer(server) {
      return () => {
        server.middlewares.use((req, res, next) => {
          const urlPath = req.url?.split('?')[0] ?? '';
          if (!urlPath.startsWith('/plotgram-wasm/')) {
            next();
            return;
          }

          const rel = decodeURIComponent(urlPath.slice('/plotgram-wasm/'.length));
          if (!rel || rel.includes('..')) {
            next();
            return;
          }

          const file = join(wasmDir, rel);
          if (!file.startsWith(wasmDir) || !existsSync(file) || !statSync(file).isFile()) {
            next();
            return;
          }

          res.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate');
          res.setHeader('Pragma', 'no-cache');
          if (file.endsWith('.wasm')) {
            res.setHeader('Content-Type', 'application/wasm');
          } else if (file.endsWith('.js')) {
            res.setHeader('Content-Type', 'text/javascript');
          }

          createReadStream(file).pipe(res);
        });
      };
    },
  };
}

export default defineConfig({
  base: process.env.VITE_BASE_PATH || '/',
  plugins: [react(), servePlotgramWasm()],
  resolve: {
    alias: {
      '@': resolve(rootDir, 'src'),
      '@agent': resolve(rootDir, 'src/agent'),
      '@components': resolve(rootDir, 'src/components'),
      '@hooks': resolve(rootDir, 'src/hooks'),
      '@lib': resolve(rootDir, 'src/lib'),
      // WASM 产物路径：开发时用占位文件，部署前用 wasm-pack 构建真实产物覆盖
      '../plotgram-wasm/plotgram_wasm.js': resolve(wasmDir, 'plotgram_wasm.js'),
    },
  },
  server: {
    port: 3200,
    strictPort: false,
    // 代理 Agent 中转请求到 plotgram-server，避免跨域
    proxy: {
      '/agent': {
        target: 'http://localhost:6080',
        changeOrigin: true,
      },
    },
  },
  optimizeDeps: {
    exclude: ['../crates/plotgram-wasm'],
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    target: 'es2022',
  },
  experimental: {
    renderBuiltUrl(filename, { type }) {
      if (!cdnBase || type !== 'asset') {
        return { relative: true };
      }
      return `${cdnBase}${filename}`;
    },
  },
});
