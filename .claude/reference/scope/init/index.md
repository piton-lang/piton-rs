# Initialization

## Description

The init plugin, bundled with the compiler, adds `piton init`, which starts a new project from the answers to a few questions. It is a plugin, described in [Plugins](../plugins/index.md#plugins), like any other: the compiler has no init command of its own.
Its source is in the compiler's repository at plugins/init: its declaration in plugin.pi, its code in src, and its templates in templates. It has no Piton files of its own.

## Command

[Init](./Init.md#init)

## Declaration

Its declaration names the init command and its arguments: name, positional; type, short t; renderer, short r; belay and no-belay, flags that can't be given together; adapter, short a, which can be given more than once; code-root; shape-root; testing and no-testing, and crux and no-crux, each a pair of flags that can't be given together; and no-tether, a flag. The command is standalone, so it runs outside a project, and inside one whose configuration has errors.

## Capabilities

It may ask the user on the terminal, which is interactive, and start new projects, which is scaffold: learn the name of the directory it writes into, write the project's files there, never over a file, and tether the new project's dependencies. It reads no project files and runs no programs.

## Templates

The parts it builds a project from are in the templates directory of the plugin, one directory for each, as [Init](./Init.md#init) describes. The plugin lists them with its resources, and reads each file as a resource. Authors change what a project starts with by editing a part's files. The types, and which parts each one takes, are in the plugin's code, as is the configuration it writes.

## Adapters

The adapters it offers are those @piton/belay exports, each with its target id, the name of the anchor exported for it, and the platform's name. The plugin keeps that table in its code.
