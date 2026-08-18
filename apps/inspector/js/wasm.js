// wasm loader: single init, promise-cached.
import init, { debugTrace, renderSvg, version } from "../pkg/tautcore_wasm.js";

let ready = null;

export async function wasmReady() {
  if (!ready) ready = init();
  await ready;
  return { debugTrace, renderSvg, version };
}
