//! Checks the shipped editor definitions against the language.
//!
//! There are nine syntax definitions in `editors/`, each in a different format,
//! and every one of them repeats the language's keyword list. That is nine
//! chances to add a keyword to the compiler and forget it everywhere else, so
//! each definition is read here and checked against
//! `piton_syntax::language`.

use std::path::{Path, PathBuf};

use piton_syntax::language;

/// Every query shipped with the language.
///
/// The grammar's own queries are written in the vocabulary Helix and Neovim
/// read; Zed's are written in Zed's, which is why the two sets are not the same
/// files. Both are checked against the grammar here, because a query naming a
/// node the grammar does not define fails to compile whoever reads it.
const QUERY_FILES: &[&str] = &[
    "tree-sitter-piton/queries/highlights.scm",
    "tree-sitter-piton/queries/injections.scm",
    "tree-sitter-piton/queries/locals.scm",
    "tree-sitter-piton/queries/folds.scm",
    "tree-sitter-piton/queries/indents.scm",
    "zed/languages/piton/highlights.scm",
    "zed/languages/piton/injections.scm",
    "zed/languages/piton/brackets.scm",
    "zed/languages/piton/outline.scm",
    "zed/languages/piton/overrides.scm",
    "zed/languages/piton/textobjects.scm",
];

/// The queries Zed reads, and the capture names each one is allowed to use.
///
/// Zed matches these by name and silently ignores the rest, which is how a
/// `zed/languages/piton/indents.scm` written in Neovim's vocabulary -- with
/// `@indent.begin` where Zed wants `@indent` -- sat in this directory doing
/// nothing at all. A wrong capture name is not an error anywhere; it is just an
/// editor that does not do the thing.
const ZED_CAPTURES: &[(&str, &[&str])] = &[
    ("brackets", &["open", "close"]),
    ("outline", &["name", "item", "context", "context.extra", "annotation"]),
    (
        "textobjects",
        &[
            "function.around",
            "function.inside",
            "class.around",
            "class.inside",
            "comment.around",
            "comment.inside",
        ],
    ),
    ("injections", &["injection.language", "injection.content"]),
    ("indents", &["indent", "start", "end", "outdent"]),
];

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
        ("vim/indent/piton.vim", "shiftwidth()"),
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

/// Pressing enter after a line that opens a block lands inside it.
///
/// The specification asks for it in `Lsp.pi`, and the server answers
/// `textDocument/onTypeFormatting` for the clients that ask. Not every client
/// does, so the editors that can express the rule themselves have to -- and
/// they have to express it the same way, so the pattern lives in one place.
const BLOCK_OPENS: &str = r"^[^/\s][^:]*:\s*$|^\s*[^:/\s]+(::\s*[^:/\s]+)*:\s*$|^\s*[-+]{1,2}\s+[^:/\s]+(::\s*[^:/\s]+)*:\s*$";

