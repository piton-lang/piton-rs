; Pairs Zed highlights the partner of, and colours as a rainbow pair.
;
; Piton's structural nesting is indentation, which is not in the grammar, so
; the pairs here are the ones written inside a line.

(inline_list "[" @open "]" @close)

; An interpolation opens with its sigil -- `${`, `#{`, `@{` or `{` -- and closes
; with a plain brace.
(interpolation (sigil) @open "}" @close)

; A fence is delimited by two runs of backticks. They are a pair worth jumping
; between, but colouring them would fight with the injected language between
; them, which is highlighted as itself.
((fence
   open: (fence_marker) @open
   close: (fence_marker) @close)
 (#set! rainbow.exclude))
