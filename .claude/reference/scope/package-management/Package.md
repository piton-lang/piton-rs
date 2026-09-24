# Package

## Description

A package declaration is a `piton-package` anchor that can be located anywhere within a specbase, but it must be included in the main piton.config.pi file.
The PitonPackage anchor is exposed by the `@piton/packaging` import/use.
```piton
export abstract anchor PitonPackage as piton-package:
    name:: string:: null: null
    root:: string
    dependencies:: list:: null: null
```
The name is what the package installs under, and it is separate from the anchor's name because it can be kebab-case, like my-package. Left out, the anchor's own name is used. Package names can't have a `/` in them. Names like `@piton/belay` are only for the packages that come with the compiler.
and in piton.config.pi
```piton fragment
packages:
    - {MyPackage}
```
Note here that a project can define multiple packages with different roots and their own different dependencies. A package's root is relative to the piton.config.pi file, like the project's own root.
packages is what your repository offers to other projects. A project never installs its own packages. When another project tethers your repository, it gets every package listed here, each in its own directory under tethers/. It can pick just some of them with packages: (see Dependencies).

## Dependencies

If dependencies are specified at a project level, they only apply to the project. A Package must define its own, the same way.
There are no nested dependencies; if MyPackage is required by two different packages at different versions, the one with the newest commit will be chosen and a warning will be displayed.

## Installation

A package is installed into tethers/ according to its name. So if a repository defines ui-kit and my-package, a project that tethers it gets tethers/ui-kit and tethers/my-package, with the files from each package's root.
