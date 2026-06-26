const vscode = acquireVsCodeApi();

/** @type {Promise<{ render: (source: string) => RenderResult }>} */
let runtimePromise;

/** @type {ReturnType<typeof setTimeout> | undefined} */
let pendingUpdate;

/**
 * @typedef {{ success: boolean; svg: string | null; errors: string[]; warnings: string[] }} RenderResult
 */

function getRuntime() {
  if (!runtimePromise) {
    if (!globalThis.__drawifyRuntimePromise) {
      runtimePromise = Promise.reject(
        new Error('Drawify runtime 未初始化，请重新打开预览面板'),
      );
    } else {
      runtimePromise = globalThis.__drawifyRuntimePromise;
    }
  }
  return runtimePromise;
}

function clearDiagramError() {
  const existing = document.querySelector('.drawify-preview-error');
  existing?.remove();
}

/**
 * @param {RenderResult} result
 */
function showResult(result) {
  const status = document.getElementById('status');
  const diagram = document.getElementById('diagram');
  if (!status || !diagram) {
    return;
  }

  clearDiagramError();

  if (result.success && result.svg) {
    status.textContent = result.warnings.length
      ? `已渲染（${result.warnings.length} 条警告）`
      : '已渲染';
    diagram.innerHTML = result.svg;
    return;
  }

  status.textContent = '渲染失败';
  diagram.innerHTML = '';

  if (result.errors.length > 0) {
    const err = document.createElement('pre');
    err.className = 'error drawify-preview-error';
    err.textContent = result.errors.join('\n');
    diagram.appendChild(err);
  }
}

window.addEventListener('message', (event) => {
  const { type, source } = event.data ?? {};
  if (type !== 'update' || typeof source !== 'string') {
    return;
  }

  clearTimeout(pendingUpdate);
  pendingUpdate = setTimeout(async () => {
    const status = document.getElementById('status');
    if (status) {
      status.textContent = '渲染中…';
    }

    try {
      const runtime = await getRuntime();
      showResult(runtime.render(source));
    } catch (error) {
      clearDiagramError();
      if (status) {
        status.textContent = 'WASM 加载失败';
      }
      const diagram = document.getElementById('diagram');
      if (diagram) {
        diagram.innerHTML = '';
        const err = document.createElement('pre');
        err.className = 'error drawify-preview-error';
        err.textContent = error instanceof Error ? error.message : String(error);
        diagram.appendChild(err);
      }
    }
  }, 150);
});
