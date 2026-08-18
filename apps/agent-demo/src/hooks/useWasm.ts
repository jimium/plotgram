/**
 * useWasm Hook
 *
 * 管理 tautcore-wasm 模块的加载状态。
 * 演示版裁剪了 studio 的 capabilities 检测（demo 始终使用最新 wasm，能力齐全）。
 */

import { useEffect, useState } from 'react';
import { loadWasm, type TautcoreWasm } from '@lib/wasm';

interface UseWasmResult {
  wasm: TautcoreWasm | null;
  ready: boolean;
  error: string | null;
  version: string;
}

export function useWasm(): UseWasmResult {
  const [wasm, setWasm] = useState<TautcoreWasm | null>(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState('');

  useEffect(() => {
    let cancelled = false;
    loadWasm()
      .then((mod) => {
        if (cancelled) return;
        setWasm(mod);
        setVersion(mod.version());
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

  return { wasm, ready, error, version };
}
