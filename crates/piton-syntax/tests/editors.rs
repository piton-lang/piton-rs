//! Checks the shipped editor definitions against the language.
//!
//! There are nine syntax definitions in `editors/`, each in a different format,
//! and every one of them repeats the language's keyword list. That is nine
//! chances to add a keyword to the compiler and forget it everywhere else, so
//! each definition is read here and checked against
//! `piton_syntax::language`.

use std::path::{Path, PathBuf};

use piton_syntax::language;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn editors() -> PathBuf {
    repo_root().join("editors")
}

fn read(relative: &str) -> String {
    let path = editors().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// Reports which of `words` never appear in `text`.
fn missing<'a>(text: &str, words: impl IntoIterator<Item = &'a &'a str>) -> Vec<String> {
    words
        .into_iter()
        .filter(|word| !text.contains(**word))
        .map(|word| word.to_string())
        .collect()
}

/// Every definition has to know every word that carries meaning.
#[test]
fn every_definition_covers_the_keywords() {
    let definitions = [
        "vscode/syntaxes/piton.tmLanguage.json",
        "tree-sitter-piton/grammar.js",
        "vim/syntax/piton.vim",
        "emacs/piton-mode.el",
        "sublime/Piton.sublime-syntax",
        "kate/piton.xml",
    ];

    let mut problems = Vec::new();
    for definition in definitions {
        let text = read(definition);
        let gaps = missing(&text, language::all_keywords().iter());
        if !gaps.is_empty() {
            problems.push(format!("{definition}: missing {}", gaps.join(", ")));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The four interpolation sigils have to be recognized, or `@{...}` renders as
/// prose in the editor while the compiler treats it as a reference.
#[test]
fn every_definition_covers_the_sigils() {
    for definition in [
        "vscode/syntaxes/piton.tmLanguage.json",
        "tree-sitter-piton/grammar.js",
        "vim/syntax/piton.vim",
        "emacs/piton-mode.el",
        "sublime/Piton.sublime-syntax",
        "kate/piton.xml",
    ] {
        let text = read(definition);
        for sigil in ["${", "#{", "@{"] {
            // Some formats escape the `$`, `#` or `@`, so the brace and the
            // introducer are checked rather than the literal pair.
            let introducer = &sigil[..sigil.len() - 1];
            assert!(
                text.contains(introducer),
                "{definition} does not mention the `{sigil}` sigil"
            );
        }
    }
}

/// `.pi` is the extension, and every definition has to claim it.
#[test]
fn every_definition_claims_the_file_extension() {
    let extension = language::EXTENSION;
    for definition in [
        "vscode/package.json",
        "vscode/syntaxes/piton.tmLanguage.json",
        "tree-sitter-piton/package.json",
        "vim/ftdetect/piton.vim",
        "emacs/piton-mode.el",
        "sublime/Piton.sublime-syntax",
        "kate/piton.xml",
        "zed/languages/piton/config.toml",
        "helix/languages.toml",
        "neovim/piton.lua",
    ] {
        let text = read(definition);
        assert!(
            text.contains(extension),
            "{definition} does not claim the `.{extension}` extension"
        );
    }
}

/// Comments are line-only, so nothing may configure a block comment.
#[test]
fn no_definition_invents_a_block_comment() {
    for definition in [
        "vscode/language-configuration.json",
        "zed/languages/piton/config.toml",
        "helix/languages.toml",
        "kate/piton.xml",
    ] {
        let text = read(definition);
        assert!(
            !text.contains("blockComment") && !text.contains("block_comment"),
            "{definition} configures a block comment, which the language does not have"
        );
        assert!(
            text.contains(language::COMMENT_PREFIX),
            "{definition} does not configure the line comment"
        );
    }
}

/// The language prefers four spaces and does not make it configurable, so no
/// definition may set anything else.
#[test]
fn every_definition_uses_four_spaces() {
    for (definition, needle) in [
        ("zed/languages/piton/config.toml", "tab_size = 4"),
        ("helix/languages.toml", "tab-width = 4"),
        ("vim/ftplugin/piton.vim", "shiftwidth=4"),
        ("neovim/piton.lua", "shiftwidth = 4"),
        ("emacs/piton-mode.el", "tab-width 4"),
    ] {
        let text = read(definition);
        assert!(text.contains(needle), "{definition} should set `{needle}`");
    }
    let zed = read("zed/languages/piton/config.toml");
    assert!(
        zed.contains("hard_tabs = false"),
        "tabs are not the indent character"
    );
}

/// Each editor named by the specification has a directory.
#[test]
fn every_specified_editor_is_covered() {
    let spec = repo_root().join("spec/scope/tooling/editors");
    let Ok(entries) = std::fs::read_dir(&spec) else {
        return; // The specification does not list editors in this checkout.
    };

    let mut uncovered = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "pi") {
            continue;
        }
        let name = path.file_stem().expect("stem").to_string_lossy().to_string();
        if name == "index" {
            continue;
        }
        let directory = editors().join(name.to_lowercase());
        if !directory.is_dir() {
            uncovered.push(name);
        }
    }
    assert!(
        uncovered.is_empty(),
        "the specification names editors with no support directory: {}",
        uncovered.join(", ")
    );
}

/// Every editor that runs the server has to invoke it the same way.
#[test]
fn the_server_is_always_invoked_the_same_way() {
    for (definition, needle) in [
        ("vscode/client.js", r#"args: ["lsp"]"#),
        ("helix/languages.toml", r#"args = ["lsp"]"#),
        ("neovim/piton.lua", r#""lsp""#),
        ("emacs/piton-mode.el", r#""lsp""#),
        ("vim/README.md", "'piton', 'lsp'"),
        ("sublime/README.md", r#"["piton", "lsp"]"#),
        ("kate/README.md", r#"["piton", "lsp"]"#),
        ("jetbrains/README.md", "piton lsp"),
    ] {
        let text = read(definition);
        assert!(
            text.contains(needle),
            "{definition} should start the server with `{needle}`"
        );
    }
}
