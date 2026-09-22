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
 */

const TYPES = [
  "string", "number", "boolean", "null", "list", "dictionary",
  "anchor", "reference", "any", "simple", "complex",
];

module.exports = grammar({
  name: "piton",

  extras: () => [/[ \t]/],

  conflicts: ($) => [[$.property, $._inline]],

  rules: {
    source_file: ($) => repeat($._line),

    _line: ($) =>
      choice(
        $.comment,
        $.fence,
        $.escape_block,
        $.use_declaration,
        $.from_declaration,
        $.anchor_declaration,
        $.pass_statement,
        $.property,
        $.list_item,
        $.merge_item,
        $.prose,
        $._newline
      ),

    _newline: () => /\r?\n/,

    // A comment runs to the end of the line and has no block form. It only
    // begins after whitespace, so `https://x` is not one.
    comment: () => token(seq("//", /[^\n]*/)),

    // Fenced content is verbatim: nothing inside is scanned.
    fence: ($) =>
      seq(
        field("open", $.fence_marker),
        optional(field("language", $.fence_language)),
        $._newline,
        repeat(choice($.fence_content, $.escape_marker, $._newline)),
        optional(field("close", $.fence_marker))
      ),
    fence_marker: () => /`{3,}/,
    fence_language: () => /[A-Za-z0-9_+-]+/,
    fence_content: () => token(prec(-1, /[^\n`][^\n]*/)),

    // A line of backslashes delimits a block whose contents are literal. The
    // compiler opens a block on a line that is nothing but backslashes and
    // closes it on a run of the same length.
    escape_block: ($) =>
      seq(
        field("open", $.escape_marker),
        repeat(choice($.escape_content, $._newline)),
        optional(field("close", $.escape_marker))
      ),
    // The newline is part of the token rather than matched after it, because
    // tree-sitter's regexes have no lookahead and the marker has to be the
    // whole line: without that, `\ \ {1 + 2 + 3}` in ordinary prose would
    // lex as a marker, since lexical precedence outranks match length.
    //
    // The cost is a closing marker on the last line of a file with no trailing
    // newline, which leaves the block unclosed. `close` is optional, so that
    // parses as an unterminated block rather than an error.
    escape_marker: () => token(prec(2, /\\+[ \t]*\r?\n/)),
    // Contents are literal, so a line inside a block may start with anything.
    // A marker line still wins, on precedence.
    escape_content: () => token(prec(-1, /[^\n]+/)),

    use_declaration: ($) => seq("use", field("path", $.module_path)),

    from_declaration: ($) =>
      seq(
        "from",
        field("path", $.module_path),
        field("direction", choice("import", "export")),
        optional(field("names", choice("*", $.import_list)))
      ),

    import_list: ($) => seq($.import_item, repeat(seq(",", optional($._newline), $.import_item))),
    // A second name renames the import locally.
    import_item: ($) => seq(field("name", $.identifier), optional(field("alias", $.identifier))),

    module_path: () => /[@.\/][^\s]*|[A-Za-z_][^\s]*/,

    anchor_declaration: ($) =>
      seq(
        optional("export"),
        optional("abstract"),
        field("keyword", $.declaration_keyword),
        field("name", $.identifier),
        optional(seq("as", field("alias", $.keyword_name))),
        optional(seq("extends", field("bases", $.base_list))),
        ":"
      ),

    // `anchor`, or a user-defined keyword: lowercase, optionally kebab-case.
    declaration_keyword: () => /[a-z][a-z0-9]*(-[a-z0-9]+)*/,
    keyword_name: () => /[a-z][a-z0-9]*(-[a-z0-9]+)*/,
    base_list: ($) => seq($.identifier, repeat(seq(",", $.identifier))),

    // A key is any text without spaces. Keywords are keys here too: the colon
    // is what separates `anchor Name:` from `anchor: a description`.
    property: ($) =>
      seq(
        optional("export"),
        field("name", $.key),
        repeat(field("constraint", $.type_constraint)),
        ":",
        optional(field("value", $._inline))
      ),

    key: () => token(prec(1, /[^\s:{}\[\]#@$][^\s:]*/)),

    type_constraint: ($) =>
      seq("::", optional("extends"), field("type", choice($.builtin_type, $.identifier)), optional($.list_suffix)),
    builtin_type: () => choice(...TYPES),
    list_suffix: () => "[]",

    // `pass` is an intentionally empty body.
    pass_statement: () => "pass",

    list_item: ($) => seq("-", optional(field("value", $._inline))),
    // `+` merges and deduplicates; `++` keeps duplicates.
    merge_item: ($) => seq(field("operator", choice("++", "+")), optional(field("value", $._inline))),

    _inline: ($) => repeat1(choice($.interpolation, $.inline_list, $.text)),

    inline_list: ($) => seq("[", optional(seq($._inline, repeat(seq(",", $._inline)))), "]"),

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
      choice($.constant, $.self_reference, $.number, $.string, $.operator, $.identifier, "[", "]", "(", ")", ","),

    constant: () => choice("true", "false", "null"),
    self_reference: () => choice("this", "self", "super"),
    // A leading zero is required for a decimal; underscores may separate digits.
    number: () => /\d[\d_]*(\.[\d_]+)?/,
    string: () => seq('"', repeat(choice(/[^"\\]+/, /\\./)), '"'),
    operator: () => choice("++", "==", "!=", "<=", ">=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "!", "?", ":", "."),

    identifier: () => /[A-Za-z_][A-Za-z0-9_]*/,

    // Anything the rules above do not claim is prose, which in Piton is a value
    // rather than a syntax error.
    prose: ($) => repeat1(choice($.interpolation, $.text)),
    text: () => token(prec(-2, /[^\n{}$#@\[\]]+/)),
  },
});
