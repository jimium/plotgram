import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import './App.css';
import { TopBar, type ExportActions } from './components/TopBar';
import { CodeEditor, type CodeEditorHandle } from './components/CodeEditor';
import { Preview } from './components/Preview';
import { Inspector } from './components/Inspector';
import { ExampleDrawer } from './components/ExampleDrawer';
import { HelpPanel } from './components/HelpPanel';
import { ResizeHandle } from './components/ResizeHandle';
import { Toast, type ToastMessage } from './components/Toast';
import { AstViewer } from './components/AstViewer';
import { SceneJsonViewer } from './components/SceneJsonViewer';
import { CommandPalette, type Command } from './components/CommandPalette';
import { IconChevron, IconError, IconWarning } from './components/Icons';
import { useWasm } from './hooks/useWasm';
import { useLayoutCatalog } from './hooks/useLayoutCatalog';
import { useLocalStorage } from './hooks/useLocalStorage';
import { renderSource, validateSource, parseSource, type DiagramJson, type RenderFormat, type ExportReport } from './lib/wasm';
import { parseDiagnostics, formatContextLines, type Diagnostic } from './lib/errorParse';
import { type DiagramContext, type EntityInfo, type GroupInfo } from './lib/contextCompletion';
import {
  downloadSvg,
  downloadPng,
  downloadWebp,
  downloadText,
  downloadJson,
  downloadDrawio,
  openInDrawio,
  copyText,
  copyPngToClipboard,
} from './lib/exportImage';
import { buildShareUrl, readStateFromUrl } from './lib/share';
import {
  applyLayoutOptions,
  detectDiagramType,
  getDiagramDefaults,
  EMPTY_LAYOUT_OPTIONS,
  layoutOptionsFromDefaults,
  normalizeLayoutOptions,
  reconcileLayoutOptionsWithDefaults,
  type LayoutOptions,
} from './data/layoutOptions';
import {
  buildRenderOptions,
  DEFAULT_APPEARANCE_OPTIONS,
  normalizeAppearanceOptions,
  type AppearanceOptions,
} from './data/appearanceOptions';
import {
  DEFAULT_PREVIEW_BACKGROUND,
  normalizePreviewBackground,
  PREVIEW_BG_STORAGE_KEY,
  type PreviewBackground,
} from './data/previewBackground';
import {
  buildIntentOverlay,
  normalizeIntentDrafts,
  EMPTY_INTENT_DRAFTS,
  STATUS_LABELS,
  STATUS_COLORS,
  describeTopologyIntent,
  describeGeometricIntent,
  type IntentDrafts,
  type RefinementReport,
} from './data/intentOptions';
import { EXAMPLES, DEFAULT_EXAMPLE_ID, getExample, type DiagramKind } from './data/examples';

type Theme = 'light' | 'dark';
type MobilePane = 'editor' | 'preview' | 'inspector';
type PreviewTab = 'graph' | 'ast' | 'ascii' | 'scene';
type BottomTab = 'problems' | 'output' | 'stats' | 'intent';
type LayoutSource = 'source' | 'panel';

const DEFAULT_CODE = getExample(DEFAULT_EXAMPLE_ID)?.source ?? EXAMPLES[0].source;

/** PNG / WebP 浏览器栅格化导出倍率选项 */
export const RASTER_EXPORT_SCALES = [1, 2, 3] as const;
export type RasterExportScale = (typeof RASTER_EXPORT_SCALES)[number];

