; A fenced block's language tag selects a parser for its contents, so a `json`
; example inside a specification is highlighted as JSON.

((fence
   language: (fence_language) @injection.language
   (fence_content) @injection.content)
 (#set! injection.include-children))

; The usual form in a specification wraps the example in an escape block so the
; compiler reads it literally; its lines are the code.
((fence
   language: (fence_language) @injection.language
   (escape_block (escape_content) @injection.content))
 (#set! injection.combined))
