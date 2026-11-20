# Init

## Command Name

init

## Positional Arguments

```
name: The project's name, which is also the directory it is created in, created if it doesn't exist. `.` creates it in the current directory, and the project takes that directory's name. Without it, init asks for the name; with no terminal to ask on, it uses `.`.
```

## Named Arguments

```
type: Short t. The type of project: basic, application or tether.
renderer: Short r. What a basic project compiles to: json, yaml or markdown. Only a basic project takes it.
belay: A flag: an application uses Belay. It can't be given with no-belay.
no-belay: A flag: an application doesn't use Belay.
adapter: Short a. A Belay adapter to build for, by its target id, like `codex`. Repeat it, or separate ids with commas, to build for several. Only an application that uses Belay takes it.
code-root: Belay's code root. Only an application that uses Belay takes it.
shape-root: Belay's shape root, beneath the specification root. Only an application that uses Belay takes it.
testing: A flag: set the project up for piton test. Only an application or a tether takes it. It can't be given with no-testing.
no-testing: A flag: don't set the project up for piton test.
crux: A flag: add the crux skills. Only an application that uses Belay takes it. It can't be given with no-crux.
no-crux: A flag: don't add the crux skills.
no-tether: Write the project's files only, leaving the dependencies its configuration declares for piton tether to install later.
```

## Description

Starts a new project, built from the parts that come with the compiler, by asking what kind of project it is and what it should have. It is the command of the init plugin, not a command the compiler has itself. It is standalone: it runs wherever it is invoked, inside a project or not, whatever the configuration there says, and writes relative to the directory it is run in.

## Questions

Init asks, in this order, only the questions that apply, and skips each one whose answer is given on the command line:
1. The project's name. 2. The type of project, from a list: basic, application or tether, each with what it is for. 3. For a basic project, the renderer, from a list: json, yaml or markdown, json first. 4. For an application, whether it uses Belay, yes by default. When it

### Does

the adapters to build for, as a list with a box to check

beside each, naming the platform and its target id, with Claude Code checked to start with and at least one checked; then the code root, ./src unless another is typed; then the shape root, ./spec/shape unless another is typed. 5. For an application or a tether, whether to set it up for piton test, yes by default. 6. For an application that uses Belay, whether to add the crux skills, yes by default.
A yes or no question is answered with y or n, and enter takes the default. Escape or Ctrl+C at any question stops init before anything is written. With no terminal to ask on, the type has to be given with --type, and init says so; every other question takes its default.

## Name

The name is what was given or typed: the directory the project is created in, relative to the directory init runs in. The project's name is that directory's own name, its last part, so `.` is the name of the current directory. A tether's package is published under that name.

## Types

### Basic

Plain Piton compiled by one of the compiler's renderers into dist/. Its configuration sets the renderer chosen, and its specification is a Specification anchor and a Glossary.

### Application

The specification of an application. With Belay, its configuration adds a belay-config with the code root, shape root and adapters chosen, and the project gets the guidance Belay compiles for its

#### Code

a Conventions anchor, a shape instruction for the code root,

a skill, an agent and a command. Without Belay it is compiled to JSON in dist/, like a basic project that keeps the default renderer.

### Tether

A package, which other projects install with piton tether. Its configuration declares a piton-package named after the project, whose root is the specification root, and its specification is an abstract Component anchor for the projects that install it to extend, and a ReviewComponent skill, as an example of the agent guidance a package can ship. A tether can't use Belay: its configuration has no belay-config, and a project that tethers it builds the skill for its own adapters.

Every type keeps its sources in ./spec, entered at ./spec/index.pi.

## Testing

Setting a project up for piton test adds a test-config to its plugins and one example test, in spec/tests. An application's tests are judged against both the specification and the code, the code being Belay's code root, or the project root without Belay. A tether's tests are judged against the specification only, since a package is a specification.

## Crux

Adding the crux skills declares https://github.com/piton-lang/crux.git under the configuration's dependencies and re-exports everything the package exports from the entry, so once crux is tethered the first build emits every crux skill for each adapter chosen. They are offered only when the project uses Belay, since Belay is what builds them.

## Roots

The code root and the shape root are paths relative to the project, written as typed, with `./` added when they don't start with it. The shape root has to be beneath the specification root, ./spec, and not ./spec itself, since the entry exports the shape instructions from it; init says so when it isn't, and asks again or, from --shape-root, stops.

## Configuration

The plugin writes piton.config.pi from the answers, rather than reading it from a part, and writes it canonically formatted with the compiler's formatter. The adapters chosen are listed in its import from @piton/belay and in its adapters list, each once and in the order @piton/belay exports them.

## Parts

The files a project gets, other than its configuration, come from

### Parts

directories in the templates/ directory of the init plugin,

plugins/init/templates in the compiler's repository. Each part holds files laid out as the project lays them out:

- basic, application and tether hold what each type starts with.
- belay holds what Belay adds to an application.
- testing holds the example test.
- crux holds the entry's re-export of crux.

A project gets the part of its type, then belay, testing and crux, in that order, when it has them. Within a part:

- Files under `shape/` are written beneath the shape root, and files under `code/` beneath the code root. Files under `code/` aren't written when the code root is the project root.
- Files under `.adapters/<target id>/` are written only when that adapter is chosen, at their path beneath it, like the opencode.json that has OpenCode load the instructions placed in the code root.
- `{{name}}` in a file is replaced by the project's name; `{{codeRoot}}` by the code root followed by a slash, like `src/`, or by nothing when it is the project root; `{{shapeRoot}}` by the shape root relative to the specification root, like `shape`; and `{{specRoot}}` by the specification root relative to the shape root, like `..`.

When several parts have a file at the same path, the project gets them joined into one: the lines of each that come before its first declaration, its comments, uses and imports, in the order of the parts, then the rest of each in the same order, formatted with the compiler's formatter. That is how the entry gains the exports of Belay and crux.
Every Piton file is formatted with the compiler's formatter once its placeholders are filled in, since a long project name can push a line of prose past the 80 columns the formatter wraps at.
The parts are embedded with the plugin's files when the compiler is built, so init needs no network for them, and a release always writes the parts it was released with. Authors change what a project starts with by editing a part's files.
Every combination of answers has to start a project that checks without problems, is already formatted, and builds, for each adapter it can be built for.

## Tethering

When the configuration written declares dependencies, init tethers them once the files are written, as piton tether does with no source, so the project builds without another step. That needs the network. When tethering fails, the files stay written, the failure is reported with a reminder to run piton tether in the project, and init exits with errors. With --no-tether init doesn't tether, and the commands to run next include piton tether.

## Safety

Init never overwrites a file. The host writes the project, through the scaffold capability, and when any file init would write already exists, it writes none of them, and init names each one.

## Output

The path of each file written, one per line on stdout, followed by what piton tether prints for each package tethered. Then on stderr what was created, the type with what it was given: the renderer, or Belay with its adapters, testing and crux. Then how many files were written, and the commands to run next. On a terminal each path is marked as added with its directory dimmed; piped, the paths are bare.

## Diagnostics

### Errors

true

### Warnings

false

No type given without a terminal to ask on, a type, renderer or adapter that doesn't exist, an argument the type or its answers don't take, like --renderer for an application or --crux without Belay, a shape root outside the specification root, a question cancelled, a file that would be overwritten, and dependencies that fail to tether are all errors.

## Exit Code

```
success: 0
errors: 1
```
