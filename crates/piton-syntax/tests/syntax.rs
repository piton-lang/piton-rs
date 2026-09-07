//! Syntax tests.
//!
//! Three invariants hold for every input, valid or not: parsing never panics,
//! the tree reproduces the source byte for byte, and the root covers the whole
//! file. Everything else here checks that a particular construct produces the
//! particular tree the rest of the compiler expects.

use piton_syntax::kind::SyntaxKind;
use piton_syntax::{parse, NodeOrToken, SyntaxNode};

/// A compact rendering of the tree: kinds only, indented by depth.
fn shape(node: &SyntaxNode, depth: usize, out: &mut String) {
    out.push_str(&"  ".repeat(depth));
    out.push_str(&format!("{:?}\n", node.kind()));
    for child in node.children() {
        shape(&child, depth + 1, out);
    }
}

fn tree(source: &str) -> String {
    let mut out = String::new();
    shape(&parse(source).syntax(), 0, &mut out);
    out
}

/// Every non-trivia token, as `KIND "text"`, which is what the lexer tests want.
fn tokens(source: &str) -> Vec<(SyntaxKind, String)> {
    parse(source)
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| match element {
            NodeOrToken::Token(token) if !token.kind().is_trivia() => {
                Some((token.kind(), token.text().to_string()))
            }
            _ => None,
        })
        .collect()
}

/// Every token, trivia included, for the tests that care about comments.
fn all_tokens(source: &str) -> Vec<(SyntaxKind, String)> {
    parse(source)
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| match element {
            NodeOrToken::Token(token) => Some((token.kind(), token.text().to_string())),
            _ => None,
        })
        .collect()
}

fn has_errors(source: &str) -> bool {
    let parsed = parse(source);
    !parsed.errors.is_empty()
        || parsed.syntax().descendants().any(|node| node.kind() == SyntaxKind::ERROR)
}

/// Every construct in the language, for the invariants that hold universally.
const CORPUS: &[&str] = &[
    "",
    "\n",
    "\n\n\n",
    "// just a comment\n",
    "// no trailing newline",
    "x: 1",
    "x: 1\n",
    "x: 1\r\n",
    "myVariable: 42 // trailing\n",
    "// leading\nmyVariable: 42\n",
    "a:: number: 42\n",
    "a:: string:: number: 42\n",
    "a:: string[]: [x, y]\n",
    "a:: dictionary:\n    b:: number: 1\n",
    "shape:: extends Thing[]\n",
    "empty:\n",
    "text: Hello, World!\n",
    "block:\n    Hello, World!\n",
    "paragraphs:\n    one\n    two\n\n    three\n",
    "quoted: \"// not a comment\"\n",
    "escaped: \\// literal\n",
    "url: see https://example.com/a//b for more\n",
    "hyphen: a well-known thing\n",
    "maths: 3.14 - 3.14\n",
    "underscored: 1_200_000.00\n",
    "negative: -5\n",
    "list:\n    - one\n    - two\n",
    "nested:\n    - one\n        - two\n            - three\n",
    "inline: [a, b, c]\n",
    "inline_nested: [a, [b, [c]]]\n",
    "trailing_comma: [a, b,]\n",
    "spread:\n    + {super.items}\n    - one\n",
    "spread2:\n    ++ {super.items}\n    - one\n",
    "dict:\n    a:\n        b: 1\n",
    "mixed:\n    prose\n\n    - one\n\n    key:\n        deep: 1\n",
    "expr: {1 + 2}\n",
    "expr: {a.b.c}\n",
    "expr: {this.x} && false\n",
    "expr: {x ? \"y\" : \"n\"}\n",
    "expr: {(1 + 2) * 3}\n",
    "interp: Hello, ${name}!\n",
    "interp: @{Anchor} and reference{Other} and {bare}\n",
    "anchor A:\n    x: 1\n",
    "export anchor A:\n    x: 1\n",
    "abstract anchor A:\n    x:: string\n",
    "export abstract anchor A as kw:\n    x:: string\n",
    "anchor A extends B, C:\n    x: 1\n",
    "anchor A extends B as kw:\n    x: 1\n",
    "kw Child:\n    x: 1\n",
    "export kw Child extends Other:\n    x: 1\n",
    "from ./a import B\n",
    "from ./a import B, C Alias\n",
    "from ./a import\n    B,\n    C\n",
    "from ./a export *\n",
    "from ./a export B Alias, C\n",
    "from /abs/path import B\n",
    "from @piton/belay import Agent\n",
    "use ./keywords\n",
    "use @piton/config\n",
    "export Name\n",
    "a: 1\n\n\nb: 2\n",
    "anchor A:\n    // a comment inside a block\n    x: 1\n",
    "anchor A:\n    x: 1\n    // a comment after the last property\nb: 2\n",
    "unicode: café ☕ naïve\n",
    "unicode_key: café: yes\n",
];

