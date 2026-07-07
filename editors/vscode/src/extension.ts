import * as vscode from 'vscode';
import { PlotgramDiagnostics } from './diagnostics';
import { extendPlotgramMarkdownIt } from './markdown/markdownItPlugin';
import { PreviewPanel } from './preview/panel';

export function activate(context: vscode.ExtensionContext) {
  const diagnostics = new PlotgramDiagnostics(context.extensionPath);
  diagnostics.activate(context);

  context.subscriptions.push(
    vscode.commands.registerCommand('plotgram.openPreview', () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || editor.document.languageId !== 'plotgram') {
        void vscode.window.showWarningMessage('请在 Plotgram (.pgm) 文件中打开预览。');
        return;
      }
      PreviewPanel.show(context.extensionUri, editor.document);
    }),
  );

  return {
    extendMarkdownIt(md: unknown) {
      return extendPlotgramMarkdownIt(md as Parameters<typeof extendPlotgramMarkdownIt>[0], context.extensionPath);
    },
  };
}

export function deactivate(): void {
  PreviewPanel.disposeAll();
}
