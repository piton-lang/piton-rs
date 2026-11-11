# Frameworks

## Description

A framework extends what the compiler outputs. Piton itself outputs data through renderers, and a framework builds on them, like Belay's adapters build on the Markdown renderer.
Frameworks are plugins, described in [Plugins](../../plugins/index.md#plugins): a framework is a plugin with a build. Belay is one, bundled with the compiler. Its keywords and modules can be used and imported in any project, and each file still uses and imports what it needs; listing its settings under plugins in piton.config.pi is what turns on its output.

## Removed Key

The frameworks key of piton-config is gone, since a framework is listed under plugins like any plugin. A configuration that still has it is an error, frameworks-removed, whose help is the fix: move each item to plugins.
