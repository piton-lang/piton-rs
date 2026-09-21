# Cli

## Description

The CLI compiler is a command-line tool that allows you to compile Piton files into output. It also includes helper commands like format.

## Commands

- commandName: agent
  description: Launches the specified agent with Piton fluency.
  positionalArguments:
    agent: Which agent to run [ claude ]
  namedArguments: null
- commandName: analyze
  description: Runs [Analysis](../../analysis/Analysis.md) on the project
  positionalArguments: null
  namedArguments: null
- commandName: build
  description: Builds the project as configured by piton.config.pi
  positionalArguments:
    config: optional path to piton.config.pi file
  namedArguments: null
- commandName: check
  description: Checks specific files or the project and reports errors
  positionalArguments: null
  namedArguments: null
  emit: false
  validate: syntax imports references types inheritance composition exports circular-dependencies
  diagnostics:
    errors: true
    warnings: true
  exitCode:
    success: 0
    errors: 1
- commandName: compile
  description: Compiles Piton
Pointing at a single file, compile will output the compiled result to stdout.  Glob-based paths won't work unless we also use null
If null is set, the compiled result will be written to a file with the appropriate extension for the selected Valid options are [ json, yaml, markdown ] that lives next to the input file.
  positionalArguments:
    path: file or glob
  namedArguments:
    adapter: Valid options are [ json, yaml, markdown ]
    write: null
- commandName: format
  description: Applies canonical formatting.
  positionalArguments:
    path: file or glob
  namedArguments:
    check: Only check the files, and report problems.  Don't write.
- commandName: loc
  description: Counts lines of Piton source for specific files or the project
  positionalArguments: null
  namedArguments: null
  emit: false
  count: total code comments blank
  groupBy: file
  summary: true
  diagnostics:
    errors: false
    warnings: false
  exitCode:
    success: 0
