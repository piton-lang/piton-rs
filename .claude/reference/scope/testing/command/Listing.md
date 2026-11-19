# Test Listing

## Description

`--list` prints the tests a run would include, under the file each is declared in, files in order and separated by blank lines. Each test is its name, then dimmed what it is judged against and its tags, then its statement wrapped beneath. The outcome is how many tests in how many files, like `✓ 9 tests in 3 files`. Nothing is run.
```text
spec/tests/Editor.pi
  SaveButtonSaves  against code · editor, save
    The Save button saves the document
```
