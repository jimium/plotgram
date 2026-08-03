// URL hash state: #layers=product,ranks&sel=node:a&layout=hierarchical
// `layout` is a redundant cross-check — a stale hash for another layout is
// dropped rather than applied to a different trace (debug-inspector.md §6.3).

export function readHash() {
  const p = new URLSearchParams(location.hash.slice(1));
  return {
    layers: p.get("layers")?.split(",").filter(Boolean) ?? null,
    sel: p.get("sel") || null,
    layout: p.get("layout") || null,
  };
}

export function writeHash({ layers, sel, layout }) {
  const p = new URLSearchParams();
  if (layers?.length) p.set("layers", layers.join(","));
  if (sel) p.set("sel", sel);
  if (layout) p.set("layout", layout);
  const next = "#" + p.toString();
  if (location.hash !== next) history.replaceState(null, "", next);
}