/// Inputs that must be reported, but must not stop the file being parsed.
const INVALID: &[&str] = &[
    "x::number: 42\n",
    "a:\n  b: 1\nc:\n    d: 2\n",
    "a:\n \tb: 1\n",
    "!!! nonsense\n",
    "expr: {unclosed\n",
    "quoted: \"unterminated\n",
];

#[test]
fn parsing_is_lossless_and_total() {
    for source in CORPUS.iter().chain(INVALID) {
        let parsed = parse(source);
        let root = parsed.syntax();
        assert_eq!(root.text().to_string(), *source, "round trip failed for {source:?}");
        assert_eq!(
            usize::from(root.text_range().end()),
            source.len(),
            "root does not cover {source:?}"
        );
        assert_eq!(root.kind(), SyntaxKind::ROOT);
    }
}

#[test]
fn the_corpus_parses_without_errors() {
    for source in CORPUS {
        assert!(!has_errors(source), "unexpected error parsing {source:?}\n{}", tree(source));
    }
}

#[test]
fn invalid_input_is_reported() {
    for source in INVALID {
        assert!(has_errors(source), "expected an error for {source:?}");
    }
}

#[test]
fn an_error_does_not_swallow_the_rest_of_the_file() {
    let source = "!!! nonsense\n\nanchor Good:\n    x: 1\n";
    let parsed = parse(source);
    assert!(has_errors(source));
    assert!(
        parsed.syntax().descendants().any(|node| node.kind() == SyntaxKind::ANCHOR_DECL),
        "{}",
        tree(source)
    );
}

// ---- the lexer ---------------------------------------------------------------

#[test]
fn a_comment_needs_a_word_boundary_so_urls_survive() {
    use SyntaxKind::*;
    let kinds: Vec<SyntaxKind> =
        all_tokens("url: https://example.com/a//b\n").into_iter().map(|(kind, _)| kind).collect();
    assert!(!kinds.contains(&COMMENT), "a URL must not become a comment: {kinds:?}");

    let commented = all_tokens("x: 1 // note\n");
    assert!(
        commented.iter().any(|(kind, text)| *kind == COMMENT && text == "// note"),
        "{commented:?}"
    );
    let at_start = all_tokens("// note\n");
    assert!(at_start.iter().any(|(kind, _)| *kind == COMMENT));
}

#[test]
fn operators_are_only_operators_when_they_stand_alone() {
    use SyntaxKind::*;
    let hyphenated = tokens("x: a well-known thing\n");
    assert!(
        !hyphenated.iter().any(|(kind, _)| *kind == MINUS),
        "a hyphen inside a word is not subtraction: {hyphenated:?}"
    );
    let arithmetic = tokens("x: 3.14 - 3.14\n");
    assert!(arithmetic.iter().any(|(kind, _)| *kind == MINUS));
}

#[test]
fn numbers_keep_their_spelling_and_stop_at_words() {
    use SyntaxKind::*;
    assert!(tokens("x: 1_200_000.00\n")
        .iter()
        .any(|(kind, text)| *kind == NUMBER && text == "1_200_000.00"));
    // `42 things` is prose, not a number followed by a word.
    let prose = tokens("x: 42 things\n");
    assert!(prose.iter().any(|(kind, text)| *kind == TEXT && text == "things"), "{prose:?}");
    // A leading dot is not a decimal.
    assert!(!tokens("x: .14\n").iter().any(|(kind, _)| *kind == NUMBER));
}

#[test]
fn sigils_are_recognised_before_a_brace_only() {
    use SyntaxKind::*;
    let sigils: Vec<String> = tokens("x: ${a} @{b} reference{c} {d} costs $5\n")
        .into_iter()
        .filter(|(kind, _)| *kind == SIGIL)
        .map(|(_, text)| text)
        .collect();
    assert_eq!(sigils, vec!["$", "@", "reference"], "a bare `{{}}` has no sigil, `$5` is prose");
}

