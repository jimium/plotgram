#!/usr/bin/env node
/**
 * 构建后同步 plotgram-wasm/ → dist/plotgram-wasm/，并阻断 public/ 旧副本。
 */
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const src = resolve(root, 'plotgram-wasm');
const dest = resolve(root, 'dist/plotgram-wasm');
const publicWasm = resolve(root, 'public/plotgram-wasm');
const wasmBin = resolve(src, 'plotgram_wasm_bg.wasm');

if (existsSync(publicWasm)) {
  console.warn('warn: 删除 public/plotgram-wasm（避免与 CDN 源目录不一致）');
  rmSync(publicWasm, { recursive: true, force: true });
}

if (!existsSync(wasmBin)) {
  console.error('缺少 plotgram-wasm 产物，请先运行 ./start.sh 或 wasm-pack build');
  process.exit(1);
}

const md5 = createHash('md5').update(readFileSync(wasmBin)).digest('hex');
const stamp = process.env.VITE_WASM_BUILD_STAMP;
if (stamp && stamp !== md5) {
  console.warn(`warn: VITE_WASM_BUILD_STAMP=${stamp} 与 wasm md5=${md5} 不一致，以 wasm 内容为准`);
}

rmSync(dest, { recursive: true, force: true });
mkdirSync(dest, { recursive: true });
cpSync(src, dest, { recursive: true });

writeFileSync(resolve(root, '.wasm-build-stamp'), `${md5}\n`, 'utf8');
console.log(`synced plotgram-wasm → dist/plotgram-wasm (md5=${md5})`);
