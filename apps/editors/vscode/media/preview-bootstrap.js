function readConfig() {
  const config = document.getElementById('tautcore-config');
  const wasmJs = config?.dataset.wasmJs;
  const wasmBin = config?.dataset.wasmBin;

  if (!wasmJs || !wasmBin) {
    throw new Error('Tautcore 配置缺失：未找到 WASM 资源 URI');
  }

  return { wasmJs, wasmBin };
}

function showBootstrapError(error) {
  const status = document.getElementById('status');
  if (status) {
    status.textContent = 'WASM 加载失败';
  }

  const diagram = document.getElementById('diagram');
  if (!diagram) {
    return;
  }

  const err = document.createElement('pre');
  err.className = 'error tautcore-preview-error';
  err.textContent = error instanceof Error ? error.message : String(error);
  diagram.appendChild(err);
}

const { wasmJs, wasmBin } = readConfig();

globalThis.__tautcoreRuntimePromise = (async () => {
  const { default: init, render } = await import(/* webpackIgnore: true */ wasmJs);
  await init(wasmBin);
  return {
    render: (source) => JSON.parse(render(source)),
  };
})();

globalThis.__tautcoreRuntimePromise
  .then(() => {
    const status = document.getElementById('status');
    if (status) {
      status.textContent = '等待渲染…';
    }
  })
  .catch(showBootstrapError);
