# Build Guarantees

## Description

Rules that keep Belay's output safe and repeatable.

## Requirements

- Produce identical bytes for identical source, configuration, directory state, and toolchain.
- Define a stable order for instructions combined into a shared output file.
- Emit metadata and primary body fields once rather than duplicating them in the remainder.
- Quote and escape frontmatter values according to the target format.
- Detect collisions caused by normalized names or competing adapter output paths.
- Report a missing generated reference target instead of emitting a broken link.
- Report unsupported target metadata instead of silently claiming it was enforced.
- Track generated files so cleanup cannot delete unrelated user-authored files.
- Report the originating source anchor and property when generation fails.
- Keep resolved output paths within their configured output boundaries.

# Permission Boundary

## Description

Written instructions express intended behavior. Actual restrictions depend on enforcement by the consuming platform.

## Requirements

- Distinguish advisory prompt text from platform-enforced permissions.
- Claim enforcement only for controls the target platform actually applies.
- Diagnose a requested enforced restriction that the adapter cannot represent.

# Open Decisions

## Description

Things that aren't decided yet. Tooling shouldn't guess here; it should say it's not specified.

## Instruction Scope

Define placement for instructions outside shapeRoot.

## Metadata Types

Specify tools, allowed-tools, and model types and their mapping for each adapter.

## Naming

Specify skill filename normalization, command name normalization before the x- prefix, acronym splitting, and name-collision handling.

## Workflow Model

Decide whether workflows remain compositions of the four constructs. Do not add a workflow keyword solely from examples of agent processes.
