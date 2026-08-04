import { useEffect, useState } from 'react';
import type { PlotgramWasm } from '../lib/wasm';
import { parseLayoutCatalog, type LayoutCatalog } from '../data/layoutOptions';

export function useLayoutCatalog(wasm: PlotgramWasm | null, ready: boolean) {
  const [catalog, setCatalog] = useState<LayoutCatalog | null>(null);

  useEffect(() => {
    if (!wasm || !ready || typeof wasm.layout_catalog !== 'function') return;
    const json = wasm.layout_catalog();
    setCatalog(parseLayoutCatalog(json));
  }, [wasm, ready]);

  return catalog;
}
