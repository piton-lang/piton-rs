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
    "dict_in_list:\n    - key: value\n    - other: thing\n",
    "dict_in_list:\n  - outer:\n      nested: value\n",
    "typed_in_list:\n    - key:: string: value\n",
    "inset_block:\n  key:\n      prose inset past one unit\n",
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
    "fenced:\n    ```css\n    .button {\n      color: #fff; // not a comment\n    }\n    ```\n",
    "fenced:\n    ~~~\n    key: ${not} {an} \\escape\n\n    ~~~\nnext: 1\n",
    "fenced:\n    Prose first:\n    ````md\n    ```\n    ````\n    and after.\n",
    "fenced:\r\n    ```\r\n    crlf\r\n    ```\r\n",
    "fenced:\n    ```\n    ```",
];

/// Inputs that must be reported, but must not stop the file being parsed.
const INVALID: &[&str] = &[
    "x::number: 42\n",
    // A width that is not a whole number of the file's indent unit.
    "a:\n    b: 1\nc:\n      d: 2\n",
    "a:\n \tb: 1\n",
    "!!! nonsense\n",
    "expr: {unclosed\n",
    "quoted: \"unterminated\n",
    "fenced:\n    ```\n    never closed\n\nnext: 1\n",
    "fenced:\n    ```",
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

#[test]
fn an_escape_group_is_one_token() {
    use SyntaxKind::*;
    let group = tokens("x: \\ a group \\\n");
    assert!(
        group.iter().any(|(kind, text)| *kind == ESCAPE_GROUP && text == "\\ a group \\"),
        "{group:?}"
    );
    // Nothing inside is lexed, so a comment, a key and an interpolation are
    // all just characters in it.
    let literal = tokens("x: \\ // ${a} key: v \\\n");
    assert_eq!(
        literal.iter().filter(|(kind, _)| matches!(*kind, COMMENT | SIGIL | COLON)).count(),
        1,
        "only the key head's colon: {literal:?}"
    );
    // A line may hold more than one, each closing at its own delimiter.
    let two = tokens("x: \\ one \\ and \\ two \\\n");
    assert_eq!(two.iter().filter(|(kind, _)| *kind == ESCAPE_GROUP).count(), 2, "{two:?}");
    // One more backslash on each delimiter holds a run that would close it.
    let nested = tokens("x: \\\\ \\ inner \\ \\\\\n");
    assert!(
        nested
            .iter()
            .any(|(kind, text)| *kind == ESCAPE_GROUP && text == "\\\\ \\ inner \\ \\\\"),
        "{nested:?}"
    );
}

#[test]
fn a_backslash_with_nothing_to_close_it_stays_an_escape() {
    use SyntaxKind::*;
    for source in ["x: \\ unclosed\n", "x: a\\ b \\\n", "x: C:\\ path\n"] {
        let lexed = tokens(source);
        assert!(lexed.iter().all(|(kind, _)| *kind != ESCAPE_GROUP), "{source:?}: {lexed:?}");
        assert!(lexed.iter().any(|(kind, _)| *kind == ESCAPE), "{source:?}: {lexed:?}");
    }
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

#[test]
fn any_word_immediately_before_a_brace_is_a_sigil() {
    use SyntaxKind::*;
    let sigils = |source: &str| -> Vec<String> {
        tokens(source)
            .into_iter()
            .filter(|(kind, _)| *kind == SIGIL)
            .map(|(_, text)| text)
            .collect()
    };
    // The two the language ships with, a framework's own word, and shapes a
    // stricter rule would have rejected.
    assert_eq!(sigils("x: ${a}\n"), vec!["$"]);
    assert_eq!(sigils("x: @{a}\n"), vec!["@"]);
    assert_eq!(sigils("x: reference{a}\n"), vec!["reference"]);
    assert_eq!(sigils("x: link-to{a}\n"), vec!["link-to"]);
    assert_eq!(sigils("x: SeeAlso{a}\n"), vec!["SeeAlso"]);
    assert_eq!(sigils("x: doc.ref{a}\n"), vec!["doc.ref"]);
    assert_eq!(sigils("x: !{a}\n"), vec!["!"]);
    assert_eq!(sigils("x: Hello${a}\n"), vec!["$"], "`$` still wins over the word before it");

    // A brace with nothing against it is an expression, not an interpolation.
    assert!(sigils("x: {a}\n").is_empty());
    assert!(sigils("x: see {a}\n").is_empty(), "whitespace separates, so there is no sigil");
    // And a sigil-looking thing with no brace is just prose.
    assert!(sigils("x: this costs $5 plus tax\n").is_empty());
}

/// A realistic document counts the way a reader would count it.
///
/// The unit tests in `loc` cover one construct at a time; this one checks the
/// shape a real anchor takes, where a table of rows and a block of prose sit
/// under the same declaration.
#[test]
fn counting_a_real_document_separates_what_is_written_from_what_is_built() {
    let source = "\
// Every command the editor has, in one place.
//
// Each row is `id - label - shortcut - what it does`.
export anchor ActionCatalogue:
    file:
        - file.new - New - Ctrl+N - empties the buffer, after the unsaved check
        - file.open - Open - Ctrl+O - native open dialog, after the unsaved check

    row:
        id: Stable, lower case, `group.verb`, and never reused
        limit: 64
        run:
            One function, taking the application state.
            It returns nothing.
";
    let counts = piton_syntax::loc::count(source);
    assert_eq!(counts.total, 14);
    assert_eq!(counts.comment, 3);
    assert_eq!(counts.blank, 1);
    // `export anchor ActionCatalogue:`, `file:`, `row:`, `run:`, and the one
    // numeric value. Everything filed under them is what the document says.
    assert_eq!(counts.code, 5, "{counts:?}");
    // Two rows, one key with a sentence, and two lines of a text block.
    assert_eq!(counts.prose, 5, "{counts:?}");
    assert_eq!(counts.code + counts.prose + counts.comment + counts.blank, counts.total);
}

/// The prose rule, replayed by `in_prose_run`, must agree with the lexer.
///
/// Two descriptions of one rule is one too many, and the one a tool consults
/// while a line is half-typed is the one that would drift unnoticed.
#[test]
fn in_prose_run_agrees_with_the_lexer() {
    let source = "\
description:
    This is a string
    and: this is still part of the string

    however:
        that: is a dictionary
    sibling: value

pure:
    a: 1
    b: 2
";
    // One flag per line, in order: is this line inside a run of prose?
    let expected = [
        false, // description:
        false, // This is a string        — opens the run, is not inside one yet
        true,  // and: this is still part of the string
        false, // (blank)
        false, // however:
        false, //     that: is a dictionary
        false, // sibling: value
        false, // (blank)
        false, // pure:
        false, //     a: 1
        false, //     b: 2
    ];
    let mut line_start = 0usize;
    for (index, line) in source.split_inclusive('\n').enumerate() {
        assert_eq!(
            piton_syntax::in_prose_run(source, line_start),
            expected[index],
            "line {index}: {:?}",
            line.trim_end()
        );
        line_start += line.len();
    }

    // And the lexer agrees about what those lines are: `and` never becomes a
    // key, while the keys outside the run still do.
    let lexed = piton_syntax::lex(source);
    let keys: Vec<&str> = lexed
        .tokens
        .iter()
        .filter(|token| token.kind == piton_syntax::SyntaxKind::IDENT)
        .map(|token| &source[token.range])
        .collect();
    assert!(keys.contains(&"however"), "{keys:?}");
    assert!(keys.contains(&"sibling"), "{keys:?}");
    assert!(!keys.contains(&"and"), "`and:` is prose, not a key: {keys:?}");
}

// ---- list item continuation -------------------------------------------------------

#[test]
fn a_line_lined_up_under_a_list_item_continues_it() {
    let source = "x:\n    - one\n      two: three\n        four\n    - five\n";
    assert!(!has_errors(source), "alignment is not an indentation step");
    let item = parse(source)
        .syntax()
        .descendants()
        .find_map(<piton_syntax::ast::ListItem as piton_syntax::ast::AstNode>::cast)
        .expect("a list item");
    assert_eq!(item.continuation().count(), 2);
    assert!(item.block().is_none(), "a continuation opens no block");
    let rendered = tree(source);
    assert!(!rendered.contains("PROPERTY"), "`two:` is text: {rendered}");
    assert_eq!(rendered.matches("LIST_ITEM").count(), 2, "{rendered}");
}

#[test]
fn only_text_after_the_marker_can_be_continued() {
    // An empty item, a `- key:` item, a marker, and a fence are not continued.
    for source in [
        "x:\n    -\n      two\n",
        "x:\n    - k: v\n      two\n",
        "x:\n    + a\n      two\n",
        "x:\n    - a\n      - b\n",
        "x:\n    - a\n      ```\n      ```\n",
    ] {
        let continued = parse(source)
            .syntax()
            .descendants()
            .filter_map(<piton_syntax::ast::ListItem as piton_syntax::ast::AstNode>::cast)
            .any(|item| item.continuation().next().is_some());
        assert!(!continued, "{source:?}\n{}", tree(source));
    }
}

#[test]
fn a_list_item_continuation_is_prose_to_in_prose_run() {
    let source = "x:\n    - one\n      two: three\n    key: value\n    - a\n\n      b: c\n";
    let expected = [false, false, true, false, false, false, false];
    let mut line_start = 0usize;
    for (index, line) in source.split_inclusive('\n').enumerate() {
        assert_eq!(piton_syntax::in_prose_run(source, line_start), expected[index], "line {index}: {line:?}");
        line_start += line.len();
    }
}

// ---- fenced code blocks ------------------------------------------------------------

#[test]
fn a_fence_keeps_every_line_inside_it_as_code() {
    use SyntaxKind::*;
    let source = "x:\n    ```css\n    a: ${b} // c\n        - d\n\n    \\e\n    ```\n    after\n";
    let rendered = tree(source);
    assert!(rendered.contains("CODE_BLOCK"), "{rendered}");
    assert!(!rendered.contains("PROPERTY") && !rendered.contains("LIST_ITEM"), "{rendered}");
    let kinds: Vec<(SyntaxKind, String)> = all_tokens(source);
    for absent in [COMMENT, SIGIL, ESCAPE, DASH, BLANK] {
        assert!(!kinds.iter().any(|(kind, _)| *kind == absent), "{absent:?} in {kinds:?}");
    }
    assert_eq!(kinds.iter().filter(|(kind, _)| *kind == COLON).count(), 1, "only `x:` is a key");
    let code: Vec<&str> =
        kinds.iter().filter(|(kind, _)| *kind == CODE).map(|(_, text)| text.as_str()).collect();
    // Indentation past the fence's own is content.
    assert_eq!(code, vec!["a: ${b} // c", "    - d", "\\e"]);
    let fences = kinds.iter().filter(|(kind, _)| *kind == FENCE).count();
    assert_eq!(fences, 2);
    assert!(!has_errors(source));
}

#[test]
fn a_fence_compiles_to_its_text_less_the_fence_indentation() {
    let source = "x:\n    ```css\n    .a {\n      b: c;\n\n    }\n    ```   \n";
    let block = parse(source)
        .syntax()
        .descendants()
        .find_map(<piton_syntax::ast::CodeBlock as piton_syntax::ast::AstNode>::cast)
        .expect("a code block");
    assert!(block.is_closed());
    assert_eq!(block.text(), "```css\n.a {\n  b: c;\n\n}\n```");
}

#[test]
fn only_a_matching_fence_closes_one() {
    use SyntaxKind::*;
    // A shorter run, the other character, and a fence with an info string are
    // all content.
    let source = "x:\n    ````\n    ```\n    ~~~~\n    ````js\n    `````\n";
    let code: Vec<String> = tokens(source)
        .into_iter()
        .filter(|(kind, _)| *kind == CODE)
        .map(|(_, text)| text)
        .collect();
    assert_eq!(code, vec!["```", "~~~~", "````js"]);
    assert!(!has_errors(source));
}

#[test]
fn inline_code_is_not_a_fence() {
    let source = "x:\n    ```a``` is inline\n";
    assert!(!tree(source).contains("CODE_BLOCK"), "{}", tree(source));
}

#[test]
fn an_unclosed_fence_ends_where_its_block_does() {
    let source = "x:\n    ```\n    code\n\ny: 1\n";
    let parsed = parse(source);
    assert!(parsed.errors.iter().any(|it| it.message.contains("never closed")), "{:?}", parsed.errors);
    let rendered = tree(source);
    assert!(rendered.contains("CODE_BLOCK"), "{rendered}");
    // `y` is still a declaration of its own.
    assert_eq!(rendered.matches("VAR_DECL").count(), 2, "{rendered}");
}

#[test]
fn a_fence_is_prose_to_in_prose_run() {
    let source = "x:\n    ```\n    key: value\n    ```\n    after: still prose\n\n    key: value\n";
    let expected = [false, false, true, true, true, false, false];
    let mut line_start = 0usize;
    for (index, line) in source.split_inclusive('\n').enumerate() {
        assert_eq!(piton_syntax::in_prose_run(source, line_start), expected[index], "line {index}: {line:?}");
        line_start += line.len();
    }
    let lexed = piton_syntax::lex(source);
    let keys: Vec<&str> = lexed
        .tokens
        .iter()
        .filter(|token| token.kind == SyntaxKind::IDENT)
        .map(|token| &source[token.range])
        .collect();
    assert_eq!(keys, vec!["x", "key"], "only the key after the blank line is a key");
}

#[test]
fn every_kind_recovers_from_its_own_raw_value() {
    // `from_raw` indexes `ALL_KINDS`, so a kind added to the enum but put
    // anywhere else in the table would silently rename every kind after it —
    // the tree would still build, and every consumer of it would be wrong.
    for (index, kind) in piton_syntax::kind::ALL_KINDS.iter().enumerate() {
        assert_eq!(*kind as usize, index, "{kind:?} is out of place in ALL_KINDS");
        assert_eq!(SyntaxKind::from_raw(index as u16), *kind);
    }
}
