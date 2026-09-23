; Indentation, in Helix's vocabulary (`@indent`, `@extend`). The grammar's own
; queries/indents.scm is written for nvim-treesitter (`@indent.begin`), which
; Helix does not read, so Helix gets this file instead.
;
; A declaration, or a property with nothing after its colon, opens a block: the
; line after it lands one level in. `@indent` applies to the lines after a
; node's first line, and when Enter is pressed at the end of a node Helix
; counts the new line as one of those. The grammar is line-oriented, so no node
; spans the block's body; `@extend` stretches the node over the more-indented
; lines that follow it, so lines already inside the block keep their level when
; they are re-indented.
;
; A property with a value on its line opens nothing, and neither does a list
; item or a merge line: a list item is never a key, even with a colon at the
; end.
;
; Helix cannot express the blank-line dedent (Enter on a blank line inside a
; dictionary or anchor comes back one level), and it does not use the language
; server's on-type formatting. See editors/helix/README.md.

(anchor_declaration) @indent @extend

((property !value) @indent @extend)
