# Importing And Using

## Description

Where most `from...import` and `use` typically refer to relative paths, or the project root path via `/` a package can be imported simply by naming it.  So presuming a package is called my-package it can be imported as
```piton fragment
from my-package import MyAnchor
```
If it doesn't exist, it's a compiler error; same as trying to import something that doesn't exist.
Similarly, you can do
```piton fragment
use my-package
```
or
```piton fragment
use my-package/MyAnchor
```
Note that regular path resolution behaves as it does anywhere else, it's just that a named package can serve as a locational placeholder.
