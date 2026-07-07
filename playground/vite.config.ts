import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

const cdnBase = process.env.VITE_CDN_BASE || '';

// https://vite.dev/config/
export default defineConfig({
  base: process.env.VITE_BASE_PATH || '/',
  plugins: [react()],
  optimizeDeps: {
    exclude: ['plotgram-wasm', 'public/plotgram-wasm'],
  },
  server: {
    port: 3000,
    fs: {
      allow: ['..'],
    },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        animation: resolve(__dirname, 'animation.html'),
        audit: resolve(__dirname, 'audit.html'),
        sequence: resolve(__dirname, 'sequence.html'),
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
