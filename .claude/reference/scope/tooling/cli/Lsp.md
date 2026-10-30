# Lsp

## Description

Runs the Piton language server. For what enter and indenting should do, see the editor behavior rules in the editors section.

## Command Name

lsp

## Positional Arguments

null

## Named Arguments

null

## Protocol

Positions are counted in UTF-16 code units, the protocol's default, and the server offers no other encoding. Buffers arrive whole on every change. Diagnostics carry the version of the buffer they were computed from. After a save, a file changed on disk, a closed buffer or a new configuration, the server asks the editor to fetch semantic tokens, inlay hints and code lenses again, so what one file's change does to another shows up; typing in a buffer does not, because the editor asks again for that buffer by itself. Edits the server sends, for a rename, a code action or a moved file, name the version of each open buffer they were computed against when the editor accepts that, so an edit made against an older buffer is refused rather than misapplied. Formatting, folding and selection read the buffer as it is and never wait for the workspace to compile.

## Features

```
diagnostics: Validate Piton syntax and Belay semantics continuously, reporting invalid constructs, unresolved symbols, inheritance problems, type mismatches, circular dependencies, and invalid compositions directly in the editor. Don't report a missing optional property (one with a default value).
completion: Suggest anchors, skills, agents, properties, keywords, imports, inherited members, and valid values based on the current scope and semantic context.
  However, don't autocomplete things that don't exist.
  Offer each thing once. A name an index re-exports is exported by several modules that all lead to the same anchor, and it is offered once, imported from the shortest path. Two different anchors that share a name, like a project's copy of a spec and the one bundled with the compiler, are both offered, each naming the module it would be imported from so they can be told apart.
  Don't propose autocomplete on nothing. If I type `property: ` it shouldn't propose anything because the likely intent is to type unstructured next.
  Typing a [PropertyAccessOperator](../../language/operators/access/PropertyAccessOperator.md#property-access-operator) in the middle of a string shouldn't autocomplete because there's nothing to complete on a string.
  What the cursor is in the middle of is read with the parser's own line rules, so completion never believes a line is something the compiler would not make of it. Nothing is offered in a comment, in a sentence, inside an inline escape or an escape block, in a quoted string, for the name a declaration declares, or for a keyword being invented after `as`. A list item without its space, `-word`, is a sentence too.
  In an anchor's body, the properties it still lacks are offered and the ones it already has are not. Required properties come first. `pass` is offered only in a body with nothing else in it. A key completed in front of a colon that is already written keeps that one colon. Inside a dictionary nested in a property, nothing is offered, because nothing says what keys belong there.
  An editor that can replace a whole word gets both ranges, what is written before the cursor and the whole word, so accepting an item in the middle of a word can replace all of it. An item's documentation is filled in when the editor asks for it, and an item made before the workspace last compiled gets none, rather than the description of whatever its name means now.
autoImport: When a referenced symbol exists elsewhere in the specbase, offer to automatically add the appropriate `use` or import declaration.
hoverInformation: Show the resolved definition of a symbol, including its type, source, documentation, inheritance chain, exported status, and where applicable its compiled interpretation.
  Hover only on what means something: a symbol, a declaring name, or a composition operator. Plain text in a string gets no hover, even a word that happens to name an anchor, and neither does the prose around a `+` line. Where a construct compiles to is shown on the name that declares it, not on every word of its value.
  A composition is explained on its `+` or `++` alone, so a name inside the expression keeps a hover of its own. `this`, `self` and `super` say what they stand for where they are written. A property says whether it is inherited, overrides a base's value, or implements what an abstract base requires, and a required property says it has to be written. Where something is declared is shown as a path relative to the workspace, linking to its line. A literal, a language keyword, a comment, escaped text and a name that does not resolve get no hover; the diagnostic speaks for the last.
goToDefinition: Navigate from any reference to the anchor, property, skill, agent, import, or other symbol that defines it.
  A module path opens its file. A user keyword lands on the anchor it stands for. A member read through `this` or `super` lands on the declaration that supplies it. On the name of a property that replaces a base's, it lands on the one replaced. When the editor can show it, the answer also says which word was followed and the whole declaration it lands in.
typeDefinition: From a property or a variable, go to the anchors its constraints name, in the order they are written; from an anchor, to the anchor itself. A built-in type has nowhere to go.
documentHighlight: Mark every place in the file that names what is under the cursor, with where it is declared marked apart from where it is read.
findReferences: Show every place a symbol is referenced, inherited, composed, interpolated, exported, or otherwise depended upon across the specbase.
renameSymbol: Rename a symbol safely across the entire specbase while updating all imports, references, expressions, inheritance relationships, and compositions that depend on it.
  The new name has to be one the symbol can have. An anchor name starts with a letter or an underscore and holds letters, digits and underscores; a variable or a key holds letters, digits, underscores and hyphens; a keyword is lowercase and may be kebab-case; and neither an anchor, a variable nor a keyword may be a reserved word. A name already used where the rename reaches is refused, and so is a property name already in the family. A module path is renamed by moving its file, and a symbol a bundled package declares cannot be renamed. Each refusal says why. An imported name and its alias are different names, so renaming one never rewrites the other, and the module path on the same line is never touched.
moduleRename: When a Piton file or a folder is moved or renamed, every `use` and `from` path that named it is pointed at its new place, and the moved file's own relative paths are pointed back at what they named, before the move happens. A path keeps its style. A rooted one stays rooted, a relative one stays relative, `.pi` stays written or left off, and a path to a folder's index stays a path to the folder. Package paths never change.
documentSymbols: Expose the structural contents of the current Piton file as an outline of anchors, skills, agents, properties, exports, and other named constructs.
  A property shows its constraints as written, and the keys of a dictionary it holds sit beneath it. List items are values rather than names, and are not in the outline.
workspaceSymbols: Allow fast searching across all named constructs in the entire specbase regardless of which file defines them.
  Matching ignores case. The same name ranks first, then names that start with the query, then names that contain it, then names that hold its letters in order; within a rank, shorter names come first. At most 256 are returned. Each names the module it is in, relative to the workspace. A bundled package has no file to open, so its names are not offered.
semanticHighlighting: Highlight Piton constructs according to their semantic meaning rather than syntax alone, distinguishing anchors, references, properties, inherited values, exports, imports, expressions, types, and keywords.
  A string is a string. Prose gets no tokens of its own: a word in it that looks like a keyword, number, constant, or anchor name is part of the string. Only the expressions inside `{...}` forms are highlighted within a value.
  The server colours names, by what they resolve to, and module paths, and nothing else. Comments, keywords, literals, operators, punctuation and prose belong to the syntax highlighter. For each kind of token the server sends, the editor is told the same scope the syntax highlighter already uses, so a token arriving changes a colour only where resolving the name says something the text could not. An anchor is a class, abstract or not; a property or a dictionary key is a property; a variable is a variable; a user keyword is a keyword; a module path is a namespace. A name that does not resolve gets no token.
inlayHints: Show useful inferred information inline, such as resolved types, inherited origins, composition sources, or the anchor from which a value ultimately derives.
  A hint says only what the source does not. After the name of a property or a variable whose value is a single {...} and that no constraint types, the type it turned out to have, written the way a constraint is, so accepting the hint makes the type explicit. After a member read through `this`, `super` or an anchor, an arrow and the anchor that supplies it, when that is not the anchor read through. `self` is whichever anchor is compiled, so it names none. Types of literal values, overrides and composition operators are already visible and are not hinted.
codeActions: Offer context-sensitive fixes and transformations such as importing a missing symbol, creating an unresolved anchor, adding an export, qualifying an ambiguous reference, or resolving a simple inheritance conflict.
  Each action belongs to something at the cursor: an unresolved name, a declaration's header line, or an import. Plain text offers nothing. Whole-file actions, organizing imports and formatting, are offered when the editor asks for that kind of action, and organizing imports also when the cursor is on an import.
importOrganization: Detect unused, duplicate, invalid, or unnecessarily broad imports and provide an action to clean and normalize them.
formatting: Format Piton source according to the canonical language style, particularly indentation, spacing, declaration layout, expressions, imports, and multiline structures.
  The edits replace only the lines that change, so cursors and marks elsewhere stay where they were. Formatting part of a file is not offered, because re-indenting some lines of a block and not their siblings would change the structure.
inheritanceResolution: Understand `extends` relationships and expose the fully resolved inheritance chain, including which parent contributed each inherited property.
compositionResolution: Understand `+`, `++`, and other Piton composition semantics and show how multiple inputs combine into the resulting construct.
overrideTracking: Identify when a property overrides an inherited value and allow navigation between the overriding declaration and the declaration it replaces.
  Writing a property an abstract base only requires is implementing it, not overriding it, and gets no lens. Replacing a value does, and the lens opens the declaration replaced. Looking for the implementations of a property lists the anchors that write it themselves.
conflictDetection: Report incompatible inherited or composed values where Piton or Belay cannot resolve the result deterministically.
referenceResolution: Resolve symbolic references such as anchors and `@{...}` expressions to their actual target and report unresolved or ambiguous references.
expressionValidation: Parse and validate Piton expressions inside `{...}`, `${...}`, `#{...}`, and `@{...}` expression forms according to their expected output type.
expressionTypeInformation: Show the inferred output type of an expression and warn when the expression cannot produce the type required by its interpolation form or destination.
exportValidation: Track explicit exports and report attempts to import or reference symbols that are not visible outside their defining module.
moduleResolution: Resolve relative and root-based Piton imports according to the project configuration and report missing modules and invalid paths. Circular imports are fine, so don't report them.
circularDependencyDetection: Find cycles that can't resolve, and anchors that end up extending themselves, and show the cycle that caused the error. Circular imports are fine and aren't reported.
relatedSymbolNavigation: Provide navigation between closely related constructs such as an abstract and its implementations, a base anchor and its extensions, or a symbol and the constructs that compose it.
hierarchyView: Expose inheritance and composition relationships as a hierarchy so the editor can show parents, children, extensions, and implementations of a selected construct.
resolvedValueInspection: Allow the editor to show the final resolved value of a property after inheritance, overrides, composition, and expressions have been applied.
provenanceInspection: For any resolved value, show exactly where it came from: its original declaration, inheritance path, composition step, override, or expression.
compiledOutputPreview: Allow a Piton construct or file to be previewed as its compiled representation, such as Markdown, JSON, agent instructions, or another Belay target.
sourceToOutputMapping: Maintain mappings between Piton source and compiled output so an editor can identify which source construct produced a particular section of generated output.
  The mapping is shown in the hover, under Compiled to, and in a code lens on the construct's first line. It is never offered as document links. A construct's range is its whole body, and it usually compiles to one file per adapter, so links would mark the whole body as navigable and stack one tooltip per adapter on every hover.
