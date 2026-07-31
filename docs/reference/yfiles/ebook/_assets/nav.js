/* ==========================================================================
   yFiles 参考文库 · ebook 共享导航脚本
   职责：渲染顶栏 / 左侧目录 / 上下篇 / 暗色切换 / 移动端抽屉 / KaTeX 自动加载
   各篇 HTML 只需 <body data-page="01"> 即可，其余由本脚本生成
   ========================================================================== */

// ---- 文档元数据（21 篇 + 索引） ----
const PAGES = [
  { n: "00", file: "00-索引与阅读指南.html", title: "索引与阅读指南", short: "索引", cat: "总览" },
  { n: "01", file: "01-sugiyama分层布局.html", title: "Sugiyama 分层布局", short: "分层布局", cat: "核心算法" },
  { n: "02", file: "02-正交布局与TSM.html", title: "正交布局与 TSM", short: "正交布局", cat: "核心算法" },
  { n: "03", file: "03-正交边路由.html", title: "正交边路由", short: "边路由", cat: "核心算法" },
  { n: "04", file: "04-力导向与stress布局.html", title: "力导向与 stress 布局", short: "力导向", cat: "核心算法" },
  { n: "05", file: "05-树与径向布局.html", title: "树与径向布局", short: "树布局", cat: "核心算法" },
  { n: "06", file: "06-标签放置.html", title: "标签放置", short: "标签", cat: "核心算法" },
  { n: "07", file: "07-约束增量与mentalmap.html", title: "约束、增量与 mental map", short: "约束与稳定性", cat: "核心算法" },
  { n: "08", file: "08-分组泳道与端口约束.html", title: "分组、泳道与端口约束", short: "分组与端口", cat: "核心算法" },
  { n: "09", file: "09-yfiles类引擎架构.html", title: "yFiles 类引擎架构", short: "引擎架构", cat: "工程" },
  { n: "10", file: "10-行业图应用.html", title: "行业图应用", short: "行业图", cat: "应用" },
  { n: "11", file: "11-质量度量与基准.html", title: "质量度量与基准", short: "质量度量", cat: "工程" },
  { n: "12", file: "12-论文书目.html", title: "论文书目（带注解）", short: "论文书目", cat: "参考" },
  { n: "13", file: "13-实现路线图与选型.html", title: "实现路线图与选型", short: "路线图", cat: "工程" },
  { n: "14", file: "14-图论与优化工具箱.html", title: "图论与优化基础工具箱", short: "工具箱", cat: "参考" },
  { n: "15", file: "15-几何与落笔层.html", title: "几何与落笔层", short: "几何落笔", cat: "工程" },
  { n: "16", file: "16-大图性能与wasm.html", title: "大图性能与 WASM 工程", short: "性能/WASM", cat: "工程" },
  { n: "17", file: "17-动画过渡与视图.html", title: "动画、过渡与视图交互", short: "动画过渡", cat: "工程" },
  { n: "18", file: "18-超图总线与eda路由.html", title: "超图、总线与 EDA 路由", short: "超图/EDA", cat: "应用" },
  { n: "19", file: "19-序列图与一维排列.html", title: "序列图与一维排列布局", short: "序列图", cat: "应用" },
  { n: "20", file: "20-图种profile与参数映射.html", title: "图种 profile 与参数映射", short: "Profile", cat: "工程" },
  { n: "21", file: "../21-术语表.html", title: "专业术语表（图解版）", short: "术语表", cat: "参考" },
];

const CAT_ORDER = ["总览", "核心算法", "工程", "应用", "参考"];

// ---- 工具 ----
function el(tag, attrs = {}, children = []) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k === "html") e.innerHTML = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v);
  }
  for (const c of [].concat(children)) {
    if (c == null) continue;
    e.appendChild(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return e;
}

function currentPage() {
  return document.body.dataset.page || "";
}

function findPage(n) {
  return PAGES.find(p => p.n === n);
}

// ---- 暗色模式 ----
function initTheme() {
  const saved = localStorage.getItem("ebook-theme");
  if (saved) document.documentElement.setAttribute("data-theme", saved);
  else if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) {
    document.documentElement.setAttribute("data-theme", "dark");
  }
}
function toggleTheme() {
  const cur = document.documentElement.getAttribute("data-theme");
  const next = cur === "dark" ? "" : "dark";
  if (next) document.documentElement.setAttribute("data-theme", next);
  else document.documentElement.removeAttribute("data-theme");
  localStorage.setItem("ebook-theme", next);
}

