# Test Tag Expressions

## Description

A tag in the expression is true of a test that has it, matched exactly, case and all. `and` and `or` combine, `not` before an expression negates it, and `not` between two means and not, so `editor not slow` is `editor and not slow`. Parentheses group.
`not` binds tightest, then `and` and the `not` between two, then `or`, so `a or b and c` is `a or (b and c)`. `&&`, `||` and `!` are the same operators, spelled as Piton's own expressions spell them.
A test with no tags matches only through `not`: `--tags "not slow"` includes it, and `--tags editor` doesn't. A tag is any run of characters other than spaces, parentheses, `!`, `&` and `|`, so `needs-network` and `v1.2` are tags; `and`, `or` and `not` are operators and can't be.
An expression that doesn't parse is an error that says what was expected and where, and nothing is compiled or run.
