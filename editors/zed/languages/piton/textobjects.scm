; Text objects for Zed's vim mode.
;
; There is no `@function.around` or `@class.around` here, and that is not an
; omission. An anchor's body is the indented lines beneath it, and indentation
; is deliberately absent from the grammar -- no node spans a body, so there is
; nothing to hand `ac` or `af`. Zed gets that structure from `piton lsp`, which
; has the resolved program: selection ranges expand from a value to its
; property to the enclosing anchor, and folding ranges cover the bodies.

; Adjacent line comments are one comment, which is what `gc` should take.
(comment)+ @comment.around
