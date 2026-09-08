# CLI reference

Every command exits non-zero when it reports an error.

## `piton build`

Builds the project described by `piton.config.pi`, found in the working
directory or any ancestor. Reads the entry point, compiles everything reachable
from it, and asks each configured framework to write its output.

```sh
piton build          # write the output
piton build check    # report problems without writing anything
```

## `piton check <path>...`

Reports problems in specific files without writing anything. Accepts files,
directories, and globs.

```sh
piton check spec/
piton check 'spec/**/*.pi'
```

The project is looked for beside the files you name, not beside your shell, so
checking a subdirectory that is its own project uses that project's `root` and
frameworks.

## `piton compile <path>...`

Compiles files to data. Each file becomes one document keyed by its top-level
names, because a Piton file has no single root value.

```sh
piton compile spec/Button.pi                  # writes spec/Button.json
piton compile spec/ --format yaml             # writes .yaml beside each input
piton compile spec/Button.pi --stdout         # prints instead of writing
piton compile spec/ --out-dir build/          # writes somewhere else
```

| Flag | Effect |
| --- | --- |
| `--format json\|yaml` | Output format, JSON by default |
| `--out-dir DIR` | Write here instead of beside each input |
| `--stdout` | Print instead of writing |

## `piton format <path>...`

Applies the canonical style: four spaces per level, one space after `//`, sorted
import lists wrapped once they exceed two names or eighty columns. It is not
configurable.

```sh
piton format .              # rewrite in place
piton format . --check      # list unformatted files, exit non-zero
cat x.pi | piton format -   # format standard input
```

Formatting never changes what a file compiles to, and keeps the line endings the
file already used.

## `piton lsp`

Runs the language server over stdio. Editors start this for you; see
[editor setup](editors.md).

## `piton docs`

Prints the language reference, generated from the compiler's own tables and from
every registered framework's own module source.

```sh
piton docs
piton docs --out docs/language.md
```

## `piton claude`

Launches Claude Code with a Piton and Belay fluency brief appended to its system
prompt, so it can read and write Piton without being taught first.

```sh
piton claude                    # launch
piton claude --print-prompt     # print the brief instead
piton claude --install          # install it as a reusable skill
piton claude -- --model opus    # anything else passes through
```

## `piton grammar [dir]`

Regenerates the editor integrations in `editors/` from the compiler's own token
tables. See [development](development.md).

## `piton ast <path>`

Prints a file's concrete syntax tree. Useful when a line is not being read the
way you expect — which, in a whitespace-structured language, happens.
