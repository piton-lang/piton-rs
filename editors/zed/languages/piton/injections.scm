; A code fence is text, and the lines inside it are Piton like any others. The
; one thing a fence can do is name the language of an escape block directly
; inside it, which is how a specification shows an example, so that block is
; highlighted as the language the fence names:
;
;     ```json
;     \\\
;     {"a": 1}
;     \\\
;     ```

((fence_open
   language: (fence_language) @injection.language)
 .
 (escape_block
   (escape_content) @injection.content)
 (#set! injection.combined))
