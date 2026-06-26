import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react';
import { json } from '@codemirror/lang-json';
import { EditorState, Compartment } from '@codemirror/state';
import {
  EditorView,
  lineNumbers,
  highlightSpecialChars,
  drawSelection,
} from '@codemirror/view';
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  syntaxHighlighting,
  defaultHighlightStyle,
  foldAll,
  unfoldAll,
} from '@codemirror/language';
import { keymap } from '@codemirror/view';
import { defaultKeymap } from '@codemirror/commands';
import { oneDark } from '@codemirror/theme-one-dark';

export interface JsonReadonlyViewerHandle {
  foldAll: () => void;
  unfoldAll: () => void;
}

interface JsonReadonlyViewerProps {
  value: string;
  theme: 'light' | 'dark';
}

const themeCompartment = new Compartment();

const baseTheme = EditorView.theme({
  '&': { height: '100%', fontSize: '12.5px' },
  '.cm-scroller': {
    fontFamily: "'JetBrains Mono', 'SF Mono', 'Monaco', 'Menlo', monospace",
    lineHeight: '1.5',
  },
  '.cm-content': { padding: '8px 0' },
  '.cm-gutters': { borderRight: '1px solid var(--border)' },
  '&.cm-focused': { outline: 'none' },
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
      },
      { dark: false },
    ),
  ];
}

function darkTheme() {
  return oneDark;
}

export const JsonReadonlyViewer = forwardRef<JsonReadonlyViewerHandle, JsonReadonlyViewerProps>(
  function JsonReadonlyViewer({ value, theme }, ref) {
    const hostRef = useRef<HTMLDivElement>(null);
    const viewRef = useRef<EditorView | null>(null);

    useImperativeHandle(ref, () => ({
      foldAll: () => {
        const view = viewRef.current;
        if (view) foldAll(view);
      },
      unfoldAll: () => {
        const view = viewRef.current;
        if (view) unfoldAll(view);
      },
    }));

    useEffect(() => {
      if (!hostRef.current) return;

      const state = EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          highlightSpecialChars(),
          drawSelection(),
          bracketMatching(),
          foldGutter(),
          json(),
          baseTheme,
          themeCompartment.of(theme === 'dark' ? darkTheme() : lightTheme()),
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          keymap.of([...defaultKeymap, ...foldKeymap]),
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

    useEffect(() => {
      const view = viewRef.current;
      if (!view) return;
      view.dispatch({
        effects: themeCompartment.reconfigure(theme === 'dark' ? darkTheme() : lightTheme()),
      });
    }, [theme]);

    return <div className="json-readonly-viewer-host" ref={hostRef} />;
  },
);
