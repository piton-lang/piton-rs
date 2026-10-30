/**
 * Piton grammar for tree-sitter.
 *
 * Piton's structure is carried by indentation, and the editors that use this
 * grammar all pair it with the language server: the spec lists Zed, Helix and
 * Emacs as `["tree-sitter", "lsp"]`. That pairing sets the division of labour
 * here. This grammar recognises lines and the tokens inside them, which is what
 * highlighting needs; folding, symbols, indentation and every structural
 * question come from `piton lsp`, which has the resolved program and can answer
 * them properly.
 *
 * The consequence is that no external scanner is needed. There is no INDENT or
 * DEDENT token, so leading whitespace is ordinary and the grammar stays small
 * enough to reason about.
 *
 * What the grammar has to get right is what every byte is, and the parser is
 * the judge of that: `editors/fixtures/*.expected` is written from it, and
 * `crates/piton-syntax/tests/highlight_corpus.rs` runs `queries/highlights.scm`
 * over the same fixtures and compares. Two rules keep the queries simple enough
 * that every editor reads them the same way. No pattern needs a predicate, so
 * Emacs (which reads `#match?` differently, if at all) can load the file as it
 * is; and no node is captured by two patterns, so it does not matter that
 * Neovim lets the last pattern win and tree-sitter-highlight the first. Both
 * follow from the shape of the tree: a capitalised name and a lowercase one are
 * different tokens, and a name's role is the node it sits in.
 */

const TYPES = [
  "string", "number", "boolean", "null", "list", "dictionary",
  "anchor", "reference", "any", "simple", "complex",
];

// The whole-value number: exactly the compiler's `is_number_literal`. A
// decimal needs its leading digit, and `_` separates digits.
const NUMBER = /-?[0-9](_?[0-9])*(\.[0-9](_?[0-9])*)?/;

// A word at the start of a line, written once so that the keyword tokens below
// can say "this word, then a space".
const keywordThenSpace = (word) => alias(token(prec(1, seq(word, /[ \t]+/))), word);

