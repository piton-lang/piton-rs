//! The Zed extension for Piton.
//!
//! Zed reads highlighting, brackets, the outline and the rest from the query
//! files in `languages/piton/`, and it fetches and builds the tree-sitter
//! grammar itself from the repository named in `extension.toml`. None of that
//! needs code.
//!
//! A language server does. `extension.toml` can declare that Piton has one, but
//! it has nowhere to say which program to run, so an extension that names a
//! language server and ships no WebAssembly names a server Zed cannot start.
//! That is what this file is for: it finds `piton` and hands Zed the command,
//! and it turns the server's completions and symbols into labels Zed can
//! highlight.

use zed_extension_api::{
    self as zed,
    lsp::{Completion, CompletionKind, Symbol, SymbolKind},
    settings::LspSettings,
    CodeLabel, CodeLabelSpan, LanguageServerId, Result,
};

/// The binary `cargo xtask install` installs.
const BINARY: &str = "piton";

/// The subcommand that runs the language server. Every editor in `editors/`
/// starts it the same way, and a test in `piton-syntax` holds them to it.
const ARGUMENTS: &[&str] = &["lsp"];

struct PitonExtension {
    /// A path that has already been found to run.
    ///
    /// Zed asks for the command again every time the server restarts, and the
    /// fallback below spends a process spawn to check a candidate. Only a
    /// success is remembered, so a `cargo xtask install` part-way through a
    /// session is still picked up the next time the server starts.
    server: Option<String>,
}

impl zed::Extension for PitonExtension {
    fn new() -> Self {
        PitonExtension { server: None }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let configured = LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.binary);

        let args = configured
            .as_ref()
            .and_then(|binary| binary.arguments.clone())
            .unwrap_or_else(|| ARGUMENTS.iter().map(|argument| argument.to_string()).collect());

        // The server resolves imports and reads `piton.config.pi` from disk, so
        // it wants the environment the project is configured with rather than
        // whatever Zed itself was launched with.
        let env = match configured.as_ref().and_then(|binary| binary.env.clone()) {
            Some(env) => {
                let mut env: Vec<(String, String)> = env.into_iter().collect();
                // A map has no order of its own, and a command that differs
                // only in the order of its environment reads as a change.
                env.sort();
                env
            }
            None => worktree.shell_env(),
        };

        // A path the user wrote is used as written. Second-guessing it would
        // hide a typo behind a search of `$PATH` that quietly found something
        // else.
        if let Some(path) = configured.and_then(|binary| binary.path) {
            return Ok(zed::Command { command: path, args, env });
        }

        Ok(zed::Command {
            command: self.locate(worktree)?,
            args,
            env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.initialization_options))
    }

    fn language_server_workspace_configuration(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.settings))
    }

    fn label_for_completion(
        &self,
        _id: &LanguageServerId,
        completion: Completion,
    ) -> Option<CodeLabel> {
        label(
            &completion.label,
            highlight_for_completion(completion.kind?)?,
            completion.detail.as_deref(),
            None,
        )
    }

    fn label_for_symbol(&self, _id: &LanguageServerId, symbol: Symbol) -> Option<CodeLabel> {
        // The server reports an abstract anchor as an interface, which is the
        // nearest thing LSP has to "a declaration that cannot stand alone".
        // Saying so is more use in the symbol list than a different colour.
        let prefix = matches!(symbol.kind, SymbolKind::Interface).then_some("abstract ");
        label(
            &symbol.name,
            highlight_for_symbol(&symbol.kind)?,
            None,
            prefix,
        )
    }
}

impl PitonExtension {
    /// The `piton` binary, or an error saying how to get one.
    fn locate(&mut self, worktree: &zed::Worktree) -> Result<String> {
        if let Some(found) = &self.server {
            return Ok(found.clone());
        }

        if let Some(path) = worktree.which(BINARY) {
            self.server = Some(path.clone());
            return Ok(path);
        }

        // `cargo xtask install` puts the binary in the cargo bin directory,
        // which is on the shell's `$PATH` but not necessarily on the `$PATH` of
        // a Zed started from a desktop launcher. Looking there directly is what
        // separates "Piton is not installed" from "your editor cannot see it",
        // and the two have very different fixes.
        for candidate in cargo_bin_candidates(worktree) {
            if runs(&candidate) {
                self.server = Some(candidate.clone());
                return Ok(candidate);
            }
        }

        Err(format!(
            "`{BINARY}` is not on $PATH. Install it from a checkout of the Piton \
             repository with `cargo xtask install`, or point Zed at a copy by \
             adding this to your settings:\n\n\
             \"lsp\": {{\n  \"piton\": {{\n    \"binary\": {{ \"path\": \"/path/to/{BINARY}\" }}\n  }}\n}}"
        ))
    }
}

