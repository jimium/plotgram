import { useCallback, useMemo, useRef, useState } from 'react';
import { JsonReadonlyViewer, type JsonReadonlyViewerHandle } from './JsonReadonlyViewer';

interface SceneJsonViewerProps {
  sceneJson: string;
  theme: 'light' | 'dark';
}

interface SceneSummary {
  schemaVersion: string;
  format: string;
  diagramType: string;
  nodeCount: number;
  edgeCount: number;
  groupCount: number;
  canvasSize: string | null;
}

function parseSummary(sceneJson: string): SceneSummary | null {
  try {
    const data = JSON.parse(sceneJson) as {
      schema_version?: string;
      format?: string;
      diagram_type?: string;
      nodes?: unknown[];
      edges?: unknown[];
      groups?: unknown[];
      canvas?: { width?: number; height?: number };
    };

    const width = data.canvas?.width;
    const height = data.canvas?.height;

    return {
      schemaVersion: data.schema_version ?? '—',
      format: data.format ?? '—',
      diagramType: data.diagram_type ?? '—',
      nodeCount: data.nodes?.length ?? 0,
      edgeCount: data.edges?.length ?? 0,
      groupCount: data.groups?.length ?? 0,
      canvasSize:
        width != null && height != null ? `${width}×${height}` : null,
    };
  } catch {
    return null;
  }
}

export function SceneJsonViewer({ sceneJson, theme }: SceneJsonViewerProps) {
  const viewerRef = useRef<JsonReadonlyViewerHandle>(null);
  const [copied, setCopied] = useState(false);
  const summary = useMemo(() => parseSummary(sceneJson), [sceneJson]);

  const handleCopyJson = useCallback(() => {
    navigator.clipboard.writeText(sceneJson);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [sceneJson]);

  return (
    <div className="scene-json-viewer-root">
      <div className="ast-toolbar">
        <span className="ast-toolbar-info">
          {summary ? (
            <>
              <code>{summary.format}</code>
              {' · '}
              v{summary.schemaVersion}
              {' · '}
              {summary.nodeCount} 节点 · {summary.edgeCount} 边 · {summary.groupCount} 组
              {summary.canvasSize && <> · {summary.canvasSize}</>}
            </>
          ) : (
            'Scene JSON'
          )}
        </span>
        <div className="scene-json-toolbar-actions">
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => viewerRef.current?.foldAll()}
          >
            全部折叠
          </button>
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => viewerRef.current?.unfoldAll()}
          >
            全部展开
          </button>
          <button type="button" className="btn btn-ghost btn-sm" onClick={handleCopyJson}>
            {copied ? '已复制!' : '复制 JSON'}
          </button>
        </div>
      </div>
      <JsonReadonlyViewer ref={viewerRef} value={sceneJson} theme={theme} />
    </div>
  );
}
