//! Checks the editor definitions that this repository ships against the language.
//!
//! The `editors/` directory has nine syntax definitions. Each one has a
//! different format. Each one repeats the keyword list of the language. Thus
//! there are nine chances to add a keyword to the compiler and forget it in the
//! other places. This file reads each definition and checks it against
//! `piton_syntax::language`.

use std::path::{Path, PathBuf};

use piton_syntax::language;

/// Every query that the language ships.
///
/// The queries of the grammar use the vocabulary that Helix and Neovim read.
/// The queries of Zed use the vocabulary of Zed. Thus the two sets are not the
/// same files. This file checks both sets against the grammar. Sometimes a query
/// names a node that the grammar does not define. Then the query does not
/// compile in any editor that reads it.
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

/// The queries that Zed reads, and the capture names that each query can use.
///
/// Zed matches these names and ignores all other names without a message.
/// Thus a `zed/languages/piton/indents.scm` in the vocabulary of Neovim stayed
/// in this directory and did nothing. It used `@indent.begin`, but Zed needs
/// `@indent`. A wrong capture name is not an error in any place. The result is
/// only an editor that does not do the task.
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

/// The syntax definitions that have their own rules. Emacs is not one of
/// them. It reads `highlights.scm` from the tree-sitter grammar.
/// `emacs_highlights_match_the_grammars_own` holds Emacs to that file.
const DEFINITIONS: &[&str] = &[
    "vscode/syntaxes/piton.tmLanguage.json",
    "tree-sitter-piton/grammar.js",
    "vim/syntax/piton.vim",
    "sublime/Piton.sublime-syntax",
    "kate/piton.xml",
];

/// Every definition must know every word that has a sense.
///
/// This is a fast, early signal. The highlighter corpus
/// (`tests/highlight_corpus.rs`) has a different question. It asks if each word
/// has colour where it has a sense, and only there.
#[test]
fn every_definition_covers_the_keywords() {
    let definitions = DEFINITIONS;

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

/// The four interpolation sigils must have a match. If not, the editor shows
/// `@{...}` as prose, but the compiler uses it as a reference.
#[test]
fn every_definition_covers_the_sigils() {
    for definition in DEFINITIONS {
        let text = read(definition);
        for sigil in ["${", "#{", "@{"] {
            // Some formats escape the `$`, `#` or `@`. Thus the test checks the brace
            // and the introducer, not the literal pair.
            let introducer = &sigil[..sigil.len() - 1];
            assert!(
                text.contains(introducer),
                "{definition} does not mention the `{sigil}` sigil"
            );
        }
    }
}

/// `.pi` is the extension. Every definition must claim it.
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

/// Comments are line comments only. Thus no definition can configure a block comment.
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

/// The language prefers four spaces, and the user cannot change this value.
/// Thus no definition can set a different value.
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

/// When the user presses enter after a line that opens a block, the cursor
/// goes into the block.
///
/// `Lsp.pi` in the specification asks for this behaviour. The server answers
/// `textDocument/onTypeFormatting` for the clients that ask. Not all clients
/// ask. Thus the editors that can express the rule must express it. They must
/// express it in the same way, so the pattern is in one place.
const BLOCK_OPENS: &str = r"^[^-+/\s][^:]*:\s*$|^\s*[^-+:/\s][^:/\s]*(::\s*[^:/\s]+)*:\s*$";

#[test]
fn every_definition_indents_after_a_colon() {
    // A line opens a block when it ends where a value can start. Examples are
    // a declaration at the margin, `key:`, `key:: type:` and `- key:`. A colon
    // in a value opens no block. `prompt: Careful: ` ends a sentence, not a
    // property. JSON and TOML double every backslash.
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

/// The pattern that the editors use must agree with the language server about
/// which lines open a block.
///
/// Zed compiles the pattern with the regex crate of Rust. Thus the pattern must
/// compile there. VS Code can accept a pattern that Rust rejects. If that occurs,
/// Zed uses its own old rule for indentation. Also, the pattern must find the
/// same boundary as the server. This is the boundary between the colon of a
/// key and the colon of a value. If not, the two will disagree on each newline.
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
        // A list item is never a key, also when it has a colon at the end. A
        // merge line takes a value. It does not open a value.
        "    - frameworks:",
        "    ++ key:",
    ] {
        assert!(!pattern.is_match(line), "`{line}` opens nothing");
    }
}

