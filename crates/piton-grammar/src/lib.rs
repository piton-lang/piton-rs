//! Editor grammars, generated from the compiler.
//!
//! Every word list in every generated grammar comes from
//! [`piton_syntax::kind`], so adding a keyword to the language adds it to every
//! editor the next time `piton grammar` runs. Framework keywords are passed in
//! by the caller, which is how `agent`, `skill`, and friends get highlighted
//! without the core knowing what Belay is.

mod classic;
mod extensions;
mod textmate;
pub(crate) mod treesitter;

use std::path::PathBuf;

use piton_syntax::kind::{BUILTIN_TYPES, LITERAL_KEYWORDS, RESERVED_KEYWORDS, SELF_KEYWORDS};

/// The git remote a publish pushes the grammar to.
///
/// Only the *name* is fixed. The URL lives in `.git/config`, where remotes
/// belong, so a clone can publish somewhere else without editing any source:
///
/// ```sh
/// git remote add grammar <url>
/// ```
pub const GRAMMAR_REMOTE_NAME: &str = "grammar";

/// Written until the grammar has actually been published somewhere.
///
/// `.invalid` is reserved, so nothing can mistake this for a real home.
pub const UNPUBLISHED_REPOSITORY: &str = "https://example.invalid/tree-sitter-piton.git";

/// Written until a real commit has been published.
pub const UNPUBLISHED_REV: &str = "0000000000000000000000000000000000000000";

/// Where the published Tree-sitter grammar lives.
///
/// Zed, Helix, and nvim-treesitter all fetch a Tree-sitter grammar over git
/// rather than from a directory, so the generated files have to name a remote
/// even though this repository is the source of truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarSource {
    pub repository: String,
    /// A full commit SHA. Zed will not accept a branch name.
    pub rev: String,
}

impl Default for GrammarSource {
    fn default() -> GrammarSource {
        GrammarSource {
            repository: UNPUBLISHED_REPOSITORY.to_string(),
            rev: UNPUBLISHED_REV.to_string(),
        }
    }
}

impl GrammarSource {
    /// True once the grammar has been pushed somewhere and the commit recorded.
    pub fn is_published(&self) -> bool {
        self.repository != UNPUBLISHED_REPOSITORY && self.rev != UNPUBLISHED_REV
    }
}

/// A file the generator produced.
#[derive(Clone, Debug)]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub contents: String,
}

impl GeneratedFile {
    pub fn new(path: impl Into<PathBuf>, contents: impl Into<String>) -> GeneratedFile {
        GeneratedFile { path: path.into(), contents: contents.into() }
    }
}

/// The vocabulary every grammar is built from.
#[derive(Clone, Debug)]
pub struct Vocabulary {
    /// Words that introduce or modify a declaration.
    pub declaration: Vec<String>,
    /// `this`, `self`, `super`.
    pub self_words: Vec<String>,
    /// `true`, `false`, `null`.
    pub literals: Vec<String>,
    /// Built-in constraint names.
    pub types: Vec<String>,
    /// Keywords contributed by registered frameworks.
    pub framework: Vec<String>,
    /// Interpolation sigils registered by frameworks, without `$`.
    pub sigils: Vec<String>,
}

impl Default for Vocabulary {
    fn default() -> Vocabulary {
        Vocabulary::from_compiler()
    }
}

impl Vocabulary {
    /// Read every word list out of the compiler's kind tables.
    pub fn from_compiler() -> Vocabulary {
        let reserved: Vec<String> = RESERVED_KEYWORDS.iter().map(|it| it.to_string()).collect();
        let self_words: Vec<String> = SELF_KEYWORDS.iter().map(|it| it.to_string()).collect();
        let literals: Vec<String> = LITERAL_KEYWORDS.iter().map(|it| it.to_string()).collect();
        Vocabulary {
            declaration: reserved
                .iter()
                .filter(|word| !self_words.contains(word) && !literals.contains(word))
                .cloned()
                .collect(),
            self_words,
            literals,
            types: BUILTIN_TYPES.iter().map(|it| it.to_string()).collect(),
            framework: Vec::new(),
            sigils: Vec::new(),
        }
    }

    /// Add the keywords and sigils a set of frameworks contributes.
    pub fn with_framework(mut self, keywords: Vec<String>, sigils: Vec<String>) -> Vocabulary {
        self.framework = keywords;
        self.framework.sort();
        self.framework.dedup();
        self.sigils = sigils;
        self.sigils.sort();
        self.sigils.dedup();
        self
    }

    /// The characters a sigil may contain: anything a run of prose may.
    ///
    /// A sigil is whatever is written immediately in front of a `{`, so the
    /// class is defined by what ends a run of text rather than by a list of
    /// approved spellings.
    pub(crate) const SIGIL_CHARS: &str = r#"[^\s{}\[\](),"\\]"#;

    /// A regex alternation, longest first so greedy matching works.
    pub(crate) fn alternation(words: &[String]) -> String {
        let mut sorted = words.to_vec();
        sorted.sort_by_key(|word| std::cmp::Reverse(word.len()));
        sorted.join("|")
    }

    /// The regex for a sigil: any run of prose pressed against a `{`.
    pub(crate) fn sigil_pattern() -> String {
        format!("{}+", Vocabulary::SIGIL_CHARS)
    }
}

