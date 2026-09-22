# TODO — Piton spec vs. implementation gaps

Gap analysis of the implementation (`crates/`, `editors/`, `packages/`, `xtask`) against the
spec in `.claude/reference/scope/` (language, tooling, Belay, package management) and the
acceptance corpus in `spec/`. Generated 2026-09-22.

Every section starts with its checkbox list; the evidence and detail follow below it. The
`## Checklist` block at the top of this file is the short-form index of all items.

Legend: **MISSING** = required by spec, absent · **PARTIAL** = present but incomplete ·
**DIVERGENT** = implemented differently than written.

---

## Checklist

**Missing — language & compiler**

- [ ] L1 Reject abstract anchors left with no concrete implementer
- [ ] L2 Error on type-constraint conflicts across multiple abstract inheritance
- [ ] L3 Accept empty `${}` as a string expression
- [x] L4 Implement `${list}` / `${dict}` string conversions

**Missing — CLI, packages, LSP**

- [ ] T1 `reach`: produce `imports` traversal edges
- [ ] T2 Package management: project-level version pins inherited by package dependencies
- [x] T3 LSP ImportOrganization
- [x] T4 LSP CompositionResolution
- [x] T5 LSP ConflictDetection
- [x] L6 LSP SourceToOutputMapping
- [x] T7 LSP UnusedSymbolDetection + DuplicateRedundantDefinitionDetection

**Missing — Belay adapters**

- [ ] B1 Codex: diagnose `AGENTS.override.md` shadowing generated guidance
- [ ] B2 Codex: account for the configured instruction byte limit
- [ ] B3 Codex: report shadowed / over-budget instruction output before it is "usable"
- [ ] B4 OpenCode: diagnose scope behavior that cannot be established for the target version

**Partial**

- [ ] P1 Key charset: accept any space-free string-coercible key
- [ ] P2 `::`/`:` spacing violations must error in blocks too, not only at file top level
- [ ] P3 Quoted values inside inline lists keep their quoted identity under coercion
- [ ] P4 Attempt TypeCoercion before raising operator / type errors
- [ ] P5 `@{...}` in Markdown: no plain-name fallback; link to the specific anchor on shared files
- [ ] P6 `extends T[]` must only be satisfied by concrete anchors
- [ ] P7 `check` circular-dependency coverage (module cycles) + show the cycle path
- [ ] P8 `reach` default report includes `paths`
- [ ] P9 Validate unknown project-config keys (incl. the "output directories" the spec promises)
- [ ] P10 LSP SignatureHelp: defaults, per-field documentation, active parameter
- [ ] P11 LSP CodeActions: add the two missing fixes (qualify ambiguous ref, resolve simple conflict)
- [ ] P12 LSP OverrideTracking: navigate base → overriding declarations too
- [ ] P13 LSP ReferenceResolution: diagnose ambiguous references
- [ ] P14 LSP ExpressionTypeInformation: per-expression types + mismatch warnings
- [ ] P15 LSP ProvenanceInspection: composition-step and expression provenance
- [ ] P16 LSP CompiledOutputPreview: JSON / YAML / Belay-target previews, not just Markdown hover
- [ ] P17 LSP RelatedSymbolNavigation: composition-based navigation
- [ ] P18 Belay validation: "metadata and body emitted exactly once"
- [ ] P19 Belay validation: native option _values_ against the target schema
- [ ] P20 Belay: record the target version used to validate artifacts (un-dead `documentation_checked`)
- [ ] P21 Belay: enforce the skill description minimum (1 char), not just the 1024 ceiling
- [ ] P22 Belay: hard-fail unrepresentable _required_ behavior instead of warning-and-drop
- [ ] P23 Belay: `collect_references` recurses into embedded `Value::Anchor` values
- [ ] P24 Belay: per-target output-boundary containment check
- [ ] P25 Belay: require an explicit deployment choice when cross-discovery changes behavior
- [ ] P26 JetBrains: ship a plugin and the spec'd `jetbrains` highlighter
- [ ] P27 NeoVim: wire the `vim` highlighter into `piton.lua`; add a README
- [ ] P28 README accuracy: `git` requirement, `reach --paths`, `tether --as`, `untether --as/--no-rewrite`

