# Belay

## Description

A Piton framework for structured agent instructions, skills, commands, and agents, compiled into the formats used by agentic coding tools.

## Framework

- description: Belay is the framework bundled with Piton for describing agentic instructions, skills, commands, and agents, and compiling them into artifacts understood by configured agentic platforms.
  requirements:
    - Express reusable agent behavior as structured Piton source.
    - Connect descriptions of what should exist with guidance for building it.
    - Preserve composition through Piton anchors and inheritance.
    - Produce platform-specific artifacts through configured adapters.
    - Keep Piton and Belay open source independently of Suspense licensing.
- description: Piton source and project configuration define the intended system and agent guidance. Compiled artifacts are derived representations of that source, rather than an independent specification.
  requirements:
    - Make lasting specification changes in the originating Piton source.
    - Derive generated guidance from the resolved source and configuration.
    - Treat implementation as something to assess against the specification.
    - Distinguish predictable artifact generation from probabilistic agent behavior.
- description: Piton supplies language semantics, Belay supplies agentic vocabulary and output behavior, and Suspense supplies an authoring and orchestration environment around them.
  requirements:
    - Defer parsing and expression evaluation to Piton.
    - Defer imports, exports, inheritance, and reachability to Piton.
    - Use adapters to translate resolved Belay constructs into target artifacts.
    - Leave execution of generated instructions to the consuming agentic platform.
    - Do not require Suspense to author or compile a Belay project.
    - Do not treat successful compilation as proof of correct agent execution.
- description: A project enables Belay through the frameworks property of its Piton configuration, using an instance of belay-config.
  requirements:
    - Register the Belay configuration in the project frameworks list.
    - Use codeRoot to identify the application's source-code root.
    - Use shapeRoot, when configured, to identify the architectural instruction root.
    - Use adapters to select the output targets for the project.
    - Require individual files to use or import the framework definitions they need.
    - Do not interpret framework registration as an implicit import in every file.
  documentedNames:
    package: @piton/belay
    configurationKeyword: belay-config
    agentAdapterKeyword: belay-agent-adapter
    codeRoot: ./src/
    shapeRoot: ./spec/shape/
  qualification: These names document the existing configuration surface. This file does not redefine the configuration or adapter schemas, whose complete required fields are not established by the available material.
- description: Belay constructs are ordinary Piton anchors with framework-specific meaning. Their keyword forms provide reusable vocabulary without introducing a separate composition language.
  requirements:
    - Expose Agent, Instruction, Skill, and Command as the four agentic constructs.
    - Associate those anchors with agent, instruction, skill, and command.
    - Allow abstract anchors to provide shared structure and behavior.
    - Resolve inherited properties according to the Piton language specification.
    - Validate required properties on concrete constructs after inheritance resolves.
    - Preserve additional properties for serialization into the generated guidance.
    - Compile only source files reachable from the configured entrypoints.
  qualification: The four-construct surface does not exclude configuration anchors, adapters, or special compile-time exports. The older Reference and Instruction-only design is not used as the public construct model here.

## Anchors

- description: An instruction supplies persistent guidance associated with an application scope. Its placement is part of its meaning.
  requirements:
    - Require description and prompt strings.
    - Map instructions under shapeRoot to the corresponding codeRoot scope.
    - Combine instructions assigned to the same scope into one guidance file per target.
    - Use the target's supported scoped guidance filename and representation.
    - Also preserve shape instructions in the target's compiled shape-reference tree.
    - Serialize additional properties according to the common serialization rules.
- description: A skill is a selectively loaded set of instructions for a particular kind of work. Its discovery metadata explains when it is useful.
  requirements:
    - Require description, prompt, and useWhen strings.
    - Emit the skill into the configured target's skill directory and format.
    - Derive the skill name from the anchor using the adapter's naming rules.
    - Form the discovery description from description followed by Use when and useWhen.
    - Emit prompt as the primary body of the skill.
    - Serialize additional properties after the primary prompt.
    - Leave skill selection and loading to the consuming platform.
