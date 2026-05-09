const vscode = require('vscode');

const LANGUAGE_BY_EXTENSION = {
  '.lzi': 'lazuli'
};

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

  const path = document.uri.fsPath.toLowerCase();
  const match = Object.keys(LANGUAGE_BY_EXTENSION).find((ext) => path.endsWith(ext));
  if (!match) {
    return;
  }

  const targetLanguage = LANGUAGE_BY_EXTENSION[match];
  if (document.languageId === targetLanguage) {
    return;
  }

  void vscode.languages.setTextDocumentLanguage(document, targetLanguage);
}

module.exports = {
  activate,
  deactivate
};
