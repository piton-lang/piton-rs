# Package Management

## Description

Piton supports package management and dependencies.
Package sources are git repos and can be pinned to a specific commit, branch, or tag.  If no specifier is provided, the latest commit on the primary/default branch is used.
The strategy for packages is managed vendored dependencies.  What that means is installed packages are stored as part of the project, committed to version control, but can be added, updated, removed, and inspected by the package management tooling.

## Packages

### Description

A package declaration is just a `Package as package` anchor that can be located anywhere within a specbase, but it must be included in the main piton.config.pi file.
The Package anchor is exposed by the `@piton/packaging` import/use.
```piton
export abstract anchor PitonPackage as piton-package:
    name:: string:: null: null
    root:: string
    dependencies:: string[]:: null: null
```
The name is what the package installs under, and it is separate from the anchor's name because it can contain a `/`. Left out, the anchor's own name is used.
and in piton.config.pi
```piton
packages:
    - {MyPackage}
```
Note here that a project can define multiple packages with different roots and their own different dependencies.
When installed without additional filters, all packages will be installed into tethers/ according to how they're defined.

### Dependencies

If dependencies are specified at a project level, they only apply to the project.  A Package must define its own dependencies.
It is possible to pin versions at a project level, and if not specified at a package level, those version pins will be inherited.
There are no nested dependencies; if MyPackage is required by two different packages, the most recent version will be chosen and a warning will be displayed.

### Installation

A package is installed into tethers/ according to its name.  So while a single project can defined multiple packages, let's say MyScope/package and MyPackage, they will both be installed into tethers/MyScope/package and tethers/MyPackage respectively.

## Dependencies

### Description

Dependencies are listed under the dependencies property of the piton.config.pi file for a project.
They're specified as a list of URLs with optional properties.
```piton
dependencies:
    - https://github.com/piton-lang/piton-rs
        commit: abc
        tag: 1.0
        branch: main
```

## Importing And Using

### Description

Where most `from...import` and `use` typically refer to relative paths, or the project root path via `/` a package can be imported simply by naming it.  So presuming a package is called my-package it can be imported as
```piton
from my-package import MyAnchor
```
If it doesn't exist, it's a compiler error; same as trying to import something that doesn't exist.
Similarly, you can do
```piton
use my-package
```
or
```piton
use my-package/MyAnchor
```
Note that regular path resolution behaves as it does anywhere else, it's just that a named package can serve as a locational placeholder.

## Commands

- [Remove](../tooling/cli/package-management/Remove.md)
- [Tether](../tooling/cli/package-management/Tether.md)
- [Untether](../tooling/cli/package-management/Untether.md)
- [Update](../tooling/cli/package-management/Update.md)

## Locations

Tethered packages are stored in the same directory as the piton.config.pi file inside a tethers/ directory.
When a package is untethered, they've moved into specRoot/untethered and the LSP should be used to rework any imports.

## Clone

Cloning with git (tethering) should remove any traces of git; these are just plain files now once they're tethered.

## Conflict Resolution

When a package is updated (or tethered or anything else), it should check the current status of the files against the locked version, and only if there are no differences can it update.  If it appears that the package has been modified, the user should be informed of the problem and asked to untether.

Links in this document point at reference files. Read one when the work touches what it describes.
