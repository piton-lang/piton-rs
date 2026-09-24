# Package Management

## Description

Piton supports package management and dependencies.
Package sources are git repos and can be pinned to a specific commit, branch, or tag. If no specifier is provided, the latest commit on the primary/default branch is used.
The strategy for packages is managed vendored dependencies.  What that means is installed packages are stored as part of the project, committed to version control, but can be added, updated, removed, and inspected by the package management tooling.

## Packages

[Package](./Package.md#package)

## Dependencies

[Dependencies](./Dependencies.md#dependencies)

## Importing And Using

[ImportingAndUsing](./ImportingAndUsing.md#importing-and-using)

## Commands

- [Remove](../tooling/cli/package-management/Remove.md#remove)
- [Tether](../tooling/cli/package-management/Tether.md#tether)
- [Untether](../tooling/cli/package-management/Untether.md#untether)
- [Update](../tooling/cli/package-management/Update.md#update)

## Locations

Tethered packages are stored in the same directory as the piton.config.pi file inside a tethers/ directory.
When a package is untethered, it's moved into untethered/ inside root and its imports are rewritten.

## Clone

Cloning with git (tethering) should remove any traces of git; these are just plain files now once they're tethered.

## Lock File

.piton/tether.lock lives next to piton.config.pi. For each tethered package it keeps the URL, the commit, and a hash of the files. tether and update write it, and it gets committed with the project.

## Conflict Resolution

When a package is updated (or tethered or anything else), it should check the current status of the files against the lock file, and only if there are no differences can it update. If it appears that the package has been modified, the user should be informed of the problem and asked to untether.
