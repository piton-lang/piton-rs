# Test Running

## Description

A test runs as one headless session of the tool, in the project root, with nothing on its standard input. Its request gives the test's context, when it has one, as a preamble before the statement, then the statement, the test's description as context from the test's author, the test's name and the file it is declared in, and what to judge it against, with the paths of the source root and code root relative to the project. Tags are only for selection and are not sent.
The tool is asked to read whatever it needs but not to change, create or delete any file, to base its verdict on what it finds rather than on what is likely, so a statement fails when the evidence contradicts it or isn't there, and to end its answer with one verdict line.
```text
PITON-VERDICT: {"pass": true, "reason": "SaveButton's onClick calls saveDocument"}
PITON-VERDICT: {"pass": false, "reason": "No handler saves the document", "explanation": "The Save button has no click handler. No function in src writes the document to disk.", "fix": "Add a click handler to SaveButton in src/editor.ts. Make the handler call saveDocument."}
```
Every tool is run in its own non-interactive mode, read-only wherever the tool can enforce it. Asking is not enforcement, so the command never claims more than the tool provides: Claude is limited to reading and searching, Codex runs in its read-only sandbox, OpenCode has editing and the shell denied, and Muse relies on its own default sandbox.
Each tool gets the prompt Belay gives it, as `piton agent` does. Claude and OpenCode read the full fluency prompt from a temporary file, written for the run and removed when the run ends; Belay writes it, with its own capabilities. Codex and Muse take their instructions only as an argument, which the operating system limits in size, so they get Belay's short prompt, which tells them to read the piton-fluency skill, or a temporary file holding the reference when no build wrote the skill. The plugin declares temporary files among its capabilities, as running the four tools is why it declares them.
