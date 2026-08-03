// Right panel: envelope header + selection field view. Unknown kinds fall
// back to a raw JSON panel (§6.2 shell rule).

const envelopeEl = document.getElementById("envelope");
const selectionEl = document.getElementById("selection");

function kv(pairs) {
  const dl = document.createElement("dl");
  dl.className = "kv";
  for (const [k, v] of pairs) {
    if (v == null) continue;
    const dt = document.createElement("dt");
    dt.textContent = k;
    const dd = document.createElement("dd");
    dd.textContent = typeof v === "object" ? JSON.stringify(v) : String(v);
    dl.append(dt, dd);
  }
  return dl;
}

function jsonBlock(label, value) {
  const d = document.createElement("details");
  d.open = true;
  const s = document.createElement("summary");
  s.textContent = label;
  const pre = document.createElement("pre");
  pre.textContent = JSON.stringify(value, null, 2);
  d.append(s, pre);
  return d;
}

export function renderEnvelope(trace) {
  const ext = trace.extension ?? {};
  const rows = [
    ["schema_version", trace.schema_version],
    ["layout", trace.layout],
    ["kind", ext.kind],
    ["orientation", trace.orientation],
    ["space", trace.space],
    ["nodes", trace.common?.nodes?.length],
    ["edges", trace.common?.edges?.length],
    ["groups", trace.common?.groups?.length],
    ["elems", ext.elems?.length],
    ["layers", ext.layers?.length],
    ["metrics", ext.metrics && JSON.stringify(ext.metrics)],
    ["channels", ext.channels === null ? "null (未实现，如实)" : ext.channels],
    ["notes", (trace.notes ?? []).join("；")],
  ];
  envelopeEl.replaceChildren(kv(rows));
}

function findElem(trace, key) {
  return (trace.extension?.elems ?? []).find((e) => {
    const k = e.key ?? {};
    if (key.type === "real") return k.type === "real" && k.id === key.id;
    return (
      k.type === "virtual" &&
      k.owner_edge === key.owner_edge &&
      k.ordinal === key.ordinal
    );
  });
}

// Resolve a `data-sel` key into the trace fields it points at.
export function renderSelection(trace, sel) {
  selectionEl.replaceChildren();
  if (!sel) {
    selectionEl.textContent = "点选画布元素";
    selectionEl.style.opacity = ".6";
    return;
  }
  selectionEl.style.opacity = "1";
  const [kind, rest] = sel.split(":");
  selectionEl.appendChild(kv([["sel", sel]]));

  const show = (label, value) => {
    if (value == null) return;
    selectionEl.appendChild(jsonBlock(label, value));
  };

  if (kind === "node") {
    show("common.nodes", (trace.common.nodes ?? []).find((n) => n.id === rest));
    show("extension.elems", findElem(trace, { type: "real", id: rest }));
  } else if (kind === "edge") {
    show("common.edges", (trace.common.edges ?? []).find((e) => e.edge_id === rest));
    show("edge_plans", (trace.extension?.edge_plans ?? []).find((p) => p.edge_id === rest));
    const ports = (trace.extension?.ports ?? []).filter((p) => p.edge_id === rest);
    if (ports.length) show("ports", ports);
  } else if (kind === "dummy") {
    const [edge, ord] = rest.split("#");
    show("extension.elems", findElem(trace, { type: "virtual", owner_edge: edge, ordinal: Number(ord) }));
    show("edge_plans", (trace.extension?.edge_plans ?? []).find((p) => p.edge_id === edge));
  } else if (kind === "port") {
    const [edge, end] = rest.split("@");
    show("ports", (trace.extension?.ports ?? []).find((p) => p.edge_id === edge && p.end === end));
  } else if (kind === "group") {
    show("common.groups", (trace.common.groups ?? []).find((g) => g.group_id === rest));
  } else {
    show("raw", null);
  }
}

// Unknown kind fallback: common layers only + whole trace as raw JSON (§6.2).
export function renderRawFallback(trace) {
  selectionEl.replaceChildren();
  selectionEl.style.opacity = "1";
  const note = document.createElement("div");
  note.textContent = `未知 kind「${trace.extension?.kind}」：仅 common 层 + raw JSON`;
  note.style.opacity = ".6";
  selectionEl.appendChild(note);
  selectionEl.appendChild(jsonBlock("trace (raw)", trace));
}
