; Piton highlighting.
;
; Every capture here is one of the canonical names in
; editors/fixtures/scopes.toml, and what it paints is tested byte for byte
; against the parser by crates/piton-syntax/tests/highlight_corpus.rs.
;
; Two rules hold throughout, and the corpus checks the second:
;
; - No pattern has a predicate. Capitalisation is a token of its own
;   (`type_name` against `name`), so nothing needs `#match?`, and Emacs reads
;   this file unchanged.
; - No node is captured twice. Neovim lets the last matching pattern win and
;   tree-sitter-highlight the first, so a node with two captures is coloured
;   differently in different editors. A name's role comes from the node it sits
;   in, never from a general pattern that a specific one overrides.
;
; Prose -- `text`, `punctuation`, and the first word of a sentence -- has no
; capture at all: it is shown in the plain text style, so that the structure
; around it stands out. What a name resolves to comes from the language
; server's semantic tokens on top of this.

(comment) @comment

; Keywords. A user keyword is a keyword where it declares an anchor, and where
; `as` declares it.
[
  "use"
  "from"
  "import"
  "export"
  "abstract"
  "extends"
  "as"
] @keyword
(pass_statement) @keyword
(declaration_keyword) @keyword
(keyword_name) @keyword

; Declarations and the names they refer to.
(anchor_declaration name: (_) @type.definition)
(base_list (_) @type)

; Imports. Only capitalisation says whether an imported name is an anchor.
(module_path) @string.special.path
(import_item (type_name) @type)
(import_item (name) @variable)
(reexport_declaration (type_name) @type)
(reexport_declaration (name) @variable)

; Properties and their constraints.
(key) @property
(builtin_type) @type.builtin
(type_constraint type: (type_name) @type)
(type_constraint type: (name) @type)

; A value that is only a literal. A leading literal is always a node, so it is
; coloured only when nothing follows it: `42` is a number, `42 things` is text.
(property (value_literal (number) @number) .)
(property (value_literal (constant) @constant.builtin) .)
(list_item (value_literal (number) @number) .)
(list_item (value_literal (constant) @constant.builtin) .)

; Structure markers.
(list_item "-" @punctuation.list_marker)
(merge_item operator: _ @operator)

; Expressions.
(interpolation (sigil) @punctuation.special)
(interpolation "}" @punctuation.special)
(interpolation (constant) @constant.builtin)
(interpolation (self_reference) @variable.builtin)
(interpolation (number) @number)
(interpolation (string) @string)
(interpolation (operator) @operator)
(interpolation (type_name) @type)
(interpolation (name) @variable)
(member "." @operator)
(member name: (_) @property)

; Escapes. The delimiters are escapes; what an escape block holds is verbatim,
; and what an inline escape holds is plain text like the prose around it.
(escape_marker) @string.escape
(escape_content) @string.special
(escape_open) @string.escape
(escape_close) @string.escape
