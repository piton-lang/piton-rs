# Import Export

## Description

To make something defined within a file available to other files, you use the export keyword. For example:
```piton
export pi: 3.14
myVariable: 42
export anchor MyAnchor:
    description: This is my anchor
```
To bring those into another file you use the from...import syntax:
```piton fragment
from ./FirstFile import pi, MyAnchor
```
Note that we can’t import myVariable because it wasn’t exported.
Paths for from...import are relative to the current file. The .pi extension is optional, and piton format removes it. Just . or .. points at the index.pi in that directory. If the project is configured with a piton.config.pi file, you can also use absolute path imports relative to the root value defined in the project config.
```piton fragment
from /subdir/subdir/file import AnAnchor
```
It’s also possible to import under an alias by placing the alias after the imported symbol:
```piton fragment
from ./FirstFile import pi SliceOf, MyAnchor MyAliasedAnchor
```
Worth clarifying that pi will be imported as and only as SliceOf (i.e. pi will not be available in scope).
