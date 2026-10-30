; The outline is the shape of the document: what a file declares, and the keys
; under each declaration.
;
; Zed takes the depth of an item from the indentation of the line it starts on,
; which is exactly how Piton nests, so nothing here has to describe the nesting.

(anchor_declaration
  "export"? @context
  "abstract"? @context
  keyword: (declaration_keyword) @context
  name: (_) @name) @item

(property
  "export"? @context
  name: (key) @name) @item
