# Build

## Description

Builds the project as configured by piton.config.pi

## Command Name

build

## Positional Arguments

```
config: optional path to piton.config.pi file
```

## Named Arguments

null

## Entry

The entry file is set by entry in the project config. If it's left out, it's index.pi inside root. If that doesn't exist, it's an error.

## Emitted

Anything that is exported from the entry file will be compiled; it doesn't have to be explicitly *used* to be compiled. Anything those exports reference also gets compiled, so the links have somewhere to go. Everything else is left out, and abstract anchors never compile.

## Output

Without a framework, build writes to the output directory set in the project config. That's the entry file, plus any file with something the entry's exports reference. Files keep their place under root and get the renderer's extension, so with the defaults spec/components/Button.pi becomes dist/components/Button.json.
A framework decides where its own output goes, and that replaces the renderer's: a project configured for Belay's Claude Code adapter gets .claude and nothing in dist. Setting output or renderer in the project config asks for the renderer's output as well, and the build writes both.

## Manifest

In a .piton directory that lives alongside the piton.config.pi file, a manifest.json file will be written with the build output.
```
{
    "generated": [ pathToGeneratedFiles ],
    "targets": { targetId: documentationChecked }
}
```
targets records, for each framework target, the date its documentation was last checked, which is the version the generated files were validated against.
