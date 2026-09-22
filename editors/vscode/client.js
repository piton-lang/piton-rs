// Starts the Piton language server.
//
// The server is the `piton` binary itself: `piton lsp` speaks LSP over stdio,
// so there is nothing separate to install.

const { workspace, window, commands } = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;

function activate(context) {
  const settings = workspace.getConfiguration("piton");

  // autoFormatOnSave is an option: nothing happens unless piton.formatOnSave
  // is turned on. The edits are asked for before the save, so the file on
  // disk is the formatted one, and the formatter (the language server) leaves
  // commented content alone. Without the server there is no provider, and the
  // command resolves to nothing.
  context.subscriptions.push(
    workspace.onWillSaveTextDocument((event) => {
      if (!workspace.getConfiguration("piton").get("formatOnSave", false)) {
        return;
      }
      if (event.document.languageId !== "piton") {
        return;
      }
      event.waitUntil(
        commands
          .executeCommand("vscode.executeFormatDocumentProvider", event.document.uri, {
            tabSize: 4,
            insertSpaces: true,
          })
          .then((edits) => edits || [])
      );
    })
  );

  if (!settings.get("server.enabled", true)) {
    return;
  }

  const command = settings.get("server.path", "piton");
  const serverOptions = {
    run: { command, args: ["lsp"], transport: TransportKind.stdio },
    debug: { command, args: ["lsp"], transport: TransportKind.stdio },
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "piton" }],
    synchronize: {
      // The project configuration decides what is reachable, so a change to it
      // changes every file's analysis.
      fileEvents: workspace.createFileSystemWatcher("**/piton.config.pi"),
    },
  };

  client = new LanguageClient("piton", "Piton", serverOptions, clientOptions);
  client.start().catch((error) => {
    window.showErrorMessage(
      `Piton: cannot start \`${command} lsp\`. Install it with \`cargo xtask install\`, or set piton.server.path. (${error.message})`
    );
  });
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
