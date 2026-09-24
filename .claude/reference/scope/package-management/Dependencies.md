# Dependencies

## Description

Dependencies are listed under the dependencies property of the piton.config.pi file for a project. Packages list theirs the same way.
They're specified as a list of URLs, each with an optional commit, tag, or branch. Only one of those though; more than one is a compiler error. If there isn't one, you get the latest commit on the default branch.
```piton
dependencies:
    - https://github.com/piton-lang/piton-rs
        tag: 1.0
    - https://github.com/piton-lang/other
```
@piton/config types commit, tag, and branch as strings, so `tag: 1.0` stays “1.0” instead of becoming the number 1.
A repository can offer more than one package. You get all of them unless you list the ones you want with packages:
```piton
dependencies:
    - https://github.com/acme/kits
        tag: v2
        packages: [ui-kit]
```
Project dependencies and package dependencies are separate. The project's only apply to the project, and each package has its own.
