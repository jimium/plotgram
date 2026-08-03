// Inspector orchestration: source editor -> wasm -> trace -> layers/panel.
// Shell rule (§6.2): layers = common ∪ plugins[kind]; hash keeps layer
// toggles + selection shareable (§6.3).

import { wasmReady } from "./wasm.js";
import { FIXTURES } from "../fixtures.js";
import { layersFor, contentBBox, el } from "./layers.js";
import { fit, setWorld, setSelectHandler, markSelected } from "./canvas.js";
import { renderEnvelope, renderSelection, renderRawFallback } from "./panel.js";
import { readHash, writeHash } from "./state.js";

const sourceEl = document.getElementById("source");
const fixtureEl = document.getElementById("fixture");
const errorEl = document.getElementById("error");
const layerbarEl = document.getElementById("layerbar");
const statusEl = document.getElementById("status");
const dropzoneEl = document.getElementById("dropzone");
const modeNoteEl = document.getElementById("mode-note");

let trace = null;          // current LayoutDebugTrace
let enabled = new Set();   // enabled layer ids
let selection = null;      // data-sel key
let readonlyMode = false;  // viewing a dropped trace.json

const t0 = performance.now();
const { debugTrace, version } = await wasmReady();
statusEl.textContent = `wasm ready (${version()}), ${(performance.now() - t0).toFixed(0)}ms`;

// ---- fixtures + editor -----------------------------------------------------

for (const [i, f] of FIXTURES.entries()) {
  const opt = document.createElement("option");
  opt.value = i;
  opt.textContent = f.name;
  fixtureEl.appendChild(opt);
}
fixtureEl.addEventListener("change", () => {
  sourceEl.value = FIXTURES[fixtureEl.value].source;
  exitReadonly();
  rerun();
});

let debounce = null;
sourceEl.addEventListener("input", () => {
  exitReadonly();
  clearTimeout(debounce);
  debounce = setTimeout(rerun, 250);
});

function showError(msg) {
  errorEl.style.display = msg ? "block" : "none";
  errorEl.textContent = msg ?? "";
}

function exitReadonly() {
  readonlyMode = false;
  modeNoteEl.style.display = "none";
}

// ---- core: rebuild trace + repaint ------------------------------------------

async function rerun() {
  showError(null);
  const start = performance.now();
  try {
    trace = JSON.parse(debugTrace(sourceEl.value));
  } catch (e) {
    showError(String(e.message ?? e));
    return;
  }
  afterTraceChanged(start);
}

function afterTraceChanged(startMs) {
  const layers = layersFor(trace);
  const knownIds = new Set(layers.map((l) => l.id));

  // Hash restore: stale layout cross-check drops the saved selection.
  const hash = readHash();
  if (hash.layout && hash.layout !== trace.layout) hash.sel = null;
  const hashLayers = hash.layers?.filter((id) => knownIds.has(id)) ?? [];
  enabled = new Set(
    hashLayers.length
      ? hashLayers
      : layers.filter((l) => l.defaultOn).map((l) => l.id),
  );
  selection = hash.sel;

  renderLayerbar(layers);
  repaint();
  renderEnvelope(trace);
  if (trace.extension?.kind && !(trace.extension.kind in { hierarchical: 1 })) {
    renderRawFallback(trace);
  } else {
    renderSelection(trace, selection);
  }
  markSelected(selection);
  statusEl.textContent =
    `${trace.layout}/${trace.extension?.kind} · ${trace.orientation} · ` +
    `${trace.common.nodes?.length ?? 0} nodes · ${trace.common.edges?.length ?? 0} edges · ` +
    `${startMs != null ? (performance.now() - startMs).toFixed(0) + "ms" : "trace.json"}`;
  syncHash();
}

function repaint() {
  const layers = layersFor(trace);
  const world = el("g");
  for (const layer of layers) {
    if (!enabled.has(layer.id)) continue;
    world.appendChild(layer.paint(trace));
  }
  setWorld(world);
  fit(contentBBox(trace));
  markSelected(selection);
}

// ---- layer toggles -----------------------------------------------------------

function renderLayerbar(layers) {
  for (const label of layerbarEl.querySelectorAll("label")) label.remove();
  for (const layer of layers) {
    const label = document.createElement("label");
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = enabled.has(layer.id);
    cb.addEventListener("change", () => {
      cb.checked ? enabled.add(layer.id) : enabled.delete(layer.id);
      repaint();
      syncHash();
    });
    label.append(cb, document.createTextNode(layer.label));
    layerbarEl.appendChild(label);
  }
}

// ---- selection ----------------------------------------------------------------

setSelectHandler((sel) => {
  selection = sel;
  renderSelection(trace, selection);
  markSelected(selection);
  syncHash();
});

// ---- hash -------------------------------------------------------------------------

function syncHash() {
  if (!trace) return;
  writeHash({
    layers: [...enabled],
    sel: selection,
    layout: trace.layout,
  });
}

// ---- trace.json dropzone (read-only view) -----------------------------------------

for (const type of ["dragover", "dragenter"]) {
  dropzoneEl.addEventListener(type, (ev) => {
    ev.preventDefault();
    dropzoneEl.classList.add("hover");
  });
}
for (const type of ["dragleave", "drop"]) {
  dropzoneEl.addEventListener(type, (ev) => {
    ev.preventDefault();
    dropzoneEl.classList.remove("hover");
  });
}
dropzoneEl.addEventListener("drop", async (ev) => {
  const file = ev.dataTransfer?.files?.[0];
  if (!file) return;
  try {
    trace = JSON.parse(await file.text());
  } catch (e) {
    showError(`trace.json 解析失败: ${e.message}`);
    return;
  }
  readonlyMode = true;
  modeNoteEl.style.display = "block";
  showError(null);
  afterTraceChanged(null);
});

// ---- boot ---------------------------------------------------------------------------

sourceEl.value = FIXTURES[0].source;
await rerun();