- description: A command provides an explicit entrypoint for invoking a prompt or directing work through skills and other guidance.
  requirements:
    - Require description and prompt strings.
    - Emit a native command or the explicit-invocation equivalent defined by the adapter.
    - Prefix the generated command name with x- to distinguish it from a skill.
    - Map description and prompt to the command representation selected by the adapter.
    - Emit allowed-tools and model metadata when supplied and supported by the adapter.
    - Serialize additional properties after the primary prompt.
- description: An agent describes a role and the instructions for performing it, expressed in the format understood by the target platform.
  requirements:
    - Require description, role, and prompt strings.
    - Emit the agent into the configured target's agent directory and format.
    - Use the kebab-case anchor name as agent identity unless the target requires another form.
    - Emit description as discovery metadata.
    - Begin the body with You are a followed by the role.
    - Emit prompt after the role introduction.
    - Map explicit tools and model settings only through supported target configuration fields.
    - Serialize additional properties after the primary prompt.

## Compilation

### Description

Resolve a Belay project and emit the artifacts selected by its adapters.

### Requirements

- Obtain reachable and validated constructs from the Piton compiler.
- Build a target-specific output plan for each configured adapter.
- Resolve instruction placement and reference destinations before rendering.
- Detect shared paths and cross-target discovery conflicts across the entire plan.
- Serialize content using the shared rules and each adapter's native format.
- Validate links, metadata, names, and output ownership before writing.

### Serialization

#### Description

Belay serializes resolved construct content as readable Markdown while preserving the structure of the source.

#### Requirements

- Render anchor names and prose-bearing property names as word-separated titles.
- Use heading levels to represent the hierarchy of prose-bearing properties.
- Use bold labels when the hierarchy exceeds Markdown's six heading levels.
- Serialize primitive values to their textual representation.
- Render explicit lists as Markdown lists with nested indentation.
- Render pure dictionaries as indentation-based structures inside code fences.
- Preserve the order of prose and structured content within implicit mixed lists.
- Render pure dictionaries embedded in mixed content as fenced structures.
- Apply target-specific frontmatter and body layout around the serialized content.

#### Examples

```
propertyTitle:
  sourceName: myProperty
  renderedTitle: My Property
primitiveText:
  booleanValue: false
  renderedBoolean: false
  numericValue: 42
  renderedNumber: 42
```

### Interpolation

#### Description

Piton evaluates expressions; the selected output mode determines how their results appear in generated artifacts.

#### Requirements

- Follow the current Piton specification for expression evaluation and casts.
- Preserve the distinction between intrinsic values, strings, and references.
- Render Belay references as links to the applicable compiled target.
- Do not infer reference identity from a display heading alone.

#### Historical Behavior

The September 11 text describes string interpolation of a complex anchor as its compiled name without title expansion. Subsequent discussion revisits name, value, and reference semantics. Final behavior must be reconciled with the current language specification.

## Scope

- description: The shape tree approximately mirrors the application source tree. Instructions attach to the closest existing implementation scope.
  requirements:
    - Determine an instruction's relative directory beneath shapeRoot.
    - Resolve that relative directory beneath codeRoot.
    - Place scoped guidance there when the corresponding directory exists.
    - Otherwise walk upward to the nearest existing corresponding directory.
    - Stop fallback at codeRoot.
    - Combine all instructions resolving to the same destination scope.
    - Preserve the original relative structure in the compiled shape-reference tree.
  examples:
    matchingDirectory:
      source: spec/shape/components/button/Button.pi
      existingScope: src/components/button
      genericOutput: src/components/button/AGENTS.md
    sharedDirectory:
      sources:
        - spec/shape/components/input/Input.pi
        - spec/shape/components/input/InputDesign.pi
      existingScope: src/components/input
      genericOutput: src/components/input/AGENTS.md
    missingDirectory:
      source: spec/shape/components/nonexistent/Component.pi
      existingScope: src/components
      genericOutput: src/components/AGENTS.md
  qualification: AGENTS.md illustrates generic placement. Each adapter selects its supported filename. Fallback changes scoped placement, not the original location preserved in the shape-reference tree.
