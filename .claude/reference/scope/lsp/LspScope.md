# Lsp Scope

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

- [SymbolIdentity](model/SymbolIdentity.md)
- [ReferenceResolution](model/ReferenceResolution.md)
- [WorkspaceProjects](model/WorkspaceProjects.md)

## Navigation

- [GoToDefinition](navigation/GoToDefinition.md)
- [FindReferences](navigation/FindReferences.md)
- [DocumentHighlight](navigation/DocumentHighlight.md)
- [Hover](navigation/Hover.md)
- [GoToImplementation](navigation/GoToImplementation.md)
- [TypeHierarchy](navigation/TypeHierarchy.md)
- [Outline](navigation/Outline.md)

## Editing

- [Rename](editing/Rename.md)
- [ImportSpecifiers](editing/ImportSpecifiers.md)
- [FileMoves](editing/FileMoves.md)
- [Completion](editing/Completion.md)
- [CodeActions](editing/CodeActions.md)
- [Formatting](editing/Formatting.md)

## Feedback

- [Diagnostics](feedback/Diagnostics.md)
- [InlayHints](feedback/InlayHints.md)
- [Presentation](feedback/Presentation.md)

## Performance

- [Responsiveness](performance/Responsiveness.md)

## Testing

- [ProtocolTesting](testing/ProtocolTesting.md)

Links in this document point at reference files. Read one when the work touches what it describes.
