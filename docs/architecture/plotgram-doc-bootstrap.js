/**
 * 架构文档 Plotgram 渲染引导（与 playground 生产环境同源 CDN）。
 * 缩放策略：只缩小、不放大；按容器宽度与最大高度 fit（类似 Preview fitToView）。
 */
(function () {
  const WASM_CDN = 'https://assets.plotgram.cn/plotgram-wasm/';
  const RENDER_OPTIONS = JSON.stringify({
    theme_id: 'common.clean-light',
    transparent_background: true,
    show_title: false, // 页面已有 caption，避免 SVG 内重复大标题
  });

  /** 默认插图最大宽度（px） */
  const DEFAULT_MAX_WIDTH = 720;
  /** 默认插图最大高度（px） */
  const DEFAULT_MAX_HEIGHT = 420;

  const WIDE_DIAGRAMS = new Set(['diag-layers', 'diag-recipe', 'diag-paths']);
  const TALL_DIAGRAMS = new Set([
    'diag-pipeline',
    'diag-sugiyama',
    'diag-kernel',
    'diag-freeze',
    'diag-routing',
  ]);

  function setStatus(text, kind) {
    const el = document.getElementById('wasm-status');
    if (!el) return;
    el.textContent = text;
    el.dataset.kind = kind || 'info';
  }

  function showError(container, message, detail) {
    container.innerHTML =
      '<div class="plotgram-error">' +
      '<strong>渲染失败</strong><p>' +
      escapeHtml(message) +
      '</p>' +
      (detail ? '<pre>' + escapeHtml(detail) + '</pre>' : '') +
      '</div>';
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  }

  function readViewBox(svgEl) {
    const vb = svgEl.viewBox && svgEl.viewBox.baseVal;
    if (vb && vb.width > 0 && vb.height > 0) {
      return { width: vb.width, height: vb.height };
    }
    const raw = svgEl.getAttribute('viewBox');
    if (!raw) return null;
    const parts = raw.trim().split(/\s+/).map(Number);
    if (parts.length !== 4 || parts[2] <= 0 || parts[3] <= 0) return null;
    return { width: parts[2], height: parts[3] };
  }

  function limitsForDiagram(diagramId) {
    if (WIDE_DIAGRAMS.has(diagramId)) {
      return { maxWidth: DEFAULT_MAX_WIDTH, maxHeight: 320 };
    }
    if (TALL_DIAGRAMS.has(diagramId)) {
      return { maxWidth: DEFAULT_MAX_WIDTH, maxHeight: 520 };
    }
    return { maxWidth: DEFAULT_MAX_WIDTH, maxHeight: DEFAULT_MAX_HEIGHT };
  }

  /**
   * 将 SVG 缩放到容器内（scale <= 1，永不放大）。
   * 参考 playground Preview.tsx 的 fitToView。
   */
  function fitSvgToContainer(svgEl, viewport, diagramId) {
    const vb = readViewBox(svgEl);
    if (!vb) return;

    const { maxWidth, maxHeight } = limitsForDiagram(diagramId);
    const containerWidth = Math.min(
      viewport.clientWidth || maxWidth,
      maxWidth,
    );

    const scale = Math.min(
      containerWidth / vb.width,
      maxHeight / vb.height,
      1, // 禁止放大：原生尺寸已够清晰时保持 1:1
    );

    const displayW = Math.max(1, Math.round(vb.width * scale));
    const displayH = Math.max(1, Math.round(vb.height * scale));

    svgEl.setAttribute('preserveAspectRatio', 'xMidYMid meet');
    svgEl.setAttribute('width', String(displayW));
    svgEl.setAttribute('height', String(displayH));
    svgEl.style.width = displayW + 'px';
    svgEl.style.height = displayH + 'px';
    svgEl.style.maxWidth = '100%';
    svgEl.setAttribute('role', 'img');

    viewport.dataset.naturalW = String(Math.round(vb.width));
    viewport.dataset.naturalH = String(Math.round(vb.height));
    viewport.dataset.displayScale = scale.toFixed(3);
  }

  function viewportClassFor(diagramId) {
    if (WIDE_DIAGRAMS.has(diagramId)) return 'plotgram-viewport plotgram-viewport--wide';
    if (TALL_DIAGRAMS.has(diagramId)) return 'plotgram-viewport plotgram-viewport--tall';
    return 'plotgram-viewport';
  }

  function mountSvg(container, diagramId, svgText) {
    const viewport = document.createElement('div');
    viewport.className = viewportClassFor(diagramId);

    const wrap = document.createElement('div');
    wrap.className = 'plotgram-svg-wrap';
    wrap.innerHTML = svgText;

    const svgEl = wrap.querySelector('svg');
    if (!svgEl) {
      showError(container, '未找到 SVG 根节点', '');
      return;
    }

    viewport.appendChild(wrap);
    container.replaceChildren(viewport);

    const applyFit = () => fitSvgToContainer(svgEl, viewport, diagramId);
    applyFit();

    if (typeof ResizeObserver !== 'undefined') {
      const ro = new ResizeObserver(() => applyFit());
      ro.observe(viewport);
    } else {
      window.addEventListener('resize', applyFit);
    }
  }

  async function renderAll(wasm) {
    const diagrams = window.PLOTGRAM_DIAGRAMS || {};
    const entries = Object.entries(diagrams);
    let ok = 0;
    let fail = 0;

    for (const [id, source] of entries) {
      const container = document.getElementById(id);
      if (!container) continue;
      container.classList.add('plotgram-loading');
      try {
        const json =
          typeof wasm.render_with_options === 'function'
            ? wasm.render_with_options(source.trim(), 'svg', RENDER_OPTIONS)
            : wasm.render(source.trim(), 'svg');
        const result = JSON.parse(json);
        if (result.success && result.text) {
          mountSvg(container, id, result.text);
          ok += 1;
        } else {
          fail += 1;
          const detail = (result.errors || [])
            .map((e) => e.message || JSON.stringify(e))
            .join('\n');
          showError(container, 'Plotgram 诊断报错', detail || json);
        }
      } catch (err) {
        fail += 1;
        showError(container, String(err), '');
      } finally {
        container.classList.remove('plotgram-loading');
      }
    }

    const ver = typeof wasm.version === 'function' ? wasm.version() : '';
    setStatus(
      `Plotgram WASM 已加载${ver ? ' · ' + ver : ''} · 渲染 ${ok}/${entries.length}` +
        (fail ? `（${fail} 失败）` : ''),
      fail ? 'warn' : 'ok',
    );
  }

  async function boot() {
    setStatus('正在加载 Plotgram WASM…', 'loading');
    try {
      const mod = await import(/* webpackIgnore: true */ WASM_CDN + 'plotgram_wasm.js');
      await mod.default({ module_or_path: WASM_CDN + 'plotgram_wasm_bg.wasm' });
      await renderAll(mod);
    } catch (err) {
      setStatus('WASM 加载失败（需通过 http(s) 打开本页）', 'error');
      console.error(err);
      document.querySelectorAll('.plotgram-render').forEach((el) => {
        showError(
          el,
          '无法加载 WASM。请通过本地 HTTP 服务打开，例如：python3 -m http.server 8080',
          String(err),
        );
      });
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})();
