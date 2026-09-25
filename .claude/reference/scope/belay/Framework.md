# Framework Purpose

## Description

Belay is the framework bundled with Piton for describing agentic instructions, skills, commands, and agents, and compiling them into artifacts understood by configured agentic platforms.

## Requirements

- Express reusable agent behavior as structured Piton source.
- Connect descriptions of what should exist with guidance for building it.
- Preserve composition through Piton anchors and inheritance.
- Produce platform-specific artifacts through configured adapters.

# Source Authority

## Description

Piton source and project configuration define the intended system and agent guidance. Compiled artifacts are derived representations of that source, rather than an independent specification.

## Requirements

- Make lasting specification changes in the originating Piton source.
- Derive generated guidance from the resolved source and configuration.
- Treat implementation as something to assess against the specification.
- Distinguish predictable artifact generation from probabilistic agent behavior.
- Load the four constructs from their own .pi files in this spec, so the compiler and the spec can't disagree.

# Framework Boundaries

## Description

Piton handles the language and the renderers. Belay adds the agentic pieces and the adapters that turn them into files for each platform.

## Requirements

- Defer parsing and expression evaluation to Piton.
- Defer imports, exports, inheritance, and reachability to Piton.
- Use adapters to translate resolved Belay constructs into target artifacts.
- Leave execution of generated instructions to the consuming agentic platform.
- Do not treat successful compilation as proof of correct agent execution.

# Project Configuration

## Description

A project turns on Belay by adding a belay-config anchor to the frameworks property of its Piton config.

## Requirements

- Register the Belay configuration in the project frameworks list.
- Use codeRoot to identify the application's source-code root.
- Use shapeRoot, when configured, to identify the architectural instruction root.
- Use adapters to select the output targets for the project.
- Require individual files to use or import the framework definitions they need.
- Do not interpret framework registration as an implicit import in every file.

## Package

@piton/belay

## Configuration

```
keyword: belay-config
fields:
  codeRoot: Required. Where the app's code lives, relative to the project config.
  shapeRoot: Optional. Where the shape instructions live. Without it, BELAY_SHAPE_ROOT is the project root and nothing is placed by shape.
  adapters: Required. The list of adapters to build for.
  crossDiscovery: Optional. separate when each tool's output is deployed apart, so every adapter writes its own complete tree; allow to accept that one tool offers another tool's command skills as ordinary skills. Without it, the enabled tools share one project.
  instructionByteLimit: Optional. The instruction byte limit to check against, for targets that have one.
example:
  codeRoot: ./src/
  shapeRoot: ./spec/shape/
```

# Construct Composition

## Description

Belay constructs are ordinary Piton anchors with framework-specific meaning. Their keyword forms provide reusable vocabulary without introducing a separate composition language.

## Requirements

- Expose Agent, Instruction, Skill, and Command as the four agentic constructs.
- Associate those anchors with agent, instruction, skill, and command.
- Allow abstract anchors to provide shared structure and behavior.
- Resolve inherited properties according to the Piton language specification.
- Validate required properties on concrete constructs after inheritance resolves.
- Preserve additional properties for serialization into the generated guidance.
- Emit only what the build command emits: the entry file's exports and anything they reference.

## Other Anchors

Belay has more than the four constructs. The config anchor, the adapters, and the special imports are part of it too.
