import * as vscode from 'vscode';
import { isNodeWasmAvailable, validateSource } from './wasm/node';

const DIAGNOSTIC_SOURCE = 'drawify';
const LINE_COL_RE = /\[line (\d+):(\d+)\]/;

function cleanMessage(message: string): string {
  return message.replace(/^[✗⚠]\s+\S+\s+/, '').replace(/\[line \d+:\d+\]\s*/, '');
}

function toDiagnostic(message: string, severity: vscode.DiagnosticSeverity): vscode.Diagnostic {
  const match = message.match(LINE_COL_RE);
  const text = cleanMessage(message);

  if (!match) {
    return new vscode.Diagnostic(new vscode.Range(0, 0, 0, 1), text, severity);
  }

  const line = Math.max(0, parseInt(match[1], 10) - 1);
  const column = Math.max(0, parseInt(match[2], 10) - 1);
  const range = new vscode.Range(line, column, line, Math.max(column + 1, column));

  return new vscode.Diagnostic(range, text, severity);
}

export class DrawifyDiagnostics {
  private readonly collection = vscode.languages.createDiagnosticCollection(DIAGNOSTIC_SOURCE);
  private readonly timers = new Map<string, ReturnType<typeof setTimeout>>();

  constructor(private readonly extensionPath: string) {}

  activate(context: vscode.ExtensionContext): void {
    context.subscriptions.push(this.collection);

    context.subscriptions.push(
      vscode.workspace.onDidOpenTextDocument((doc) => this.schedule(doc)),
      vscode.workspace.onDidChangeTextDocument((event) => this.schedule(event.document)),
      vscode.workspace.onDidCloseTextDocument((doc) => {
        this.collection.delete(doc.uri);
        this.clearTimer(doc.uri.toString());
      }),
    );

    for (const doc of vscode.workspace.textDocuments) {
      this.schedule(doc);
    }
  }

  private schedule(document: vscode.TextDocument): void {
    if (document.languageId !== 'drawify') {
      return;
    }

    const key = document.uri.toString();
    this.clearTimer(key);

    this.timers.set(
      key,
      setTimeout(() => {
        this.timers.delete(key);
        this.publish(document);
      }, 300),
    );
  }

  private clearTimer(key: string): void {
    const timer = this.timers.get(key);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(key);
    }
  }

  private publish(document: vscode.TextDocument): void {
    if (!isNodeWasmAvailable(this.extensionPath)) {
      this.collection.set(document.uri, [
        new vscode.Diagnostic(
          new vscode.Range(0, 0, 0, 1),
          'Drawify WASM 未构建，请执行 npm run build:wasm',
          vscode.DiagnosticSeverity.Warning,
        ),
      ]);
      return;
    }

    try {
      const result = validateSource(this.extensionPath, document.getText());
      const diagnostics = [
        ...result.errors.map((message) => toDiagnostic(message, vscode.DiagnosticSeverity.Error)),
        ...result.warnings.map((message) =>
          toDiagnostic(message, vscode.DiagnosticSeverity.Warning),
        ),
      ];
      this.collection.set(document.uri, diagnostics);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.collection.set(document.uri, [
        new vscode.Diagnostic(
          new vscode.Range(0, 0, 0, 1),
          message,
          vscode.DiagnosticSeverity.Error,
        ),
      ]);
    }
  }

  dispose(): void {
    for (const key of this.timers.keys()) {
      this.clearTimer(key);
    }
    this.collection.dispose();
  }
}
