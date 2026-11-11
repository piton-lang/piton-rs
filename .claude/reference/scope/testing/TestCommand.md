# Test Command

## Description

Runs the project's tests, described in [Testing](./index.md#testing). Each test is one statement an agent judges true or false against the specification, the code, or both.

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

The project is the piton.config.pi in the working directory. Every .pi file under its source root is compiled, not only what the entry reaches, and every concrete anchor declared with export that has @piton/test's Test in its inheritance chain is a test, wherever in the source root it is, and whatever keyword or chain it got there by. Tests in tethers/ belong to installed packages and are left out. The plugin finds them with one query of the compiled project, described in the plugin API. Tests are ordered by the file they are declared in, then by name, so a run and a listing order them the same way every time.
The project has to compile cleanly first: tests are not run over a specification with errors, which are reported as diagnostics and stop the run. So does a test that isn't well-formed, one whose expects is empty or whose against isn't spec, code or both: each is reported by name with the file it is in, and none of the tests run.

## Selection

Targets and `--tags` narrow the run together: a test runs when a target names it, or there are no targets, and its tags match the expression, or there is none. When the selection leaves nothing, the run is an error, since a run that tests nothing is never what was meant.
A target is first a test's name, matched exactly. A target that is no test's name is a path, relative to the project root or absolute, and names every test declared in that file or in a file under that directory.

## Tag Expressions

A tag in the expression is true of a test that has it, matched exactly, case and all. `and` and `or` combine, `not` before an expression negates it, and `not` between two means and not, so `editor not slow` is `editor and not slow`. Parentheses group.
`not` binds tightest, then `and` and the `not` between two, then `or`, so `a or b and c` is `a or (b and c)`. `&&`, `||` and `!` are the same operators, spelled as Piton's own expressions spell them.
A test with no tags matches only through `not`: `--tags "not slow"` includes it, and `--tags editor` doesn't. A tag is any run of characters other than spaces, parentheses, `!`, `&` and `|`, so `needs-network` and `v1.2` are tags; `and`, `or` and `not` are operators and can't be.
An expression that doesn't parse is an error that says what was expected and where, and nothing is compiled or run.

## Order

Problems are found before anything costly. The configuration is read first, then the flags are checked against the declaration, then the test settings, then the tag expression is parsed, then the project is compiled and the tests collected and selected. Only then is the tool needed: `--list` stops there, so listing works without the tool installed, and a run checks the tool is on PATH before launching it for any test.

## Settings

Each test is judged against what `--against` says, else what the test says, else the project's test settings, else both. The agent and the number of jobs come from the flag, else the test settings, else claude and 4. The model comes from the flag, else the test settings, else the tool's own default. The settings are described in the testing specification.

## Models

When neither `--model` nor the settings give a model, the tool judges with its default model, and before the tests run a note lists the models of the tool, from the same list `piton agent` shows, and says to choose one with `--model`. The run is headless, so it lists them rather than asking.
`--model` with no model lists the models of the agent instead, one on each line on stdout, from the same list, like `piton test --agent opencode --model`. It runs no tests and exits with 0. An agent that lists no models is an error, and the exit code is 1.
A `--model` value is read as `piton agent` reads it: one of the agent's models is that model, and any other value is a search string. The models that contain it, ignoring case, are listed, like `piton test --agent opencode --model sonnet`, or every model when none contain it. It runs no tests and exits with 0. A model from the test settings is never a search.

## What The Agent Reads

```
spec: The .pi files under the source root, with the Piton fluency prompt `piton agent` gives the same tool.
code: The code root, which is Belay's codeRoot when the project configures Belay, and otherwise the project root, leaving out the source root, tethers/, the build output, and generated agent directories.
both: Both, and the statement has to hold in each.
```

## Running

A test runs as one headless session of the tool, in the project root, with nothing on its standard input. Its request gives the statement, the test's description as context from the test's author, the test's name and the file it is declared in, and what to judge it against, with the paths of the source root and code root relative to the project. Tags are only for selection and are not sent.
The tool is asked to read whatever it needs but not to change, create or delete any file, to base its verdict on what it finds rather than on what is likely, so a statement fails when the evidence contradicts it or isn't there, and to end its answer with one verdict line.
```text
PITON-VERDICT: {"pass": true, "reason": "SaveButton's onClick calls saveDocument"}
PITON-VERDICT: {"pass": false, "reason": "No handler saves the document", "explanation": "The Save button has no click handler. No function in src writes the document to disk.", "fix": "Add a click handler to SaveButton in src/editor.ts. Make the handler call saveDocument."}
```
Every tool is run in its own non-interactive mode, read-only wherever the tool can enforce it. Asking is not enforcement, so the command never claims more than the tool provides: Claude is limited to reading and searching, Codex runs in its read-only sandbox, OpenCode has editing and the shell denied, and Muse relies on its own default sandbox.
OpenCode takes extra instructions only as a file, so the fluency prompt is written to a temporary file for the run and removed when the run ends. Writing it is why the plugin declares temporary files among its capabilities, as running the four tools is why it declares them.

