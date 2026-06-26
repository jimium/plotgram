import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  optimizeDeps: {
    exclude: ['drawify-wasm'],
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
});