- description: The special export __BELAY_SHAPE__ identifies the compiled shape root for the current output target.
  requirements:
    - Make __BELAY_SHAPE__ available as an explicit import from Belay.
    - Resolve it during compilation rather than at agent runtime.
    - Resolve its path relative to the generated file containing its use.
    - Resolve it separately for each adapter and output location.
  documentedClaudeLocation: .claude/reference/shape
- description: A reference connects generated guidance to another anchor's compiled representation without embedding all of its content at the use site.
  requirements:
    - Serialize reached, referenced anchors into the target's reference directory.
    - Preserve their relative source structure in that directory.
    - Render referential interpolation as a Markdown link to the compiled artifact.
    - Keep referential links distinct from inline value serialization.
    - Do not replace lazy Markdown links with Claude-specific eager import syntax.
  qualification: The documented referential interpolation uses the at-sign sigil. Later discussions leave its underlying reference type and non-Markdown representation unresolved; this specifies only the Belay Markdown intent.

## Adapters

- description: Compile Belay constructs into project-local Claude Code artifacts.
  targetId: claude-code
  instructionFile: CLAUDE.md
  referenceRoot: .claude/reference
  status: proposed Belay adapter contract
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve __BELAY_SHAPE__ to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the application project root, while scoped instruction destinations are resolved through codeRoot and shapeRoot. The project-root configuration mechanism remains open.
    proposedConvention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter's mapping. Their placement in author-facing Piton configuration remains open.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: Check the complete output plan before writing. Codex and OpenCode can share AGENTS.md paths, and OpenCode can discover other adapters' skills. File separation alone does not guarantee target isolation.
    requirements:
      - Reject incompatible writes to a shared instruction file.
      - Coalesce identical shared guidance only once.
      - Diagnose duplicate discoverable skills across enabled adapters.
      - Require an explicit deployment choice when cross-discovery changes behavior.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: CLAUDE.md at the scope selected by ShapeMapping
    format: Markdown
    rules:
      - Combine guidance for the same scope in one file.
      - Preserve a separate compiled shape reference beneath .claude/reference/shape.
    loading: Ancestor guidance loads at startup; nested guidance loads when Claude reads within that subtree. Placement does not make every nested instruction part of the initial context.
  skill:
    output: .claude/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
    rules:
      - Use the same normalized name for the directory and metadata.
      - Preserve native allowed-tools and model options only when explicitly configured.
  command:
    support: translated to a manually invoked Claude skill
    output: .claude/skills/x-<name>/SKILL.md
    metadata:
      name: x- followed by the normalized construct name
      description: Command description
      disable-model-invocation: true
    body: Command prompt followed by serialized additional content
    invocation: /x-<name>
    rationale: Claude unifies custom commands with skills. The older commands directory remains supported, but this adapter chooses skills for new output. Do not emit both representations of one command.
  agent:
    output: .claude/agents/<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Agent description
      tools: Explicit native tool list when configured
      model: Explicit native model selector when configured
    body: Role introduction followed by prompt and serialized additional content
    rules:
      - Preserve agent identity independently of display headings.
      - Omit optional settings when absent so native defaults remain effective.
  permissions:
    description: Skill allowed-tools grants and subagent tool selection serve different purposes. Do not interpret either as a portable sandbox definition or copy it into another platform without a mapping.
  checks:
    - A command produces one x-prefixed skill with automatic invocation disabled.
    - Two constructs normalizing to the same skill identity cause a collision diagnostic.
    - Nested instructions retain their mapped scope and target-relative links.