**Divergent**

- [ ] D1 Dependency conflicts: implement "most recent version wins" (or fix the spec)
- [ ] D2 `untether`: rewrite imports through the LSP, not textually
- [ ] D3 Frameworks: gate `@piton/*` keywords/imports on the `frameworks` config entry
- [ ] D4 LSP incremental analysis instead of full specbase recompile per keystroke
- [ ] D5 `BELAY_CODE_ROOT` falls back to the project root (and `codeRoot` optional)
- [ ] D6 Same-normalized skill identity: emit a collision diagnostic (ClaudeCode `checks`)
- [ ] D7 `@{...}` non-anchor result is an error, not a warning + value fallback
- [ ] D8 Invalid `{...}` expressions are errors, not `braces-as-text` warnings
- [ ] D9 Booleans must not coerce to strings (resolves S1)
- [ ] D10 `null + 1`, `1 ++ 2`, `{Anchor} + "x"` must error per "Unsupported Operators"
- [ ] D11 Leading zero mandatory for decimals (`.14` invalid); tighten `42_` / `1__2`
- [ ] D12 `extends` constraint legal only inside abstract anchors (resolves S3)
- [ ] D13 Config discovery in the current working directory only (no ancestor walk)
- [ ] D14 Agent body: literal `You are a` + role (no article rewriting)
- [ ] D15 Skill description: literal `description` + `Use when` + `useWhen` (no `". "` insertion)
- [ ] D16 `format` `path` argument contract (required positional per spec vs optional here)
- [ ] D17 Decide `piton format` import sorting semantics (statement order vs names-in-decl)
- [ ] D18 Decide the comment trigger rule (`foo//bar`) and document it

**Spec self-contradictions to resolve (spec-side work)**

- [ ] S1 Boolean↔string coercion: `TypeCoercion.md` forbids, `Booleans.md` requires
- [ ] S2 `{this.booleanTypes} && false` example unreachable under `Expressions.md`
- [ ] S3 Keyword reservation vs "Valid Keys" (`string`, `false`, `null`)
- [ ] S4 Circular imports: forbidden (`Cli.md` LSP list) vs legal (`CircularImports.md`)
- [ ] S5 Per-type "Supported Operators" vs `ComparisonOperators.md` / `Null.md`
- [ ] S6 "An abstract anchor alone will never compile": must-be-extended vs emits-no-output
- [ ] S7 Truthiness used by ternary/logical operators but never defined
- [ ] S8 Belay `Unresolved` section: reference identity, anchor stringification, metadata schemas, naming

**Scope creep — document in the spec or drop**

- [ ] C1 `pass` keyword
- [ ] C2 Unary negation `-x`
- [ ] C3 Quoted-paragraph "mention" semantics
- [ ] C4 `@`-packages / bundled preludes (`@piton/config`, `@piton/packaging`, `@piton/belay`)
- [ ] C5 `tether --as`, `build --dry-run`, `agent` arg passthrough, `reach` flags, `format` pathless mode
- [ ] C6 JSON `$ref` reference encoding + generated Markdown link footer
- [ ] C7 `format_number` NaN/Infinity handling
- [ ] C8 Mixed-block JSON grouping of adjacent `key: value` runs
- [ ] C9 `xtask publish-grammar` + Zed grammar pinning
- [ ] C10 Dead `RESERVED_WORDS` / `is_reserved` machinery (keep, wire up, or delete)

---

## Missing — language & compiler

### Checklist

- [ ] **L1** Reject abstract anchors left with no concrete implementer — `crates/piton-compile/src/resolve.rs`
- [ ] **L2** Error on type-constraint conflicts across multiple abstract inheritance — `crates/piton-compile/src/resolve.rs` (`build_slots`)
- [ ] **L3** Accept empty `${}` as a string expression — `crates/piton-syntax/src/prose.rs`, `expr.rs`
- [ ] **L4** Implement `${list}` / `${dict}` string conversions — `crates/piton-compile/src/eval.rs` (`stringify`)

