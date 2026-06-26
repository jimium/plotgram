import * as vscode from 'vscode';
import { DrawifyDiagnostics } from './diagnostics';
import { extendDrawifyMarkdownIt } from './markdown/markdownItPlugin';
import { PreviewPanel } from './preview/panel';

export function activate(context: vscode.ExtensionContext) {
  const diagnostics = new DrawifyDiagnostics(context.extensionPath);
  diagnostics.activate(context);

  context.subscriptions.push(
    vscode.commands.registerCommand('drawify.openPreview', () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || editor.document.languageId !== 'drawify') {
        void vscode.window.showWarningMessage('请在 Drawify (.dfy) 文件中打开预览。');
        return;
      }
      PreviewPanel.show(context.extensionUri, editor.document);
    }),
  );

  return {
    extendMarkdownIt(md: unknown) {
      return extendDrawifyMarkdownIt(md as Parameters<typeof extendDrawifyMarkdownIt>[0], context.extensionPath);
    },
  };
}

export function deactivate(): void {
  PreviewPanel.disposeAll();
}