#[test]
fn a_key_head_needs_a_space_after_its_colon() {
    use SyntaxKind::*;
    assert!(tokens("name: value\n").iter().any(|(kind, _)| *kind == COLON));
    assert!(tokens("name:\n").iter().any(|(kind, _)| *kind == COLON));
    // Inside a block, `Note that:` is prose: a key is a single word.
    let prose = tokens("block:\n    Note that: something\n");
    assert!(prose.iter().any(|(kind, text)| *kind == TEXT && text == "Note"), "{prose:?}");
    // A single word does make a key, exactly as it does in YAML.
    let key = tokens("block:\n    Note: something\n");
    assert!(key.iter().any(|(kind, text)| *kind == IDENT && text == "Note"), "{key:?}");
}

#[test]
fn escapes_and_quotes_are_distinct_tokens() {
    use SyntaxKind::*;
    assert!(tokens("x: \\// literal\n").iter().any(|(kind, text)| *kind == ESCAPE && text == "\\/"));
    assert!(tokens("x: \"quoted\"\n")
        .iter()
        .any(|(kind, text)| *kind == QUOTED_STRING && text == "\"quoted\""));
    // A quote in the middle of prose is just a character.
    let middle = tokens("x: he said \"hi\" loudly\n");
    assert!(middle.iter().any(|(kind, _)| *kind == QUOTED_STRING));
    assert!(middle.iter().any(|(kind, text)| *kind == TEXT && text == "loudly"));
}

// ---- tree shapes ---------------------------------------------------------------

#[test]
fn a_value_is_an_expression_only_when_the_whole_value_is_one() {
    assert!(tree("x: 1 + 2\n").contains("BIN_EXPR"), "{}", tree("x: 1 + 2\n"));
    assert!(tree("x: a + b\n").contains("TEXT_VALUE"), "{}", tree("x: a + b\n"));
    assert!(tree("x: 42 things\n").contains("TEXT_VALUE"));
    assert!(tree("x: {a + b}\n").contains("BRACE_EXPR"));
    assert!(tree("x: Hello ${name}\n").contains("INTERPOLATION"));
}

#[test]
fn a_declaration_has_the_shape_the_compiler_reads() {
    let rendered = tree("export abstract anchor A extends B as kw:\n    x:: string: 1\n");
    for expected in
        ["ANCHOR_DECL", "EXTENDS_CLAUSE", "AS_CLAUSE", "BLOCK", "PROPERTY", "TYPE_ANNOTATION"]
    {
        assert!(rendered.contains(expected), "missing {expected}:\n{rendered}");
    }
}

#[test]
fn a_nested_list_item_becomes_its_own_block() {
    let rendered = tree("items:\n    - one\n        - two\n");
    assert_eq!(
        rendered,
        "ROOT\n  VAR_DECL\n    BLOCK\n      LIST_ITEM\n        VALUE\n          TEXT_VALUE\n        BLOCK\n          LIST_ITEM\n            VALUE\n              TEXT_VALUE\n"
    );
}

#[test]
fn an_import_list_keeps_its_separators() {
    let source = "from ./a import B, C Alias\n";
    let rendered = tree(source);
    assert!(rendered.contains("IMPORT_LIST"), "{rendered}");
    assert_eq!(parse(source).syntax().text().to_string(), source);
    let commas = tokens(source).into_iter().filter(|(kind, _)| *kind == SyntaxKind::COMMA).count();
    assert_eq!(commas, 1, "the comma must be in the tree, not swept up as trivia");
}

#[test]
fn a_shape_declaration_needs_no_value() {
    let rendered = tree("abstract anchor A:\n    x:: string\n");
    assert!(rendered.contains("PROPERTY"), "{rendered}");
    assert!(rendered.contains("TYPE_ANNOTATION"), "{rendered}");
    assert!(!has_errors("abstract anchor A:\n    x:: string\n"));
}

#[test]
fn blank_lines_are_significant_but_comment_lines_are_not() {
    let with_blank = tree("x:\n    one\n\n    two\n");
    assert!(with_blank.contains("BLANK_LINE"), "{with_blank}");
    // A comment-only line is trivia, so it cannot break a string in two.
    let with_comment = tree("x:\n    one\n    // note\n    two\n");
    assert!(!with_comment.contains("BLANK_LINE"), "{with_comment}");
}
