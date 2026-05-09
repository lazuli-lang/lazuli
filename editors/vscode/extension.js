const vscode = require('vscode');

function activate(context) {
  for (const document of vscode.workspace.textDocuments) {
    ensureLazuliLanguage(document);
  }

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => {
      ensureLazuliLanguage(document);
    })
  );
}

function deactivate() {}

function ensureLazuliLanguage(document) {
  if (document.uri.scheme !== 'file') {
    return;
  }

  if (!document.uri.fsPath.toLowerCase().endsWith('.lzi')) {
    return;
  }

  if (document.languageId === 'lazuli') {
    return;
  }

  void vscode.languages.setTextDocumentLanguage(document, 'lazuli');
}

module.exports = {
  activate,
  deactivate
};
