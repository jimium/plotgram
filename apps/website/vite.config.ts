import { createReadStream, existsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';

const rootDir = fileURLToPath(new URL('.', import.meta.url));
const wasmDir = resolve(rootDir, 'tautcore-wasm');
const cdnBase = process.env.VITE_CDN_BASE || '';

/**
 * 开发/预览时从 `website/tautcore-wasm/` 提供 `/tautcore-wasm/*`，
 * 让浏览器加载本地最新构建产物，避免每次改 Rust 都要发布到 CDN。
 */
function serveTautcoreWasm(): Plugin {
  return {
    name: 'serve-tautcore-wasm',
    configureServer(server) {
      return () => {
        server.middlewares.use((req, res, next) => {
          const urlPath = req.url?.split('?')[0] ?? '';
          if (!urlPath.startsWith('/tautcore-wasm/')) {
            next();
            return;
          }

          const rel = decodeURIComponent(urlPath.slice('/tautcore-wasm/'.length));
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
  base: '/',
  plugins: [react(), serveTautcoreWasm()],
  resolve: {
    alias: {
      '../../tautcore-wasm/tautcore_wasm.js': resolve(rootDir, 'tautcore-wasm/tautcore_wasm.js'),
    },
  },
  optimizeDeps: {
    exclude: ['tautcore-wasm'],
  },
  server: {
    port: 3001,
    strictPort: true,
    fs: {
      allow: ['..'],
    },
  },
  build: {
    outDir: 'dist',
    rollupOptions: {
      input: {
        main: resolve(rootDir, 'index.html'),
      },
    },
  },
  experimental: {
    renderBuiltUrl(filename, { type }) {
      if (!cdnBase) {
        return { relative: true };
      }
      // type 可能是 'asset' | 'publicScript' | 'publicCss' 等
      // 当配置了 CDN 前缀时，所有打包产物（JS/CSS/资源）都走 CDN
      if (type === 'asset' || type === 'publicScript' || type === 'publicCss' || type === 'asset-proxy') {
        return `${cdnBase}${filename}`;
      }
      return { relative: true };
    },
  },
});
