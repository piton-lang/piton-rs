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
    requires:: string[]:: null: null
    placeholders:: string[]:: null: null
    build:: boolean:: null: null
    inspect:: boolean:: null: null
    reach:: boolean:: null: null
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

## Requires

Optional. The names of plugins this one builds on, whose exports it uses, described in plugin dependencies.

## Placeholders

Optional. Names its Piton files export whose value is only known when its output is written, described in plugin rendering.

## Build

Optional. true when the plugin produces build output through a build hook, described in plugin build. Left out, `piton build` doesn't run it.

## Inspect

Optional. true when the plugin inspects projects before commands run, through an inspect hook, described in plugin inspection.

## Reach

Optional. true when the plugin names anchors it reaches itself, through a reach hook, described in plugin reach. `piton reach` and the language server count those anchors as roots beside the entry.

## Commands

Optional. A dictionary from each command's name to what it is. A name is lowercase words joined by hyphens, and is what follows piton on the command line. Two plugins can't add the same command, and a plugin can't add one the compiler has: the second is reported and not added. The same holds within a group, and a group can't be named like a command, the compiler's or a plugin's.

### About

One line, shown in `piton --help` and at the top of the command's own help.

### Group

Optional. A word the command is grouped under, so `slice` grouped under belay is `piton belay slice`. Commands in a group are listed under it in `piton --help`, and the group itself is a command that lists them.

### Help

More, shown under it in `piton <command> --help`.

### Standalone

Optional. true for a command that doesn't use a project, like `piton init`. It runs wherever it is invoked, outside a project and inside one whose configuration has errors, and the project's plugin problems are not reported before it.

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

### Default

The value it has when it isn't given.

### Trailing

For the last positional, multiple argument: true to take everything after it as it is, values that look like options included, to pass on to a program, like `piton agent claude --resume`.

### Optional Value

For an option that takes one value: true to let it also be given with no value, like `piton test --model`. The plugin then receives an empty string, which is different from null, the option not given.

A positional argument has no type flag and no short name. Only an option that takes one value, not a positional, multiple or flag argument, can have an optional value. A setting that isn't one of these, and a conflict naming an argument the command doesn't have, make the declaration invalid.

## Values

The plugin receives its arguments by name: a flag as true or false, a number as a number, a multiple argument as a list, an option with an optional value given without one as an empty string, and anything not given as null. The CLI checks them against the declaration first, so a value outside its choices or below its minimum never reaches the plugin, and is reported the way the compiler's own commands report bad arguments.

## Errors

A declaration that isn't well-formed is reported as a warning naming the file it is in and what is wrong, and that plugin is not loaded; every other plugin and command still works.
