import * as vscode from 'vscode';

function escapeHtmlAttr(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
}

/** Webview HTML 模板；WASM 在 media/wasm/ 中独立构建。 */
export function getPreviewHtml(webview: vscode.Webview, extensionUri: vscode.Uri): string {
  const wasmJsUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, 'media', 'wasm', 'drawify_wasm.js'),
  );
  const wasmBinUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, 'media', 'wasm', 'drawify_wasm_bg.wasm'),
  );
  const bootstrapScriptUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, 'media', 'preview-bootstrap.js'),
  );
  const previewScriptUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, 'media', 'preview.js'),
  );
  const cspSource = webview.cspSource;

  // VS Code Webview 禁止 inline script，WASM 初始化放在 preview-bootstrap.js
  return `<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8" />
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${cspSource} data:; script-src ${cspSource} 'wasm-unsafe-eval'; style-src ${cspSource} 'unsafe-inline'; connect-src ${cspSource};" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta id="drawify-config" data-wasm-js="${escapeHtmlAttr(wasmJsUri.toString())}" data-wasm-bin="${escapeHtmlAttr(wasmBinUri.toString())}" />
  <title>Drawify Preview</title>
  <style>
    body {
      margin: 0;
      padding: 16px;
      font-family: var(--vscode-font-family);
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
    }
    #status { opacity: 0.7; font-size: 13px; }
    #diagram { margin-top: 12px; }
    #diagram svg { max-width: 100%; height: auto; }
    .error {
      color: var(--vscode-errorForeground);
      white-space: pre-wrap;
      margin: 0;
    }
  </style>
</head>
<body>
  <div id="status">正在加载 WASM…</div>
  <div id="diagram"></div>
  <script type="module" src="${bootstrapScriptUri}"></script>
  <script type="module" src="${previewScriptUri}"></script>
</body>
</html>`;
}
