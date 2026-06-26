import { useEffect, useState } from 'react';
import type { DrawifyWasm } from '../lib/wasm';
import { parseLayoutCatalog, type LayoutCatalog } from '../data/layoutOptions';

export function useLayoutCatalog(wasm: DrawifyWasm | null, ready: boolean) {
  const [catalog, setCatalog] = useState<LayoutCatalog | null>(null);

  useEffect(() => {
    if (!wasm || !ready || typeof wasm.layout_catalog !== 'function') return;
    const json = wasm.layout_catalog();
    setCatalog(parseLayoutCatalog(json));
  }, [wasm, ready]);

  return catalog;
}
