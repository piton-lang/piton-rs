---
name: build-lsp
description: Builds or changes the LspScope scope Use when you need to build, change, or fix anything the LspScope scope describes
---

Read the scope and every feature it lists that the work touches before changing anything, then read [Testing](../../reference/lib/skills/Testing.md).
The spec is the source of truth. Where the code disagrees with it, change the code. Where the spec is wrong or says nothing about what you need, change the Piton source under `spec` first, run `piton build`, and only then change the code to match. Never leave a behaviour in the code that the spec does not state.
If nothing has been built yet, build from the spec. Otherwise compare the existing code against the spec and decide whether the work is a small change or a refactor; either way the code ends up matching the spec, not the other way round.

# Testing

[Testing](../../reference/lib/skills/Testing.md)

# Scope

## Concept

### Pitch

The Piton language server is how an editor understands a Piton project. Piton files are mostly prose, dense with references to anchors declared in other files, so the server's whole job is to make those references trustworthy: every name can be followed, every change to a name or a file carries its references with it, and every problem is reported once, where it was made.
It is modelled on the TypeScript server for everything a name can do, and on the Markdown server for everything a document and its links can do. Where a feature here behaves differently from those, the difference is written down in this spec with the reason for it.

### Principles

- Correct or silent. A feature that cannot give the right answer gives none, and a change it cannot make safely is refused with a sentence saying why.
- The compiler decides. What a name, a module specifier, or a problem means is answered by `piton-core` and never by a second implementation in the server, so the editor and `piton build` cannot disagree.
- One identity per symbol. Definition, references, highlighting, hover, and rename ask the same question of the same model, so they cannot contradict each other.
- Quiet by default. Nothing is offered, hinted, or highlighted unless it helps the person writing, and prose is left alone.
- Editor-neutral. The server behaves the same in every editor, and stays correct whatever order an editor sends its messages in and whichever parts of the protocol it supports.
- Every rule in this spec has a test, and a bug is fixed by first writing the test that fails because of it.

## Model

- [SymbolIdentity](../../reference/scope/lsp/model/SymbolIdentity.md)
- [ReferenceResolution](../../reference/scope/lsp/model/ReferenceResolution.md)
- [WorkspaceProjects](../../reference/scope/lsp/model/WorkspaceProjects.md)

## Navigation

- [GoToDefinition](../../reference/scope/lsp/navigation/GoToDefinition.md)
- [FindReferences](../../reference/scope/lsp/navigation/FindReferences.md)
- [DocumentHighlight](../../reference/scope/lsp/navigation/DocumentHighlight.md)
- [Hover](../../reference/scope/lsp/navigation/Hover.md)
- [GoToImplementation](../../reference/scope/lsp/navigation/GoToImplementation.md)
- [TypeHierarchy](../../reference/scope/lsp/navigation/TypeHierarchy.md)
- [Outline](../../reference/scope/lsp/navigation/Outline.md)

## Editing

- [Rename](../../reference/scope/lsp/editing/Rename.md)
- [ImportSpecifiers](../../reference/scope/lsp/editing/ImportSpecifiers.md)
- [FileMoves](../../reference/scope/lsp/editing/FileMoves.md)
- [Completion](../../reference/scope/lsp/editing/Completion.md)
- [CodeActions](../../reference/scope/lsp/editing/CodeActions.md)
- [Formatting](../../reference/scope/lsp/editing/Formatting.md)

## Feedback

- [Diagnostics](../../reference/scope/lsp/feedback/Diagnostics.md)
- [InlayHints](../../reference/scope/lsp/feedback/InlayHints.md)
- [Presentation](../../reference/scope/lsp/feedback/Presentation.md)

## Performance

- [Responsiveness](../../reference/scope/lsp/performance/Responsiveness.md)

## Testing

- [ProtocolTesting](../../reference/scope/lsp/testing/ProtocolTesting.md)

# Code

The language server is `crates/piton-lsp`. What a name, a module specifier, or a problem means belongs to `crates/piton-core`, and the server asks the compiler rather than working it out a second time. When a feature needs an answer the compiler does not give yet, add it to the compiler.

# Finishing

- Run `cargo test -p piton-lsp` and `cargo test -p piton-core`
- Run `cargo run -p piton-cli -- build check` from the repository root
- Install the binary with `cargo xtask install` and restart the language server in an editor before saying a behaviour works there

Links in this document point at reference files. Read one when the work touches what it describes.
