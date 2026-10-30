; Scopes that the editor's per-construct settings hang off.
;
; `config.toml` uses them to stop auto-closing a bracket where the text is not
; Piton, which is the difference between typing a quotation mark in prose and
; typing one in an expression.

; A line comment's scope has to reach the newline that ends it, or a bracket
; typed at the end of a comment still auto-closes.
(comment) @comment.inclusive

(string) @string

; An escape is verbatim: what it holds is not Piton, and the editor should not
; be closing Piton's brackets inside it. A code fence is not one -- the lines
; inside it are Piton like any others.
[
  (escape_content)
  (escape_text)
] @verbatim
