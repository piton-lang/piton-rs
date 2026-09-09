//! Installable editor extensions: VS Code, Zed, and JetBrains.

use crate::{textmate, GeneratedFile, GrammarSource, Vocabulary};

pub fn files(vocabulary: &Vocabulary, source: &GrammarSource) -> Vec<GeneratedFile> {
    let mut files = vec![
        GeneratedFile::new("vscode/package.json", vscode_package()),
        GeneratedFile::new("vscode/language-configuration.json", textmate::language_configuration()),
        GeneratedFile::new(
            "vscode/syntaxes/piton.tmLanguage.json",
            textmate::grammar(vocabulary),
        ),
        GeneratedFile::new("vscode/src/extension.js", vscode_client()),
        GeneratedFile::new("vscode/.vscodeignore", ".vscode/**\nsrc/**/*.map\n"),
        GeneratedFile::new("vscode/README.md", vscode_readme()),
        GeneratedFile::new("zed/extension.toml", zed_manifest(source)),
        GeneratedFile::new("zed/languages/piton/config.toml", zed_language_config()),
        GeneratedFile::new("zed/Cargo.toml", zed_cargo()),
        GeneratedFile::new("zed/src/lib.rs", zed_extension()),
        GeneratedFile::new("zed/.gitignore", ZED_GITIGNORE),
        GeneratedFile::new("zed/README.md", zed_readme(source)),
        GeneratedFile::new("jetbrains/build.gradle.kts", jetbrains_gradle()),
        GeneratedFile::new("jetbrains/settings.gradle.kts", jetbrains_settings()),
        GeneratedFile::new(
            "jetbrains/gradle/wrapper/gradle-wrapper.properties",
            jetbrains_wrapper(),
        ),
        GeneratedFile::new("jetbrains/gradle.properties", jetbrains_properties()),
        GeneratedFile::new("jetbrains/.gitignore", "/build\n/.gradle\n"),
        GeneratedFile::new(
            "jetbrains/src/main/resources/META-INF/plugin.xml",
            jetbrains_plugin(),
        ),
        GeneratedFile::new(
            "jetbrains/src/main/kotlin/dev/piton/PitonLspServerSupportProvider.kt",
            jetbrains_lsp(),
        ),
        GeneratedFile::new(
            "jetbrains/bundles/piton/piton.tmLanguage.json",
            textmate::grammar(vocabulary),
        ),
        // IntelliJ imports TextMate bundles shaped like a VS Code extension.
        GeneratedFile::new("jetbrains/bundles/piton/package.json", textmate_bundle_manifest()),
        GeneratedFile::new(
            "jetbrains/bundles/piton/language-configuration.json",
            textmate::language_configuration(),
        ),
        GeneratedFile::new("jetbrains/README.md", jetbrains_readme()),
    ];
    // Zed reads Tree-sitter queries out of the extension directory itself.
    for query in ["highlights", "injections", "folds", "indents", "outline", "brackets"] {
        let query_files = super::treesitter::files(vocabulary, source);
        let source = query_files
            .into_iter()
            .find(|file| file.path.ends_with(format!("{query}.scm")))
            .map(|file| file.contents)
            .unwrap_or_default();
        files.push(GeneratedFile::new(format!("zed/languages/piton/{query}.scm"), source));
    }
    files
}

/// The manifest IntelliJ reads when importing a TextMate bundle.
fn textmate_bundle_manifest() -> String {
    r#"{
  "name": "piton-textmate",
  "displayName": "Piton",
  "version": "0.1.0",
  "engines": { "vscode": "^1.85.0" },
  "contributes": {
    "languages": [
      {
        "id": "piton",
        "aliases": ["Piton"],
        "extensions": [".pi"],
        "configuration": "./language-configuration.json"
      }
    ],
    "grammars": [
      {
        "language": "piton",
        "scopeName": "source.piton",
        "path": "./piton.tmLanguage.json"
      }
    ]
  }
}
"#
    .to_string()
}

