# Test Recap

## Description

When anything didn't pass, the run ends with a recap headed Not

### Passing

one line per failed or unjudged test, with its status, name

and file, so the tests to look at aren't lost above a long run. A failed test against both also says which side failed, like `(spec pass, code FAIL)`.
After the recap, under the heading Fix prompts, comes each failed test's fix prompt, in the order the tests are listed, whatever order they finished in. Each is headed by a rule with the test's name, and is printed whole, not wrapped or indented, so it can be copied to an agent as it is.
The outcome comes last, on stderr: `✓ 9 tests passed in 4:12` when everything passed, and otherwise the count, what passed, failed and could not be judged, and how long the run took, like `✗ 9 tests: 7 passed, 1 failed, 1 could not be judged in 4:12`.
