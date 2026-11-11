# Plugin Declaration

## Description

A plugin is declared with a piton-plugin anchor, from @piton/plugin, which is bundled with the compiler. The declaration is everything the CLI needs without running the plugin: `piton --help` lists its commands, and clap checks their arguments, from the declaration alone.
```piton
use @piton/plugin

export piton-plugin TestPlugin:
    name: test
    entry: ./src/index.ts
    settings: TestConfig
    description: Has an agentic tool judge the project's tests.

    capabilities:
        process: [claude, codex, opencode, muse]
        temporaryFiles: true

    commands:
        test:
            about: Have an agent judge the project's tests
            arguments:
                targets:
                    help: Tests to run, by name, file or directory
                    positional: true
                    multiple: true
                jobs:
                    short: j
                    type: number
                    minimum: 1
                serial:
                    type: flag
                    conflicts: [jobs]
```

## Shape

```piton
export abstract anchor PitonPlugin as piton-plugin:
    name:: string
    entry:: string
    description:: string:: null: null
    settings:: string:: null: null
    commands:: dictionary:: null: null
    capabilities:: dictionary:: null: null
```

## Name

Required. Lowercase words joined by hyphens, like test or my-plugin. A bundled plugin's Piton files are imported as @piton/ and its name, so the test plugin's are @piton/test. Two plugins can't share a name: the second is reported and not loaded.

## Entry

Required. The TypeScript or JavaScript module the plugin's behaviour starts from, relative to the file the declaration is in. It has to be inside that file's directory and has to exist. Its default export is what definePlugin returns, described in the plugin API.

## Description Property

Optional. What the plugin is for, shown by `piton plugins`.

## Settings

Optional. The name of an abstract anchor the plugin's Piton files export, which a project's settings for the plugin implement, like TestConfig. The project lists its settings anchor under plugins in piton.config.pi, and the plugin reads it.

## Capabilities

Optional. What the plugin may do beyond reading the compiled project, described in plugin capabilities. Left out, it may do nothing else.

## Commands

Optional. A dictionary from each command's name to what it is. A name is lowercase words joined by hyphens, and is what follows piton on the command line. Two plugins can't add the same command, and a plugin can't add one the compiler has: the second is reported and not added.

### About

One line, shown in `piton --help` and at the top of the command's own help.

### Help

More, shown under it in `piton <command> --help`.

### Arguments

A dictionary from each argument's name to what it is, in the order they are listed in help.

## Arguments

Each argument is named by its key, which is also its long option, like `--jobs`, and the name the plugin receives its value by. Each setting is optional.

### Help

What it is, for help.

### Positional

true for an argument given without an option name, like the targets of `piton test`.

### Multiple

true when it can be given more than once, or take several values.

### Type

string, the default, number, or flag. A flag is on or off and takes no value.

### Choices

The values it accepts. Anything else is an error before the plugin runs.

### Short

One letter, for a short option like -j.

### Minimum

For a number, the least it may be.

### Conflicts

Other arguments of the command it can't be given with.

A positional argument has no type flag and no short name. A setting that isn't one of these, and a conflict naming an argument the command doesn't have, make the declaration invalid.

## Values

The plugin receives its arguments by name: a flag as true or false, a number as a number, a multiple argument as a list, and anything not given as null. The CLI checks them against the declaration first, so a value outside its choices or below its minimum never reaches the plugin, and is reported the way the compiler's own commands report bad arguments.

## Errors

A declaration that isn't well-formed is reported as a warning naming the file it is in and what is wrong, and that plugin is not loaded; every other plugin and command still works.