## Explanation

A test judged by an AI can fail for reasons that are hard to see from one sentence, so a failure also asks for an explanation, written in ASD-STE100 Simplified Technical English: a controlled language made to be read quickly, and without ambiguity, also by readers whose first language isn't English. It is requested in the same session as the verdict, so it costs no extra run.
The request gives the rules that matter most in a few sentences. At most four sentences, on the verdict's one line. Only words from the ASD-STE100 dictionary with their approved meanings, plus technical names like file, anchor and property names, and one word for one meaning. Sentences of no more than 20 words, with one topic each. The active voice and the simple present, past or future tense. No -ing forms of verbs, contractions or phrasal verbs. Articles where they belong, and noun clusters of three words or fewer. It says what is missing or different, and where.
The explanation is the verdict's explanation, kept only when pass is false. A failure without one still fails, and shows only its reason. Asking for ASD-STE100 is not checking it: the text is shown as the tool wrote it.

## Fix

A failure also asks the tool for a fix: the instructions an agent needs to make the statement true, in ASD-STE100 like the explanation, written as commands, naming each file to change and what to change in it. The fix aims at what the test examines. Against spec it changes the specification, in its .pi source, and not the code. Against code it changes the code, and not the specification. Against both it gives the changes to each as two parts, a part only for a side that doesn't make the statement true, so that afterwards the two agree. It is asked for in the same session as the verdict, so it costs no extra run.

## Fix Prompt

Every failed test is offered a prompt that fixes it, to give to an agent. The prompt stands on its own, so an agent with no other context can act on it, and it aims at what the test examines.
```text
Fix the code so that this statement is true:

The Save button saves the document

The test `SaveButtonSaves` in `spec/tests/Editor.pi` examines the statement in the code (the files under `src`). It failed.
Reason: No handler saves the document
Explanation: The Save button has no click handler.

What to change:
Add a click handler to SaveButton in src/editor.ts. Make the handler call saveDocument.

Rules:
- Change only the code. Do not change the specification.
- Do not change the test.
- Do not edit generated files.
- To examine the fix, run `piton test SaveButtonSaves --against code`.
```
Its first line says what to fix: the Piton specification, the code, or both, so that the statement is true in it. Then the statement, the test and the file it is declared in, where the statement was examined, the verdict's reason, for a test against both the result in the specification and in the code, and the explanation. What to change is the tool's fix; when the tool gave none, the prompt asks the agent to find where what the test examines doesn't make the statement true, and change it.
The rules keep the agent on that target. Against spec or code, it changes only that one. Against both, it changes both, and they must agree afterwards. It never changes the test: the statement is what the project should say. When the specification may change, it is changed in its .pi source, never in generated files, and `piton check` and `piton build` run afterwards. The last rule is the command that judges the test again against the same scope, with `--against`, so the check matches the failure even when the scope came from a flag.
A test that could not be judged has no fix prompt: nothing is known about why, beyond the error.

## Handoff

Once every failed test's fix prompt is printed and the outcome is written, the run offers to hand them to an agent, so a failure can go straight from the report to a fix. A menu on the terminal lists the agentic tools Belay knows, claude, codex, opencode and muse, and No agent, which ends the run as before.
The chosen agent starts in the project root, interactively, with the

### Terminal

the same fluency prompt `piton agent` gives it, and all the

fix prompts as its first message. One prompt is given as it is; several are given together, each under a heading that numbers it, with a line asking the agent to fix them one after another. Belay says how each tool is started with a first message, and the test plugin starts it, which is why it declares the tools among its processes and the interactive capability.
The menu only shows when stdin and stdout are terminals and at least one test failed; piped, in CI, or when every test passed or could not be judged, the run ends without asking. A tool that is not on PATH is reported instead of started. The run's exit code is the same whatever is chosen: it says how the tests went, not how the agent did.
After the agent is chosen, a second menu lists its models, with Default model first, the same models `piton agent` lists. It is skipped when the chosen agent judged the tests and a model was given for it, which it then keeps. Cancelling it starts nothing. An agent that lists no models gets a note and starts with its default.

## Launch

```
claude: claude -p <request> --append-system-prompt <fluency> --allowedTools Read,Grep,Glob --disallowedTools Edit,Write,NotebookEdit,Bash --output-format stream-json --verbose
codex: codex exec --sandbox read-only --skip-git-repo-check -c developer_instructions=<fluency as a TOML string> <request>
opencode: opencode run --standalone <request>, with OPENCODE_CONFIG_CONTENT naming the fluency file as instructions and denying edit and bash
muse: muse exec -c run.developer_prompt=<fluency as a JSON string> <request>, inside Muse's own sandbox
model: With a model, each tool also gets its model option before the request, as `piton agent` gives it.
```

## Verdict

