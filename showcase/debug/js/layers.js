// Overlay plugin for kind == "hierarchical" (debug-inspector.md §6.2).
// Every paint function receives the trace and returns SVG nodes; selection
// targets carry a `data-sel` key resolved by panel.js.

const SVG_NS = "http://www.w3.org/2000/svg";

export function el(tag, attrs = {}, ...children) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  for (const c of children) {
    if (c == null) continue;
    node.appendChild(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return node;
}

function contentBBox(trace) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  const grow = (x, y) => {
    x0 = Math.min(x0, x); y0 = Math.min(y0, y);
    x1 = Math.max(x1, x); y1 = Math.max(y1, y);
  };
  for (const n of trace.common.nodes ?? []) {
    grow(n.frame.x, n.frame.y);
    grow(n.frame.x + n.frame.width, n.frame.y + n.frame.height);
  }
  for (const e of trace.common.edges ?? [])
    for (const p of e.path ?? []) grow(p.x, p.y);
  if (!isFinite(x0)) return { x: 0, y: 0, width: 100, height: 100 };
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

const isVertical = (t) =>
  t.orientation === "top-to-bottom" || t.orientation === "bottom-to-top";

// ---- common layers (always registrable) ----------------------------------

function paintProduct(trace) {
  const g = el("g", { "data-layer": "product" });
  for (const e of trace.common.edges ?? []) {
    const pts = (e.path ?? []).map((p) => `${p.x},${p.y}`).join(" ");
    g.appendChild(el("polyline", {
      points: pts, fill: "none", stroke: "#666", "stroke-width": 1.2,
      class: "sel-hit", "data-sel": `edge:${e.edge_id}`,
    }));
    const mid = e.path?.[Math.floor(e.path.length / 2)];
    if (mid) g.appendChild(el("text", {
      x: mid.x + 3, y: mid.y - 3, "font-size": 8, fill: "#666",
      class: "sel-hit", "data-sel": `edge:${e.edge_id}`,
    }, e.edge_id));
  }
  for (const n of trace.common.nodes ?? []) {
    const f = n.frame;
    g.appendChild(el("rect", {
      x: f.x, y: f.y, width: f.width, height: f.height, rx: 3,
      fill: "#9cf2", stroke: "#369", "stroke-width": 1.2,
      class: "sel-hit", "data-sel": `node:${n.id}`,
    }));
    g.appendChild(el("text", {
      x: n.center.x, y: n.center.y + 3, "text-anchor": "middle",
      "font-size": 9, fill: "#123",
      class: "sel-hit", "data-sel": `node:${n.id}`,
    }, n.id));
  }
  return g;
}

function paintGroups(trace) {
  const g = el("g", { "data-layer": "groups" });
  const nodesByGroup = new Map();
  for (const elem of trace.extension?.elems ?? []) {
    if (elem.key?.type !== "real") continue;
    const node = (trace.common.nodes ?? []).find((n) => n.id === elem.key.id);
    if (!node) continue;
    for (const gid of elem.group_path ?? []) {
      if (!nodesByGroup.has(gid)) nodesByGroup.set(gid, []);
      nodesByGroup.get(gid).push(node);
    }
  }
  for (const grp of trace.common.groups ?? []) {
    const members = nodesByGroup.get(grp.group_id) ?? [];
    if (!members.length) continue;
    const pad = 8;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const n of members) {
      x0 = Math.min(x0, n.frame.x); y0 = Math.min(y0, n.frame.y);
      x1 = Math.max(x1, n.frame.x + n.frame.width);
      y1 = Math.max(y1, n.frame.y + n.frame.height);
    }
    g.appendChild(el("rect", {
      x: x0 - pad, y: y0 - pad, width: x1 - x0 + 2 * pad, height: y1 - y0 + 2 * pad,
      rx: 6, fill: "none", stroke: "#a6a", "stroke-width": 1, "stroke-dasharray": "4 3",
      class: "sel-hit", "data-sel": `group:${grp.group_id}`,
    }));
    g.appendChild(el("text", {
      x: x0 - pad + 3, y: y0 - pad - 3, "font-size": 8, fill: "#a6a",
      "pointer-events": "none",
    }, `${grp.group_id} · frame:${grp.frame_source}${grp.parent ? ` · parent:${grp.parent}` : ""}`));
  }
  return g;
}

// ---- hierarchical plugin layers ------------------------------------------