// ---- VS Code ---------------------------------------------------------------

fn vscode_package() -> String {
    r#"{
  "name": "piton",
  "displayName": "Piton",
  "description": "Syntax highlighting and language server support for the Piton language",
  "version": "0.1.0",
  "publisher": "piton-lang",
  "license": "MIT",
  "repository": { "type": "git", "url": "https://github.com/piton-lang/piton-rs.git" },
  "engines": { "vscode": "^1.85.0" },
  "categories": ["Programming Languages"],
  "main": "./src/extension.js",
  "activationEvents": ["onLanguage:piton", "workspaceContains:**/piton.config.pi"],
  "contributes": {
    "languages": [
      {
        "id": "piton",
        "aliases": ["Piton", "piton"],
        "extensions": [".pi"],
        "configuration": "./language-configuration.json"
      }
    ],
    "grammars": [
      {
        "language": "piton",
        "scopeName": "source.piton",
        "path": "./syntaxes/piton.tmLanguage.json"
      }
    ],
    "configuration": {
      "title": "Piton",
      "properties": {
        "piton.serverPath": {
          "type": "string",
          "default": "piton",
          "description": "Path to the piton executable. The extension runs `piton lsp`."
        },
        "piton.trace.server": {
          "type": "string",
          "enum": ["off", "messages", "verbose"],
          "default": "off",
          "description": "Trace the communication with the Piton language server."
        }
      }
    },
    "commands": [
      { "command": "piton.restartServer", "title": "Piton: Restart Language Server" },
      { "command": "piton.build", "title": "Piton: Build Project" }
    ]
  },
  "dependencies": { "vscode-languageclient": "^9.0.1" },
  "devDependencies": { "@types/vscode": "^1.85.0", "@vscode/vsce": "^3.0.0" },
  "scripts": { "package": "vsce package" }
}
"#
    .to_string()
}

fn vscode_client() -> String {
    r#"// Generated by `piton grammar`.
//
// A thin client: everything interesting happens in `piton lsp`.

const { workspace, window, commands } = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function serverOptions() {
  const command = workspace.getConfiguration('piton').get('serverPath', 'piton');
  const run = { command, args: ['lsp'], transport: TransportKind.stdio };
  return { run, debug: run };
}

function start() {
  client = new LanguageClient('piton', 'Piton Language Server', serverOptions(), {
    documentSelector: [{ scheme: 'file', language: 'piton' }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher('**/*.pi'),
    },
  });
  return client.start();
}

async function activate(context) {
  context.subscriptions.push(
    commands.registerCommand('piton.restartServer', async () => {
      if (client) await client.stop();
      await start();
      window.showInformationMessage('Piton language server restarted.');
    }),
    commands.registerCommand('piton.build', async () => {
      const terminal = window.createTerminal('piton build');
      terminal.sendText('piton build');
      terminal.show();
    }),
  );
  await start();
}

async function deactivate() {
  if (client) await client.stop();
}

module.exports = { activate, deactivate };
"#
    .to_string()
}

fn vscode_readme() -> String {
    r#"# Piton for VS Code

Generated by `piton grammar`. Works in VS Code, Cursor, and Windsurf.

```sh
npm install
npx vsce package
code --install-extension piton-0.1.0.vsix
```

The extension contributes the TextMate grammar and starts `piton lsp`. Point
`piton.serverPath` at the binary if it is not on `$PATH`.
"#
    .to_string()
}

/// Zed's own build output, which lands inside the extension directory.
///
/// Zed compiles the extension to `extension.wasm` and clones the Tree-sitter
/// grammar into `grammars/`. Committing the clone records it as a stray
/// gitlink, and a fresh checkout then materialises an empty `grammars/piton`
/// that Zed refuses to overwrite: "grammar directory already exists, but is
/// not a git clone of ...".
const ZED_GITIGNORE: &str = "/target\n/extension.wasm\n/grammars/\n";