- commandName: lsp
  description: Runs the Piton language server
  positionalArguments: null
  namedArguments: null
  Diagnostics:
    - Validate Piton syntax and Belay semantics continuously, reporting invalid constructs, unresolved symbols, inheritance problems, type mismatches, circular dependencies, and invalid compositions directly in the editor.
    - CodeCompletion: Suggest anchors, specs, skills, agents, properties, keywords, imports, inherited members, and valid values based on the current scope and semantic context.
      HoverInformation: Show the resolved definition of a symbol, including its type, source, documentation, inheritance chain, exported status, and where applicable its compiled interpretation.
      GoToDefinition: Navigate from any reference to the anchor, property, spec, skill, agent, import, or other symbol that defines it.
      FindReferences: Show every place a symbol is referenced, inherited, composed, interpolated, exported, or otherwise depended upon across the specbase.
      RenameSymbol: Rename a symbol safely across the entire specbase while updating all imports, references, expressions, inheritance relationships, and compositions that depend on it.
      DocumentSymbols: Expose the structural contents of the current Piton file as an outline of anchors, specs, skills, agents, properties, exports, and other named constructs.
      WorkspaceSymbols: Allow fast searching across all named constructs in the entire specbase regardless of which file defines them.
      SemanticHighlighting: Highlight Piton constructs according to their semantic meaning rather than syntax alone, distinguishing anchors, references, properties, inherited values, exports, imports, expressions, types, and keywords.
      InlayHints: Show useful inferred information inline, such as resolved types, inherited origins, composition sources, or the anchor from which a value ultimately derives.
      SignatureHelp: When using constructs with parameters or structured inputs, show the expected fields, types, defaults, and documentation for the active argument.
      CodeActions: Offer context-sensitive fixes and transformations such as importing a missing symbol, creating an unresolved anchor, adding an export, qualifying an ambiguous reference, or resolving a simple inheritance conflict.
      AutoImport: When a referenced symbol exists elsewhere in the specbase, offer to automatically add the appropriate `use` or import declaration.
      ImportOrganization: Detect unused, duplicate, invalid, or unnecessarily broad imports and provide an action to clean and normalize them.
      Formatting: Format Piton source according to the canonical language style, particularly indentation, spacing, declaration layout, expressions, imports, and multiline structures.
      InheritanceResolution: Understand `extends` relationships and expose the fully resolved inheritance chain, including which parent contributed each inherited property.
      CompositionResolution: Understand `+`, `++`, and other Belay composition semantics and show how multiple inputs combine into the resulting construct.
      OverrideTracking: Identify when a property overrides an inherited value and allow navigation between the overriding declaration and the declaration it replaces.
      ConflictDetection: Report incompatible inherited or composed values where Piton or Belay cannot resolve the result deterministically.
      ReferenceResolution: Resolve symbolic references such as anchors and `@{...}` expressions to their actual target and report unresolved or ambiguous references.
      ExpressionValidation: Parse and validate Piton expressions inside `{...}`, `${...}`, `#{...}`, `!{...}`, and related expression forms according to their expected output type.
      ExpressionTypeInformation: Show the inferred output type of an expression and warn when the expression cannot produce the type required by its interpolation form or destination.
      ExportValidation: Track explicit exports and report attempts to import or reference symbols that are not visible outside their defining module.
      ModuleResolution: Resolve relative and root-based Piton imports according to the project configuration and report missing modules, invalid paths, and forbidden circular imports.
      CircularDependencyDetection: Detect cycles between modules, anchors, inheritance chains, or references where Piton semantics prohibit them, and show the cycle that caused the error.
      RelatedSymbolNavigation: Provide navigation between closely related constructs such as a spec and its implementations, a base anchor and its extensions, or a symbol and the constructs that compose it.
      HierarchyView: Expose inheritance and composition relationships as a hierarchy so the editor can show parents, children, extensions, and implementations of a selected construct.
      ResolvedValueInspection: Allow the editor to show the final resolved value of a property after inheritance, overrides, composition, and expressions have been applied.
      ProvenanceInspection: For any resolved value, show exactly where it came from: its original declaration, inheritance path, composition step, override, or expression.
      CompiledOutputPreview: Allow a Piton construct or file to be previewed as its compiled representation, such as Markdown, JSON, agent instructions, or another Belay target.
      SourceToOutputMapping: Maintain mappings between Piton source and compiled output so an editor can identify which source construct produced a particular section of generated output.
      UnusedSymbolDetection: Warn about anchors, imports, properties, exports, or other declarations that are never referenced or contribute nothing to the compiled result.
      DuplicateRedundantDefinitionDetection: Identify declarations that unnecessarily repeat inherited or composed values and could be removed without changing the resolved specification.
      DocumentationIntegration: Surface documentation comments and descriptions through hover, completion, symbol search, and hierarchy views so the specbase remains understandable while navigating it.
      IncrementalAnalysis: Only re-evaluate the portions of the dependency graph affected by an edit rather than recompiling the entire specbase after every keystroke.
      WorkspaceIndexing: Maintain an index of symbols, relationships, references, exports, inheritance, and composition across the project so navigation and completion remain fast.
      EditorSelectionRanges: Understand Piton's semantic structure so expanding selection moves naturally from a value to a property, declaration, anchor, and enclosing spec.
      Folding: Provide folding ranges for anchors, specs, skills, agents, multiline values, documentation blocks, and other structural Piton constructs.
- commandName: reach
  description: Analyzes which parts of the specbase are reachable from specific files, anchors, or the project
  positionalArguments: null
  namedArguments: null
  emit: false
  traverse: imports references inheritance composition
  direction: outgoing
  include: direct transitive
  report: reachable unreachable depth paths
  groupBy: source
  summary: true
  diagnostics:
    errors: false
    warnings: false
  exitCode:
    success: 0

Links in this document point at reference files. Read one when the work touches what it describes.