unusedSymbolDetection: Warn about anchors, imports, properties, exports, or other declarations that are never referenced or contribute nothing to the compiled result.
duplicateRedundantDefinitionDetection: Identify declarations that unnecessarily repeat inherited or composed values and could be removed without changing the resolved specification.
documentationIntegration: Surface documentation comments and descriptions through hover, completion, symbol search, and hierarchy views so the specbase remains understandable while navigating it.
incrementalAnalysis: Only re-evaluate the portions of the dependency graph affected by an edit rather than recompiling the entire specbase after every keystroke.
  Not yet met. The server recompiles the workspace once typing pauses for a quarter of a second, which takes tens of milliseconds for a specbase the size of this one.
workspaceIndexing: Maintain an index of symbols, relationships, references, exports, inheritance, and composition across the project so navigation and completion remain fast.
  The index covers every source under the project root and every open file, whether or not the entry reaches it. A missing or unreadable entry is reported, and everything else still loads, so the files that would fix the project keep their diagnostics, symbols, and completion. Without an entry nothing is reachable, so nothing is reported as unused.
editorSelectionRanges: Understand Piton's semantic structure so expanding selection moves naturally from a value to a property, declaration, and enclosing anchor.
  From a name, selection grows to each expression around it, the braces, the value, the property, each enclosing block, the declaration and the file. Each step is wider than the last and leaves out surrounding blank lines.
folding: Provide folding ranges for anchors, skills, agents, multiline values, documentation blocks, and other structural Piton constructs.
  A declaration folds, and so does a property whose value is a block, an escape block, a run of two or more comment lines and a run of two or more import lines. An escape block written directly inside a code fence folds with the fence. A range runs from its first line to its last line with content, whole lines only. A declaration whose header is half-written still folds, with its body.
```