### Detail

| #   | Requirement                                                                                                           | Spec                                                    | Evidence                                                                                                                                                                                   |
| --- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| L1  | An abstract anchor with no concrete implementer must fail compilation ("an abstract anchor alone will never compile") | `anchors/Abstract.md`                                   | Only `unimplemented-property` / `multiple-abstract-bases` are checked; an unextended abstract compiles clean                                                                               |
| L2  | Type-constraint conflicts across multiple abstract inheritance must throw a compiler error                            | `anchors/Abstract.md`                                   | `build_slots` resolves collisions silently ("right-most wins") for every anchor; no diagnostic exists                                                                                      |
| L3  | `${}` (empty interpolation) is a valid string expression                                                              | `types/Inference.md`, `expressions/StringExpression.md` | `prose.rs` swallows the `empty-expression` error and emits literal text `${}` with only a warning; the tree-sitter grammar _does_ accept the empty form — the two implementations disagree |
| L4  | `${list}` → string representation, `${dict}` → the owning variable/property's name                                    | `expressions/StringExpression.md → Conversion`          | `eval.rs stringify` raises `no-string-form` for `List`, `Mixed`, `Dict`; the defined conversion is unimplemented                                                                           |

---

## Missing — CLI, packages, LSP

### Checklist

- [ ] **T1** `reach`: construct `EdgeKind::Import` so traversal kind `imports` works — `crates/piton-compile/src/reach.rs`
- [ ] **T2** Inherit project-level version pins into package dependencies — `crates/piton-cli/src/cli/packages.rs` (`Installer::add`), `crates/piton-compile/src/packages.rs`
- [ ] **T3** LSP ImportOrganization (unused / duplicate / invalid / broad imports + clean action) — `crates/piton-lsp/src/features.rs`
- [ ] **T4** LSP CompositionResolution (show how `+` / `++` inputs combine) — `crates/piton-lsp/src/features.rs`
- [ ] **T5** LSP ConflictDetection (incompatible inherited/composed values, incl. Belay plan checks) — `crates/piton-lsp/src/`
- [ ] **T6** LSP SourceToOutputMapping — `crates/piton-lsp/src/`
- [ ] **T7** LSP UnusedSymbolDetection + DuplicateRedundantDefinitionDetection — `crates/piton-lsp/src/`

### Detail

| #   | Requirement                                                                 | Spec                                             | Evidence                                                                                                                      |
| --- | --------------------------------------------------------------------------- | ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| T1  | `reach` traversal kind `imports`                                            | `cli/Cli.md → Reach` (Reach.pi)                  | `EdgeKind::Import` is never constructed; `via imports` can never appear                                                       |
| T2  | "if not specified at a package level, those version pins will be inherited" | `PackageManagement.md → Packages → Dependencies` | `Installer::add` installs each package dep at `Pin::Default`; `piton tether` of one repo never consults project pins          |
| T3  | ImportOrganization                                                          | `cli/Cli.md → Lsp`                               | grep `organiz*` in `piton-lsp` → no matches                                                                                   |
| T4  | CompositionResolution                                                       | `cli/Cli.md → Lsp`                               | no composition code in `piton-lsp`                                                                                            |
| T5  | ConflictDetection                                                           | `cli/Cli.md → Lsp`                               | Belay plan validation exists only in `build`/`agent`, never in the editor; nothing reports inherited/composed value conflicts |
| T6  | SourceToOutputMapping                                                       | `cli/Cli.md → Lsp`                               | grep `mapping` in `piton-lsp` → no matches                                                                                    |
| T7  | UnusedSymbolDetection, DuplicateRedundantDefinitionDetection                | `cli/Cli.md → Lsp`                               | grep `unused`/`redundant` → no matches (`piton reach` reports unreachable anchors but nothing warns in-editor)                |

---

