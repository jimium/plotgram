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

interface DrawifyNodeWasm {
  version(): string;
  validate(source: string): string;
  render(source: string): string;
}

let cached: DrawifyNodeWasm | null | undefined;

function wasmEntryPath(extensionPath: string): string {
  return path.join(extensionPath, 'media', 'node', 'drawify_wasm.js');
}

export function isNodeWasmAvailable(extensionPath: string): boolean {
  return fs.existsSync(wasmEntryPath(extensionPath));
}

export function getNodeWasm(extensionPath: string): DrawifyNodeWasm {
  if (cached === undefined) {
    const entry = wasmEntryPath(extensionPath);
    if (!fs.existsSync(entry)) {
      throw new Error(
        '未找到 Drawify WASM 产物，请在 editors/vscode 目录执行: npm run build:wasm',
      );
    }
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    cached = require(entry) as DrawifyNodeWasm;
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