/// Where `cargo xtask install` would have put the binary.
///
/// `CARGO_HOME` first, then the default it falls back to, which is the layout
/// `cargo install` uses as well.
fn cargo_bin_candidates(worktree: &zed::Worktree) -> Vec<String> {
    let (os, _) = zed::current_platform();
    let separator = if os == zed::Os::Windows { '\\' } else { '/' };
    let suffix = if os == zed::Os::Windows { ".exe" } else { "" };

    let environment = worktree.shell_env();
    let variable = |name: &str| {
        environment
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };

    let mut candidates = Vec::new();
    if let Some(cargo_home) = variable("CARGO_HOME") {
        candidates.push(format!("{cargo_home}{separator}bin{separator}{BINARY}{suffix}"));
    }
    let home = variable("HOME").or_else(|| variable("USERPROFILE"));
    if let Some(home) = home {
        candidates.push(format!(
            "{home}{separator}.cargo{separator}bin{separator}{BINARY}{suffix}"
        ));
    }
    candidates.dedup();
    candidates
}

/// Whether a candidate path is a `piton` that can be run.
///
/// An extension cannot look at the filesystem outside its own directory, so
/// the only way to tell a real path from a guess is to run it. `--version`
/// costs one short-lived process and reads nothing.
fn runs(path: &str) -> bool {
    zed::Command::new(path)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status == Some(0))
}

/// A label reading `name  detail`, with the name the part Zed filters on.
fn label(
    name: &str,
    highlight: &str,
    detail: Option<&str>,
    prefix: Option<&str>,
) -> Option<CodeLabel> {
    let mut spans = Vec::new();
    let start = match prefix {
        Some(prefix) => {
            spans.push(CodeLabelSpan::literal(prefix, Some("keyword".into())));
            prefix.len()
        }
        None => 0,
    };
    spans.push(CodeLabelSpan::literal(name, Some(highlight.into())));

    if let Some(detail) = detail.map(str::trim).filter(|detail| !detail.is_empty()) {
        spans.push(CodeLabelSpan::literal("  ", None));
        spans.push(CodeLabelSpan::literal(detail, Some("comment".into())));
    }

    Some(CodeLabel {
        // The spans are literals rather than ranges into parsed code, so there
        // is nothing for Zed to parse.
        code: String::new(),
        spans,
        // Typing filters against the name. The detail is there to be read, not
        // to be matched: `inherited from Agent` should not make a property
        // match a search for `agent`.
        filter_range: (start..start + name.len()).into(),
    })
}

/// The theme highlight for a completion, by what the server said it is.
///
/// The server's kinds come from `piton-lsp`: an anchor is a class, a property
/// is a property, a value in an expression is a variable, and everything the
/// language itself spells is a keyword.
fn highlight_for_completion(kind: CompletionKind) -> Option<&'static str> {
    Some(match kind {
        CompletionKind::Class => "type",
        CompletionKind::Interface => "type",
        CompletionKind::Property => "property",
        CompletionKind::Keyword => "keyword",
        CompletionKind::Variable => "variable",
        CompletionKind::Module => "string.special.path",
        CompletionKind::Constant => "constant",
        _ => return None,
    })
}

/// The theme highlight for a document or workspace symbol.
fn highlight_for_symbol(kind: &SymbolKind) -> Option<&'static str> {
    Some(match kind {
        SymbolKind::Class | SymbolKind::Interface => "type",
        SymbolKind::Property => "property",
        SymbolKind::Variable => "variable",
        SymbolKind::Module | SymbolKind::Namespace => "string.special.path",
        SymbolKind::Constant => "constant",
        _ => return None,
    })
}

zed::register_extension!(PitonExtension);
