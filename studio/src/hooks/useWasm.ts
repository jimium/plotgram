/**
 * useWasm Hook
 *
 * 管理 drawify-wasm 模块的加载状态
 */

import { useEffect, useState } from 'react';
import { loadWasm, checkStudioCapabilities, type DrawifyWasm } from '@lib/wasm';

interface UseWasmResult {
  wasm: DrawifyWasm | null;
  ready: boolean;
  error: string | null;
  version: string;
  capabilities: {
    diff: boolean;
    applyPatch: boolean;
    astToSource: boolean;
  };
}

/** 加载 drawify-wasm 模块 */
export function useWasm(): UseWasmResult {
  const [wasm, setWasm] = useState<DrawifyWasm | null>(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState('');
  const [capabilities, setCapabilities] = useState({
    diff: false,
    applyPatch: false,
    astToSource: false,
  });

  useEffect(() => {
    let cancelled = false;

    loadWasm()
      .then((mod) => {
        if (cancelled) return;
        setWasm(mod);
        setVersion(mod.version());
        setCapabilities(checkStudioCapabilities(mod));
        setReady(true);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return { wasm, ready, error, version, capabilities };
}