// ---- 图标 SVG ----
const ICONS = {
  sun: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"/></svg>',
  moon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>',
  menu: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 12h18M3 6h18M3 18h18"/></svg>',
  prev: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 18l-6-6 6-6"/></svg>',
  next: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18l6-6-6-6"/></svg>',
};

// ---- 顶栏 ----
function buildTopbar() {
  const page = currentPage();
  const cur = findPage(page);
  const idx = PAGES.findIndex(p => p.n === page);
  const prev = idx > 0 ? PAGES[idx - 1] : null;
  const next = idx < PAGES.length - 1 ? PAGES[idx + 1] : null;

  const topbar = el("div", { class: "topbar" });

  // 移动端菜单按钮
  topbar.appendChild(el("button", {
    class: "icon-btn menu-btn",
    onclick: () => {
      document.querySelector(".sidenav").classList.toggle("open");
      document.querySelector(".sidenav-overlay").classList.toggle("open");
    }
  }, [el("span", { html: ICONS.menu })]));

  // 品牌
  topbar.appendChild(el("div", { class: "brand" }, [
    el("a", { href: "index.html" }, ["yFiles 参考文库"]),
    el("span", { class: "vol" }, ["· ebook"])
  ]));

  // 面包屑
  if (cur) {
    topbar.appendChild(el("div", { class: "crumb" }, [
      el("a", { href: "index.html" }, ["目录"]),
      document.createTextNode(" / "),
      el("span", {}, [`${cur.n} · ${cur.title}`])
    ]));
  }

  topbar.appendChild(el("div", { class: "spacer" }));

  // 上一篇
  if (prev) {
    topbar.appendChild(el("a", { class: "nav-btn", href: prev.file, title: `上一篇：${prev.title}` }, ["上一篇"]));
  } else {
    topbar.appendChild(el("span", { class: "nav-btn disabled" }, ["上一篇"]));
  }

  // 下一篇
  if (next) {
    topbar.appendChild(el("a", { class: "nav-btn", href: next.file, title: `下一篇：${next.title}` }, ["下一篇"]));
  } else {
    topbar.appendChild(el("span", { class: "nav-btn disabled" }, ["下一篇"]));
  }

  // 暗色切换
  topbar.appendChild(el("button", {
    class: "icon-btn theme-btn",
    onclick: toggleTheme,
    title: "切换暗色 / 浅色"
  }, [el("span", { class: "theme-icon", html: ICONS.moon })]));

  document.body.insertBefore(topbar, document.body.firstChild);
}

// ---- 左侧目录 ----
function buildSidenav() {
  const page = currentPage();

  const nav = el("aside", { class: "sidenav" });
  nav.appendChild(el("div", { class: "toc-title" }, ["章节"]));

  let lastCat = null;
  for (const p of PAGES) {
    if (p.cat !== lastCat) {
      nav.appendChild(el("div", { class: "toc-title" }, [p.cat]));
      lastCat = p.cat;
    }
    const item = el("a", {
      class: "toc-item" + (p.n === page ? " active" : ""),
      href: p.file
    }, [
      el("span", { class: "num" }, [p.n]),
      document.createTextNode(p.short)
    ]);
    nav.appendChild(item);

    // 当前页的页内 H2 目录（运行时填充）
    if (p.n === page) {
      const ptoc = el("div", { class: "page-toc", id: "page-toc" });
      nav.appendChild(ptoc);
    }
  }

  document.body.insertBefore(nav, document.body.firstChild.nextSibling);

  // 移动端遮罩
  const overlay = el("div", {
    class: "sidenav-overlay",
    onclick: () => {
      document.querySelector(".sidenav").classList.remove("open");
      document.querySelector(".sidenav-overlay").classList.remove("open");
    }
  });
  document.body.appendChild(overlay);
}

// ---- 页内 H2 目录 + 滚动监听 ----
function buildPageToc() {
  const ptoc = document.getElementById("page-toc");
  if (!ptoc) return;

  const headings = Array.from(document.querySelectorAll("article h2"));
  if (headings.length === 0) return;

  // 给每个 h2 加 id（若没有）
  headings.forEach((h, i) => {
    if (!h.id) h.id = "sec-" + (i + 1);
  });

  headings.forEach((h, i) => {
    const a = el("a", {
      class: "ptoc-item",
      href: "#" + h.id,
      "data-target": h.id
    }, [h.textContent.replace(/^\d+\.\s*/, "").replace(/^§\s*/, "")]);
    ptoc.appendChild(a);
  });

  // 滚动监听高亮
  const links = Array.from(ptoc.querySelectorAll(".ptoc-item"));
  const observer = new IntersectionObserver((entries) => {
    entries.forEach(e => {
      if (e.isIntersecting) {
        links.forEach(l => l.classList.remove("active"));
        const link = ptoc.querySelector(`[data-target="${e.target.id}"]`);
        if (link) link.classList.add("active");
      }
    });
  }, { rootMargin: "-80px 0px -70% 0px", threshold: 0 });
  headings.forEach(h => observer.observe(h));
}