// ---- Zed ---------------------------------------------------------------------

fn zed_manifest(source: &GrammarSource) -> String {
    let note = if source.is_published() {
        String::new()
    } else {
        format!(
            "# NOT PUBLISHED: the grammar has no home yet, so Zed cannot fetch it.\n\
             #   git remote add {remote} <url>\n\
             #   cargo xtask publish-grammar\n",
            remote = crate::GRAMMAR_REMOTE_NAME,
        )
    };
    format!(
        r#"# Generated by `piton grammar`.
id = "piton"
name = "Piton"
description = "Piton language support: highlighting and the piton language server"
version = "0.1.0"
schema_version = 1
authors = ["Piton"]
repository = "https://github.com/piton-lang/piton-rs"

[language_servers.piton]
name = "Piton Language Server"
languages = ["Piton"]

# Zed fetches grammars over git and wants a full commit SHA, not a branch.
{note}[grammars.piton]
repository = "{remote}"
commit = "{rev}"
"#,
        note = note,
        remote = source.repository,
        rev = source.rev
    )
}

/// The extension is a WebAssembly component; Zed builds it when installing.
fn zed_cargo() -> String {
    r#"# Generated by `piton grammar`.
#
# Its own workspace: this crate is built for wasm32-wasip1 by Zed, not by the
# Piton workspace it happens to live inside.
[workspace]

[package]
name = "zed_piton"
version = "0.1.0"
edition = "2021"
publish = false
license = "MIT"

[lib]
path = "src/lib.rs"
crate-type = ["cdylib"]

[dependencies]
zed_extension_api = "0.7"
"#
    .to_string()
}

