import * as fs from 'fs';
import * as path from 'path';

export interface RenderResult {
  success: boolean;
  svg: string | null;
  errors: string[];
  warnings: string[];
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

interface TautcoreNodeWasm {
  version(): string;
  validate(source: string): string;
  render(source: string): string;
}

let cached: TautcoreNodeWasm | null | undefined;

function wasmEntryPath(extensionPath: string): string {
  return path.join(extensionPath, 'media', 'node', 'tautcore_wasm.js');
}

export function isNodeWasmAvailable(extensionPath: string): boolean {
  return fs.existsSync(wasmEntryPath(extensionPath));
}

export function getNodeWasm(extensionPath: string): TautcoreNodeWasm {
  if (cached === undefined) {
    const entry = wasmEntryPath(extensionPath);
    if (!fs.existsSync(entry)) {
      throw new Error(
        '未找到 Tautcore WASM 产物，请在 editors/vscode 目录执行: npm run build:wasm',
      );
    }
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    cached = require(entry) as TautcoreNodeWasm;
  }
  return cached!;
}

export function validateSource(extensionPath: string, source: string): ValidationResult {
  const json = getNodeWasm(extensionPath).validate(source);
  return JSON.parse(json) as ValidationResult;
}

export function renderSource(extensionPath: string, source: string): RenderResult {
  const json = getNodeWasm(extensionPath).render(source);
  return JSON.parse(json) as RenderResult;
}
