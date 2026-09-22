; Scopes. A file is the scope of the variables declared in it.

(source_file) @local.scope

(anchor_declaration name: (identifier) @local.definition.type)
(import_item name: (identifier) @local.definition.import)
(import_item alias: (identifier) @local.definition.import)
(property name: (key) @local.definition.field)

(identifier) @local.reference
