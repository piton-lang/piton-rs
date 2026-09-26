# Update

## Description

Using the dependencies in the piton.config.pi, update all packages

## Command Name

update

## Positional Arguments

```
packages: optional list of package names to update; if not provided, all packages will be updated
```

## Named Arguments

```
force: Replace a package even when it was edited since it was installed, discarding the edits. Each package whose edits were discarded is named in a warning, with what changed in it.
diff: Show a unified diff of every file each update changed, from what was on disk before to what is installed now, local edits included. Removed lines are red and added lines green on a terminal; piped, it is a plain unified diff.
```