function paintRanks(trace) {
  const g = el("g", { "data-layer": "ranks" });
  const box = contentBBox(trace);
  const vert = isVertical(trace);
  for (const layer of trace.extension?.layers ?? []) {
    const { start, end } = layer.main_band ?? {};
    if (start == null || end == null) continue;
    const lo = Math.min(start, end), hi = Math.max(start, end);
    const rect = vert
      ? { x: box.x - 20, y: lo, width: box.width + 40, height: hi - lo }
      : { x: lo, y: box.y - 20, width: hi - lo, height: box.height + 40 };
    g.appendChild(el("rect", {
      ...rect, fill: layer.rank % 2 ? "#4af1" : "#fa41", stroke: "none",
      "pointer-events": "none",
    }));
    const tx = vert ? box.x - 24 : lo + 2;
    const ty = vert ? lo + 9 : box.y - 24;
    g.appendChild(el("text", {
      x: tx, y: ty, "font-size": 8, fill: "#e80", "pointer-events": "none",
    }, `rank ${layer.rank}`));
  }
  return g;
}

function paintDummies(trace) {
  const g = el("g", { "data-layer": "dummies" });
  for (const elem of trace.extension?.elems ?? []) {
    if (elem.key?.type !== "virtual" || !elem.center) continue;
    const { x, y } = elem.center, r = 4;
    const sel = `dummy:${elem.key.owner_edge}#${elem.key.ordinal}`;
    g.appendChild(el("line", {
      x1: x - r, y1: y - r, x2: x + r, y2: y + r,
      stroke: "#c0c", "stroke-width": 2, class: "sel-hit", "data-sel": sel,
    }));
    g.appendChild(el("line", {
      x1: x - r, y1: y + r, x2: x + r, y2: y - r,
      stroke: "#c0c", "stroke-width": 2, class: "sel-hit", "data-sel": sel,
    }));
    g.appendChild(el("text", {
      x: x + r + 2, y: y - r, "font-size": 7, fill: "#c0c", "pointer-events": "none",
    }, `${elem.key.owner_edge}#${elem.key.ordinal}`));
  }
  return g;
}

function paintReversed(trace) {
  const g = el("g", { "data-layer": "reversed" });
  const paths = new Map((trace.common.edges ?? []).map((e) => [e.edge_id, e.path]));
  for (const plan of trace.extension?.edge_plans ?? []) {
    if (!plan.reversed) continue;
    const path = paths.get(plan.edge_id);
    if (!path) continue;
    const pts = path.map((p) => `${p.x},${p.y}`).join(" ");
    g.appendChild(el("polyline", {
      points: pts, fill: "none", stroke: "#f80", "stroke-width": 3, opacity: 0.8,
      class: "sel-hit", "data-sel": `edge:${plan.edge_id}`,
    }));
  }
  return g;
}

function paintPorts(trace) {
  const g = el("g", { "data-layer": "ports" });
  for (const p of trace.extension?.ports ?? []) {
    if (!p.point) continue;
    const sel = `port:${p.edge_id}@${p.end}`;
    g.appendChild(el("circle", {
      cx: p.point.x, cy: p.point.y, r: 2.5,
      fill: p.constraint === "fixed" ? "#c00" : "#0a7",
      class: "sel-hit", "data-sel": sel,
    }));
    g.appendChild(el("text", {
      x: p.point.x + 4, y: p.point.y + 3, "font-size": 6.5, fill: "#0a7",
      "pointer-events": "none",
    }, `${p.side}${p.slot}`));
  }
  return g;
}

// ---- registry --------------------------------------------------------------
// Shell rule (§6.2): layers = common ∪ plugins[kind]; unknown kind gets
// common only (app.js handles the raw-JSON fallback panel).

export const COMMON_LAYERS = [
  { id: "product", label: "产品几何", paint: paintProduct, defaultOn: true },
  { id: "groups", label: "组（成员 bbox）", paint: paintGroups, defaultOn: true },
];

export const PLUGIN_LAYERS = {
  hierarchical: [
    { id: "ranks", label: "ranks", paint: paintRanks, defaultOn: false },
    { id: "dummies", label: "dummies", paint: paintDummies, defaultOn: false },
    { id: "reversed", label: "reversed", paint: paintReversed, defaultOn: false },
    { id: "ports", label: "ports", paint: paintPorts, defaultOn: false },
  ],
};

export function layersFor(trace) {
  const plugin = PLUGIN_LAYERS[trace.extension?.kind] ?? [];
  return [...COMMON_LAYERS, ...plugin];
}

export { contentBBox };
