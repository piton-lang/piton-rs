# Cli

## Description

The CLI compiler is a command-line tool that allows you to compile Piton files into output. It also includes helper commands like format.

## Terminal Output

Every command prints in the same order. First what it produced, like the files it wrote or the result it computed. Then its outcome, one line that says how it went: a green check when it did what it was asked, a red cross when it stopped short. Problems come last, whatever the command printed before them: diagnostics, errors, and warnings, each with its help and notes beneath it, then the count of errors and warnings as the final line. A command that finished cleanly prints no count unless checking was what it was asked to do.
What matters most stands out. On a terminal, outcomes are bold beside their mark, errors are red and warnings yellow, names to type or look for are highlighted, each path written or removed is marked + or - with its directory dimmed, and details like depths, counts, and provenance are dimmed. Piped output, and any run with NO_COLOR set, is plain text, and stdout is then the bare results: colour and marks never carry something the words don't.

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
