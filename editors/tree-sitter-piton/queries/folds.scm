; Folding for the regions tree-sitter can see without indentation.
;
; Declaration and property bodies fold through the language server, which knows
; where a block ends.

(fence) @fold
(escape_block) @fold
(import_list) @fold
