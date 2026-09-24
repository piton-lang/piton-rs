// Starts the Piton language server.
//
// The server is the `piton` binary itself: `piton lsp` speaks LSP over stdio,
// so there is nothing separate to install.

const {
  workspace,
  window,
  commands,
  Uri,
  Position,
  Selection,
  Range,
  ViewColumn,
  TextEditorRevealType,
} = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");
const { execFile } = require("child_process");
const fs = require("fs");

let client;

/// The server reports offsets into compiled files in bytes (UTF-8); VS Code
/// positions count UTF-16 code units. Converting through the text keeps a
/// non-ASCII character before the offset from shifting the target.
function positionAtByte(document, byteOffset) {
  const text = document.getText();
  const bytes = Buffer.from(text, "utf8");
  const clamped = Math.max(0, Math.min(byteOffset, bytes.length));
  return document.positionAt(bytes.subarray(0, clamped).toString("utf8").length);
}

function byteAtPosition(document, position) {
  return Buffer.byteLength(document.getText(new Range(document.positionAt(0), position)), "utf8");
}

/// Opens `path` with the cursor at `offset` (a byte offset), beside the
/// current editor.
async function openAt(path, offset) {
  if (!path || !fs.existsSync(path)) {
    window.showWarningMessage(
      `Piton: ${path || "the compiled output"} has not been written yet. Build the project, then try again.`
    );
    return;
  }
  const document = await workspace.openTextDocument(Uri.file(path));
  const position = positionAtByte(document, Number(offset) || 0);
  const editor = await window.showTextDocument(document, {
    viewColumn: ViewColumn.Beside,
    preserveFocus: false,
  });
  editor.selection = new Selection(position, position);
  editor.revealRange(new Range(position, position), TextEditorRevealType.InCenterIfOutsideViewport);
}

function registerCommands(context, binary) {
  // The code lenses' commands (`piton.sourceToOutput`, `piton.showLocation`)
  // belong to the server: it lists them as its own, the language client
  // registers them, and running one asks the server to open the file. So they
  // must not be registered here too -- VS Code refuses a second registration,
  // and the language client would fail to start.
  //
  // From the palette, this asks the server what the cursor produced
  // (`piton/sourceToOutput`) and offers a choice when there is more than one
  // output.
  context.subscriptions.push(
    commands.registerCommand("piton.showOutput", async () => {
      const editor = window.activeTextEditor;
      if (!client || !editor || editor.document.languageId !== "piton") {
        return;
      }
      const hits = await client.sendRequest("piton/sourceToOutput", {
        uri: editor.document.uri.toString(),
        line: editor.selection.active.line,
        character: editor.selection.active.character,
      });
      if (!Array.isArray(hits) || hits.length === 0) {
        window.showInformationMessage("Piton: nothing in the compiled output comes from here.");
        return;
      }
      const pick =
        hits.length === 1
          ? hits[0]
          : await window.showQuickPick(
              hits.map((hit) => ({ label: hit.label, description: hit.path, hit })),
              { placeHolder: "Which output?" }
            ).then((item) => item && item.hit);
      if (pick) {
        await openAt(pick.path, pick.start);
      }
    })
  );

  // The other direction: from a compiled file back to the construct that
  // produced the text under the cursor (`piton/outputToSource`).
  context.subscriptions.push(
    commands.registerCommand("piton.outputToSource", async () => {
      const editor = window.activeTextEditor;
      if (!client || !editor || editor.document.uri.scheme !== "file") {
        return;
      }
      const found = await client.sendRequest("piton/outputToSource", {
        path: editor.document.uri.fsPath,
        offset: byteAtPosition(editor.document, editor.selection.active),
      });
      if (!found || !found.path) {
        window.showInformationMessage("Piton: no Piton source produced this text.");
        return;
      }
      const document = await workspace.openTextDocument(Uri.file(found.path));
      const position = document.validatePosition(new Position(found.line || 0, found.character || 0));
      const shown = await window.showTextDocument(document, { viewColumn: ViewColumn.One });
      shown.selection = new Selection(position, position);
      shown.revealRange(new Range(position, position), TextEditorRevealType.InCenterIfOutsideViewport);
    })
  );

  // The compiled output preview: the current file through `piton compile`,
  // with the renderer of your choice, in a read-only side editor.
  context.subscriptions.push(
    commands.registerCommand("piton.previewOutput", async () => {
      const editor = window.activeTextEditor;
      if (!editor || editor.document.languageId !== "piton" || editor.document.uri.scheme !== "file") {
        return;
      }
      const renderer = await window.showQuickPick(["json", "yaml", "markdown"], {
        placeHolder: "Render as",
      });
      if (!renderer) {
        return;
      }
      const file = editor.document.uri.fsPath;
      const cwd = workspace.getWorkspaceFolder(editor.document.uri)?.uri.fsPath;
      execFile(
        binary,
        ["compile", "--renderer", renderer, file],
        { cwd, maxBuffer: 64 * 1024 * 1024 },
        async (error, stdout, stderr) => {
          if (error) {
            window.showErrorMessage(`Piton: ${(stderr || error.message).trim()}`);
            return;
          }
          const document = await workspace.openTextDocument({ content: stdout, language: renderer });
          await window.showTextDocument(document, { viewColumn: ViewColumn.Beside, preview: true });
        }
      );
    })
  );
}

function activate(context) {
  const settings = workspace.getConfiguration("piton");
  const command = settings.get("server.path", "piton");

  registerCommands(context, command);

  // autoFormatOnSave is an option: nothing happens unless piton.formatOnSave
  // is turned on. The edits are asked for before the save, so the file on
  // disk is the formatted one. The formatter (the language server) adds the
  // space after `//` and touches nothing else that is commented. Without the
  // server there is no provider, and the command resolves to nothing.
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
