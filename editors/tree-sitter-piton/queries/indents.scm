; Indentation, in nvim-treesitter's vocabulary (`@indent.begin`). Helix reads a
; different vocabulary and has its own copy in editors/helix/queries/; Zed's
; indentation is a line pattern in its config.toml.
;
; A declaration, or a property with nothing after its colon, opens a block, so
; the line after it lands one level in. The grammar is line-oriented -- no node
; spans a body -- so each of these nodes is a single line, and
; `indent.immediate` is what lets a single-line node indent the line after it.
;
; A property with a value on its line opens nothing, and neither does a list
; item or a merge line: a list item is never a key, even with a colon at the
; end.
;
; The blank-line dedent (Enter on a blank line inside a dictionary or anchor
; comes back one level) cannot be expressed as a query. Editors that need it
; take it from the Vim indent file or the language server's on-type
; formatting.

((anchor_declaration) @indent.begin
  (#set! indent.immediate 1))

((property !value) @indent.begin
  (#set! indent.immediate 1))