module.exports = grammar({
  name: "piton",

  extras: () => [/[ \t]/],

  // There is deliberately no `word` token. Keyword extraction would lex every
  // word with it first, including the first word of a value, where no name is
  // valid and the word has to be text. Every keyword has the same lexical
  // precedence as a name, so the longer match wins without it: `password` is
  // a name, and `pass` is the keyword.

  rules: {
    // Every line ends at its newline. Without that anchor a line rule ending
    // in something optional could stop anywhere, and the parser would have no
    // way to tell "this list item has no value" from "this list item ended and
    // a new line began" -- an ambiguity at the end of nearly every rule here.
    //
    // The last line of a file may have no newline, so it is admitted once, at
    // the end, where it cannot be confused with anything else.
    source_file: ($) =>
      seq(repeat($._line), optional(choice($._statement, seq($._indent, $._block_statement)))),

    _line: ($) =>
      choice(
        seq($._statement, $._newline),
        seq($._indent, $._block_statement, $._newline),
        seq(optional($._indent), $.escape_block),
        seq(optional($._indent), $._newline)
      ),

    // Leading whitespace. It is not an extra at the start of a line, because
    // what may follow depends on it: imports and declarations are top-level
    // only, so an indented `use the spec` or `export the result` is prose in a
    // string, which is what the compiler makes of it. A token that can start
    // with a space wins over the extra, so this only has to be valid here. It
    // also has to outrank the keywords that carry their own trailing space
    // (`use `), or an indented `use the spec` would skip its indentation as an
    // extra and lex as an import.
    _indent: () => token(prec(3, /[ \t]+/)),

    // What a line at the margin can be. Prose is not one of them: text at the
    // margin is a compile error, and admitting it here is what used to let
    // `anchor Other extends Base:` parse as a sentence. An escape block is not
    // here either: its closing delimiter takes its own newline.
    _statement: ($) =>
      choice(
        $.comment,
        $.use_declaration,
        $.from_declaration,
        $.reexport_declaration,
        $.anchor_declaration,
        // Still a `property` node, with `export` inside it as before, so
        // queries written against the grammar keep their shape.
        alias($._exported_property, $.property),
        $.property,
        $.list_item,
        $.merge_item
      ),

    // What an indented line can be: a block's contents, never a declaration.
    _block_statement: ($) =>
      choice(
        $.comment,
        $.fence_open,
        $.pass_statement,
        $.property,
        $.list_item,
        $.merge_item,
        $.prose
      ),

    _newline: () => /\r?\n/,

    // A comment runs to the end of the line and has no block form, and it has
    // to be on its own line: it is a statement, so it can only begin where a
    // line does (after the indentation, which is an extra). At the end of a
    // line of code `//` is just text -- `url: https://example.com // note` is
    // the string `https://example.com // note` -- because no rule inside a
    // line admits a comment, and `text` runs straight through the slashes.
    comment: () => token(seq("//", /[^\n]*/)),

    // A code fence protects nothing: the compiler parses the lines inside it
    // like any others, so the grammar does too, and the fence is one line of
    // text that opens nothing. The closing marker is the same line without a
    // language. What a fence is good for is naming the language of the escape
    // block that follows it, which `injections.scm` reads.
    fence_open: ($) =>
      seq(
        $.fence_marker,
        optional(field("language", $.fence_language)),
        optional(field("info", $.fence_info))
      ),
    // Three or more backticks. Written out because tree-sitter reads `{3,}`
    // as exactly three, which would split a ```` marker.
    fence_marker: () => /```+/,
    fence_language: () => /[A-Za-z0-9_+-]+/,
    // Whatever follows the language on the opening line: ```piton fragment.
    fence_info: () => /[^\n]+/,

    // A line of backslashes delimits a block whose contents are literal. The
    // compiler opens a block on a line that is nothing but backslashes and
    // closes it on a run of the same length.
    escape_block: ($) =>
      seq(
        field("open", $.escape_marker),
        repeat($.escape_content),
        field("close", $.escape_marker)
      ),
    // The newline is part of the token rather than matched after it, because
    // tree-sitter's regexes have no lookahead and the marker has to be the
    // whole line: without that, `\ \ {1 + 2 + 3}` in ordinary prose would
    // lex as a marker, since lexical precedence outranks match length.
    //
    // The cost is a closing marker on the last line of a file with no trailing
    // newline, which leaves the block unclosed.
    escape_marker: () => token(prec(2, /\\+[ \t]*\r?\n/)),
    // Contents are literal, so a line inside a block may start with anything.
    // A marker line still wins, on precedence.
    escape_content: () => token(prec(-1, /[^\n]*\r?\n/)),

    // `use`, `from`, `export` and `abstract` take the space after them into the
    // token. Each is also a perfectly good key (`use: text`), and the only
    // thing that tells `use ./keywords` from `use: text` is the character after
    // the word; without the space, the keyword would be lexed first and the
    // key would be a syntax error.
    use_declaration: ($) => seq(keywordThenSpace("use"), field("path", $.module_path)),

    from_declaration: ($) =>
      seq(
        keywordThenSpace("from"),
        field("path", $.module_path),
        field("direction", choice("import", "export")),
        optional(field("names", choice("*", $.import_list)))
      ),

    // A long import list continues on indented lines:
    //
    //     from ./file import
    //         First,
    //         Second
    //
    // The break is one token, newline and indentation together, so that the
    // parser can tell it from the newline that ends the declaration by looking
    // one token ahead.
    import_list: ($) =>
      seq(
        optional($._continuation),
        $.import_item,
        repeat(seq(",", optional($._continuation), $.import_item))
      ),
    _continuation: () => token(seq(/\r?\n/, /[ \t]+/)),
    // A second name renames the import locally.
    import_item: ($) =>
      seq(field("name", $._name), optional(field("alias", $._name))),

    // `export Name` re-exports something declared or imported.
    reexport_declaration: ($) => seq(keywordThenSpace("export"), field("name", $._name)),

    module_path: () => /[@.\/][^\s]*|[A-Za-z_][^\s]*/,

    anchor_declaration: ($) =>
      seq(
        optional(keywordThenSpace("export")),
        optional(keywordThenSpace("abstract")),
        // `anchor`, or a user-defined keyword: lowercase, optionally
        // kebab-case. It is the same token as a key, because at the margin
        // `anchor Name:` and `anchor: a description` differ only in what
        // follows the word.
        field("keyword", alias($.name, $.declaration_keyword)),
        field("name", $._name),
        // `extends Construct as skill` -- bases first, then the keyword the
        // anchor introduces, which is the order the language is written in.
        optional(seq("extends", field("bases", $.base_list))),
        optional(seq("as", field("alias", alias($.name, $.keyword_name)))),
        ":"
      ),

    base_list: ($) => seq($._name, repeat(seq(",", $._name))),

    // A name, split by capitalisation. Nothing a highlighter can see says
    // whether a bare name is an anchor, but the convention is that anchors are
    // capitalised, and two tokens let a query say so without a predicate.
    // A hyphen belongs to the name, as it does in a key and in an expression
    // (`{config.foo-bar}`); subtraction needs spaces.
    _name: ($) => choice($.name, $.type_name),
    name: () => /[a-z_][A-Za-z0-9_-]*/,
    type_name: () => /[A-Z][A-Za-z0-9_-]*/,

    property: ($) => $._property_body,
    _exported_property: ($) => seq(keywordThenSpace("export"), $._property_body),
    _property_body: ($) =>
      seq(
        field("name", $.key),
        token.immediate(":"),
        repeat(field("constraint", $.type_constraint)),
        optional(field("value", $._value))
      ),

    // The key is the word before the colon. It is the same token as the first
    // word of a sentence, and the colon straight after it is what makes it a
    // key: `title: a book` and `The title is long` differ only there, and the
    // parser decides on that one token of lookahead.
    //
    // `pass` is lexed as the keyword wherever a body could be empty, so a key
    // spelled `pass` is admitted as that token too, and the colon decides.
    key: ($) => choice($._name, alias("pass", $.name)),

    // `name:: string: a description` -- the key took the first colon, so a
    // constraint opens on the second and closes on the one before the value.
    // Constraints chain: `p:: dictionary:: null: null`. An abstract anchor's
    // required property has no value, and so no closing colon:
    // `title:: string`.
    //
    // A colon after the type always closes it: the next constraint opens on
    // the colon after that. Right associativity says so.
    type_constraint: ($) =>
      prec.right(
        seq(
          ":",
          optional("extends"),
          field("type", choice($.builtin_type, $._name)),
          optional($.list_suffix),
          optional(":")
        )
      ),
    builtin_type: () => choice(...TYPES),
    list_suffix: () => "[]",

    // `pass` is an intentionally empty body.
    pass_statement: () => "pass",

    list_item: ($) => seq("-", optional(field("value", $._value))),
    // `+` merges and deduplicates; `++` keeps duplicates. A merge line's value
    // is never a whole-value literal: `+ 3` is the text `3`.
    merge_item: ($) => seq(field("operator", choice("++", "+")), optional(field("value", $._inline))),

    // A property or list item's value. When the whole value is a number,
    // `true`, `false` or `null`, it is that literal; otherwise it is text.
    // Without lookahead the grammar cannot see "the whole value", so a
    // leading literal is always lexed as one -- `3 apples` is `3` and then
    // text -- and the highlight query colours it only when nothing follows it
    // on the line, which is the compiler's rule.
    _value: ($) => choice(seq($.value_literal, optional($._inline_text)), $._inline),
    value_literal: ($) =>
      choice(
        alias(token(prec(1, NUMBER)), $.number),
        alias(token(prec(1, /true|false|null/)), $.constant)
      ),

    // A list literal is a list only when it opens the value: `[x, y]` is a
    // list, `see [x, y] there` and `[x](./x.md)` are strings. Text after a
    // leading list is admitted so that the link form still parses.
    _inline: ($) => choice(seq($.inline_list, optional($._inline_text)), $._inline_text),
    _inline_text: ($) => repeat1($._text_part),
    _text_part: ($) => choice($.interpolation, $.inline_escape, $.text, $.punctuation),

    inline_list: ($) => seq("[", optional(seq($._inline, repeat(seq(",", $._inline)))), "]"),

    // `\ {1 + 2} \` is the text `{1 + 2}`: an inline escape opens nothing, so
    // no interpolation is found inside it. The opening delimiter is a
    // backslash and a space; the closing one is the next lone backslash.
    // Tree-sitter cannot count, so the doubled form `\\ \ {x} \ \\` closes
    // at its first single backslash, which `gaps.toml` records.
    inline_escape: ($) =>
      seq($.escape_open, repeat(choice($.escape_text, $.backslash_run)), $.escape_close),
    escape_open: () => token(seq("\\", " ")),
    escape_text: () => token(prec(1, /[^\\\n]+/)),
    backslash_run: () => /\\\\+/,
    escape_close: () => "\\",

    interpolation: ($) =>
      seq(
        field("sigil", $.sigil),
        optional(field("expression", $._expression)),
        "}"
      ),
    // Intrinsic, string, numeric, and reference.
    sigil: () => choice("${", "#{", "@{", "{"),

    _expression: ($) => repeat1(choice($.interpolation, $._expression_token)),
    _expression_token: ($) =>
      choice(
        $.constant,
        $.self_reference,
        $.number,
        $.string,
        $.operator,
        $.member,
        $._name,
        "[",
        "]",
        "(",
        ")",
        ","
      ),

    constant: () => choice("true", "false", "null"),
    self_reference: () => choice("this", "self", "super"),
    // Minus is never a prefix in an expression: `{0 - 1}`, not `{-1}`.
    number: () => /[0-9](_?[0-9])*(\.[0-9](_?[0-9])*)?/,
    string: () => seq('"', repeat(choice(/[^"\\]+/, /\\./)), '"'),
    operator: () => choice("++", "==", "!=", "<=", ">=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "!", "?", ":"),
    // `.name`: the dot and the key after it. A member is a key whatever its
    // capitalisation, which is why it is a node of its own.
    member: ($) => seq(".", field("name", $._name)),

    // A line of a block that is not a key, a list item or a composition line
    // is prose, which in Piton is a value rather than a syntax error. It can
    // start with a word: that word is the token a key would be, and the
    // missing colon is what makes it the first word of a sentence.
    prose: ($) =>
      choice(seq($._name, repeat($._text_part)), repeat1($._text_part)),

    // A sigil character that opens nothing. `@piton/config`, `$x-<name>` and a
    // backtick quoting a word in prose are all ordinary text, but the
    // characters have to be kept out of `text` so that `@{` can open an
    // interpolation and ``` can open a fence. So is a backslash before a space
    // that opens no escape. Lowest precedence, so it only applies where nothing
    // longer matched.
    punctuation: () => token(prec(-3, choice(/[`$#@{}\[\]]/, /\\+/))),
    // Text may not begin with whitespace. Indentation is an extra, and a
    // token that can start with a space matches from column zero -- which
    // beats every token that begins at the first real character, so an
    // indented ``` would be claimed as prose before it could open a fence.
    // A backslash followed by a space is left out, because it opens an
    // inline escape; any other backslash is text (`\:` is literal).
    text: () =>
      token(
        prec(
          -2,
          /([^\s`{}$#@\[\]\\]|\\+[^\s\\`{}$#@\[\]])([^\n{}$#@\[\]\\]|\\+[^ \t\n\\{}$#@\[\]])*/
        )
      ),
  },
});