/// Each editor that the specification names has a directory.
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

/// Every editor that runs the server must start it in the same way.
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
    // Tree-sitter does not use a general-purpose engine for the regexes in
    // `grammar.js`. It compiles them into its own lexer. The lexer accepts a
    // subset that has no lookaround and no backreferences. A pattern that uses
    // one of these is not a small bug. `tree-sitter generate` refuses the
    // grammar, and the editor reports that the grammar does not compile.
    //
    // That failure shows in an editor, not here. This repository has no
    // tree-sitter CLI to generate the grammar. Thus the check reads the source.
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
    // If a query names a node that does not exist, the query does not compile
    // against the grammar. An editor reports this failure in the same way as a
    // broken grammar. The repository ships the queries two times. One copy is
    // in the grammar directory and one copy is in the Zed extension. Thus the
    // test checks both copies.
    let grammar = read("tree-sitter-piton/grammar.js");
    let body = &grammar[grammar.find("rules: {").expect("rules block")..];
    let mut defined: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        // Rule names are at one level of indentation in `rules: {`.
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
    // `alias(rule, $.name)` gives a node a name that no rule has. This is how a
    // name token becomes a `declaration_keyword` where it declares one.
    for piece in body.split("alias(").skip(1) {
        let Some(at) = piece.find(", $.") else {
            continue;
        };
        let name: String = piece[at + 4..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        if !name.is_empty() {
            defined.push(name);
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

/// The node names that a query matches. These are all bare words after a `(`.
///
/// A quoted word is an anonymous token, not a rule. A `@capture` is a name that
/// the query makes. Thus the grammar does not have to define these as nodes.
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
        // `(_)` is the wildcard. It matches all named nodes.
        if cursor > start && !(cursor == start + 1 && bytes[start] == '_') {
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
    // quote an anonymous token to match it, for example `"::"` or `"pass"`. When
    // the grammar no longer writes such a token, the query breaks in the same way.
    // A change from `::` to `:` in the grammar made two queries match a token that
    // did not exist. No check in this file found the problem.
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
    // them from `languages/<name>/`. Two copies become different. One copy still
    // matched `"pass"` after the other copy changed to `(pass_statement)`.
    //
    // Only the queries that share the capture vocabulary of Zed are copies.
    // `indents.scm` is not a copy. Refer to `zed_indentation_is_a_line_rule`.
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

/// Every string literal that the grammar writes. These are all the anonymous tokens.
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
            // A literal never goes across a line. An unterminated quote is a
            // different item, for example a regex.
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

/// The tokens that a query quotes to match them.
///
/// `@` starts a capture name, and a predicate argument is also a string. Thus
/// the function collects only the quotes where a pattern can be. These quotes
/// are directly after `(`, or after whitespace in a `(` form.
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
            // The argument of a predicate is also a string. Such arguments are in a
            // `(#...)` form, and the code above skips that form.
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
// Highlight names
// ---------------------------------------------------------------------------

/// The contents of `editors/fixtures/scopes.toml`. For each highlight class,
/// it gives the name in each format.
fn scopes() -> toml::Table {
    read("fixtures/scopes.toml")
        .parse()
        .expect("editors/fixtures/scopes.toml is TOML")
}

/// The names in one column of `scopes.toml`, in row order.
fn scope_column(column: &str) -> Vec<(String, String)> {
    scopes()
        .iter()
        .map(|(class, row)| {
            let name = row
                .get(column)
                .and_then(|value| value.as_str())
                .unwrap_or_else(|| panic!("scopes.toml: `{class}` has no `{column}`"));
            (class.clone(), name.to_string())
        })
        .collect()
}

/// The test converts the output of a tool to a class through these names.
/// Thus in one format, two classes cannot share one name. The default styles
/// and faces of the names can repeat, because a style is a colour, not an
/// identity.
#[test]
fn scopes_toml_names_are_unique_per_format() {
    for column in ["tm", "ts", "kate", "vim"] {
        let mut seen: Vec<(String, String)> = Vec::new();
        for (class, name) in scope_column(column) {
            if let Some((other, _)) = seen.iter().find(|(_, seen_name)| *seen_name == name) {
                panic!("scopes.toml gives `{name}` to both {other} and {class} in `{column}`");
            }
            seen.push((class, name));
        }
    }
}

/// If a definition never uses a canonical name, that format cannot show the
/// class. Thus a rule is not there, or the rule has a different name.
#[test]
fn every_canonical_name_appears_in_its_definition() {
    for (column, definitions) in [
        ("tm", &["vscode/syntaxes/piton.tmLanguage.json", "sublime/Piton.sublime-syntax"][..]),
        ("kate", &["kate/piton.xml"][..]),
        ("vim", &["vim/syntax/piton.vim"][..]),
    ] {
        for definition in definitions {
            let text = read(definition);
            for (class, name) in scope_column(column) {
                let quoted = [format!("\"{name}\""), format!(" {name}\n"), format!(" {name} ")];
                assert!(
                    text.contains(&name) && (column != "tm" || quoted.iter().any(|q| text.contains(q.as_str()))),
                    "{definition} never uses `{name}`, the {column} name for {class}"
                );
            }
        }
    }
    // The tree-sitter query is the only place that contains every capture.
    let query = read("tree-sitter-piton/queries/highlights.scm");
    let used = captures(&query);
    for (class, name) in scope_column("ts") {
        assert!(
            used.contains(&name),
            "highlights.scm never captures `@{name}`, the tree-sitter name for {class}"
        );
    }
    for capture in &used {
        assert!(
            scope_column("ts").iter().any(|(_, name)| name == capture),
            "highlights.scm captures `@{capture}`, which is not a canonical name in scopes.toml"
        );
    }
}

/// No pattern in the highlights query has a predicate.
///
/// Capitalisation is a separate token in the grammar. Thus no pattern needs
/// `#match?`. Also, Emacs, Neovim, Helix and Zed all read a query without
/// predicates in the same way.
#[test]
fn highlights_have_no_predicates() {
    let query = read("tree-sitter-piton/queries/highlights.scm");
    for (number, line) in query.lines().enumerate() {
        let code = line.split(';').next().unwrap_or("");
        assert!(
            !code.contains("(#"),
            "highlights.scm:{}: `{}` uses a predicate",
            number + 1,
            line.trim()
        );
    }
}

// ---------------------------------------------------------------------------
// Emacs
// ---------------------------------------------------------------------------

/// `piton-ts-mode` reads the highlights query of the grammar from a copy next
/// to `piton-mode.el`. Thus the copy must be the same as the query.
#[test]
fn emacs_highlights_match_the_grammars_own() {
    assert_eq!(
        read("tree-sitter-piton/queries/highlights.scm"),
        read("emacs/queries/highlights.scm"),
        "`emacs/queries/highlights.scm` has drifted from the grammar's own \
         `queries/highlights.scm`. It is a copy and has to stay byte for byte \
         the same."
    );
}

/// Every capture that the query uses has a face in `piton-ts-capture-faces`.
/// It is the face that `scopes.toml` names. The mode drops a capture that has
/// no entry. Then the text has colour in all other editors, but no colour in
/// Emacs.
#[test]
fn emacs_maps_every_capture() {
    let mode = read("emacs/piton-mode.el");
    let start = mode
        .find("(defconst piton-ts-capture-faces")
        .expect("piton-mode.el defines piton-ts-capture-faces");
    let alist = &mode[start..];
    let alist = &alist[..alist.find("\"Face for each").expect("the alist's docstring")];
    let mut faces: Vec<(String, String)> = Vec::new();
    for entry in alist.split("(\"").skip(1) {
        let (capture, rest) = entry.split_once('"').expect("a quoted capture");
        let face = rest
            .trim_start()
            .trim_start_matches('.')
            .trim()
            .split(')')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        faces.push((capture.to_string(), face));
    }

    let expected: Vec<(String, String)> = scope_column("ts")
        .into_iter()
        .zip(scope_column("emacs"))
        .map(|((_, capture), (_, face))| (capture, face))
        .collect();
    for capture in captures(&read("emacs/queries/highlights.scm")) {
        let face = faces
            .iter()
            .find(|(name, _)| *name == capture)
            .unwrap_or_else(|| panic!("piton-ts-capture-faces has no entry for `@{capture}`"));
        let wanted = expected
            .iter()
            .find(|(name, _)| *name == capture)
            .map(|(_, face)| face.clone())
            .unwrap_or_default();
        assert_eq!(
            face.1, wanted,
            "piton-ts-capture-faces shows `@{capture}` in `{}`, but scopes.toml says `{wanted}`",
            face.1
        );
    }
    assert!(
        !mode.contains("piton-font-lock-keywords"),
        "piton-mode.el highlights with regexps again; highlighting is the grammar's"
    );
}

// ---------------------------------------------------------------------------
// Zed
// ---------------------------------------------------------------------------

/// The Zed extension must ship the code that starts the server.
///
/// `extension.toml` can declare that Piton has a language server. But it cannot
/// tell which program to run. Only a compiled `language_server_command` can tell
/// Zed that. An extension can declare a server and ship no WebAssembly. Then Zed
/// cannot start the server. Zed shows this problem only when the user opens a
/// `.pi` file.
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

/// The workspace of the repository must not include the extension.
///
/// The extension is a `cdylib` for `wasm32-wasip2`. A build for the host does
/// not link. Thus `cargo test` at the root stops on it.
#[test]
fn the_workspace_excludes_the_zed_extension() {
    let path = repo_root().join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path).expect("workspace manifest");
    assert!(
        manifest.contains(r#"exclude = ["editors/zed"]"#),
        "the workspace should exclude `editors/zed`"
    );
}

/// Zed reads captures by name. Thus a query must use the names that Zed reads.
///
/// This check finds the old `indents.scm` in this directory. That
/// file was a copy of the grammar file in the vocabulary of Neovim. Zed ignored
/// every capture in it.
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

/// Indentation in Zed is a line pattern, and it must stay a line pattern.
///
/// The `@indent` of Zed measures a node that spans the lines that it indents.
/// The grammar has no such node. Indentation is intentionally not in the
/// grammar. Thus the header of an anchor is a line, and the body is not part of
/// the header. Thus the rule can be only in `config.toml`.
#[test]
fn zed_indentation_is_a_line_rule() {
    let config = read("zed/languages/piton/config.toml");
    // `every_definition_indents_after_a_colon` asserts the rule against the
    // shared pattern. Here, the important point is that the rule is a line
    // pattern in `config.toml`.
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

/// Every scope that a bracket excludes must be a scope that `overrides.scm`
/// defines.
///
/// If `not_in = ["verbatim"]` names a scope that no query captures, it is not
/// an error in any place. It never applies.
#[test]
fn zed_brackets_only_name_scopes_the_overrides_define() {
    let overrides = read("zed/languages/piton/overrides.scm");
    let defined = captures(&overrides)
        .into_iter()
        // `@comment.inclusive` defines the `comment` scope with an inclusive
        // range. Inclusive is a property of the range, not part of the name.
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

/// The capture names that a query makes. These are all the `@name` items in it.
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