The verdict is the last line in the tool's answer that carries PITON-VERDICT followed by a JSON object with a boolean pass, a reason, and, when pass is false, an explanation and a fix, even wrapped in backticks. So the request echoed back, whose example isn't valid JSON, doesn't count, and a verdict the tool revised is replaced by the revision. For Claude the answer is the text of its messages and its final result; for the other tools it is what they print, on stdout first, then stderr. The reason is the verdict's reason, and is empty when it gives none.
A test against both is judged in the specification and in the code separately, so its verdict also carries a result for each, spec and code, each with its own pass and reason.
```text
PITON-VERDICT: {"pass": false, "reason": "The code has no save handler", "spec": {"pass": true, "reason": "The spec says Save saves"}, "code": {"pass": false, "reason": "No handler saves the document"}, "explanation": "...", "fix": "..."}
```
With both results, the test passes only when both pass, whatever its overall pass says, so a tool can't pass a test whose specification and code disagree: true in one and false in the other is a failure. A verdict without both results falls back to its overall pass. A test against spec or code alone has one result, and any results for each side in its verdict are ignored.
A session that exits with an error, or ends without a verdict, is an error rather than a failure: the test was never judged. The error says which: the tool's exit code with the last three lines it wrote to stderr, or to stdout when stderr is empty, or that it ended without a verdict line.

## Concurrency

Tests run in parallel, up to the number of jobs at once, and each result is printed as it finishes. `--serial` runs them one after another, in the order they are listed.

## Progress

A test can take minutes, so a run never sits silent while one is being judged. On a terminal, a status block below the results is redrawn as things change: a spinner with how many tests are done and running, then one line per running test with its name, how long it has been running, and what its tool did last. Results are printed above the block as they finish, and the block is gone when the run ends.
What a tool did last comes from its output as it is written. Claude's events name its tool calls, like reading crates/piton-cli/src/main.rs or searching spec for a pattern, with paths relative to the project, and the first line of what it says; it reads thinking while it works on its first turn. For the other tools it is the last line they printed.
When stderr isn't a terminal, a block would only clutter a log, so a line saying running and the test's name is written as each test starts instead.

## Output

Results go to stdout, one block per test, separated by blank lines, laid out at the terminal's width up to 100 columns, or 100 when stdout isn't a terminal.
```text
✗ FAIL   SaveButtonSaves                                  1:02
         spec/tests/Editor.pi · editor, save
         No handler saves the document
         The Save button has no click handler. No function in src writes the document to disk.
```
The first line is the status as a mark and a word, ✓ pass, ✗ FAIL or ! error, then the test's name, and how long it ran at the right edge, as minutes and seconds. The second, dimmed, is the file it is declared in and its tags. For a test against both with a result for each side, a line for the specification and one for the code follow, each with its own mark and word and its reason, so a test that holds in one and not the other shows both, though it fails.
```text
✗ FAIL   SaveButtonSaves                                  1:02
         spec/tests/Editor.pi · editor, save
         spec ✓ pass  The spec says Save saves
         code ✗ FAIL  No handler saves the document
         The code has no save handler
         The Save button has no click handler.
```
Then the agent's reason, aligned under the name, and for a failure its ASD-STE100 explanation beneath the reason, in the status's colour. The reason, the explanation and each side's reason are each printed as one

### Line

not hard-wrapped, with no indent on continuation lines and no

bar. The terminal wraps a long line, so a user can select a result in the console and copy and paste it as coherent text. A name too long for the line is cut with an ellipsis.
When anything didn't pass, the run ends with a recap headed Not

### Passing

one line per failed or unjudged test, with its status, name

and file, so the tests to look at aren't lost above a long run. A failed test against both also says which side failed, like `(spec pass, code FAIL)`.
After the recap, under the heading Fix prompts, comes each failed test's fix prompt, in the order the tests are listed, whatever order they finished in. Each is headed by a rule with the test's name, and is printed whole, not wrapped or indented, so it can be copied to an agent as it is.
The outcome comes last, on stderr: `✓ 9 tests passed in 4:12` when everything passed, and otherwise the count, what passed, failed and could not be judged, and how long the run took, like `✗ 9 tests: 7 passed, 1 failed, 1 could not be judged in 4:12`.

## List

`--list` prints the tests a run would include, under the file each is declared in, files in order and separated by blank lines. Each test is its name, then dimmed what it is judged against and its tags, then its statement wrapped beneath. The outcome is how many tests in how many files, like `✓ 9 tests in 3 files`. Nothing is run.
```text
spec/tests/Editor.pi
  SaveButtonSaves  against code · editor, save
    The Save button saves the document
```

## Colour

On a terminal, the status is coloured, green for pass, red for FAIL and yellow for error, names are bold, files in a listing are highlighted, and details, times and the progress block are dimmed. Piped output, and any run with NO_COLOR set, is the same text without colour, since the mark and the word already say the status.

## Errors

A project with no tests is reported and succeeds. Targets and tags that select no tests, and a target that matches no test by name, file or directory, are errors. A tool that is not on PATH is an error naming it, with a suggestion to choose another with `--agent`, and nothing is run. `--jobs` with `--serial` is an error, since they disagree, and so is a jobs value below 1.

## Exit Code

```
success: 0
failures: 1
errors: 1
```