#[test]
fn every_definition_indents_after_a_colon() {
    // A line opens a block when it ends where a value would begin: a
    // declaration at the margin, `key:`, `key:: type:`, `- key:`. A colon
    // inside a value opens nothing -- `prompt: Careful: ` ends a sentence
    // rather than a property. JSON and TOML double every backslash.
    let config_form = BLOCK_OPENS.replace('\\', "\\\\");

    for (definition, needle) in [
        (
            "zed/languages/piton/config.toml",
            format!("increase_indent_pattern = \"{config_form}\""),
        ),
        (
            "vscode/language-configuration.json",
            format!("\"increaseIndentPattern\": \"{config_form}\""),
        ),
        (
            "vscode/language-configuration.json",
            format!("\"beforeText\": \"{config_form}\""),
        ),
        ("vim/indent/piton.vim", r"':\s*$'".to_string()),
        ("emacs/piton-mode.el", r#".*:[ \t]*$"#.to_string()),
    ] {
        let text = read(definition);
        assert!(
            text.contains(&needle),
            "{definition} should indent after a line that opens a block (`{needle}`)"
        );
    }
}

/// The pattern the editors run has to agree with the language server about
/// which lines open a block.
///
/// Zed compiles it with Rust's regex crate, so it has to compile there -- a
/// pattern VS Code accepts but Rust rejects would leave Zed indenting on its
/// own stale rule. And it has to draw the same line the server draws between
/// a key's colon and a value's, or the two would fight over every newline.
#[test]
fn the_block_opens_pattern_agrees_with_the_server() {
    let pattern = regex::Regex::new(BLOCK_OPENS).expect(
        "the pattern has to compile under Rust's regex crate, which is what Zed runs it through",
    );

    for line in [
        "export type N:",
        "anchor A extends B as command:",
        "    frameworks:",
        "    config:: dictionary:",
        "    - frameworks:",
        "    ++ key:",
    ] {
        assert!(pattern.is_match(line), "`{line}` opens a block");
    }

    for line in [
        "    prompt: Careful:",
        "    a note: like this:",
        "    // note:",
        "    //note:",
        "    title: a book",
        "greeting: Well:",
    ] {
        assert!(!pattern.is_match(line), "`{line}` opens nothing");
    }
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
        ("zed/src/piton.rs", r#"&["lsp"]"#),
    ] {
        let text = read(definition);
        assert!(
            text.contains(needle),
            "{definition} should start the server with `{needle}`"
        );
    }
}

#[test]
fn the_grammar_uses_only_regex_features_tree_sitter_has() {
    // Tree-sitter does not run the regexes in `grammar.js` through a
    // general-purpose engine. It compiles them into its own lexer, and the
    // subset it accepts has no lookaround and no backreferences. A pattern
    // using one is not a subtle bug: `tree-sitter generate` refuses the
    // grammar, and the editor reports that the grammar will not compile.
    //
    // That failure surfaces in an editor rather than here, because this
    // repository has no tree-sitter CLI to generate with, so the check is a
    // read of the source instead.
    let grammar = read("tree-sitter-piton/grammar.js");
    let unsupported = [
        ("(?=", "lookahead"),
        ("(?!", "negative lookahead"),
        ("(?<=", "lookbehind"),
        ("(?<!", "negative lookbehind"),
        (r"\b", "word boundary"),
        (r"\B", "non-word boundary"),
    ];
    for (needle, name) in unsupported {
        assert!(
            !grammar.contains(needle),
            "`grammar.js` uses {name} (`{needle}`), which tree-sitter's lexer \
             cannot compile. Express the constraint in the grammar's structure \
             instead, or fold the surrounding character into the token."
        );
    }
}

#[test]
fn every_query_matches_a_node_the_grammar_defines() {
    // A query naming a node that does not exist fails to compile against the
    // grammar, which an editor reports the same way it reports a broken
    // grammar. The queries are shipped twice -- once in the grammar directory
    // and once inside the Zed extension -- so both are checked.
    let grammar = read("tree-sitter-piton/grammar.js");
    let body = &grammar[grammar.find("rules: {").expect("rules block")..];
    let mut defined: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        // Rule names sit at one level of indentation inside `rules: {`.
        if indent != 4 {
            continue;
        }
        if let Some(name) = trimmed.split(':').next() {
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                defined.push(name.to_string());
            }
        }
    }
    assert!(defined.contains(&"source_file".to_string()), "{defined:?}");

    for relative in QUERY_FILES {
        let query = read(relative);
        for name in node_names(&query) {
            assert!(
                defined.contains(&name),
                "{relative} matches `({name})`, which `grammar.js` does not define"
            );
        }
    }
}