function systemTheme(): Theme {
  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

let toastSeq = 0;

/** 简单统计源码中 entity 和 edge 数量 */
function countEntities(source: string): number {
  const matches = source.match(/^\s*entity\s+\w+/gm);
  return matches ? matches.length : 0;
}

function countEdges(source: string): number {
  const matches = source.match(/^\s*\w+\s*-{1,2}>\s*\w+/gm);
  return matches ? matches.length : 0;
}

function App() {
  const { wasm, ready, error: wasmError, version } = useWasm();
  const layoutCatalog = useLayoutCatalog(wasm, ready);

  // ─── 持久化状态 ──────────────────────────────────────────
  const [code, setCode] = useLocalStorage('drawify.code', DEFAULT_CODE);
  const [layoutOptionsStored, setLayoutOptions] = useLocalStorage<LayoutOptions>(
    'drawify.layout',
    EMPTY_LAYOUT_OPTIONS,
  );
  const [appearanceOptionsStored, setAppearanceOptions] = useLocalStorage<AppearanceOptions>(
    'drawify.appearance',
    DEFAULT_APPEARANCE_OPTIONS,
  );
  const [theme, setTheme] = useLocalStorage<Theme>('drawify.theme', systemTheme());
  const [editorWidth, setEditorWidth] = useLocalStorage('drawify.editorWidth', 380);
  const [inspectorWidth, setInspectorWidth] = useLocalStorage('drawify.inspectorWidth', 300);
  const [rasterExportScale, setRasterExportScale] = useLocalStorage<RasterExportScale>(
    'drawify.rasterScale',
    2,
  );
  const [intentDraftsStored, setIntentDrafts] = useLocalStorage<IntentDrafts>(
    'drawify.intents',
    EMPTY_INTENT_DRAFTS,
  );
  const [previewBackgroundStored, setPreviewBackground] = useLocalStorage<PreviewBackground>(
    PREVIEW_BG_STORAGE_KEY,
    DEFAULT_PREVIEW_BACKGROUND,
  );

  // ─── 会话状态 ────────────────────────────────────────────
  const [activeExampleId, setActiveExampleId] = useState(DEFAULT_EXAMPLE_ID);
  const [svg, setSvg] = useState('');
  const [ascii, setAscii] = useState('');
  const [sceneJson, setSceneJson] = useState('');
  const [, setDrawio] = useState('');
  const [drawioExportReport, setDrawioExportReport] = useState<ExportReport | null>(null);
  const [success, setSuccess] = useState(false);
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const [renderMs, setRenderMs] = useState<number | null>(null);

  // ─── Workbench 新增状态 ──────────────────────────────────
  const [leftCollapsed, setLeftCollapsed] = useState(false);
  const [rightCollapsed, setRightCollapsed] = useState(false);
  const [activePreviewTab, setActivePreviewTab] = useState<PreviewTab>('graph');
  const [activeBottomTab, setActiveBottomTab] = useState<BottomTab>('problems');
  const [bottomPanelExpanded, setBottomPanelExpanded] = useState(false);
  const [layoutSource, setLayoutSource] = useState<LayoutSource>('panel');
  const [filename, setFilename] = useState('未命名.dfy');
  const [dirty, setDirty] = useState(false);
  const [examplesDrawerOpen, setExamplesDrawerOpen] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [renderLog, setRenderLog] = useState<string[]>([]);
  const [entityCount, setEntityCount] = useState<number | null>(null);
  const [edgeCount, setEdgeCount] = useState<number | null>(null);
  const [astData, setAstData] = useState<DiagramJson | null>(null);
  const [refinementReport, setRefinementReport] = useState<RefinementReport | null>(null);

  // ─── 原有状态 ────────────────────────────────────────────
  const [helpOpen, setHelpOpen] = useState(false);
  const [toast, setToast] = useState<ToastMessage | null>(null);
  const [fitSignal, setFitSignal] = useState(0);
  const [mobilePane, setMobilePane] = useState<MobilePane>('preview');

  const editorRef = useRef<CodeEditorHandle>(null);
  const debounceRef = useRef<number | null>(null);
  const autoExpandedRef = useRef(false);
  const fileHandleRef = useRef<FileSystemFileHandle | null>(null);
  const prevDiagramTypeRef = useRef<DiagramKind | null>(null);

  const showToast = useCallback((text: string, kind: ToastMessage['kind'] = 'info') => {
    setToast({ id: ++toastSeq, text, kind });
  }, []);

  // ─── Fix Action 一键修复 ──────────────────────────────────
  const applyFix = useCallback((diag: Diagnostic) => {
    const fix = diag.suggestion?.fix;
    if (!fix) return;
    const { action, payload } = fix;
    const p = payload as Record<string, unknown>;
    let newCode = code;

    try {
      switch (action) {
        case 'replace_text': {
          const oldText = String(p.old ?? '');
          const newText = String(p.new ?? '');
          if (oldText && newCode.includes(oldText)) {
            newCode = newCode.replace(oldText, newText);
          }
          break;
        }
        case 'rename_entity': {
          const oldId = String(p.old_id ?? '');
          const newId = String(p.new_id ?? '');
          if (oldId && newId) {
            // 全词替换（标识符边界）
            const re = new RegExp(`\\b${oldId.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\b`, 'g');
            newCode = newCode.replace(re, newId);
          }
          break;
        }
        case 'replace_attribute_value': {
          const oldVal = String(p.old_value ?? '');
          const newVal = String(p.new_value ?? '');
          if (oldVal && newCode.includes(oldVal)) {
            newCode = newCode.replace(oldVal, newVal);
          }
          break;
        }
        case 'rename_attribute': {
          const oldKey = String(p.old_key ?? '');
          const newKey = String(p.new_key ?? '');
          if (oldKey && newCode.includes(oldKey)) {
            newCode = newCode.replace(oldKey, newKey);
          }
          break;
        }
        case 'add_entity': {
          const id = String(p.id ?? '');
          const label = String(p.label ?? '');
          if (id) {
            const line = `    entity ${id} "${label}"\n`;
            // 在最后一个 `}` 前插入
            const lastBrace = newCode.lastIndexOf('}');
            if (lastBrace >= 0) {
              newCode = newCode.slice(0, lastBrace) + line + newCode.slice(lastBrace);
            }
          }
          break;
        }
        case 'remove_entity': {
          const id = String(p.id ?? '');
          if (id) {
            const re = new RegExp(
              `\\s*entity\\s+${id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s+"[^"]*"[\\s\\S]*?\\n`,
              'g',
            );
            newCode = newCode.replace(re, '\n');
          }
          break;
        }
        case 'remove_relation': {
          const from = String(p.from ?? '');
          const to = String(p.to ?? '');
          if (from && to) {
            const re = new RegExp(
              `\\s*${from.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*->\\s*${to.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}[^\\n]*\\n`,
              'g',
            );
            newCode = newCode.replace(re, '\n');
          }
          break;
        }
        case 'add_relation': {
          const from = String(p.from ?? '');
          const to = String(p.to ?? '');
          if (from && to) {
            const line = `    ${from} -> ${to}\n`;
            const lastBrace = newCode.lastIndexOf('}');
            if (lastBrace >= 0) {
              newCode = newCode.slice(0, lastBrace) + line + newCode.slice(lastBrace);
            }
          }
          break;
        }
        case 'remove_group': {
          const id = String(p.id ?? '');
          if (id) {
            const re = new RegExp(
              `\\s*group\\s+${id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s+"[^"]*"[\\s\\S]*?\\n`,
              'g',
            );
            newCode = newCode.replace(re, '\n');
          }
          break;
        }
        default:
          showToast(`修复类型 "${action}" 暂不支持一键应用`, 'info');
          return;
      }

      if (newCode !== code) {
        setCode(newCode);
        showToast(`已应用修复：${action}`, 'success');
      } else {
        showToast('修复未产生变更（可能源码已不匹配）', 'info');
      }
    } catch {
      showToast('修复应用失败', 'error');
    }
  }, [code, setCode, showToast]);

  const handleRasterScaleChange = useCallback((scale: number) => {
    if (scale === 1 || scale === 2 || scale === 3) {
      setRasterExportScale(scale);
    }
  }, [setRasterExportScale]);

  // ─── 主题 ────────────────────────────────────────────────
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  // ─── 首次加载：从分享 URL 还原 ────────────────────────────
  useEffect(() => {
    const shared = readStateFromUrl();
    if (shared) {
      setCode(shared.code);
      if (shared.layout) setLayoutOptions(normalizeLayoutOptions(shared.layout));
      if (shared.appearance) setAppearanceOptions(shared.appearance);
      setActiveExampleId('');
      setDirty(false);
      window.history.replaceState(null, '', window.location.pathname);
      showToast('已从分享链接载入', 'success');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ─── 代码变更标记 dirty ──────────────────────────────────
  const handleCodeChange = useCallback((value: string) => {
    setCode(value);
    setDirty(true);
  }, [setCode]);

  const diagramType = useMemo(() => detectDiagramType(code), [code]);
  const diagramDefaults = useMemo(
    () => getDiagramDefaults(layoutCatalog, diagramType),
    [layoutCatalog, diagramType],
  );
  const layoutOptions = useMemo(
    () => normalizeLayoutOptions(layoutOptionsStored, diagramDefaults),
    [layoutOptionsStored, diagramDefaults],
  );
  const appearanceOptions = useMemo(
    () => normalizeAppearanceOptions(appearanceOptionsStored),
    [appearanceOptionsStored],
  );
  const previewBackground = useMemo(
    () => normalizePreviewBackground(previewBackgroundStored),
    [previewBackgroundStored],
  );
  const intentDrafts = useMemo(
    () => normalizeIntentDrafts(intentDraftsStored),
    [intentDraftsStored],
  );
  const intentOverlay = useMemo(
    () => buildIntentOverlay(intentDrafts),
    [intentDrafts],
  );
  const entityIds = useMemo(
    () => (astData?.entities?.map((e) => e.id) ?? []),
    [astData],
  );

  // 从 AST 提取上下文信息，供编辑器自动补全使用
  const diagramContext = useMemo<DiagramContext>(() => {
    if (!astData) return { entities: [], groups: [] };
    const entities: EntityInfo[] = (astData.entities ?? []).map((e) => ({
      id: e.id,
      label: e.label,
      groupId: e.group_id,
    }));
    const groups: GroupInfo[] = (astData.groups ?? []).map((g) => ({
      id: g.id,
      label: g.label,
      parentId: g.parent_id,
    }));
    return { entities, groups };
  }, [astData]);

  // ─── 布局选项：迁移 legacy auto、图表类型切换时同步默认 ───
  useEffect(() => {
    if (!diagramDefaults || !diagramType) return;

    setLayoutOptions((stored) => {
      const hasLegacyAuto = !stored.layoutAlgo
        || stored.layoutAlgo === 'auto'
        || stored.edgeRouting === 'auto';

      if (prevDiagramTypeRef.current !== null && prevDiagramTypeRef.current !== diagramType) {
        prevDiagramTypeRef.current = diagramType;
        return layoutOptionsFromDefaults(diagramDefaults);
      }

      prevDiagramTypeRef.current = diagramType;

      const normalized = normalizeLayoutOptions(stored, diagramDefaults);
      const reconciled = reconcileLayoutOptionsWithDefaults(
        normalized,
        layoutCatalog,
        diagramType,
        diagramDefaults,
      );

      if (hasLegacyAuto || JSON.stringify(reconciled) !== JSON.stringify(stored)) {
        return reconciled;
      }
      return stored;
    });
  }, [diagramDefaults, diagramType, layoutCatalog, setLayoutOptions]);

  // ─── 外观选项：迁移 legacy styleId ───────────────────────
  useEffect(() => {
    setAppearanceOptions((stored) => {
      const normalized = normalizeAppearanceOptions(stored);
      if (JSON.stringify(normalized) !== JSON.stringify(stored)) {
        return normalized;
      }
      return stored;
    });
  }, [setAppearanceOptions]);

  // ─── 防抖渲染 ────────────────────────────────────────────
  // 按当前页签渲染单一格式;AST 页签只做 validate 获取诊断。
  useEffect(() => {
    if (!wasm || !ready) return;
    if (debounceRef.current) window.clearTimeout(debounceRef.current);

    debounceRef.current = window.setTimeout(() => {
      const effectiveSource =
        layoutSource === 'source'
          ? code
          : applyLayoutOptions(code, layoutOptions, layoutCatalog, diagramDefaults);
      const optionsJson = JSON.stringify(
        buildRenderOptions(appearanceOptions, intentOverlay, previewBackground),
      );

      const t0 = performance.now();

      // AST 页签不需要渲染,只做 validate 获取诊断
      if (activePreviewTab === 'ast') {
        const validation = validateSource(wasm, effectiveSource);
        const elapsed = performance.now() - t0;
        const diags = parseDiagnostics(validation.errors, validation.warnings);
        setDiagnostics(diags);
        setSuccess(validation.valid);
        setRenderMs(elapsed);
        setEntityCount(countEntities(code));
        setEdgeCount(countEdges(code));
        const logEntry = validation.valid
          ? `[${new Date().toLocaleTimeString()}] 校验成功 ${elapsed.toFixed(1)}ms`
          : `[${new Date().toLocaleTimeString()}] 校验失败 ${elapsed.toFixed(1)}ms — ${diags.filter(d => d.severity === 'error').length} 个错误`;
        setRenderLog(prev => [...prev.slice(-99), logEntry]);
        const hasErrors = diags.some(d => d.severity === 'error');
        if (hasErrors) {
          autoExpandedRef.current = true;
          setActiveBottomTab('problems');
          setBottomPanelExpanded(true);
        } else if (autoExpandedRef.current) {
          autoExpandedRef.current = false;
          setBottomPanelExpanded(false);
        }
        return;
      }

      const format: RenderFormat =
        activePreviewTab === 'graph' ? 'svg'
        : activePreviewTab === 'ascii' ? 'ascii'
        : 'json';

      const result = renderSource(wasm, effectiveSource, format, optionsJson);
      const elapsed = performance.now() - t0;

      const diags = parseDiagnostics(result.errors ?? [], result.warnings ?? []);
      setDiagnostics(diags);
      setSuccess(result.success);
      setRenderMs(elapsed);
      setRefinementReport(result.refinement_report ?? null);

      setEntityCount(countEntities(code));
      setEdgeCount(countEdges(code));

      const logEntry = result.success
        ? `[${new Date().toLocaleTimeString()}] 渲染成功 ${elapsed.toFixed(1)}ms`
        : `[${new Date().toLocaleTimeString()}] 渲染失败 ${elapsed.toFixed(1)}ms — ${diags.filter(d => d.severity === 'error').length} 个错误`;
      setRenderLog(prev => [...prev.slice(-99), logEntry]);

      if (result.success && result.text) {
        switch (format) {
          case 'svg': setSvg(result.text); break;
          case 'ascii': setAscii(result.text); break;
          case 'json': setSceneJson(result.text); break;
        }
      } else {
        switch (format) {
          case 'svg': setSvg(''); break;
          case 'ascii': setAscii(''); break;
          case 'json': setSceneJson(''); break;
        }
      }

      // drawio 格式采用按需渲染：仅在用户点击导出时才触发
      // （移除了之前每次编辑都自动渲染 drawio 的逻辑）

      // 自动展开/收起底部面板
      const hasErrors = diags.some(d => d.severity === 'error');
      if (hasErrors) {
        autoExpandedRef.current = true;
        setActiveBottomTab('problems');
        setBottomPanelExpanded(true);
      } else if (autoExpandedRef.current) {
        autoExpandedRef.current = false;
        setBottomPanelExpanded(false);
      }
    }, 200);

    return () => {
      if (debounceRef.current) window.clearTimeout(debounceRef.current);
    };
  }, [code, layoutOptions, appearanceOptions, intentOverlay, previewBackground, layoutSource, layoutCatalog, diagramDefaults, wasm, ready, activePreviewTab]);

  // ─── AST 解析 ────────────────────────────────────────────
  useEffect(() => {
    if (!wasm || !ready) return;
    const result = parseSource(wasm, code);
    setAstData(result.diagram);
  }, [code, wasm, ready]);

  // ─── 布局/外观变更 ───────────────────────────────────────
  const handleLayoutChange = (key: 'layoutAlgo' | 'edgeRouting' | 'layoutDirection' | 'gridSnap', value: string | boolean) => {
    setLayoutOptions((prev) => {
      const next = normalizeLayoutOptions(prev);
      if (key === 'gridSnap') {
        return { ...next, gridSnap: Boolean(value) };
      }
      if (key === 'layoutAlgo' && value !== next.layoutAlgo) {
        return { ...next, layoutAlgo: value as string, layoutConfig: {} };
      }
      if (key === 'edgeRouting' && value !== next.edgeRouting) {
        return { ...next, edgeRouting: value as string, edgeRoutingConfig: {} };
      }
      return { ...next, [key]: value };
    });
  };

  const handleLayoutConfigChange = (key: string, value: number | null) => {
    setLayoutOptions((prev) => {
      const next = normalizeLayoutOptions(prev);
      return {
        ...next,
        layoutConfig: { ...next.layoutConfig, [key]: value },
      };
    });
  };

  const handleEdgeRoutingConfigChange = (key: string, value: number | null) => {
    setLayoutOptions((prev) => {
      const next = normalizeLayoutOptions(prev);
      return {
        ...next,
        edgeRoutingConfig: { ...next.edgeRoutingConfig, [key]: value },
      };
    });
  };

  const handleAppearanceChange = <K extends keyof AppearanceOptions>(
    key: K,
    value: AppearanceOptions[K],
  ) => {
    setAppearanceOptions((prev) => ({ ...prev, [key]: value }));
  };

  // ─── 示例选择 ────────────────────────────────────────────
  const handleSelectExample = (id: string) => {
    const ex = getExample(id);
    if (!ex) return;
    setActiveExampleId(id);
    setCode(ex.source);
    const exDefaults = getDiagramDefaults(layoutCatalog, detectDiagramType(ex.source));
    setLayoutOptions(exDefaults ? layoutOptionsFromDefaults(exDefaults) : EMPTY_LAYOUT_OPTIONS);
    setAppearanceOptions(DEFAULT_APPEARANCE_OPTIONS);
    setFilename(`${ex.title}.dfy`);
    setDirty(false);
    setFitSignal((s) => s + 1);
  };

  const handleResetLayout = () => {
    if (!diagramDefaults) return;
    setLayoutOptions(layoutOptionsFromDefaults(diagramDefaults));
  };

  const handleReset = () => {
    handleResetLayout();
    setAppearanceOptions(DEFAULT_APPEARANCE_OPTIONS);
  };

  // ─── 分享 ────────────────────────────────────────────────
  const handleShare = async () => {
    const url = buildShareUrl({ code, layout: layoutOptions, appearance: appearanceOptions });
    try {
      await copyText(url);
      showToast('分享链接已复制到剪贴板', 'success');
    } catch {
      showToast('链接已写入地址栏（复制失败）', 'info');
    }
  };

  // ─── drawio 按需渲染 ────────────────────────────────────────
  // 仅在用户点击 drawio 相关导出/打开按钮时触发，避免每次编辑的 WASM 开销。
  const generateDrawio = useCallback((): string | null => {
    if (!wasm || !ready) return null;
    const effectiveSource =
      layoutSource === 'source'
        ? code
        : applyLayoutOptions(code, layoutOptions, layoutCatalog, diagramDefaults);
    const optionsJson = JSON.stringify(buildRenderOptions(appearanceOptions, intentOverlay));
    const result = renderSource(wasm, effectiveSource, 'drawio', optionsJson);
    if (result.success && result.text) {
      setDrawio(result.text);
      setDrawioExportReport(result.export_report ?? null);
      return result.text;
    }
    setDrawio('');
    setDrawioExportReport(null);
    showToast('drawio 渲染失败', 'error');
    return null;
  }, [wasm, ready, code, layoutOptions, appearanceOptions, intentOverlay, previewBackground, layoutSource, layoutCatalog, diagramDefaults, showToast]);

  // ─── 导出 ────────────────────────────────────────────────
  const exportActions = useMemo<ExportActions>(() => ({
    downloadSvg: () => {
      if (svg) downloadSvg(svg);
    },
    downloadPng: () => {
      if (!svg) return;
      downloadPng(svg, `${filename.replace(/\.dfy$/, '') || 'diagram'}.png`, rasterExportScale)
        .then(() => showToast(`PNG 已导出（${rasterExportScale}x）`, 'success'))
        .catch(() => showToast('PNG 导出失败', 'error'));
    },
    downloadWebp: () => {
      if (!svg) return;
      downloadWebp(svg, `${filename.replace(/\.dfy$/, '') || 'diagram'}.webp`, rasterExportScale)
        .then(() => showToast(`WebP 已导出（${rasterExportScale}x）`, 'success'))
        .catch(() => showToast('WebP 导出失败（浏览器可能不支持）', 'error'));
    },
    downloadAscii: () => {
      if (!ascii) return;
      downloadText(ascii, `${filename.replace(/\.dfy$/, '') || 'diagram'}.txt`);
      showToast('ASCII 已导出', 'success');
    },
    downloadJson: () => {
      if (!sceneJson) return;
      downloadJson(sceneJson, `${filename.replace(/\.dfy$/, '') || 'diagram'}.json`);
      showToast('Scene JSON 已导出', 'success');
    },
    copySvg: () => {
      if (!svg) return;
      copyText(svg)
        .then(() => showToast('SVG 源码已复制', 'success'))
        .catch(() => showToast('复制失败', 'error'));
    },
    copyPng: () => {
      if (!svg) return;
      copyPngToClipboard(svg, rasterExportScale)
        .then(() => showToast(`PNG 已复制到剪贴板（${rasterExportScale}x）`, 'success'))
        .catch(() => showToast('复制 PNG 失败（浏览器可能不支持）', 'error'));
    },
    copyAscii: () => {
      if (!ascii) return;
      copyText(ascii)
        .then(() => showToast('ASCII 文本已复制', 'success'))
        .catch(() => showToast('复制 ASCII 失败', 'error'));
    },
    copyJson: () => {
      if (!sceneJson) return;
      copyText(sceneJson)
        .then(() => showToast('Scene JSON 已复制', 'success'))
        .catch(() => showToast('复制 JSON 失败', 'error'));
    },
    downloadDrawio: () => {
      const xml = generateDrawio();
      if (!xml) return;
      downloadDrawio(xml, `${filename.replace(/\.dfy$/, '') || 'diagram'}.drawio`);
      showToast('Drawio 已导出', 'success');
    },
    copyDrawio: () => {
      const xml = generateDrawio();
      if (!xml) return;
      copyText(xml)
        .then(() => showToast('Drawio XML 已复制', 'success'))
        .catch(() => showToast('复制 Drawio 失败', 'error'));
    },
    openInDrawio: () => {
      const xml = generateDrawio();
      if (!xml) return;
      openInDrawio(xml);
      showToast('已在 draw.io 中打开', 'success');
    },
  }), [svg, ascii, sceneJson, filename, rasterExportScale, showToast, generateDrawio]);

  // ─── 文件操作 ────────────────────────────────────────────
  const handleNewFile = useCallback(() => {
    setCode(DEFAULT_CODE);
    setFilename('未命名.dfy');
    setDirty(false);
    fileHandleRef.current = null;
    setActiveExampleId(DEFAULT_EXAMPLE_ID);
    showToast('已新建文件', 'info');
  }, [setCode, showToast]);

  const handleOpenFile = useCallback(async () => {
    // 优先使用 File System Access API
    if ('showOpenFilePicker' in window) {
      try {
        const [handle] = await (window as unknown as { showOpenFilePicker: (opts?: unknown) => Promise<FileSystemFileHandle[]> }).showOpenFilePicker({
          types: [{
            description: 'Drawify 文件',
            accept: { 'text/plain': ['.dfy'] },
          }],
          multiple: false,
        });
        const file = await handle.getFile();
        const text = await file.text();
        setCode(text);
        setFilename(handle.name);
        setDirty(false);
        fileHandleRef.current = handle;
        setActiveExampleId('');
        showToast(`已打开 ${handle.name}`, 'success');
        return;
      } catch {
        // 用户取消选择，忽略
        return;
      }
    }

    // 回退到 input[type=file]
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.dfy,.txt';
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      const text = await file.text();
      setCode(text);
      setFilename(file.name);
      setDirty(false);
      fileHandleRef.current = null;
      setActiveExampleId('');
      showToast(`已打开 ${file.name}`, 'success');
    };
    input.click();
  }, [setCode, showToast]);

  const handleSaveFile = useCallback(async () => {
    // 如果已有 fileHandle，直接写回
    if (fileHandleRef.current) {
      try {
        const writable = await fileHandleRef.current.createWritable();
        await writable.write(code);
        await writable.close();
        setDirty(false);
        showToast(`已保存 ${fileHandleRef.current.name}`, 'success');
        return;
      } catch {
        showToast('保存失败', 'error');
        return;
      }
    }

    // 优先使用 File System Access API
    if ('showSaveFilePicker' in window) {
      try {
        const handle = await (window as unknown as { showSaveFilePicker: (opts?: unknown) => Promise<FileSystemFileHandle> }).showSaveFilePicker({
          suggestedName: filename,
          types: [{
            description: 'Drawify 文件',
            accept: { 'text/plain': ['.dfy'] },
          }],
        });
        const writable = await handle.createWritable();
        await writable.write(code);
        await writable.close();
        fileHandleRef.current = handle;
        setFilename(handle.name);
        setDirty(false);
        showToast(`已保存 ${handle.name}`, 'success');
        return;
      } catch {
        // 用户取消选择，忽略
        return;
      }
    }

    // 回退到下载
    const blob = new Blob([code], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
    setDirty(false);
    showToast(`已下载 ${filename}`, 'info');
  }, [code, filename, showToast]);

  // ─── 键盘快捷键 ─────────────────────────────────────────
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key === 's') {
        e.preventDefault();
        handleSaveFile();
      } else if (mod && e.key === 'Enter') {
        e.preventDefault();
        // 强制重新渲染
        setCode(prev => prev);
      } else if (mod && e.key === 'k') {
        e.preventDefault();
        setCommandPaletteOpen(prev => !prev);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [handleSaveFile]);

  // ─── 预览错误 ────────────────────────────────────────────
  const previewError = wasmError
    ? `WASM 加载失败：${wasmError}`
    : !svg && diagnostics.some((d) => d.severity === 'error')
      ? diagnostics.find((d) => d.severity === 'error')?.message ?? null
      : null;

  // ─── 拖拽调整 ────────────────────────────────────────────
  const onResizeEditor = (delta: number) => {
    setEditorWidth((w) => Math.max(280, Math.min(700, w + delta)));
  };
  const onResizeInspector = (delta: number) => {
    setInspectorWidth((w) => Math.max(200, Math.min(500, w - delta)));
  };

  // ─── 底部面板切换 ────────────────────────────────────────
  const handleToggleBottomPanel = useCallback(() => {
    setBottomPanelExpanded(prev => !prev);
    autoExpandedRef.current = false;
  }, []);

  // ─── 命令面板选项 ────────────────────────────────────────
  const commands = useMemo(() => [
    { id: 'new', label: '新建文件', shortcut: '⌘+N', action: handleNewFile },
    { id: 'open', label: '打开文件', shortcut: '⌘+O', action: handleOpenFile },
    { id: 'save', label: '保存文件', shortcut: '⌘+S', action: handleSaveFile },
    { id: 'share', label: '分享链接', action: handleShare },
    { id: 'examples', label: '打开示例库', action: () => setExamplesDrawerOpen(true) },
    { id: 'help', label: '语法速查', action: () => setHelpOpen(true) },
    { id: 'toggle-left', label: '切换编辑器面板', action: () => setLeftCollapsed(p => !p) },
    { id: 'toggle-right', label: '切换属性面板', action: () => setRightCollapsed(p => !p) },
    { id: 'toggle-bottom', label: '切换底部面板', action: handleToggleBottomPanel },
    { id: 'layout-source', label: '布局来源：跟随源码', action: () => setLayoutSource('source') },
    { id: 'layout-panel', label: '布局来源：面板覆盖', action: () => setLayoutSource('panel') },
    { id: 'theme', label: '切换主题', action: () => setTheme(t => t === 'dark' ? 'light' : 'dark') },
  ] as Command[], [handleNewFile, handleOpenFile, handleSaveFile, handleShare, handleToggleBottomPanel]);

  // ─── 渲染 ────────────────────────────────────────────────
  return (
    <div className="app">
      <TopBar
        theme={theme}
        version={version}
        filename={filename}
        dirty={dirty}
        canExport={Boolean(svg || ascii || sceneJson)}
        renderStatus={
          !ready
            ? 'idle'
            : diagnostics.some(d => d.severity === 'error')
              ? 'error'
              : diagnostics.some(d => d.severity === 'warning')
                ? 'warning'
                : success
                  ? 'success'
                  : 'idle'
        }
        errorCount={diagnostics.filter(d => d.severity === 'error').length}
        warningCount={diagnostics.filter(d => d.severity === 'warning').length}
        renderMs={renderMs}
        onOpenExamples={() => setExamplesDrawerOpen(true)}
        onOpenDocs={() => setHelpOpen(true)}
        onToggleTheme={() => setTheme((t) => (t === 'dark' ? 'light' : 'dark'))}
        onShare={handleShare}
        onNewFile={handleNewFile}
        onOpenFile={handleOpenFile}
        onSaveFile={handleSaveFile}
        onSaveAsFile={handleSaveFile}
        onOpenCommandPalette={() => setCommandPaletteOpen(true)}
        exportActions={exportActions}
        rasterScale={rasterExportScale}
        onRasterScaleChange={handleRasterScaleChange}
      />

      {/* ─── 移动端标签 ──────────────────────────────────── */}
      <div className="mobile-tabs">
        {(['editor', 'preview', 'inspector'] as MobilePane[]).map((pane) => (
          <button
            key={pane}
            type="button"
            className={mobilePane === pane ? 'active' : ''}
            onClick={() => setMobilePane(pane)}
          >
            {pane === 'editor' ? '源码' : pane === 'preview' ? '预览' : '属性'}
          </button>
        ))}
      </div>

      {/* ─── 工作区 ──────────────────────────────────────── */}
      <main className="workspace" data-mobile-pane={mobilePane}>
        {/* 编辑器列 */}
        <section
          className={`editor-col ${leftCollapsed ? 'collapsed' : ''}`}
          style={!leftCollapsed ? { width: editorWidth } : undefined}
        >
          <div className="col-head">
            <button
              type="button"
              className="icon-btn sidebar-toggle"
              onClick={() => setLeftCollapsed(true)}
              title="收起编辑器"
            >
              <IconChevron size={14} className="flip" />
            </button>
            <span>{filename}{dirty ? ' •' : ''}</span>
            {diagramType && <span className="col-head-meta">{diagramType}</span>}
          </div>
          <CodeEditor
            ref={editorRef}
            value={code}
            onChange={handleCodeChange}
            theme={theme}
            diagnostics={diagnostics}
            diagramContext={diagramContext}
          />
        </section>

        {/* 左侧折叠展开按钮 */}
        {leftCollapsed && (
          <button
            type="button"
            className="sidebar-expand-btn sidebar-expand-left"
            onClick={() => setLeftCollapsed(false)}
            title="展开编辑器"
          >
            <IconChevron size={14} />
          </button>
        )}

        {!leftCollapsed && <ResizeHandle onResize={onResizeEditor} />}

        {/* 预览列 */}
        <section className="preview-col">
          {/* 预览标签栏 */}
          <div className="preview-tabs">
            {(['graph', 'ast', 'ascii', 'scene'] as PreviewTab[]).map((tab) => (
              <button
                key={tab}
                type="button"
                className={`preview-tab ${activePreviewTab === tab ? 'active' : ''}`}
                onClick={() => setActivePreviewTab(tab)}
              >
                {tab === 'graph'
                  ? '图形'
                  : tab === 'ast'
                    ? 'AST'
                    : tab === 'ascii'
                      ? 'ASCII'
                      : 'Scene JSON'}
              </button>
            ))}
          </div>

          {activePreviewTab === 'graph' && (
            <Preview
              svg={svg}
              ready={ready}
              errorText={previewError}
              background={previewBackground}
              onBackgroundChange={setPreviewBackground}
              fitSignal={fitSignal}
            />
          )}
          {activePreviewTab === 'ast' && (
            <div className="preview-alt-pane">
              {astData ? (
                <AstViewer diagram={astData} />
              ) : (
                <pre className="ast-viewer">{ready ? '无 AST 数据' : 'WASM 加载中…'}</pre>
              )}
            </div>
          )}
          {activePreviewTab === 'ascii' && (
            <div className="preview-alt-pane">
              <pre className="ascii-preview">{ascii || '无内容'}</pre>
            </div>
          )}
          {activePreviewTab === 'scene' && (
            <div className="preview-alt-pane">
              {sceneJson ? (
                <SceneJsonViewer sceneJson={sceneJson} theme={theme} />
              ) : (
                <pre className="scene-json-preview">{ready ? '无 Scene JSON 数据' : 'WASM 加载中…'}</pre>
              )}
            </div>
          )}
        </section>

        {!rightCollapsed && <ResizeHandle onResize={onResizeInspector} />}

        {/* 右侧折叠展开按钮 */}
        {rightCollapsed && (
          <button
            type="button"
            className="sidebar-expand-btn sidebar-expand-right"
            onClick={() => setRightCollapsed(false)}
            title="展开属性面板"
          >
            <IconChevron size={14} className="flip" />
          </button>
        )}

        {/* 检查器列 */}
        <section
          className={`inspector-col ${rightCollapsed ? 'collapsed' : ''}`}
          style={!rightCollapsed ? { width: inspectorWidth } : undefined}
        >
          <div className="inspector-col-head">
            <span>属性</span>
            <button
              type="button"
              className="icon-btn sidebar-toggle"
              onClick={() => setRightCollapsed(true)}
              title="收起属性面板"
            >
              <IconChevron size={14} />
            </button>
          </div>
          <Inspector
            sourceCode={code}
            layoutOptions={layoutOptions}
            appearanceOptions={appearanceOptions}
            diagramType={diagramType}
            layoutCatalog={layoutCatalog}
            diagramDefaults={diagramDefaults}
            layoutSource={layoutSource}
            onLayoutChange={handleLayoutChange}
            onLayoutConfigChange={handleLayoutConfigChange}
            onEdgeRoutingConfigChange={handleEdgeRoutingConfigChange}
            onAppearanceChange={handleAppearanceChange}
            onResetLayout={handleResetLayout}
            onReset={handleReset}
            onLayoutSourceChange={setLayoutSource}
            intentDrafts={intentDrafts}
            onIntentChange={setIntentDrafts}
            entityIds={entityIds}
            onExportSvg={exportActions.downloadSvg}
            onExportPng={exportActions.downloadPng}
            onExportWebp={exportActions.downloadWebp}
            onExportAscii={exportActions.downloadAscii}
            onExportJson={exportActions.downloadJson}
            onExportDrawio={exportActions.downloadDrawio}
            onCopySvg={exportActions.copySvg}
            onCopyAscii={exportActions.copyAscii}
            onCopyJson={exportActions.copyJson}
            onCopyDrawio={exportActions.copyDrawio}
            onOpenInDrawio={exportActions.openInDrawio}
            drawioExportReport={drawioExportReport}
            rasterScale={rasterExportScale}
            onRasterScaleChange={handleRasterScaleChange}
            canExport={Boolean(svg || ascii || sceneJson)}
          />
        </section>
      </main>

      {/* ─── 底部面板 ────────────────────────────────────── */}
      <div className={`bottom-panel ${bottomPanelExpanded ? 'expanded' : ''}`}>
        <div className="bottom-panel-bar">
          <div className="bottom-tabs">
            {(['problems', 'output', 'stats', 'intent'] as BottomTab[]).map((tab) => (
              <button
                key={tab}
                type="button"
                className={`bottom-tab ${activeBottomTab === tab ? 'active' : ''}`}
                onClick={() => {
                  if (activeBottomTab === tab) {
                    setBottomPanelExpanded(prev => !prev);
                  } else {
                    setActiveBottomTab(tab);
                    setBottomPanelExpanded(true);
                  }
                }}
              >
                {tab === 'problems' ? '问题' : tab === 'output' ? '输出' : tab === 'stats' ? '统计' : '意图报告'}
                {tab === 'stats' && renderMs != null && (
                  <span className="tab-render-time">{renderMs.toFixed(1)} ms</span>
                )}
                {tab === 'problems' && diagnostics.filter(d => d.severity === 'error').length > 0 && (
                  <span className="tab-badge tab-badge-error">
                    {diagnostics.filter(d => d.severity === 'error').length}
                  </span>
                )}
                {tab === 'problems' && diagnostics.filter(d => d.severity === 'warning').length > 0 && (
                  <span className="tab-badge tab-badge-warn">
                    {diagnostics.filter(d => d.severity === 'warning').length}
                  </span>
                )}
                {tab === 'intent' && refinementReport && (refinementReport.conflicted + refinementReport.not_found + refinementReport.partial) > 0 && (
                  <span className="tab-badge tab-badge-warn">
                    {refinementReport.conflicted + refinementReport.not_found + refinementReport.partial}
                  </span>
                )}
              </button>
            ))}
          </div>
      </div>

        {bottomPanelExpanded && (
          <div className="bottom-panel-content">
            {activeBottomTab === 'problems' && (
              <div className="problem-list">
                {diagnostics.length === 0 ? (
                  <div className="empty-hint">没有问题</div>
                ) : (
                  diagnostics.map((d, i) => {
                    const ctxLines = formatContextLines(d.context);
                    const hasFix = !!d.suggestion?.fix;
                    return (
                      <div
                        key={`${d.code}-${d.line}-${i}`}
                        className={`problem-item problem-item--${d.severity}`}
                        onClick={() => d.line > 0 && editorRef.current?.revealLine(d.line)}
                        style={{ cursor: d.line > 0 ? 'pointer' : 'default' }}
                      >
                        <div className="problem-item-header">
                          {d.severity === 'error' ? <IconError size={14} /> : <IconWarning size={14} />}
                          {d.code && <span className="problem-code">{d.code}</span>}
                          {d.line > 0 && <span className="problem-loc">L{d.line}{d.column > 0 ? `:${d.column}` : ''}</span>}
                          <span className="problem-message">{d.message}</span>
                          {hasFix && (
                            <button
                              className="problem-fix-btn"
                              onClick={(e) => {
                                e.stopPropagation();
                                applyFix(d);
                              }}
                              title={`一键修复：${d.suggestion!.fix!.action}`}
                            >
                              修复
                            </button>
                          )}
                        </div>
                        {ctxLines.length > 0 && (
                          <div className="problem-context">
                            {ctxLines.map((line, j) => (
                              <div key={j} className="problem-context-line">{line}</div>
                            ))}
                          </div>
                        )}
                        {d.suggestion && (
                          <div className="problem-suggestion">
                            <span className="problem-suggestion-label">建议</span>
                            <span className="problem-suggestion-text">{d.suggestion.text}</span>
                          </div>
                        )}
                      </div>
                    );
                  })
                )}
              </div>
            )}
            {activeBottomTab === 'output' && (
              <div className="output-log">
                {renderLog.length === 0 ? (
                  <div className="empty-hint">暂无输出</div>
                ) : (
                  renderLog.map((entry, i) => (
                    <div key={i} className="output-entry">{entry}</div>
                  ))
                )}
              </div>
            )}
            {activeBottomTab === 'stats' && (
              <div className="stats-pane">
                <div className="stat-item">
                  <span className="stat-label">图表类型</span>
                  <span className="stat-value">{diagramType ?? '—'}</span>
                </div>
                <div className="stat-item">
                  <span className="stat-label">实体数</span>
                  <span className="stat-value">{entityCount ?? '—'}</span>
                </div>
                <div className="stat-item">
                  <span className="stat-label">边数</span>
                  <span className="stat-value">{edgeCount ?? '—'}</span>
                </div>
                <div className="stat-item">
                  <span className="stat-label">渲染耗时</span>
                  <span className="stat-value">{renderMs != null ? `${renderMs.toFixed(1)} ms` : '—'}</span>
                </div>
                <div className="stat-item">
                  <span className="stat-label">布局来源</span>
                  <span className="stat-value">{layoutSource === 'source' ? '跟随源码' : '面板覆盖'}</span>
                </div>
                <div className="stat-item">
                  <span className="stat-label">文件名</span>
                  <span className="stat-value">{filename}</span>
                </div>
              </div>
            )}
            {activeBottomTab === 'intent' && (
              <div className="intent-report-pane">
                {refinementReport ? (
                  <>
                    <div className="intent-report-summary">
                      <span className={`intent-stat intent-stat--success`}>
                        满足 {refinementReport.satisfied}
                      </span>
                      <span className={`intent-stat intent-stat--warning`}>
                        部分 {refinementReport.partial}
                      </span>
                      <span className={`intent-stat intent-stat--error`}>
                        冲突 {refinementReport.conflicted}
                      </span>
                      <span className={`intent-stat intent-stat--muted`}>
                        未找到 {refinementReport.not_found}
                      </span>
                    </div>
                    {refinementReport.results.length === 0 ? (
                      <div className="empty-hint">无意图结果</div>
                    ) : (
                      <div className="intent-result-list">
                        {refinementReport.results.map((r, i) => {
                          const isTopology = r.kind === 'below' || r.kind === 'above';
                          let desc: string;
                          if (isTopology) {
                            const draft = intentDrafts.topology[r.index];
                            desc = draft ? describeTopologyIntent(draft) : `${r.kind}[${r.index}]`;
                          } else {
                            const draft = intentDrafts.geometric[r.index];
                            desc = draft ? describeGeometricIntent(draft) : `${r.kind}[${r.index}]`;
                          }
                          return (
                            <div key={i} className={`intent-result intent-result--${STATUS_COLORS[r.status]}`}>
                              <span className={`intent-status-tag intent-status--${STATUS_COLORS[r.status]}`}>
                                {STATUS_LABELS[r.status]}
                              </span>
                              <span className="intent-result-desc">{desc}</span>
                              {r.message && <span className="intent-result-msg">{r.message}</span>}
                            </div>
                          );
                        })}
                      </div>
                    )}
                  </>
                ) : (
                  <div className="empty-hint">
                    未启用布局意图。在右侧「属性 → 布局意图」中勾选启用并添加意图后，此处将显示每条意图的满足状态报告。
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* ─── 示例库抽屉 ──────────────────────────────────── */}
      <ExampleDrawer
        open={examplesDrawerOpen}
        activeId={activeExampleId}
        onSelect={handleSelectExample}
        onClose={() => setExamplesDrawerOpen(false)}
      />

      {/* ─── 帮助面板 ────────────────────────────────────── */}
      <HelpPanel open={helpOpen} onClose={() => setHelpOpen(false)} />

      {/* ─── 命令面板 ────────────────────────────────────── */}
      {commandPaletteOpen && (
        <CommandPalette
          commands={commands}
          onClose={() => setCommandPaletteOpen(false)}
        />
      )}

      {/* ─── Toast ───────────────────────────────────────── */}
      <Toast toast={toast} onDismiss={() => setToast(null)} />
    </div>
  );
}

export default App;