/// Generate every grammar and editor integration.
pub fn generate(vocabulary: &Vocabulary, source: &GrammarSource) -> Vec<GeneratedFile> {
    let mut files = Vec::new();
    files.push(GeneratedFile::new(
        "shared/piton.tmLanguage.json",
        textmate::grammar(vocabulary),
    ));
    files.push(GeneratedFile::new(
        "shared/language-configuration.json",
        textmate::language_configuration(),
    ));
    files.extend(treesitter::files(vocabulary, source));
    files.extend(classic::files(vocabulary, source));
    files.extend(extensions::files(vocabulary, source));
    files.push(GeneratedFile::new("README.md", readme()));
    files
}

fn readme() -> String {
    r#"# Piton editor support

Everything in this directory is generated by `piton grammar`. The word lists
come from the compiler's own token tables and from the frameworks registered in
the `piton` binary, so regenerating after a language change updates every
editor at once. Edit `crates/piton-grammar`, not these files.

| Directory | Editor | Install |
| --- | --- | --- |
| `vscode/` | VS Code, Cursor, Windsurf | `cd vscode && npm install && npx vsce package && code --install-extension piton-*.vsix` |
| `zed/` | Zed | `zed: install dev extension` and pick this directory |
| `jetbrains/` | IntelliJ, WebStorm, PyCharm, ... | Build with Gradle, or import `bundles/piton` as a TextMate bundle |
| `vim/` | Vim, Neovim | Copy into `~/.vim` / `~/.config/nvim`, or point a plugin manager at it |
| `emacs/` | Emacs | `(add-to-list 'load-path "…/emacs") (require 'piton-mode)` |
| `sublime/` | Sublime Text | Copy into `Packages/Piton` |
| `helix/` | Helix | Merge `languages.toml` into `~/.config/helix/languages.toml` |
| `kate/` | Kate, KWrite | Copy `piton.xml` into `~/.local/share/org.kde.syntax-highlighting/syntax` |
| `tree-sitter-piton/` | Tree-sitter grammar | `npm install && npx tree-sitter generate` |
| `shared/` | The TextMate grammar and language configuration everything else reuses | |

Every integration launches the same language server: `piton lsp`.

Installation instructions for each editor are in
[`docs/editors.md`](../docs/editors.md).
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated() -> Vec<GeneratedFile> {
        let vocabulary = Vocabulary::from_compiler()
            .with_framework(vec!["agent".to_string(), "skill".to_string()], vec!["@".to_string()]);
        generate(&vocabulary, &GrammarSource::default())
    }

    #[test]
    fn every_json_file_parses() {
        for file in generated() {
            if file.path.extension().is_some_and(|ext| ext == "json") {
                serde_json::from_str::<serde_json::Value>(&file.contents)
                    .unwrap_or_else(|error| panic!("{}: {error}", file.path.display()));
            }
        }
    }

    #[test]
    fn every_editor_gets_something() {
        let files = generated();
        for editor in ["vscode", "zed", "jetbrains", "vim", "emacs", "sublime", "helix", "kate"] {
            assert!(
                files.iter().any(|file| file.path.starts_with(editor)),
                "no files generated for {editor}"
            );
        }
    }

    #[test]
    fn framework_keywords_reach_the_grammars() {
        let files = generated();
        for name in ["shared/piton.tmLanguage.json", "vim/syntax/piton.vim", "emacs/piton-mode.el"] {
            let file = files.iter().find(|file| file.path.ends_with(name)).expect(name);
            assert!(file.contents.contains("agent"), "{name} is missing framework keywords");
        }
    }

    #[test]
    fn an_unpublished_grammar_names_no_real_host() {
        let files = generate(&Vocabulary::from_compiler(), &GrammarSource::default());
        assert!(!GrammarSource::default().is_published());
        for file in &files {
            for line in file.contents.lines().filter(|line| line.contains("tree-sitter-piton.git"))
            {
                assert!(
                    line.contains(UNPUBLISHED_REPOSITORY),
                    "{} hard-codes a publishing host: {line}",
                    file.path.display()
                );
            }
        }
        let zed = files.iter().find(|file| file.path.ends_with("zed/extension.toml")).unwrap();
        assert!(zed.contents.contains("NOT PUBLISHED"), "{}", zed.contents);
        assert!(zed.contents.contains("git remote add grammar"), "{}", zed.contents);
    }

    #[test]
    fn the_grammar_source_reaches_every_file_that_fetches_it() {
        let vocabulary = Vocabulary::from_compiler();
        let source = GrammarSource {
            repository: "ssh://example.test/piton/tree-sitter-piton.git".to_string(),
            rev: "1234567890abcdef1234567890abcdef12345678".to_string(),
        };
        assert!(source.is_published());
        let files = generate(&vocabulary, &source);
        for name in ["zed/extension.toml", "helix/languages.toml", "vim/lua/piton/init.lua"] {
            let file = files.iter().find(|file| file.path.ends_with(name)).expect(name);
            assert!(file.contents.contains(&source.repository), "{name} is missing the remote");
        }
        let zed = files.iter().find(|file| file.path.ends_with("zed/extension.toml")).unwrap();
        assert!(zed.contents.contains(&source.rev), "the Zed extension must pin a commit");
        let helix = files.iter().find(|file| file.path.ends_with("helix/languages.toml")).unwrap();
        assert!(helix.contents.contains(&source.rev), "Helix must pin a commit");
    }

    #[test]
    fn language_words_come_from_the_compiler() {
        let vocabulary = Vocabulary::from_compiler();
        assert!(vocabulary.declaration.contains(&"anchor".to_string()));
        assert!(!vocabulary.declaration.contains(&"true".to_string()));
        assert!(vocabulary.self_words.contains(&"super".to_string()));
        assert!(vocabulary.types.contains(&"complex".to_string()));
    }
}