- description: Compile Belay constructs into project-local Codex artifacts.
  targetId: codex
  instructionFile: AGENTS.md
  referenceRoot: .codex/reference
  status: proposed Belay adapter contract
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve __BELAY_SHAPE__ to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the application project root, while scoped instruction destinations are resolved through codeRoot and shapeRoot. The project-root configuration mechanism remains open.
    proposedConvention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter's mapping. Their placement in author-facing Piton configuration remains open.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: Check the complete output plan before writing. Codex and OpenCode can share AGENTS.md paths, and OpenCode can discover other adapters' skills. File separation alone does not guarantee target isolation.
    requirements:
      - Reject incompatible writes to a shared instruction file.
      - Coalesce identical shared guidance only once.
      - Diagnose duplicate discoverable skills across enabled adapters.
      - Require an explicit deployment choice when cross-discovery changes behavior.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: AGENTS.md at the scope selected by ShapeMapping
    format: Markdown
    rules:
      - Diagnose a same-directory AGENTS.override.md that shadows generated guidance.
      - Account for the configured instruction byte limit.
      - Do not generate override files merely to win precedence.
    loading: Codex builds a root-to-working-directory instruction chain at run start. Descendant placement does not guarantee initial loading from a session started above it.
  skill:
    output: .agents/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
  command:
    support: translated to an explicitly invoked Codex skill
    output: .agents/skills/x-<name>/SKILL.md
    policyOutput: .agents/skills/x-<name>/agents/openai.yaml
    metadata:
      name: x- followed by the normalized construct name
      description: Command description
    policy:
      allow_implicit_invocation: false
    body: Command prompt followed by serialized additional content
    invocation: $x-<name>
    rules:
      - Nest allow_implicit_invocation under policy in the generated YAML.
      - Do not emit deprecated personal custom prompts as repository commands.
      - Do not invent a project-local .codex/commands discovery directory.
  agent:
    output: .codex/agents/<name>.toml
    format: TOML
    fields:
      name: Normalized construct name
      description: Agent description
      developer_instructions: Role introduction, prompt, and serialized additional content
    optionalFields:
      - model
      - model_reasoning_effort
      - sandbox_mode
    rules:
      - Serialize the complete instruction body as a valid TOML string.
      - Omit absent options rather than imposing model or sandbox defaults.
      - Do not translate a generic tool list into an invented tools field.
    qualification: This targets the documented standalone custom-agent format, not older role-registration layouts or every hosted Codex surface.
  permissions:
    description: Native sandbox and approval configuration govern execution. Generated guidance cannot override the active runtime's controls. Unsupported per-command tool or model settings must be diagnosed.
  checks:
    - Commands produce both the skill and its explicit-invocation policy file.
    - Agent output parses as TOML and contains the three required identity and instruction fields.
    - Shadowed or over-budget instruction output is reported before being called usable.