/// Node names a query matches on, which is every bare word after a `(`.
///
/// A quoted word is an anonymous token rather than a rule, and a `@capture` is
/// a name the query invents, so neither is a node the grammar has to define.
fn node_names(query: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes: Vec<char> = query.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != '(' {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        while cursor < bytes.len() && bytes[cursor] == ' ' {
            cursor += 1;
        }
        let start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_lowercase() || bytes[cursor] == '_' || bytes[cursor].is_ascii_digit())
        {
            cursor += 1;
        }
        if cursor > start {
            names.push(bytes[start..cursor].iter().collect());
        }
        index += 1;
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn every_query_token_is_one_the_grammar_writes() {
    // The node-name check above reads bare `(node)` patterns. A query can also
    // match an anonymous token by quoting it -- `"::"`, `"pass"` -- and those
    // break a query just as completely when the grammar stops writing them.
    // Changing `::` to `:` in the grammar left two queries matching a token
    // that no longer existed, and nothing here noticed.
    let grammar = read("tree-sitter-piton/grammar.js");
    let literals = string_literals(&grammar);

    for relative in QUERY_FILES {
        let query = read(relative);
        for token in quoted_tokens(&query) {
            assert!(
                literals.contains(&token),
                "{relative} matches the token `\"{token}\"`, which `grammar.js` \
                 does not write. A query that names a token the grammar dropped \
                 fails to compile, and the editor reports it the same way it \
                 reports a broken grammar."
            );
        }
    }
}

#[test]
fn the_editor_queries_match_the_grammars_own() {
    // Zed keeps its own copy of the queries, because a Zed extension reads
    // them from `languages/<name>/`. Two copies drift: one of them was still
    // matching `"pass"` after the other had moved to `(pass_statement)`.
    //
    // Only the queries whose capture vocabulary Zed shares are copies.
    // `indents.scm` is not one of them -- see `zed_indentation_is_a_line_rule`.
    for name in ["highlights", "injections"] {
        let canonical = read(&format!("tree-sitter-piton/queries/{name}.scm"));
        let copy = read(&format!("zed/languages/piton/{name}.scm"));
        assert_eq!(
            canonical, copy,
            "`zed/languages/piton/{name}.scm` has drifted from the grammar's \
             own `queries/{name}.scm`. They are the same query and have to stay \
             byte for byte the same."
        );
    }
}

/// Every string literal the grammar writes, which is every anonymous token.
fn string_literals(grammar: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = grammar.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '"' {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut cursor = start;
        while cursor < chars.len() && chars[cursor] != '"' {
            // A literal never spans a line; an unterminated quote is something
            // else, such as a regex.
            if chars[cursor] == '\n' {
                break;
            }
            cursor += 1;
        }
        if cursor < chars.len() && chars[cursor] == '"' {
            out.push(chars[start..cursor].iter().collect());
            index = cursor + 1;
        } else {
            index = start;
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Tokens a query matches by quoting them.
///
/// A capture name is introduced by `@` and a predicate argument is a string
/// too, so only quotes that sit where a pattern goes are collected: directly
/// after `(` or after whitespace inside one.
fn quoted_tokens(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in query.lines() {
        let line = line.trim();
        if line.starts_with(';') || line.starts_with("(#") {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut index = 0;
        while index < chars.len() {
            if chars[index] != '"' {
                index += 1;
                continue;
            }
            let start = index + 1;
            let mut cursor = start;
            while cursor < chars.len() && chars[cursor] != '"' {
                cursor += 1;
            }
            if cursor >= chars.len() {
                break;
            }
            let token: String = chars[start..cursor].iter().collect();
            // A predicate's argument is a string as well; those sit inside a
            // `(#...)` form, which is skipped above.
            if !token.is_empty() {
                out.push(token);
            }
            index = cursor + 1;
        }
    }
    out.sort();
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// Zed
// ---------------------------------------------------------------------------

/// The Zed extension has to ship the code that starts the server.
///
/// `extension.toml` can declare that Piton has a language server, but it has
/// nowhere to say which program to run: the only thing that can tell Zed that
/// is a compiled `language_server_command`. An extension that declares a
/// server and ships no WebAssembly declares one Zed cannot start, and says so
/// only once a `.pi` file is opened.
#[test]
fn the_zed_extension_ships_the_code_that_starts_the_server() {
    let manifest = read("zed/extension.toml");
    assert!(
        manifest.contains("[language_servers.piton]"),
        "`zed/extension.toml` should declare the language server"
    );

    let cargo = read("zed/Cargo.toml");
    assert!(
        cargo.contains("zed_extension_api"),
        "`zed/Cargo.toml` should depend on `zed_extension_api`"
    );
    assert!(
        cargo.contains(r#"crate-type = ["cdylib"]"#),
        "a Zed extension is a WebAssembly component, so the crate is a cdylib"
    );
    assert!(
        cargo.contains("[workspace]"),
        "`zed/Cargo.toml` needs its own `[workspace]`: the extension is built \
         for `wasm32-wasip2`, not for the host, so the repository's workspace \
         must not claim it"
    );

    let source = read("zed/src/piton.rs");
    for needle in [
        "fn language_server_command",
        "register_extension!",
        "fn language_server_initialization_options",
        "fn language_server_workspace_configuration",
    ] {
        assert!(
            source.contains(needle),
            "`zed/src/piton.rs` should implement `{needle}`"
        );
    }
}

/// The repository's workspace must leave the extension alone.
///
/// It is a `cdylib` for `wasm32-wasip2`. Built for the host it fails to link,
/// so `cargo test` at the root would stop on it.
#[test]
fn the_workspace_excludes_the_zed_extension() {
    let path = repo_root().join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path).expect("workspace manifest");
    assert!(
        manifest.contains(r#"exclude = ["editors/zed"]"#),
        "the workspace should exclude `editors/zed`"
    );
}

/// Zed reads captures by name, so a query has to use the names Zed reads.
///
/// This is the check that would have caught the `indents.scm` this directory
/// used to ship: it was a copy of the grammar's, written in Neovim's
/// vocabulary, and every capture in it was one Zed ignores.
#[test]
fn zed_queries_use_captures_zed_reads() {
    for (name, allowed) in ZED_CAPTURES {
        let relative = format!("zed/languages/piton/{name}.scm");
        let Ok(query) = std::fs::read_to_string(editors().join(&relative)) else {
            continue; // Not every query has to exist.
        };
        for capture in captures(&query) {
            assert!(
                allowed.contains(&capture.as_str()),
                "{relative} captures `@{capture}`, which Zed does not read. \
                 It reads {}.",
                allowed
                    .iter()
                    .map(|name| format!("`@{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

/// Indentation in Zed is a line pattern, and has to stay one.
///
/// Zed's `@indent` measures a node that spans the lines it indents. The grammar
/// has no such node -- indentation is deliberately not in it, so an anchor's
/// header is a line and its body is not part of it -- which leaves the rule
/// with nowhere to live but `config.toml`.
#[test]
fn zed_indentation_is_a_line_rule() {
    let config = read("zed/languages/piton/config.toml");
    // The rule itself is asserted against the shared pattern by
    // `every_definition_indents_after_a_colon`; what matters here is that it
    // is a line pattern in `config.toml` at all.
    assert!(
        config.contains(r#"increase_indent_pattern = ""#),
        "`zed/languages/piton/config.toml` should carry the block-opening \
         line pattern"
    );
    assert!(
        !editors().join("zed/languages/piton/indents.scm").exists(),
        "the grammar has no node an `@indent` could measure, so an \
         `indents.scm` here can only be a no-op"
    );
}

/// Every scope a bracket opts out of has to be a scope `overrides.scm` defines.
///
/// `not_in = ["verbatim"]` naming a scope no query captures is not an error
/// anywhere; it just never applies.
#[test]
fn zed_brackets_only_name_scopes_the_overrides_define() {
    let overrides = read("zed/languages/piton/overrides.scm");
    let defined = captures(&overrides)
        .into_iter()
        // `@comment.inclusive` defines the `comment` scope with an inclusive
        // range, which is a property of the range rather than part of the name.
        .map(|capture| capture.trim_end_matches(".inclusive").to_string())
        .collect::<Vec<_>>();

    let config = read("zed/languages/piton/config.toml");
    for line in config.lines() {
        let Some((_, rest)) = line.split_once("not_in = [") else {
            continue;
        };
        let list = rest.split(']').next().expect("a closing bracket");
        for scope in list.split(',') {
            let scope = scope.trim().trim_matches('"');
            if scope.is_empty() {
                continue;
            }
            assert!(
                defined.contains(&scope.to_string()),
                "`config.toml` keeps a bracket out of the `{scope}` scope, \
                 which `overrides.scm` does not define"
            );
        }
    }
}

/// Capture names a query introduces, which is every `@name` in it.
fn captures(query: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in query.lines() {
        let line = line.trim();
        if line.starts_with(';') {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        let mut index = 0;
        while index < chars.len() {
            if chars[index] != '@' {
                index += 1;
                continue;
            }
            let start = index + 1;
            let mut cursor = start;
            while cursor < chars.len()
                && (chars[cursor].is_ascii_alphanumeric()
                    || chars[cursor] == '_'
                    || chars[cursor] == '.')
            {
                cursor += 1;
            }
            if cursor > start {
                names.push(chars[start..cursor].iter().collect::<String>());
            }
            index = cursor.max(start);
        }
    }
    names.sort();
    names.dedup();
    names
}
