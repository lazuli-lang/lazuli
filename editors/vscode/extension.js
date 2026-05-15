const path = require('path');
const fs = require('fs');
const vscode = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function activate(context) {
  const serverPath = resolveServerPath(context);
  if (!serverPath) {
    const message =
      "Lazuli LSP binary not found. Set `lazuli.lspPath` to your `lazuli` executable, " +
      "or install Lazuli on PATH. The extension will keep providing syntax highlighting " +
      "and icons, but live diagnostics, hover, and completion are disabled.";
    vscode.window.showWarningMessage(message);
    return;
  }

  const serverOptions = {
    run: { command: serverPath, args: ['lsp'], transport: TransportKind.stdio },
    debug: { command: serverPath, args: ['lsp'], transport: TransportKind.stdio }
  };

  const clientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'lazuli-feature' },
      { scheme: 'file', language: 'lazuli-experience' }
    ],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.{lzi,lzx}')
    },
    outputChannelName: 'Lazuli'
  };

  client = new LanguageClient(
    'lazuli',
    'Lazuli Language Server',
    serverOptions,
    clientOptions
  );

  context.subscriptions.push({
    dispose: () => {
      if (client) {
        return client.stop();
      }
      return undefined;
    }
  });

  client.start();
}

function deactivate() {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

/**
 * Auto-detect order:
 *   1. user setting `lazuli.lspPath` (explicit override)
 *   2. bundled binary in <extension>/server/lazuli{.exe}
 *   3. `lazuli` on PATH
 * Returns null when none resolve to an existing executable.
 */
function resolveServerPath(context) {
  const config = vscode.workspace.getConfiguration('lazuli');
  const userPath = (config.get('lspPath') || '').trim();
  if (userPath && fs.existsSync(userPath)) {
    return userPath;
  }

  const exeName = process.platform === 'win32' ? 'lazuli.exe' : 'lazuli';
  const bundled = path.join(context.extensionPath, 'server', exeName);
  if (fs.existsSync(bundled)) {
    return bundled;
  }

  const pathDirs = (process.env.PATH || '').split(path.delimiter);
  for (const dir of pathDirs) {
    const candidate = path.join(dir, exeName);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  return null;
}

module.exports = {
  activate,
  deactivate
};
