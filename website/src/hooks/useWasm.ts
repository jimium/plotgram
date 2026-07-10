import { useEffect, useState } from 'react';
import { loadWasm, type PlotgramWasm } from '../lib/wasm';

export interface WasmState {
  wasm: PlotgramWasm | null;
  ready: boolean;
  error: string | null;
}

export function useWasm(): WasmState {
  const [state, setState] = useState<WasmState>({
    wasm: null,
    ready: false,
    error: null,
  });

  useEffect(() => {
    let cancelled = false;
    loadWasm()
      .then((wasm) => {
        if (cancelled) return;
        setState({ wasm, ready: true, error: null });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : 'WASM 加载失败';
        setState({ wasm: null, ready: false, error: message });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return state;
}
