# Cli

## Description

The CLI compiler is a command-line tool that allows you to compile Piton files into output. It also includes helper commands like format.

## Terminal Output

Every command prints in the same order. First what it produced, like the files it wrote or the result it computed. Then its outcome, one line that says how it went: a green check when it did what it was asked, a red cross when it stopped short. Problems come last, whatever the command printed before them: diagnostics, errors, and warnings, each with its help and notes beneath it, then the count of errors and warnings as the final line. A command that finished cleanly prints no count unless checking was what it was asked to do.
What matters most stands out. On a terminal, outcomes are bold beside their mark, errors are red and warnings yellow, names to type or look for are highlighted, each path written or removed is marked + or - with its directory dimmed, and details like depths, counts, and provenance are dimmed. Piped output, and any run with NO_COLOR set, is plain text, and stdout is then the bare results: colour and marks never carry something the words don't.

## Version

piton --version, and the version the language server reports to an editor, is the version the edge build publishes the commit as. The major and minor come from Cargo.toml, and the patch is the number of commits up to the one being built, so a local build of a commit reports what its release does, like 0.1.57. A build that sets PITON_VERSION uses that instead. Without git, or outside a checkout, it is the version in Cargo.toml.

## Commands

- [AgentCli](./Agent.md#agent-cli)
- [Build](./Build.md#build)
- [Check](./Check.md#check)
- [Compile](./Compile.md#compile)
- [Format](./Format.md#format)
- [Init](./Init.md#init)
- [Loc](./Loc.md#loc)
- [Lsp](./Lsp.md#lsp)
- [Reach](./Reach.md#reach)
- [Remove](./package-management/Remove.md#remove)
- [Slice](./Slice.md#slice)
- [Tether](./package-management/Tether.md#tether)
- [Untether](./package-management/Untether.md#untether)
- [Update](./package-management/Update.md#update)
