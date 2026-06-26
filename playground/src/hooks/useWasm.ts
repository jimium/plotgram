import { useEffect, useState } from 'react';
import { loadWasm, type DrawifyWasm } from '../lib/wasm';

export interface WasmState {
  wasm: DrawifyWasm | null;
  ready: boolean;
  error: string | null;
  version: string | null;
}

export function useWasm(): WasmState {
  const [state, setState] = useState<WasmState>({
    wasm: null,
    ready: false,
    error: null,
    version: null,
  });

  useEffect(() => {
    let cancelled = false;

    loadWasm()
      .then((wasm) => {
        if (cancelled) return;
        let version: string | null = null;
        try {
          version = wasm.version();
        } catch {
          version = null;
        }
        setState({ wasm, ready: true, error: null, version });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : 'WASM 加载失败';
        setState({ wasm: null, ready: false, error: message, version: null });
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return state;
}