/// Tells Zed how to start the language server.
///
/// The extension does not download anything: `piton lsp` is the same binary the
/// user already builds, so the extension only has to find it.
fn zed_extension() -> String {
    r#"// Generated by `piton grammar`. Edit crates/piton-grammar instead.

use zed_extension_api::{self as zed, settings::LspSettings, Command, LanguageServerId, Result};

/// The binary that serves Piton, and the subcommand that starts the server.
const BINARY: &str = "piton";
const SUBCOMMAND: &str = "lsp";

struct PitonExtension;

impl zed::Extension for PitonExtension {
    fn new() -> Self {
        PitonExtension
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Command> {
        // An explicit `lsp.piton.binary` in the user's settings wins.
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.binary);
        if let Some(binary) = settings {
            if let Some(path) = binary.path {
                return Ok(Command {
                    command: path,
                    args: binary.arguments.unwrap_or_else(|| vec![SUBCOMMAND.to_string()]),
                    env: worktree.shell_env(),
                });
            }
        }

        let path = worktree.which(BINARY).ok_or_else(|| {
            format!(
                "could not find `{BINARY}` on $PATH. Build it with `cargo xtask install`, or set                  lsp.piton.binary.path in your Zed settings."
            )
        })?;

        Ok(Command {
            command: path,
            args: vec![SUBCOMMAND.to_string()],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(PitonExtension);
"#
    .to_string()
}

fn zed_language_config() -> String {
    r#"# Generated by `piton grammar`.
name = "Piton"
grammar = "piton"
path_suffixes = ["pi"]
line_comments = ["// "]
autoclose_before = "}]),"
tab_size = 4
hard_tabs = false

# A line ending in `:` opens a block, so the next line starts indented. There is
# deliberately no decrease pattern: dedenting is the author's decision, and the
# editor should not undo it on the next keystroke.
increase_indent_pattern = ":\\s*(//.*)?$"

brackets = [
  { start = "{", end = "}", close = true, newline = false },
  { start = "[", end = "]", close = true, newline = false },
  { start = "(", end = ")", close = true, newline = false },
  { start = "\"", end = "\"", close = true, newline = false, not_in = ["string"] },
]
"#
    .to_string()
}

fn zed_readme(source: &GrammarSource) -> String {
    let grammar = if source.is_published() {
        format!(
            r#"3. **The grammar comes with it.** Zed fetches the Tree-sitter grammar over
   git, from the repository and commit pinned in `extension.toml`:

   ```
   {remote}
   ```

   Nothing to do here — that is only a maintainer's concern, and
   [publishing the grammar](../../docs/development.md#publishing-the-tree-sitter-grammar)
   says how a new commit gets pinned."#,
            remote = source.repository,
        )
    } else {
        r#"3. **Publish the grammar.** Zed fetches Tree-sitter grammars over git and wants
   a full commit SHA, so the grammar has to exist as its own repository:

   ```sh
   cargo xtask publish-grammar
   ```

   That pushes the `editors/tree-sitter-piton` subtree to whatever the `grammar`
   git remote points at, and writes the resulting commit into `[grammars.piton]`
   below. Tell git where that is once, per clone:

   ```sh
   git remote add grammar <url>
   ```

   The URL is not stored in this repository's source; `--remote` overrides it."#
            .to_string()
    };
    format!(
        r#"# Piton for Zed

Generated by `piton grammar`.

This directory is a Zed extension: a WebAssembly component that tells Zed how to
start `piton lsp`, plus the language configuration and Tree-sitter queries.

## Build and install

1. **Install the compiler**, so the extension has a server to start:

   ```sh
   cargo xtask install
   ```

2. **Add the WebAssembly target.** Zed compiles the extension itself, but it
   needs the target installed:

   ```sh
   rustup target add wasm32-wasip1
   ```

{grammar}

4. **Install the extension.** In Zed, open the command palette and run
   `zed: install dev extension`, then choose this directory. Zed builds the
   extension and compiles the grammar.

To check the extension compiles before handing it to Zed:

```sh
cargo xtask zed
```

Reinstalling after a change is the same command; Zed rebuilds a dev extension
when you run `zed: reload extensions`.

## Pointing at a different binary

The extension looks for `piton` on `$PATH`. If yours lives elsewhere:

```json
{{
  "lsp": {{
    "piton": {{ "binary": {{ "path": "/abs/path/to/piton", "arguments": ["lsp"] }} }}
  }}
}}
```

## Without the grammar

Highlighting comes from the Tree-sitter grammar. Everything else — diagnostics,
completion, hover, go to definition, rename, formatting — comes from `piton lsp`
and works whether or not Zed managed to fetch it.
"#
    )
}

// ---- JetBrains ------------------------------------------------------------------

fn jetbrains_plugin() -> String {
    r#"<!-- Generated by `piton grammar`. -->
<idea-plugin>
  <id>dev.piton.intellij</id>
  <name>Piton</name>
  <vendor>Piton</vendor>

  <description><![CDATA[
    Piton language support for JetBrains IDEs: TextMate highlighting plus the
    <code>piton</code> language server for diagnostics, completion, navigation,
    and formatting.
  ]]></description>

  <depends>com.intellij.modules.platform</depends>
  <depends>org.jetbrains.plugins.textmate</depends>

  <extensions defaultExtensionNs="com.intellij">
    <fileType name="Piton" language="Piton" extensions="pi"
              implementationClass="com.intellij.openapi.fileTypes.PlainTextFileType"
              fieldName="INSTANCE"/>
    <platform.lsp.serverSupportProvider
        implementation="dev.piton.PitonLspServerSupportProvider"/>
  </extensions>
</idea-plugin>
"#
    .to_string()
}

fn jetbrains_lsp() -> String {
    r#"// Generated by `piton grammar`.
package dev.piton

import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspServerSupportProvider
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor

/** Starts `piton lsp` for every `.pi` file in the project. */
class PitonLspServerSupportProvider : LspServerSupportProvider {
    override fun fileOpened(
        project: Project,
        file: VirtualFile,
        serverStarter: LspServerSupportProvider.LspServerStarter,
    ) {
        if (file.extension == "pi") {
            serverStarter.ensureServerStarted(PitonLspServerDescriptor(project))
        }
    }
}

private class PitonLspServerDescriptor(project: Project) :
    ProjectWideLspServerDescriptor(project, "Piton") {

    override fun isSupportedFile(file: VirtualFile) = file.extension == "pi"

    override fun createCommandLine() =
        com.intellij.execution.configurations.GeneralCommandLine("piton", "lsp")
}
"#
    .to_string()
}

/// Gradle needs a settings file to know what it is building.
fn jetbrains_settings() -> String {
    r#"// Generated by `piton grammar`.
rootProject.name = "piton"
"#
    .to_string()
}

/// Pins the Gradle version, so `gradle wrapper` produces a known one.
///
/// The wrapper's own `gradlew` script and jar are binaries and are not
/// generated; run `gradle wrapper` once, or let a JetBrains IDE do it when it
/// imports the project.
fn jetbrains_wrapper() -> String {
    r#"# Generated by `piton grammar`.
distributionBase=GRADLE_USER_HOME
distributionPath=wrapper/dists
distributionUrl=https\://services.gradle.org/distributions/gradle-8.10.2-bin.zip
networkTimeout=10000
validateDistributionUrl=true
zipStoreBase=GRADLE_USER_HOME
zipStorePath=wrapper/dists
"#
    .to_string()
}

fn jetbrains_properties() -> String {
    r#"# Generated by `piton grammar`.
org.gradle.jvmargs=-Xmx2g
org.gradle.caching=true
kotlin.stdlib.default.dependency=false
"#
    .to_string()
}

fn jetbrains_gradle() -> String {
    r#"// Generated by `piton grammar`.
plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "2.0.21"
    id("org.jetbrains.intellij.platform") version "2.1.0"
}

group = "dev.piton"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform { defaultRepositories() }
}

dependencies {
    intellijPlatform {
        // The LSP API is only available in paid IDEs.
        intellijIdeaUltimate("2024.3")
        bundledPlugin("org.jetbrains.plugins.textmate")
    }
}

intellijPlatform {
    pluginConfiguration {
        ideaVersion {
            sinceBuild = "243"
        }
    }
}
"#
    .to_string()
}

fn jetbrains_readme() -> String {
    r#"# Piton for JetBrains IDEs

Generated by `piton grammar`.

There are two ways in, and which you want depends on whether your IDE has the
LSP API. That API is only in the paid IDEs — IntelliJ IDEA Ultimate, WebStorm,
PyCharm Professional, GoLand, RustRover, and so on — which is why the TextMate
bundle is shipped separately and needs no build at all.

## Highlighting, in any IDE, with no build

1. Settings → Editor → TextMate Bundles
2. `+`, and select the `bundles/piton` directory next to this file
3. Apply

`.pi` files are highlighted from then on. Nothing else is required.

## The full plugin, in a paid IDE

This adds the language server: diagnostics, completion, hover, go to
definition, find references, rename, and formatting.

You need a JDK 17 or newer and Gradle. The wrapper script is not checked in,
because it ships as a binary; generate it once:

```sh
cd editors/jetbrains
gradle wrapper          # once — or skip, and use `gradle` in place of `./gradlew`
./gradlew buildPlugin
```

That writes `build/distributions/piton-0.1.0.zip`. Install it:

1. Settings → Plugins
2. The gear icon → **Install Plugin from Disk…**
3. Choose the zip, and restart when asked

The plugin looks for `piton` on your `PATH`. Install it with
`cargo xtask install` from the Piton repository if you have not already.

Opening this directory in a JetBrains IDE also works: it will import the Gradle
project and offer to generate the wrapper for you.
"#
    .to_string()
}
