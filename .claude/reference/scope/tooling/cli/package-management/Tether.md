# Tether

## Description

Clones and "un-gits" a repository into the tethers directory. Run it on its own and it installs everything in piton.config.pi's dependencies. Give it a source and it adds that to piton.config.pi and installs it.
The source can be a local directory as well as a URL (see [Dependencies](../../../package-management/Dependencies.md#dependencies)). A relative path given on the command line is taken from the working directory, and it's rewritten when it's recorded so that it leads to the same place from piton.config.pi's directory.

## Command Name

tether

## Positional Arguments

```
source: Optional. URL of the git repository, or the path to a directory on disk, which can be a git repository or a plain directory
```

## Named Arguments

null
