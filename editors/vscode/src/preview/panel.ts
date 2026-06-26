import * as vscode from 'vscode';
import { getPreviewHtml } from './webview';

export class PreviewPanel {
  public static readonly viewType = 'drawify.preview';

  private static readonly panels = new Map<string, PreviewPanel>();

  private readonly panel: vscode.WebviewPanel;
  private readonly disposables: vscode.Disposable[] = [];
  private updateTimer: ReturnType<typeof setTimeout> | undefined;

  private constructor(
    panel: vscode.WebviewPanel,
    private readonly extensionUri: vscode.Uri,
    private readonly document: vscode.TextDocument,
  ) {
    this.panel = panel;
    this.panel.webview.html = getPreviewHtml(this.panel.webview, this.extensionUri);
    this.scheduleUpdate(document.getText());

    this.disposables.push(
      this.panel.onDidDispose(() => this.dispose()),
      vscode.workspace.onDidChangeTextDocument((event) => {
        if (event.document.uri.toString() === document.uri.toString()) {
          this.scheduleUpdate(event.document.getText());
        }
      }),
    );
  }

  public static show(extensionUri: vscode.Uri, document: vscode.TextDocument): void {
    const key = document.uri.toString();
    const existing = PreviewPanel.panels.get(key);
    if (existing) {
      existing.panel.reveal(vscode.ViewColumn.Beside);
      existing.scheduleUpdate(document.getText());
      return;
    }

    const panel = vscode.window.createWebviewPanel(
      PreviewPanel.viewType,
      `Preview: ${document.fileName.split('/').pop() ?? 'diagram'}`,
      vscode.ViewColumn.Beside,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [extensionUri],
      },
    );

    PreviewPanel.panels.set(key, new PreviewPanel(panel, extensionUri, document));
  }

  public static disposeAll(): void {
    for (const panel of PreviewPanel.panels.values()) {
      panel.dispose();
    }
    PreviewPanel.panels.clear();
  }

  private scheduleUpdate(source: string): void {
    clearTimeout(this.updateTimer);
    this.updateTimer = setTimeout(() => {
      void this.panel.webview.postMessage({ type: 'update', source });
    }, 150);
  }

  private dispose(): void {
    clearTimeout(this.updateTimer);
    PreviewPanel.panels.delete(this.document.uri.toString());
    this.panel.dispose();
    while (this.disposables.length) {
      this.disposables.pop()?.dispose();
    }
  }
}
