; Scopes that the editor's per-construct settings hang off.
;
; `config.toml` uses them to stop auto-closing a bracket where the text is not
; Piton, which is the difference between typing a quotation mark in prose and
; typing one in an expression.

; A line comment's scope has to reach the newline that ends it, or a bracket
; typed at the end of a comment still auto-closes.
(comment) @comment.inclusive

(string) @string

; A fenced block and an escape block are verbatim: their contents are not Piton
; and the editor should not be closing Piton's brackets inside them.
[
  (fence_content)
  (escape_content)
] @verbatim
