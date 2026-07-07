import { createReadStream, existsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';

const rootDir = fileURLToPath(new URL('.', import.meta.url));
const wasmDir = resolve(rootDir, 'plotgram-wasm');
const cdnBase = process.env.VITE_CDN_BASE || '';

/**
 * 开发/预览时从 `playground/plotgram-wasm/` 提供 `/plotgram-wasm/*`。
 *
 * 若存在 `public/plotgram-wasm/` 旧副本，Vite 会优先于源码目录提供静态文件，
 * 导致浏览器一直加载过期 wasm（与 plotgram-wasm/ 内新构建产物 md5 不一致）。
 */
function servePlotgramWasm(): Plugin {
  return {
    name: 'serve-plotgram-wasm',
    configureServer(server) {
      // post hook：插入中间件栈最前，压过 public/ 里的同名路径
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

function rejectStalePublicWasm(): Plugin {
  const stale = resolve(rootDir, 'public/plotgram-wasm');
  return {
    name: 'reject-stale-public-wasm',
    buildStart() {
      if (existsSync(stale)) {
        throw new Error(
          '检测到 public/plotgram-wasm/ 过期副本，请删除后重试（会覆盖 plotgram-wasm/ 构建产物）',
        );
      }
    },
  };
}

// https://vite.dev/config/
export default defineConfig({
  base: process.env.VITE_BASE_PATH || '/',
  plugins: [react(), rejectStalePublicWasm(), servePlotgramWasm()],
  resolve: {
    alias: {
      '../../plotgram-wasm/plotgram_wasm.js': resolve(rootDir, 'plotgram-wasm/plotgram_wasm.js'),
    },
  },
  optimizeDeps: {
    exclude: ['plotgram-wasm'],
  },
  server: {
    port: 3000,
    strictPort: true,
    fs: {
      allow: ['..'],
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(rootDir, 'index.html'),
        animation: resolve(rootDir, 'animation.html'),
        audit: resolve(rootDir, 'audit.html'),
        sequence: resolve(rootDir, 'sequence.html'),
      },
    },
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