- description: Compile Belay constructs into project-local OpenCode artifacts.
  targetId: opencode
  instructionFile: AGENTS.md
  referenceRoot: .opencode/reference
  status: proposed Belay adapter contract
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve __BELAY_SHAPE__ to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the application project root, while scoped instruction destinations are resolved through codeRoot and shapeRoot. The project-root configuration mechanism remains open.
    proposedConvention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter's mapping. Their placement in author-facing Piton configuration remains open.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: Check the complete output plan before writing. Codex and OpenCode can share AGENTS.md paths, and OpenCode can discover other adapters' skills. File separation alone does not guarantee target isolation.
    requirements:
      - Reject incompatible writes to a shared instruction file.
      - Coalesce identical shared guidance only once.
      - Diagnose duplicate discoverable skills across enabled adapters.
      - Require an explicit deployment choice when cross-discovery changes behavior.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: AGENTS.md at the scope selected by ShapeMapping
    format: Markdown
    loading: OpenCode searches local rule files upward from the working directory and prefers AGENTS.md over its CLAUDE.md fallback. Do not assume Claude's nested-loading behavior or Codex's chain.
    rules:
      - Preserve scoped output placement without claiming identical activation across tools.
      - Use explicit instructions configuration when the selected deployment needs additional files.
      - Do not load all nested instructions globally merely to make them discoverable.
    openQuestion: Confirm nested-file activation for the pinned OpenCode release before promising shape-scope behavior beyond documented startup discovery.
  skill:
    output: .opencode/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
    rules:
      - Require a name of 1 to 64 lowercase alphanumeric characters with single hyphen separators.
      - Match name metadata to the containing directory.
      - Reject descriptions outside the supported 1 to 1024 character range.
      - Do not rely on unrecognized frontmatter to enforce policy.
    discovery: OpenCode also discovers compatible Claude and .agents skill directories. Check cross-adapter identities before installing several generated skill trees into one project.
  command:
    support: native
    output: .opencode/commands/x-<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      description: Command description
      agent: Explicit target agent when configured
      model: Explicit provider-qualified model when configured
    body: Command prompt followed by serialized additional content
    invocation: /x-<name>
    rules:
      - Derive command identity from the filename.
      - Preserve explicitly authored runtime argument placeholders.
      - Do not emit Claude-specific allowed-tools as an enforced command setting.
  agent:
    output: .opencode/agents/<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      description: Agent description
      mode: Explicit primary, subagent, or all selection
      model: Explicit provider-qualified model when configured
      permission: Explicit native permission map when configured
    body: Role introduction followed by prompt and serialized additional content
    proposedDefaultMode: subagent
    rules:
      - Derive agent identity from the filename.
      - Prefer native permission entries over the deprecated tools option.
      - Validate native options instead of copying metadata from another adapter.
  permissions:
    description: Map permissions only when their meanings are equivalent. Native allow, ask, and deny entries are not interchangeable with a prose tool list. Reject ambiguous conversions.
  checks:
    - A command keeps its x-prefixed filename and native frontmatter.
    - Agent mode is explicit in generated output.
    - Duplicate skills discovered through other adapters are reported.
    - Required scope behavior that cannot be established for the target version is diagnosed.

## Proposed Refinements

- description: Proposed rules that make the documented output behavior safe and reproducible enough to implement. These require adoption into the authoritative project specification.
  requirements:
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
  status: proposed
- description: Written instructions express intended behavior. Actual restrictions depend on enforcement by the consuming platform.
  requirements:
    - Distinguish advisory prompt text from platform-enforced permissions.
    - Claim enforcement only for controls the target platform actually applies.
    - Diagnose a requested enforced restriction that the adapter cannot represent.
  status: proposed

## Unresolved

### Description

Resolve these questions against the current Piton and Belay source before treating this consolidated draft as an executable contract.

### Language Syntax

Confirm whether the current foundational declaration remains anchor or has moved to spec, and confirm abstract export and keyword alias syntax.

### Reference Directory

Adapter.pi proposes a single reference directory with a shape subdirectory per target. Adopt or revise this convention before implementing it; earlier source mixed reference and references.

### Configuration

Define required and optional configuration properties, path bases, behavior without shapeRoot, and behavior when codeRoot does not exist.

### Instruction Scope

Define placement for instructions outside shapeRoot and the exact anchor-level emission rule within reachable source files.

### Reference Identity

Finalize the reference value type, cross-target identity, and behavior when an anchor has more than one generated representation.

### String Interpolation

Finalize whether stringifying an anchor yields its source name, compiled name, or another explicitly defined representation.

### Metadata Types

Specify tools, allowed-tools, and model types and their mapping for each adapter. The prose examples do not establish a complete schema.

### Naming

Specify skill filename normalization, command name normalization before the x- prefix, acronym splitting, and name-collision handling.

### Workflow Model

Decide whether workflows remain compositions of the four constructs. Do not add a workflow keyword solely from examples of agent processes.

### Target Coverage

The adapters directory now defines Claude Code, Codex, and OpenCode mappings. Pin supported tool versions before implementing those mappings; these documents specify proposed Belay behavior, not tested integrations.
