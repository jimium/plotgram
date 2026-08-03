// Canvas: pan/zoom transform over a single <g id="world"> plus hit-testing
// delegation for selection (debug-inspector.md §6.3).

const svg = document.getElementById("canvas");
const world = document.getElementById("world");

let tx = 0, ty = 0, scale = 1;

function apply() {
  world.setAttribute("transform", `translate(${tx} ${ty}) scale(${scale})`);
}

export function fit(bbox, { margin = 30 } = {}) {
  const rect = svg.getBoundingClientRect();
  // First paint may race flex layout: retry once on the next frame instead
  // of fitting into a zero/undersized container (scale -> 0).
  if (rect.width < 60 || rect.height < 60) {
    requestAnimationFrame(() => fit(bbox, { margin }));
    return;
  }
  const pad = 2 * margin;
  scale = Math.min(
    2.5,
    (rect.width - pad) / Math.max(bbox.width, 1),
    (rect.height - pad) / Math.max(bbox.height, 1),
  );
  scale = Math.max(0.05, scale);
  tx = (rect.width - bbox.width * scale) / 2 - bbox.x * scale;
  ty = (rect.height - bbox.height * scale) / 2 - bbox.y * scale;
  apply();
}

export function setWorld(node) {
  world.replaceChildren(node);
}

export function viewInfo() {
  return { tx, ty, scale };
}

// Wheel zoom around the cursor; drag to pan.
svg.addEventListener("wheel", (ev) => {
  ev.preventDefault();
  const factor = ev.deltaY < 0 ? 1.12 : 1 / 1.12;
  const rect = svg.getBoundingClientRect();
  const mx = ev.clientX - rect.left, my = ev.clientY - rect.top;
  const next = Math.min(20, Math.max(0.05, scale * factor));
  tx = mx - ((mx - tx) / scale) * next;
  ty = my - ((my - ty) / scale) * next;
  scale = next;
  apply();
}, { passive: false });

let drag = null;
svg.addEventListener("pointerdown", (ev) => {
  drag = { x: ev.clientX, y: ev.clientY, tx, ty, moved: false };
  svg.setPointerCapture(ev.pointerId);
  svg.classList.add("dragging");
});
svg.addEventListener("pointermove", (ev) => {
  if (!drag) return;
  const dx = ev.clientX - drag.x, dy = ev.clientY - drag.y;
  if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
  tx = drag.tx + dx;
  ty = drag.ty + dy;
  apply();
});
svg.addEventListener("pointerup", (ev) => {
  svg.classList.remove("dragging");
  const wasDrag = drag?.moved;
  drag = null;
  if (wasDrag) return;
  // Click-through selection: pointer capture redirects pointerup to the svg,
  // so hit-test by coordinates instead of ev.target (§6.3).
  const hit = document
    .elementFromPoint(ev.clientX, ev.clientY)
    ?.closest("[data-sel]");
  onSelect?.(hit ? hit.getAttribute("data-sel") : null);
});

let onSelect = null;
export function setSelectHandler(fn) {
  onSelect = fn;
}

export function markSelected(sel) {
  for (const n of world.querySelectorAll(".selected")) {
    n.classList.remove("selected");
    if (n.dataset.origStroke) {
      n.setAttribute("stroke", n.dataset.origStroke);
      delete n.dataset.origStroke;
    }
  }
  if (!sel) return;
  for (const n of world.querySelectorAll(`[data-sel="${CSS.escape(sel)}"]`)) {
    n.classList.add("selected");
    n.dataset.origStroke = n.getAttribute("stroke") ?? "";
    n.setAttribute("stroke", "#f0f");
  }
}