## Missing — Belay adapters

### Checklist

- [ ] **B1** Codex: diagnose a same-directory `AGENTS.override.md` that shadows generated guidance — `crates/piton-belay/src/lib.rs` (`validate` / instruction collection)
- [ ] **B2** Codex: account for the configured instruction byte limit — `crates/piton-belay/src/lib.rs` (`collect_instruction`)
- [ ] **B3** Codex: report shadowed / over-budget instruction output before it is called usable — `crates/piton-belay/src/lib.rs` (`validate`)
- [ ] **B4** OpenCode: diagnose required scope behavior that cannot be established for the target version — `crates/piton-belay/src/lib.rs` (`validate`)

### Detail

| #   | Requirement                                                                              | Spec                                                      | Evidence                                                                                        |
| --- | ---------------------------------------------------------------------------------------- | --------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| B1  | "Diagnose a same-directory `AGENTS.override.md` that shadows generated guidance"         | `Belay.md → Codex instruction.rules`                      | Nothing in `crates/` reads or diagnoses override files                                          |
| B2  | "Account for the configured instruction byte limit"                                      | `Belay.md → Codex instruction.rules`                      | No byte-limit concept anywhere in `crates/`; `collect_instruction` concatenates unconditionally |
| B3  | "Shadowed or over-budget instruction output is reported before being called usable"      | `Belay.md → Codex checks`                                 | No diagnostic exists (consequence of B1/B2)                                                     |
| B4  | "Required scope behavior that cannot be established for the target version is diagnosed" | `Belay.md → OpenCode checks` + `instruction.openQuestion` | `validate()` contains nothing about activation / scope behavior                                 |

---

## Partial

### Checklist

See the master checklist (`P1`–`P28`); each detail row below carries its fix location.

### Detail

**Language & compiler**

| #   | Gap                                                                                                                                                                                                             | Spec                                                                            | Evidence / fix location                                                                                                     |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| P1  | Key charset: spec says "anything that can support TypeCoercion to a string … so long as it doesn't contain spaces"; the parser accepts only `alnum \| _ \| -`, so `3.14: x`, `foo.bar: x` silently become prose | `types/Dictionaries.md → Valid Keys`, `types/Anchors.md → Valid Keys`           | `parser.rs` `parse_property_header`; the tree-sitter grammar is closer to the spec                                          |
| P2  | `myVariable::number: 42` must raise a compiler error — it does at file top level but silently becomes prose inside a block                                                                                      | `variables/TypeConstraints.md`                                                  | `parser.rs` `parse_block` ("anything else is prose") with no diagnostic                                                     |
| P3  | Quoted values are never coerced; `"42"` inside `["42"]` under `:: number[]` becomes `[42]`                                                                                                                      | `variables/TypeCoercion.md`                                                     | `eval.rs` `value_node` inline-list branch drops `Evaluated.quoted`; `coerce` `[]` branch fabricates `quoted: false`         |
| P4  | "Before throwing an error, the compiler will follow the rules of TypeCoercion and Inference" — no coercion pass runs before operator/type errors                                                                | every type page → `Unsupported Operators`                                       | `eval.rs` `binary` / `compare`                                                                                              |
| P5  | `@{...}` must not silently become a plain name string; link to the specific anchor when several share an output file                                                                                            | `expressions/ReferenceExpression.md → Other Formats`, `Markdown → Requirements` | `piton-emit/src/markdown.rs` `reference_markup` falls back to `name.to_string()`; `MirroredLinks` has no fragment mechanism |
| P6  | `extends T[]` "may be satisfied by any **concrete** anchor" — abstracts satisfy it                                                                                                                              | `anchors/Abstract.md`                                                           | `store.rs` `inherits_from` true for `anchor == candidate`; `eval.rs` `Named`/`extends` lacks `is_concrete`                  |

**CLI / tooling**

