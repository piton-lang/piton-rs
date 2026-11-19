# Test Command

## Description

Runs the project's tests, described in [Testing](../index.md#testing). Each test is one statement an agent judges true or false against the specification, the code, or both.

## Command Name

test

## Positional Arguments

```
targets: Optional. Which tests to run: a test's name, or a file or directory holding tests. Every test in the project when none are given.
```

## Named Arguments

```
agent: Which tool judges the tests [ claude | codex | opencode | muse ]
model: The model that judges the tests, `-m` for short, given to the tool as `piton agent --model` gives it; given with no model, it lists the models of the agent and runs nothing; given a value that is not one of the models, it lists the models that contain it, else all of them, and runs nothing
against: What every test is judged against [ spec | code | both ]
jobs: How many tests run at once
serial: Run one test at a time, the same as `--jobs 1`
list: Print the tests that would run, grouped by file, without running them
tags: Only run tests whose tags match an expression, like `--tags "((save and editor) not slow) or smoke"`
```

## Plugin

Added by the test plugin, bundled with the compiler. Its arguments, below, are its declaration's, and everything it does is the plugin's TypeScript, in the compiler's source at plugins/test/src.

## Discovery

[TestDiscovery](./Discovery.md#test-discovery)

## Selection

[TestSelection](./Selection.md#test-selection)

## Tag Expressions

[TestTagExpressions](./TagExpressions.md#test-tag-expressions)

## Order

[TestOrder](./Order.md#test-order)

## Settings

[TestSettings](./Settings.md#test-settings)

## Models

[TestModels](./Models.md#test-models)

## What The Agent Reads

[TestReading](./Reading.md#test-reading)

## Running

[TestRunning](./Running.md#test-running)

## Explanation

[TestExplanation](./Explanation.md#test-explanation)

## Fix

[TestFix](./Fix.md#test-fix)

## Fix Prompt

[TestFixPrompt](./FixPrompt.md#test-fix-prompt)

## Handoff

[TestHandoff](./Handoff.md#test-handoff)

## Launch

[TestLaunch](./Launch.md#test-launch)

## Verdict

[TestVerdict](./Verdict.md#test-verdict)

## Concurrency

[TestConcurrency](./Concurrency.md#test-concurrency)

## Progress

[TestProgress](./Progress.md#test-progress)

## Output

[TestOutput](./Output.md#test-output)

## Recap

[TestRecap](./Recap.md#test-recap)

## List

[TestListing](./Listing.md#test-listing)

## Colour

[TestColour](./Colour.md#test-colour)

## Errors

[TestErrors](./Errors.md#test-errors)

## Exit Code

```
success: 0
failures: 1
errors: 1
```