// ---- 文末上下篇导航 ----
function buildPager() {
  const page = currentPage();
  const idx = PAGES.findIndex(p => p.n === page);
  if (idx < 0) return;
  const prev = idx > 0 ? PAGES[idx - 1] : null;
  const next = idx < PAGES.length - 1 ? PAGES[idx + 1] : null;

  const article = document.querySelector("article");
  if (!article) return;

  const pager = el("nav", { class: "pager" });

  if (prev) {
    pager.appendChild(el("a", { class: "pager-item prev", href: prev.file }, [
      el("span", { class: "dir" }, ["← 上一篇 · " + prev.n]),
      el("span", { class: "title" }, [prev.title])
    ]));
  } else {
    pager.appendChild(el("span", { class: "pager-item prev disabled" }, [
      el("span", { class: "dir" }, ["已是第一篇"])
    ]));
  }

  if (next) {
    pager.appendChild(el("a", { class: "pager-item next", href: next.file }, [
      el("span", { class: "dir" }, ["下一篇 · " + next.n + " →"]),
      el("span", { class: "title" }, [next.title])
    ]));
  } else {
    pager.appendChild(el("span", { class: "pager-item next disabled" }, [
      el("span", { class: "dir" }, ["已是最后一篇"])
    ]));
  }

  article.appendChild(pager);
}

// ---- 暗色按钮图标同步 ----
function syncThemeIcon() {
  const btn = document.querySelector(".theme-btn .theme-icon");
  if (!btn) return;
  const isDark = document.documentElement.getAttribute("data-theme") === "dark";
  btn.innerHTML = isDark ? ICONS.sun : ICONS.moon;
}

// ---- KaTeX 自动加载（页面上有 .formula 或 $$ 时） ----
function loadKatex() {
  const hasFormula = document.querySelector(".formula, .katex-inline");
  if (!hasFormula) return Promise.resolve();

  return new Promise((resolve) => {
    // CSS
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = "https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.css";
    document.head.appendChild(link);

    // JS
    const script = document.createElement("script");
    script.src = "https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.js";
    script.onload = () => {
      // 渲染 .formula 里的 $$...$$ 和 \(...\)
      document.querySelectorAll(".formula").forEach(f => {
        const raw = f.getAttribute("data-tex") || f.textContent;
        try {
          katex.render(raw, f, { displayMode: true, throwOnError: false });
        } catch (e) { /* 留原文 */ }
      });
      // 行内 \(...\)
      document.querySelectorAll(".katex-inline").forEach(n => {
        const raw = n.getAttribute("data-tex") || n.textContent;
        try {
          katex.render(raw, n, { displayMode: false, throwOnError: false });
        } catch (e) { /* 留原文 */ }
      });
      resolve();
    };
    document.body.appendChild(script);
  });
}

// ---- 键盘快捷键：← → 翻页 ----
function bindKeys() {
  const page = currentPage();
  const idx = PAGES.findIndex(p => p.n === page);
  if (idx < 0) return;
  document.addEventListener("keydown", (e) => {
    if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    if (e.key === "ArrowLeft" && idx > 0) {
      window.location.href = PAGES[idx - 1].file;
    } else if (e.key === "ArrowRight" && idx < PAGES.length - 1) {
      window.location.href = PAGES[idx + 1].file;
    }
  });
}

// ---- 初始化 ----
function init() {
  initTheme();
  // 封面页不建目录树
  if (document.body.dataset.page !== undefined && document.body.dataset.page !== "index") {
    buildTopbar();
    buildSidenav();
    buildPageToc();
    buildPager();
    bindKeys();
  }
  syncThemeIcon();
  // 监听暗色切换以同步图标
  const origToggle = toggleTheme;
  window.toggleTheme = function() { origToggle(); syncThemeIcon(); };
  // 重新绑定按钮
  const btn = document.querySelector(".theme-btn");
  if (btn) btn.onclick = window.toggleTheme;

  loadKatex();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init);
} else {
  init();
}
