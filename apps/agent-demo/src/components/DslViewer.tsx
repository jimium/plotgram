/**
 * DSL 源码查看器（CodeMirror 语法高亮）
 */
import CodeMirror from '@uiw/react-codemirror';
import { EditorView } from '@codemirror/view';
import { plotgramLanguage } from '@lib/plotgramLanguage';
import { Empty } from 'antd';

interface DslViewerProps {
  source: string;
}

export function DslViewer({ source }: DslViewerProps) {
  if (!source) {
    return (
      <div className="preview-empty">
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description="还没有 DSL 源码，先让 Agent 生成图表"
        />
      </div>
    );
  }

  return (
    <CodeMirror
      value={source}
      extensions={[plotgramLanguage, EditorView.lineWrapping]}
      theme="dark"
      editable={false}
      basicSetup={{
        lineNumbers: true,
        foldGutter: false,
        highlightActiveLine: false,
        autocompletion: false,
        searchKeymap: false,
      }}
      style={{ height: '100%', fontSize: '13px' }}
      className="dsl-viewer"
    />
  );
}