| #   | Gap                                                                                                                                                    | Spec                                                      | Evidence / fix location                                                                                      |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| P7  | `check` "circular-dependencies": module import cycles are never reported (spec conflict S4); `inheritance-cycle` message does not show the cycle path  | `cli/Cli.md → Check`, `Lsp → CircularDependencyDetection` | `resolve.rs:637` message is only "X inherits from itself"; `eval.rs` `cyclic-value` does print `a -> b -> a` |
| P8  | `reach` `report` set includes `paths` by default; paths print only behind `--paths`                                                                    | `cli/Cli.md → Reach`                                      | `commands/reach.rs`, `main.rs`                                                                               |
| P9  | Unknown config keys are silently ignored — including the "output directories" `ProjectConfig.md`'s description promises (no such field is read at all) | `tooling/project-config/ProjectConfig.md`                 | `crates/piton-compile/src/config.rs`                                                                         |

**LSP (from the 33-item `Cli.md → Lsp` feature list)**

| #   | Gap                                                                                                                                 | Evidence / fix location           |
| --- | ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| P10 | SignatureHelp shows slots + constraints + "required" only: no defaults, no per-field documentation, `active_parameter: None` always | `features.rs` signature help      |
| P11 | CodeActions: 3 of the 5 named fixes — missing "qualify ambiguous reference", "resolve simple inheritance conflict"                  | `features.rs` code actions        |
| P12 | OverrideTracking is one-way (property → supplying base); no path base → overriding declarations                                     | `features.rs` definition + inlays |
| P13 | ReferenceResolution: unresolved is reported, "ambiguous" never is                                                                   | `eval.rs`, `features.rs`          |
| P14 | ExpressionTypeInformation: `type-mismatch` on property slots only; no per-expression type display                                   | `features.rs` hover/inlays        |
| P15 | ProvenanceInspection: "Inherited from"/"Declared in" only; no composition-step or expression provenance                             | `features.rs` hover               |
| P16 | CompiledOutputPreview: hover renders Markdown only; no JSON / YAML / other Belay target preview, no command                         | `features.rs` hover               |
| P17 | RelatedSymbolNavigation: implementations + type hierarchy, nothing for compositions                                                 | `features.rs`                     |

**Belay / adapters**

| #   | Gap                                                                                                                                                                                                                       | Spec                                                               | Evidence / fix location                                                                |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| P18 | "Verify discovery metadata and body content are emitted exactly once" — no verification step, only structural exclusion                                                                                                   | `Belay.md → Adapters validation`                                   | `piton-belay/src/construct.rs` `primary_properties`; `validate()` has no such check    |
| P19 | "Validate native options against the selected platform version" — only option _names_ are checked against a static list, never values                                                                                     | `Belay.md → metadata.requirements`                                 | `piton-belay/src/adapter.rs` `*_options`; `lib.rs` `native_options`                    |
| P20 | "Record the target version used to validate the generated artifacts" — `documentation_checked` is defined and never emitted, checked, or read (and it records a date, not a version)                                      | `Belay.md → adapters requirements`, `Unresolved → Target Coverage` | `adapter.rs` `documentation_checked` (dead)                                            |
| P21 | Skill descriptions must be 1–1024 chars: only `> 1024` is rejected, an empty description passes                                                                                                                           | `Belay.md → OpenCode skill.rules`                                  | `lib.rs` `description-too-long`                                                        |
| P22 | "Fail with an actionable diagnostic when required behavior cannot be represented" — unsupported options are always downgraded to a warning and dropped                                                                    | `Belay.md → adapters requirements`                                 | `lib.rs` `unsupported-option`                                                          |
| P23 | Referential interpolation renders as a link for direct references only; `collect_references` does not recurse into embedded `Value::Anchor`, so nested `@{...}` renders as a bare name with no planned reference document | `Belay.md → Scope AnchorReferences`, `### Interpolation`           | `lib.rs` `collect_references` (the `Value::Anchor` arm is a no-op despite its comment) |
| P24 | "Keep resolved output paths within their configured output boundaries" — only the project root is checked, not per-target roots                                                                                           | `Decisions.pi` ProposedBuildGuarantees                             | `lib.rs` `output-outside-project`                                                      |
| P25 | "Require an explicit deployment choice when cross-discovery changes behavior" — warning + help text only; nothing requires a choice                                                                                       | `Belay.md → multipleAdapters.requirements`                         | `lib.rs` `cross-target-discovery`                                                      |

