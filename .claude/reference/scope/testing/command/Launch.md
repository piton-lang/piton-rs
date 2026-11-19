# Test Launch

## Claude

claude -p <request> --append-system-prompt-file <fluency file> --allowedTools Read,Grep,Glob --disallowedTools Edit,Write,NotebookEdit,Bash --output-format stream-json --verbose

## Codex

codex exec --sandbox read-only --skip-git-repo-check -c developer_instructions=<short prompt as a TOML string> <request>

## Opencode

opencode run --standalone <request>, with OPENCODE_CONFIG_CONTENT naming the fluency file as instructions and denying edit and bash

## Muse

muse exec -c run.developer_prompt=<short prompt as a JSON string> <request>, inside Muse's own sandbox

## Model

With a model, each tool also gets its model option before the request, as `piton agent` gives it.
