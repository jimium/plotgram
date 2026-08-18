import * as vscode from 'vscode';
import { TautcoreDiagnostics } from './diagnostics';
import { extendTautcoreMarkdownIt } from './markdown/markdownItPlugin';
import { PreviewPanel } from './preview/panel';

export function activate(context: vscode.ExtensionContext) {
  const diagnostics = new TautcoreDiagnostics(context.extensionPath);
  diagnostics.activate(context);

  context.subscriptions.push(
    vscode.commands.registerCommand('tautcore.openPreview', () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || editor.document.languageId !== 'tautcore') {
        void vscode.window.showWarningMessage('请在 Tautcore (.taut) 文件中打开预览。');
        return;
      }
      PreviewPanel.show(context.extensionUri, editor.document);
    }),
  );

  return {
    extendMarkdownIt(md: unknown) {
      return extendTautcoreMarkdownIt(md as Parameters<typeof extendTautcoreMarkdownIt>[0], context.extensionPath);
    },
  };
}

export function deactivate(): void {
  PreviewPanel.disposeAll();
}