**Editors**

| #   | Gap                                                                                                                                                                                     | Spec                           | Evidence / fix location                                       |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ | ------------------------------------------------------------- |
| P26 | JetBrains: spec's `syntaxHighlighter` is `jetbrains` and requires an editing plugin; `editors/jetbrains/` is only a README reusing the VS Code TextMate bundle, with no plugin artifact | `tooling/editors/JetBrains.md` | `editors/jetbrains/README.md` ("Why there is no plugin here") |
| P27 | NeoVim: spec's highlighter is `vim`, but `piton.lua` ships/loads no highlighting (only tree-sitter + LSP); it is also the only editor dir with no README                                | `tooling/editors/NeoVim.md`    | `editors/neovim/piton.lua`                                    |

**Docs**

| #   | Gap                                                                                                                                                                                                       | Evidence / fix location                                                          |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| P28 | README says "There are no system dependencies" but `git` is required for `tether`/`update`; `reach` "with depth and path" (paths behind a flag); `tether --as`, `untether --as/--no-rewrite` undocumented | `README.md` vs `crates/piton-cli/src/cli/packages.rs:61-64`, `commands/reach.rs` |

---

## Divergent

### Checklist

See the master checklist (`D1`–`D18`); each detail row below carries its fix location.

### Detail

| #   | Spec says                                                                                                                      | Implementation does                                                                                                                                                | Evidence / fix location                                                |
| --- | ------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| D1  | Version conflicts resolve to "the most recent version"                                                                         | Installs the most _specific_ pin (commit > tag > branch > default); recency is never computed                                                                      | `piton-compile/src/packages.rs` `more_specific`; `cli/packages.rs:335` |
| D2  | Untether "the LSP should be used to rework any imports"                                                                        | Textual rewrite via parser spans (observable behavior matches)                                                                                                     | `cli/packages.rs` import rewriting                                     |
| D3  | Adding a framework to the config "makes those pieces available"; files must use/import what they need                          | `@piton/config`, `@piton/packaging`, `@piton/belay` (incl. keywords) resolve unconditionally — no `frameworks:` gating                                             | `module.rs` `resolve` → `prelude::is_package`; `prelude.rs`            |
| D4  | Incremental analysis: "only re-evaluate affected portions … rather than recompiling the entire specbase after every keystroke" | Every `did_change` recompiles every source under the root                                                                                                          | `piton-lsp/src/world.rs` `recompile` → `Compilation::build_workspace`  |
| D5  | `BELAY_CODE_ROOT` "if not specified, it will resolve to the project root"                                                      | `codeRoot` is a _required_ slot; loader fallback is `root/src`                                                                                                     | `prelude.rs` `BelayConfig`; `config.rs`                                |
| D6  | "Two constructs normalizing to the same skill identity cause a collision diagnostic"                                           | Identical-output duplicates are silently coalesced; `output-collision` fires only when contents differ (matches `collisionPolicy`, so the spec contradicts itself) | `piton-belay/src/lib.rs` `validate`                                    |
| D7  | `@{...}` that cannot resolve to a referenceable anchor: "Report an error"                                                      | Warning `reference-not-an-anchor` + fallback to the value                                                                                                          | `eval.rs` `reference()`                                                |
| D8  | Invalid `{...}` / `${...}` / `#{...}`: "Report an error when the expression is invalid"                                        | Expression diagnostics are discarded and the braces become literal text with a `braces-as-text` warning                                                            | `prose.rs` `scan_line`                                                 |
| D9  | Booleans are structural and "not coerced into unrelated types"                                                                 | `:: string: false` → `"true"`/`"false"` (see S1)                                                                                                                   | `eval.rs` `coerce` `String` arm                                        |
| D10 | Null/Anchors/References support _no_ operators; `++` only for lists/dicts — anything else errors after coercion                | `+`/`++` fall back to stringify-and-concat: `null + 1` → `"null1"`, `1 ++ 2` → `"12"`                                                                              | `eval.rs` `binary` `Add`/`Concat` catch-all                            |
| D11 | "Leading 0 is mandatory for decimals"                                                                                          | `.14` parses as `0.14`; `42_` → `42`, `1__2` → `12` (the tree-sitter grammar already enforces the rule)                                                            | `eval.rs` `parse_number_literal`; `expr.rs` `parse_number`             |
| D12 | "only within abstract anchor definitions" is `extends` a legal constraint                                                      | Accepted anywhere (note: the spec's own `.pi` corpus uses it at top level — see S3)                                                                                | `parser.rs` `parse_constraint`; `eval.rs` `coerce`                     |
| D13 | Config is found "in the current working directory"                                                                             | Walks up through all ancestors                                                                                                                                     | `config.rs` `find_config`                                              |
| D14 | Agent body "Begin the body with You are a followed by the role"                                                                | Article rewriting: role `an architect` → "You are an architect"                                                                                                    | `piton-belay/src/render.rs` `role_introduction`                        |
| D15 | Skill description is "description followed by Use when and useWhen"                                                            | Also inserts `". "` when the description lacks terminal punctuation                                                                                                | `piton-belay/src/render.rs` `discovery_description`                    |
| D16 | `format` `path` is a positional argument (required)                                                                            | Optional; defaults to the whole project (superset)                                                                                                                 | `piton-cli/src/main.rs`                                                |
| D17 | `piton format` "will … sort the imports" (ambiguous: statements or names?)                                                     | Sorts names _within_ each declaration only; import statements are never reordered                                                                                  | `piton-syntax/src/format.rs` `entries.sort()`                          |
| D18 | `//` starts a comment (unqualified rule)                                                                                       | Only at line start or after whitespace, so `foo//bar` is not a comment (keeps `https://…` working — defensible, but undocumented)                                  | `piton-syntax/src/prose.rs` `preceded_ok`                              |

---

## Spec self-contradictions to resolve

### Checklist

See the master checklist (`S1`–`S8`). These are spec-side edits; the implementation had to pick a side in each.

### Detail

1. **S1 Boolean↔string coercion** — `variables/TypeCoercion.md` forbids it ("booleans … are not coerced into unrelated types"); `types/Booleans.md → Supported Operators` grants `+` "adheres to rules of TypeCoercion", which requires it. Impl sides with `Booleans.md`.
2. **S2 `{this.booleanTypes} && false`** — `anchors/Anchors.md`'s documented output requires expression continuation past `}`; `expressions/Expressions.md` says unwrapped text is a string. The documented output is unreachable (impl follows `Expressions.md`).
3. **S3 Keyword reservation** — `overview/Keywords.md` reserves `string`/`false`; `types/Dictionaries.md → Valid Keys` makes `false`/`null` legal keys. Parser follows "Valid Keys"; `kind.rs`'s `RESERVED_WORDS` doc claims the other rule and is dead code.
4. **S4 Circular imports** — `cli/Cli.md → Lsp ModuleResolution` lists "forbidden circular imports"; `reuse/CircularImports.md` makes them legal (mutual `${A}`/`${B}` references). Impl follows the language spec.
5. **S5 Per-type "Supported Operators"** — literally read, `1 == 1` should error (`Numbers.md` lists only arithmetic), yet `operators/comparison/ComparisonOperators.md` defines `==` and `types/Null.md` asserts `null == null`. Impl allows `==`/`!=` everywhere.
6. **S6 "An abstract anchor alone will never compile"** — ambiguous between "must be extended" (missing, L1) and "produces no compiled output" (implemented: `compile.rs`, `construct.rs`).
7. **S7 Truthiness** — used by `operators/conditional/TernaryOperator.md` and `logical/LogicalOperators.md`, never defined. Impl defines it in `piton-core/src/value.rs` `is_truthy`.
8. **S8 Belay `Unresolved`** — the spec's own open questions: reference value type and cross-target identity, string interpolation of anchors (source name vs compiled name), metadata/tool/model schemas, naming normalization and collision handling, target version pinning. Also `NestedTypeConstraints.md` shows an example with no constraint syntax (the form lives only in the `.pi` corpus).

---

## Scope creep — document in the spec or drop

### Checklist

See the master checklist (`C1`–`C10`).

### Detail

| #   | Item                                                                                                                                                                                                            | Where                                    |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| C1  | `pass` keyword (used by the Belay corpus, defined in no language spec file)                                                                                                                                     | `kind.rs`, `parser.rs`, `grammar.js`     |
| C2  | Unary negation `-x` (absent from `operators/Operators.md → All Operators`)                                                                                                                                      | `ast.rs`, `expr.rs`                      |
| C3  | Quoted-paragraph "mention" semantics                                                                                                                                                                            | `eval.rs` `Outcome::mentioned`           |
| C4  | `@`-packages / bundled preludes and `__BELAY_SHAPE__` markers                                                                                                                                                   | `prelude.rs`, `packages.rs`, `module.rs` |
| C5  | `tether --as` (spec gives `tether` no named args), `build --dry-run`, `agent` trailing args, `reach` `targets`/`--unreachable`/`--paths`, `check`/`loc` `paths`, pathless `format`, `compile --adapter` default | `piton-cli/src/main.rs`                  |
| C6  | JSON `$ref` encoding and the generated Markdown link footer ("Links in this document…")                                                                                                                         | `piton-emit/src/json.rs`, `markdown.rs`  |
| C7  | `format_number` NaN → `"null"`, ±Infinity                                                                                                                                                                       | `piton-core/src/value.rs`                |
| C8  | Mixed-block JSON grouping of adjacent `key: value` runs                                                                                                                                                         | `value.rs` `as_list_items`               |
| C9  | `xtask publish-grammar`, Zed grammar pinning                                                                                                                                                                    | `xtask/src/main.rs`                      |
| C10 | Dead `RESERVED_WORDS` / `is_reserved` with contradictory docs (see S3)                                                                                                                                          | `piton-syntax/src/kind.rs`               |

Also unverified in tests: `crates/piton-belay/tests/golden.rs` snapshots only the **claude-code** target (self-snapshot of `.claude/`, with a staleness allowlist at `STALE_GOLDEN_LINES` whose 2 skipped lines still disagree with the spec), and nothing golden-checks the codex/opencode trees or the spec's own `examples` blocks (`Scope.pi` ShapeMapping, `Compilation.pi` serialization) — those live as hard-coded unit-test inputs, so editing the spec examples breaks no test.

---

## Bottom line

What is solid: the core language semantics pinned by worked examples (structural inheritance and
collision order, `self`/`this`/`super` incl. super-on-lists dedup, escaping, brace-required
expressions, `#{}`/`${}`/`@{}` sigils, ternary laziness, `use`/`from…import`/`export`/aliasing/
circular imports, the 4-space formatter), the claude-code Belay pipeline (constructs, serialization,
shape mapping, location exports, references, planning/cleanup), and the Vite and Astro plugins
(which implement every feature their spec pages name, with tests).

The real holes cluster in five places:

1. **Abstract-anchor enforcement** (L1, L2) and the **"Unsupported Operators + TypeCoercion"
   error policy** (D7–D10, P3, P4).
2. **Six named LSP features** (T3–T7) plus the partial ones (P10–P17) and **incremental
   analysis** (D4).
3. **Belay adapter rules**: Codex override shadowing + byte limits (B1–B3), OpenCode scope
   diagnostics (B4), and the once-only / version / schema validations (P18–P22).
4. **Package management**: pin inheritance (T2) and the "most recent version" rule (D1); plus
   `reach`'s `imports` edges (T1).
5. **Editors**: JetBrains (P26) and NeoVim's highlighter wiring (P27).
