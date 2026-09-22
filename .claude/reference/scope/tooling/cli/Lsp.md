# Lsp

## Description

Runs the Piton language server
When you've created a new property and end with a `:` pressing enter should move to the next line and indent.
Don't propose autocomplete on nothing.  If I type `property: ` it shouldn't propose anything because the likely intent is to type unstructured next.

## Command Name

lsp

## Positional Arguments

null

## Named Arguments

null

## Features

```
autocomplete: Suggests anchors, specs, skills, agents, properties, keywords, imports, inherited members, and valid values based on the current scope and semantic context.
  However, don't autocomplete things that don't exist.
  Typing a [PropertyAccessOperator](../../language/operators/access/PropertyAccessOperator.md) in the middle of a string shouldn't autocomplete because there's nothing to complete on a string.
autoimport: Automatically adds missing imports for symbols referenced in the current file.
```

## Diagnostics

Validate Piton syntax and Belay semantics continuously, reporting invalid constructs, unresolved symbols, inheritance problems, type mismatches, circular dependencies, and invalid compositions directly in the editor.

### Code Completion

Suggest anchors, specs, skills, agents, properties, keywords, imports, inherited members, and valid values based on the current scope and semantic context.

### Hover Information

Show the resolved definition of a symbol, including its type, source, documentation, inheritance chain, exported status, and where applicable its compiled interpretation.

### Go To Definition

Navigate from any reference to the anchor, property, spec, skill, agent, import, or other symbol that defines it.

### Find References

Show every place a symbol is referenced, inherited, composed, interpolated, exported, or otherwise depended upon across the specbase.

### Rename Symbol

Rename a symbol safely across the entire specbase while updating all imports, references, expressions, inheritance relationships, and compositions that depend on it.

### Document Symbols

Expose the structural contents of the current Piton file as an outline of anchors, specs, skills, agents, properties, exports, and other named constructs.

### Workspace Symbols

Allow fast searching across all named constructs in the entire specbase regardless of which file defines them.

### Semantic Highlighting

Highlight Piton constructs according to their semantic meaning rather than syntax alone, distinguishing anchors, references, properties, inherited values, exports, imports, expressions, types, and keywords.

### Inlay Hints

Show useful inferred information inline, such as resolved types, inherited origins, composition sources, or the anchor from which a value ultimately derives.

### Signature Help

When using constructs with parameters or structured inputs, show the expected fields, types, defaults, and documentation for the active argument.

### Code Actions

Offer context-sensitive fixes and transformations such as importing a missing symbol, creating an unresolved anchor, adding an export, qualifying an ambiguous reference, or resolving a simple inheritance conflict.

### Auto Import

When a referenced symbol exists elsewhere in the specbase, offer to automatically add the appropriate `use` or import declaration.

### Import Organization

Detect unused, duplicate, invalid, or unnecessarily broad imports and provide an action to clean and normalize them.

### Formatting

Format Piton source according to the canonical language style, particularly indentation, spacing, declaration layout, expressions, imports, and multiline structures.

### Inheritance Resolution

Understand `extends` relationships and expose the fully resolved inheritance chain, including which parent contributed each inherited property.

### Composition Resolution

Understand `+`, `++`, and other Belay composition semantics and show how multiple inputs combine into the resulting construct.

### Override Tracking

Identify when a property overrides an inherited value and allow navigation between the overriding declaration and the declaration it replaces.

### Conflict Detection

Report incompatible inherited or composed values where Piton or Belay cannot resolve the result deterministically.

### Reference Resolution

Resolve symbolic references such as anchors and `@{...}` expressions to their actual target and report unresolved or ambiguous references.

### Expression Validation

Parse and validate Piton expressions inside `{...}`, `${...}`, `#{...}`, `!{...}`, and related expression forms according to their expected output type.

### Expression Type Information

Show the inferred output type of an expression and warn when the expression cannot produce the type required by its interpolation form or destination.

### Export Validation

Track explicit exports and report attempts to import or reference symbols that are not visible outside their defining module.

### Module Resolution

Resolve relative and root-based Piton imports according to the project configuration and report missing modules, invalid paths, and forbidden circular imports.

### Circular Dependency Detection

Detect cycles between modules, anchors, inheritance chains, or references where Piton semantics prohibit them, and show the cycle that caused the error.

### Related Symbol Navigation

Provide navigation between closely related constructs such as a spec and its implementations, a base anchor and its extensions, or a symbol and the constructs that compose it.

### Hierarchy View

Expose inheritance and composition relationships as a hierarchy so the editor can show parents, children, extensions, and implementations of a selected construct.

### Resolved Value Inspection

Allow the editor to show the final resolved value of a property after inheritance, overrides, composition, and expressions have been applied.

### Provenance Inspection

For any resolved value, show exactly where it came from: its original declaration, inheritance path, composition step, override, or expression.

### Compiled Output Preview

Allow a Piton construct or file to be previewed as its compiled representation, such as Markdown, JSON, agent instructions, or another Belay target.

### Source To Output Mapping

Maintain mappings between Piton source and compiled output so an editor can identify which source construct produced a particular section of generated output.

### Unused Symbol Detection

Warn about anchors, imports, properties, exports, or other declarations that are never referenced or contribute nothing to the compiled result.

### Duplicate Redundant Definition Detection

Identify declarations that unnecessarily repeat inherited or composed values and could be removed without changing the resolved specification.

### Documentation Integration

Surface documentation comments and descriptions through hover, completion, symbol search, and hierarchy views so the specbase remains understandable while navigating it.

### Incremental Analysis

Only re-evaluate the portions of the dependency graph affected by an edit rather than recompiling the entire specbase after every keystroke.

### Workspace Indexing

Maintain an index of symbols, relationships, references, exports, inheritance, and composition across the project so navigation and completion remain fast.

### Editor Selection Ranges

Understand Piton's semantic structure so expanding selection moves naturally from a value to a property, declaration, anchor, and enclosing spec.

### Folding

Provide folding ranges for anchors, specs, skills, agents, multiline values, documentation blocks, and other structural Piton constructs.

Links in this document point at reference files. Read one when the work touches what it describes.
