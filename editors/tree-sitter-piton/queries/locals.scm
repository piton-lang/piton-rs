; Scopes. A file is the scope of the variables declared in it.

(source_file) @local.scope

(anchor_declaration name: (_) @local.definition.type)
(import_item name: (_) @local.definition.import)
(import_item alias: (_) @local.definition.import)
(property name: (key) @local.definition.field)

(interpolation (name) @local.reference)
(interpolation (type_name) @local.reference)
