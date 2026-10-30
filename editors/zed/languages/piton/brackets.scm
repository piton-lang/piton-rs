; Pairs Zed highlights the partner of, and colours as a rainbow pair.
;
; Piton's structural nesting is indentation, which is not in the grammar, so
; the pairs here are the ones written inside a line.

(inline_list "[" @open "]" @close)

; An interpolation opens with its sigil -- `${`, `#{`, `@{` or `{` -- and closes
; with a plain brace.
(interpolation (sigil) @open "}" @close)
