# TODO — Piton spec vs. implementation gaps

What still differs between the implementation (`crates/`, `editors/`,
`packages/`, `xtask`) and the spec in `spec/`. First written 2026-09-22;
rewritten 2026-09-23 after that day's work. Each open item was checked against
the code or by running `piton`.

## Open — compiler and language

- [ ] **Invalid braces are text, not errors.** `{1 +}`, `{l[0]}` and an empty
  `${}` compile to literal text with a `braces-as-text` warning. The spec says
  an invalid expression is an error (`StandardExpression.pi`). Decide which
  way, and settle what `${}` means (the Inference table only says "string").
- [ ] **Unimplemented abstracts.** An abstract nothing extends compiles
  without complaint. The spec's "it must be extended by a non-abstract anchor"
  (`Abstract.pi`) could mean an error; today it only means "no output".
- [ ] **Unary minus on expressions** (`{-y}`) works but isn't in the operator
  or precedence tables.
- [ ] **Comment rule.** `//` only starts a comment at line start or after
  whitespace, so `foo//bar` and URLs are text. Sensible, but not in the spec.
- [ ] **Import sorting.** `piton format` sorts names inside a declaration but
  never reorders import statements. The spec's "it will sort the imports" is
  ambiguous.

## Open — renderers and build output

- [ ] **Reference renderer option.** `Reference.pi` says "You can change this
  with a renderer option". Not implemented.
- [ ] **JSON/YAML reference fallbacks.** When there is no dot path, the spec
  wants `../file.txt:Something`, then a line number (`../file.json:42`). Only
  the dot-path form exists.

## Open — Belay

- [ ] **Reference identity** is an open decision in `Decisions.pi`, so a
  construct that is also referenced gets a `reference-identity-unspecified`
  warning and a link to its reference-tree copy. Revisit once the spec
  decides.
- [ ] **Per-target output boundaries.** Only "inside the project root" is
  checked, not each target's own roots (`BuildGuarantees`).
- [ ] **Golden tests cover claude-code only.** Nothing snapshots the Codex or
  OpenCode trees.

## Open — LSP and editors

- [ ] **Incremental analysis** only debounces edits (250 ms) and then
  recompiles the workspace. Real incremental work needs a parse/analysis cache
  API in `piton-core`/`piton-compile` first.
- [ ] **Helix** can't express the "Enter on a blank line dedents" rule.
- [ ] **Zed, Kate and JetBrains** get the blank-line rule only from the
  server's on-type formatting, so it depends on the client asking for it.
- [ ] **JetBrains** has no native `jetbrains` highlighter. It uses the VS Code
  TextMate bundle plus LSP4IJ; a real plugin is still to do.

## Open — packages/vite-plugin-piton

- [ ] The TypeScript Markdown renderer can't tell anchors from dictionaries
  (or implicit lists) in compiled JSON, so it renders every nested object as a
  dictionary. `renderFile` through the compiler is exact.

## Open — code hygiene

- [ ] `RESERVED_WORDS` in `piton-syntax/src/kind.rs` still says reserved words
  can't be property names. They can now. Fix the comment or drop the copy
  (`piton-core/src/names.rs` has the real one).
- [ ] `Outcome::mentioned` in `eval.rs` still describes quotes as syntax that
  gets stripped. Quotes are ordinary characters now; remove it or rewrite it.
- [ ] Unspecified extras to spec or drop: `build --dry-run`, `tether --as`,
  `agent` argument passthrough, `reach --no-paths/--no-unreachable`, `format`
  with no path or `-`, `format_number` NaN/Infinity, `xtask publish-grammar`.

## Done since 2026-09-22

Checked and closed, or made moot by spec changes:

- Language: conflicting abstract constraints error (`conflicting-abstracts`);
  multiple abstracts allowed; `${list}`/`${dict}` give their names;
  `extends T[]` needs concrete anchors; constraint spacing checked in blocks;
  booleans and null never coerce to strings; typed literals keep their text
  (`x:: string: 1.0` → `"1.0"`); `x:: string:: null: null` gives null;
  `null + 1`, `1 ++ 2` error; `.5` and `1e3` aren't numbers; `+`/`++` dispatch
  and block `+`/`++` lines follow the spec; `super` searches every base;
  inherited constraints can't be redeclared; `@{...}` on a non-anchor is an
  error, and references can point at properties; key charset and reserved
  words as keys match the spec; quotes are ordinary characters; `compile`
  emits exports only; `inheritance-cycle` shows the cycle.
- CLI: `reach` follows imports and shows paths by default; `compile
  --renderer`; `piton format -`; build writes renderer output to `output`;
  unknown config keys reported; `agent` no longer builds first.
- Packages: newest commit wins; `piton-package` keyword with flat names; pins
  are strings, one per URL; project and package dependencies are separate;
  lock file is `.piton/tether.lock`.
- Belay: adapters come from the spec (`ClaudeCodeAdapter`, `CodexAdapter`,
  `OpenCodeAdapter`); `BELAY_COMPILED_SHAPE`; Codex override shadowing and byte
  limit; OpenCode unguaranteed scope; name collisions; option values
  validated; unsupported options are errors; description range 1–1024;
  per-adapter required descriptions; literal "You are a" and "Use when";
  `crossDiscovery` required when discovery changes activation;
  `.piton/targets.json` records the target versions; duplicated content check;
  reference tree is one file per module with `#fragment` links.
- LSP: import organization, composition resolution, conflict detection,
  source-to-output mapping, unused and redundant definitions, ambiguous
  references with a qualify action, inheritance-conflict action, override
  navigation both ways, compiled-output preview in any renderer. Signature
  help was removed (not in the spec).
- Editors: Neovim loads the Vim highlighter and has a README.
- Docs: README, FLUENCY_PROMPT.md and the embedded fluency copy regenerated.
