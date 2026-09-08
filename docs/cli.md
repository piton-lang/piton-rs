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

## `piton reach`

Lists which `.pi` files under the project root are reached from the entry point,
and which are not.

A file is reached if the entry point imports it, or something the entry point
reached imports it, all the way down. `from` and `use` both count. Being
imported is not enough on its own: if nothing reaches the file doing the
importing, neither of them is reached.

Only reached files are compiled. An unreached file is invisible — its errors are
never reported and nothing it declares exists — which is easy to do by accident,
by renaming a file or forgetting an `index.pi` entry, and hard to notice,
because nothing goes wrong.

```sh
piton reach                      # both halves, with a summary
piton reach <FILE>               # how this one file is reached, or why it is not
piton reach --show unreached     # just the ones that are not compiled
piton reach --show reached
piton reach --chains             # explicit chains instead of a tree
piton reach --strict             # exit non-zero if anything is unreached
piton reach --entry FILE         # measure from a file instead of the project entry
```

### Asking about one file

Naming a file answers a different question: not what is reached, but how *this*
got here. Every hop names the file and line that made it, so you can go and
change one:

```
$ piton reach spec/concept/ViewSettings.pi
concept/ViewSettings.pi is reached by 5 import(s):

  index.pi:7
      ./agent  ->  agent/index.pi
  agent/index.pi:1
      ./agents  ->  agent/agents/index.pi
  agent/agents/index.pi:1
      ./InteractionAuditor  ->  agent/agents/InteractionAuditor.pi
  agent/agents/InteractionAuditor.pi:3
      /concept  ->  concept/index.pi
  concept/index.pi:10
      ./ViewSettings  ->  concept/ViewSettings.pi
```

That last pair of hops is worth reading twice: one `from /concept import
SearchAndNavigation` reaches `ViewSettings.pi`, because `/concept` resolves to
`concept/index.pi` and that file re-exports everything beside it.

A file that is not reached gets the opposite answer — what would have had to
import it:

```
$ piton reach spec/concept/Draft.pi
concept/Draft.pi is NOT reached, so it is never compiled.

  It is imported by, none of which is reached either:

  concept/Scratch.pi:1    ./Draft

  Follow one of those up with `piton reach <that file>`.
```

or, when nothing imports it at all:

```
  Nothing imports it. Add it to an index.pi, or import it where it is needed.
```

The answer is always measured from an entry point, and the entry point is
printed first so it cannot be misread. Outside a project there is no entry
point, so the command says so rather than treating every file as its own and
reporting that everything is reached.

Watch for a directory import pulling in more than you expect: `from /concept`
resolves to `concept/index.pi`, and if that re-exports every file beside it,
importing one name reaches all of them. `--chains` shows exactly which edge did
it.

By default each file is printed under the file that pulled it in, so the
indentation is the chain read downwards:

```
reached (5)
  index.pi
    agent/index.pi
      agent/agents/index.pi
        agent/agents/InteractionAuditor.pi
          concept/index.pi

unreached (1)
  scratch/Draft.pi

5 of 6 file(s) reached from the entry point
an unreached file is never compiled, so its errors are never reported
```

`--chains` prints each route in full instead, which is easier to quote:

```
  index.pi  (entry point)
  index.pi -> agent/index.pi
  index.pi -> agent/index.pi -> agent/agents/index.pi
  index.pi -> agent/index.pi -> agent/agents/index.pi -> agent/agents/InteractionAuditor.pi
```

A file reachable several ways is shown by its shortest route.

`--strict` is for CI, where an orphaned file usually means someone forgot to add
it to an `index.pi`.

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
[editor setup](editors.md). `--stdio` is accepted and ignored, because several
editor clients append it to the command they are configured with.

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
