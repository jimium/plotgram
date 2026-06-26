import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';

// Drawify Studio 开发服务器配置
// WASM 产物由仓库根目录的 drawify-wasm crate 提供,通过相对路径引用
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
      '@agent': path.resolve(__dirname, 'src/agent'),
      '@components': path.resolve(__dirname, 'src/components'),
      '@hooks': path.resolve(__dirname, 'src/hooks'),
      '@lib': path.resolve(__dirname, 'src/lib'),
      // WASM 产物路径:开发时用占位文件,部署前用 wasm-pack 构建真实产物覆盖
      '../drawify-wasm/drawify_wasm.js': path.resolve(
        __dirname,
        'drawify-wasm/drawify_wasm.js',
      ),
    },
  },
  server: {
    port: 3100,
    strictPort: false,
    // 代理 LLM API 请求,避免浏览器跨域
    proxy: {
      '/llm': {
        target: 'http://localhost:3100',
        changeOrigin: true,
      },
    },
  },
  optimizeDeps: {
    exclude: ['../crates/drawify-wasm'],
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    target: 'es2022',
  },
});
