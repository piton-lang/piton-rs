# Migration

## Description

The migrate plugin, bundled with the compiler, notices projects written for an older Piton and offers to bring them up to date. It is a plugin, described in [Plugins](../plugins/index.md#plugins), that inspects every project before a command runs, and adds `piton migrate`.
Its source is in the compiler's repository at plugins/migrate: its declaration in plugin.pi and its code in src. It has no Piton files of its own.

## Migrations

[FrameworksToPlugins](./index.md#frameworks-to-plugins)

## Command

[MigrateCommand](./index.md#migrate-command)

## Capabilities

It may read files in the project, and propose edits to piton.config.pi and the project's .pi files, which are the only files a migration changes. It runs no programs and writes nothing itself.

## Adding

A migration recognises one old way of writing a project, from the project's files as they are, without compiling them, and gives the whole new contents of each file it changes. When Piton changes in a way that breaks existing projects, the change comes with a migration.

# Frameworks To Plugins

## Description

A configuration that lists its frameworks under frameworks, which is now an error, lists them under plugins instead: a framework is a plugin.

## Detects

A key named frameworks in the piton-config anchor of piton.config.pi.

## Edit

When the configuration has no plugins key, the frameworks key is renamed plugins, and its items stay as they are. When it has both, the frameworks items move to the end of the plugins list, and the frameworks key goes. Either key can hold its items as a block or as an inline list; an inline plugins list becomes a block, so the moved items can follow its own. Every configuration that has a frameworks key gets this edit, so none is left to change by hand.

# Migrate Command

## Command Name

migrate

## Description

Applies every migration the project needs, without asking, since running it is the asking, and reports each file changed. A project that needs none is said to be up to date.

## Named Arguments

```
check: Change nothing, and fail when the project needs a migration, naming each one, for CI.
```

## Exit Code

```
success: 0
outOfDate: 1, with --check, when a migration is needed
```
