import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
} from 'react';
import { EditorState, Compartment } from '@codemirror/state';
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  drawSelection,
  rectangularSelection,
  crosshairCursor,
} from '@codemirror/view';
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from '@codemirror/commands';
import {
  bracketMatching,
  indentOnInput,
  syntaxHighlighting,
  defaultHighlightStyle,
} from '@codemirror/language';
import {
  autocompletion,
  completionKeymap,
  closeBrackets,
  closeBracketsKeymap,
} from '@codemirror/autocomplete';
import { searchKeymap } from '@codemirror/search';
import {
  lintGutter,
  lintKeymap,
  setDiagnostics,
  type Diagnostic as CmDiagnostic,
} from '@codemirror/lint';
import { oneDark } from '@codemirror/theme-one-dark';
import { drawify } from '../lib/drawifyLang';
import { setDiagramContext, type DiagramContext } from '../lib/contextCompletion';
import type { Diagnostic } from '../lib/errorParse';

export interface CodeEditorHandle {
  revealLine: (line: number) => void;
  focus: () => void;
}

interface CodeEditorProps {
  value: string;
  onChange: (value: string) => void;
  theme: 'light' | 'dark';
  diagnostics: Diagnostic[];
  diagramContext: DiagramContext;
}

const themeCompartment = new Compartment();

const baseTheme = EditorView.theme({
  '&': { height: '100%', fontSize: '13.5px' },
  '.cm-scroller': {
    fontFamily: "'JetBrains Mono', 'SF Mono', 'Monaco', 'Menlo', monospace",
    lineHeight: '1.6',
  },
  '.cm-content': { padding: '12px 0' },
  '.cm-gutters': { borderRight: '1px solid var(--border)' },
});

function lightTheme() {
  return [
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    EditorView.theme(
      {
        '&': { backgroundColor: 'var(--editor-bg)', color: 'var(--text)' },
        '.cm-gutters': {
          backgroundColor: 'var(--editor-bg)',
          color: 'var(--text-faint)',
        },
        '.cm-activeLineGutter': { backgroundColor: 'var(--panel-2)' },
        '.cm-activeLine': { backgroundColor: 'rgba(0,0,0,0.03)' },
      },
      { dark: false },
    ),
  ];
}

function darkTheme() {
  return oneDark;
}

function toCmDiagnostics(state: EditorState, diags: Diagnostic[]): CmDiagnostic[] {
  const result: CmDiagnostic[] = [];
  for (const d of diags) {
    if (!d.line || d.line < 1 || d.line > state.doc.lines) continue;
    const line = state.doc.line(d.line);
    const startCol = d.column && d.column > 0 ? Math.min(d.column - 1, line.length) : 0;
    const from = line.from + startCol;
    // 使用 location.end 计算更精确的 to 位置（同一行内）
    const endLine = d.raw.location.end.line;
    const endCol = endLine === d.line
      ? (d.raw.location.end.column > 0 ? Math.min(d.raw.location.end.column - 1, line.length) : line.length)
      : line.length;
    const to = Math.max(line.from + endCol, from + 1);
    // 消息包含建议文本，提升内联诊断信息量
    const suggestionSuffix = d.suggestion ? `\n建议: ${d.suggestion.text}` : '';
    result.push({
      from,
      to,
      severity: d.severity,
      message: d.code ? `${d.code}: ${d.message}${suggestionSuffix}` : `${d.message}${suggestionSuffix}`,
    });
  }
  return result;
}

export const CodeEditor = forwardRef<CodeEditorHandle, CodeEditorProps>(
  function CodeEditor({ value, onChange, theme, diagnostics, diagramContext }, ref) {
    const hostRef = useRef<HTMLDivElement>(null);
    const viewRef = useRef<EditorView | null>(null);
    const onChangeRef = useRef(onChange);
    onChangeRef.current = onChange;

    useImperativeHandle(ref, () => ({
      revealLine: (lineNo: number) => {
        const view = viewRef.current;
        if (!view) return;
        const total = view.state.doc.lines;
        const clamped = Math.max(1, Math.min(lineNo, total));
        const line = view.state.doc.line(clamped);
        view.dispatch({
          selection: { anchor: line.from },
          effects: EditorView.scrollIntoView(line.from, { y: 'center' }),
        });
        view.focus();
      },
      focus: () => viewRef.current?.focus(),
    }));

    // 初始化（仅一次）
    useEffect(() => {
      if (!hostRef.current) return;

      const state = EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          highlightActiveLineGutter(),
          highlightSpecialChars(),
          history(),
          drawSelection(),
          rectangularSelection(),
          crosshairCursor(),
          highlightActiveLine(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          autocompletion(),
          lintGutter(),
          drawify(),
          baseTheme,
          themeCompartment.of(theme === 'dark' ? darkTheme() : lightTheme()),
          keymap.of([
            ...closeBracketsKeymap,
            ...defaultKeymap,
            ...historyKeymap,
            ...completionKeymap,
            ...searchKeymap,
            ...lintKeymap,
            indentWithTab,
          ]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              onChangeRef.current(update.state.doc.toString());
            }
          }),
        ],
      });

      const view = new EditorView({ state, parent: hostRef.current });
      viewRef.current = view;

      return () => {
        view.destroy();
        viewRef.current = null;
      };
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    // 外部 value 变化（如切换示例 / 分享导入）时同步
    useEffect(() => {
      const view = viewRef.current;
      if (!view) return;
      const current = view.state.doc.toString();
      if (current !== value) {
        view.dispatch({
          changes: { from: 0, to: current.length, insert: value },
        });
      }
    }, [value]);

    // 主题切换
    useEffect(() => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({
        effects: themeCompartment.reconfigure(theme === 'dark' ? darkTheme() : lightTheme()),
      });
    }, [theme]);

    // 诊断标注
    useEffect(() => {
      const view = viewRef.current;
      if (!view) return;
      const cmDiags = toCmDiagnostics(view.state, diagnostics);
      view.dispatch(setDiagnostics(view.state, cmDiags));
    }, [diagnostics]);

    // 同步 diagram 上下文（entity 列表 + group 结构）用于自动补全
    useEffect(() => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({ effects: setDiagramContext.of(diagramContext) });
    }, [diagramContext]);

    return <div className="code-editor-host" ref={hostRef} />;
  },
);
