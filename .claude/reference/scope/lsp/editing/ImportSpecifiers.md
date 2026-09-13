# Import Specifiers

## Description

How the server writes a module specifier, whether rewriting one or adding one

## Ownership

Specifiers are written by `piton-core`, beside the code that resolves them, and nowhere else. Every specifier the server writes resolves, when the compiler reads it, to the file it was written for.

## Form

- A specifier never carries the `.pi` extension, and a directory with an `index.pi` is written as the directory.
- A relative specifier starts with `./` when it goes no higher than the importing file's directory, and otherwise with `../` segments alone, so `./../` is never written.
- A rooted specifier is `/` followed by the path under the project root, or by a library's name and the path under that library.
- A shared specifier is `//` followed by the path under the shared root.
- A builtin module is written by its name, such as `@piton/belay`.
- Segments are always separated by `/`.

## Rewriting

When a move changes what a specifier has to say, it is rewritten in the style it was written in: relative stays relative, rooted stays rooted, a library path keeps its library, and shared stays shared. A builtin specifier is never rewritten. When the style can no longer reach the file, the specifier is left as it is, and the compiler reports it.

## Adding

When a name is imported on the author's behalf, by completion or by a quick fix, the specifier is chosen like this:

- When the file already imports from a module that exports the name, the name is added to that import, and the import is written the way `piton format` writes it.
- Otherwise every module that exports the symbol under that name is a candidate, whether it declares the symbol or re-exports it, except the file itself and the `index.pi` of any directory that contains the file.
- A candidate inside the project root is written relative or rooted, whichever has fewer segments, and relative when they tie.
- A candidate outside the root is written shared when it is under the shared root, through its library when it is under a library, and relative otherwise.
- The candidate with the fewest segments is chosen, a tie goes to the module that declares the symbol over one that re-exports it, and a remaining tie goes to the specifier that sorts first.
- Segments are path components, where each `..` counts as one, `.` counts as none, and a library's name counts as one.

## Placement

A new import is placed after the file's last import, re-export, or `use` line. A new `use` line is placed after the file's last `use` line, or above its first import when it has none. A file with none of these gets the new line at the top, followed by a blank line.
