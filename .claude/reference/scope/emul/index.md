# Emul Plugin

## Description

The Emul plugin, bundled with the compiler, lets a project import and reference Emul files. Emul is a small language for visual designs: components and compositions that compile to SVG, PNG and HTML. A spec can thus point an agent at the design of a screen, a button or a layout, and not describe it in prose. The plugin is a plugin, as described in [Plugins](../plugins/index.md#plugins).
All Emul integration is in the plugin. The compiler holds no code for Emul: the plugin's declaration lists emul under modules, and its module hook, described in [PluginModules](../plugins/Modules.md#plugin-modules), gives the Piton module of each .emul file that a project imports. The plugin host adds its Piton files to the bundled packages, as for every bundled plugin that the compiler does not know by name.
Its source is in the compiler's repository at plugins/emul: its declaration in plugin.pi, its code in src, and its Piton files in pi. A project imports its Piton files as @piton/emul.

## Importing

A .pi file imports the exported components and compositions of an Emul file with the usual from and import, with a path that ends in .emul. Because the plugin lists emul under modules, the path names that file, as for every extension a module hook gives modules for. The extension is mandatory: without it, the path names a .pi module. The path resolves as every other module path does: relative to the importing file, from the source root when it starts with /, or in an installed package.
```piton
from ./designs/Landing.emul import Landing, PrimaryButton
```
A name that the Emul file does not export is an error, as for a .pi module. A path that names no file is also an error.

## Declarations

The plugin does not compile Emul. Its module hook reads only the declaration lines of the Emul file, at the start of a line, which have the same form as in Piton, and gives a Piton module that declares and exports the same names:

- export component Name and export composition Name Declare an exported design.
- component Name and composition Name, then export Name Export a design that the file declared before.
- from ./Other import Name, then export Name Export again a design of another Emul file.
- from ./Other export Name, and from ./Other export * Export again the designs of another Emul file.

An import or an export can continue on indented lines, with one name on each line and no comma, as `emul format` writes a long list. Each such line is one more item of the list.
A path in an Emul file is relative to that file. Without an extension, it names Other.emul, or Other/index.emul when that file is not there. A path of . or .. names the index.emul of that directory. In the module the hook gives, each such path ends in .emul, so it names the Emul file in turn. Emul's own errors stay for `emul check` to find.

## Designs

In the module the hook gives, each exported design becomes a concrete anchor with the name of the design. The anchor implements the abstract EmulDesign of @piton/emul, with the keyword emul-design. It has these properties:

- name The name of the design in the Emul file.
- kind component or composition.
- file The path of the Emul file that declares the design, relative to the project root.
- usage The instruction to the agent, from EmulDesign. Usage tells it.

The anchors are values like all others. They do not show in `piton compile` unless the compiled file exports them.

## Referencing

An @{} reference to an imported design is a reference to its anchor. The build writes the anchor, as it writes each anchor that the build reaches. Belay writes it to the reference tree, at the path of the Emul file below the source root, with the extension .md. For example spec/designs/Landing.emul gives .claude/reference/designs/Landing.md. The reference renders as a link to its heading, which holds the name, kind, file and usage of the design. A copy with {} puts the same properties in place.

## Usage

The usage of each design tells the agent these things. The `emul` compiler is already installed on the system. Piton does not install it, and the agent does not install it. To use the design, the agent does one of these things: it reads the Emul file directly, or it compiles the design to SVG, PNG or HTML with `emul build`. The agent selects the form that is most useful for the current work. SVG and PNG show how the design looks. HTML shows the structure and the CSS. The Emul source shows the exact values. The agent writes the compiled files to a temporary directory with `--out`, and does not add them to the project. `emul build` renders only compositions that the entry of an Emul project exports. Thus the agent gives `--config` for the Emul project of the file, or makes a temporary Emul project whose entry exports the design. A component shows only in a composition.

## Capabilities

The plugin may run `emul`. It finds the program on PATH. It may read project files, so that its module hook can tell if a path in an Emul file names Other.emul or Other/index.emul. It writes nothing.

## Command

[EmulCommand](./index.md#emul-command)

# Emul Command

## Command Name

emul

## Description

Lists each Emul design that the project imports: its name, its kind and its file. Then it tells if the `emul` compiler is on PATH. A project that imports no Emul file gets a report that says so.

## Exit Code

```
success: 0, also when the `emul` compiler is not on PATH, which is a warning
```
