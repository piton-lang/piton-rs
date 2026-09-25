# Init

## Description

Starts a new project from one of the templates that come with the compiler.

## Command Name

init

## Positional Arguments

```
directory: Where to create the project. Defaults to the current directory, and is created if it doesn't exist.
```

## Named Arguments

```
template: The template to start from, by name.
list: List the templates, each with what it sets up, and do nothing else.
```

## Templates

Each template is a directory in create-templates/ at the root of the compiler's repository, and holds exactly the files a new project gets, laid out as the project lays them out. A `.template` file in it describes it in one line, and is not written into the project.
The templates are compiled into the binary when the compiler is built, so init needs no network, and a release always writes the templates it was released with. Adding a template is adding a directory; nothing else lists them.
Every template has to start a project that checks without problems, is already formatted, and builds.

## Choosing

Without a template named, init lists the templates by number and asks which one to use, taking either its number or its name. When there is no terminal to ask on, a template has to be named, and init says so.

## Safety

Init never overwrites a file. When any file the template would write already exists, it names each one and writes none of them.

## Output

The path of each file written, one per line on stdout, then on stderr how many were written and what to run next.

## Diagnostics

### Errors

true

### Warnings

false

A template that doesn't exist, no template named without a terminal to ask on, an answer that isn't one of the templates, and a file that would be overwritten are all errors.

## Exit Code

```
success: 0
errors: 1
```
