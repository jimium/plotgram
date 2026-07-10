import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const rootDir = fileURLToPath(new URL('.', import.meta.url));
const cdnBase = process.env.VITE_CDN_BASE || '';

export default defineConfig({
  base: '/',
  plugins: [react()],
  server: {
    port: 3001,
    strictPort: true,
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
      if (!cdnBase || type !== 'asset') {
        return { relative: true };
      }
      return `${cdnBase}${filename}`;
    },
  },
});
