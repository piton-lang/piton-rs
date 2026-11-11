# Agent Cli

## Description

Launches the specified agent with Piton fluency using the output of the [GenerateFluencyPrompt](../../../skills/generate-fluency-prompt/SKILL.md) skill, which is FLUENCY_PROMPT.md in the project root. Without one, the copy the Belay plugin ships with, plugins/belay/fluency.md in the compiler's source, is used.

## Command Name

agent

## Positional Arguments

```
agent: Which agent to run [ claude | codex | opencode | muse ], claude by default
args: Anything after the agent is passed through to it
```

## Named Arguments

```
model: The model the agent runs with, `-m` for short, given to the tool as its own model option; given with no model, it lists the models of the tool and starts nothing; given a value that is not one of the models, it lists the models that contain it, else all of them, and starts nothing
print-fluency: Print the entire fluency prompt
```

## Plugin

Added by the Belay plugin, which knows the agentic tools and their adapters. The same tools and prompts are what Belay exports to other plugins, like the test plugin.

## Prompt

The fluency prompt is a short primer about the project followed by the fluency reference. The primer names the source root, the reference root and the scoped instruction file the launched tool reads. Those are the tool's own when the project builds for its adapter. In a shared project the reference root is the first adapter's, unless the tool's own reference tree was written. When the project doesn't build for the tool, they are the first configured adapter's.

## Short Prompt

The operating system limits the size of one argument; Linux accepts less than 128 KiB. The fluency reference is near that size, so a tool that takes its instructions only as an argument gets a short prompt

### Instead

the primer, then a line that tells it to read the fluency

reference in full before any work. The line names the piton-fluency skill the build wrote, looked for first in the tool's own skill directory and then in the other adapters', so a tool can also load it as a skill. When no build wrote the skill, the line names a temporary file that holds the reference. Codex and Muse get the short prompt. Claude and OpenCode read the full prompt from a temporary file. --print-fluency prints the full prompt for every tool.

## Launch

```
description: Each tool gets the prompt added to its own instructions, never replacing them, and the agent runs in the project root. A temporary file is removed when the command ends.
terminal: The tool runs with the terminal, so its input, output and prompts are the user's, and the command waits for it.
claude: claude --append-system-prompt-file <file holding the prompt>
codex: codex -c developer_instructions=<short prompt as a TOML string>. Codex has no option that adds developer instructions from a file; model_instructions_file replaces its base instructions, which Piton never does.
opencode: OpenCode takes extra instructions only as files. The prompt is written to a temporary file, removed when OpenCode exits, and OpenCode runs as opencode --standalone with OPENCODE_CONFIG_CONTENT set to a configuration whose instructions list that file. Its server reads that variable, so it runs a private server rather than the shared background one.
muse: muse -c run.developer_prompt=<short prompt as a JSON string>. Muse has no documented option that adds a developer prompt from a file.
tooLarge: An argument or environment value too large for the operating system is refused before the tool starts, with an error that names its size and the limit.
subcommands: Muse's exec and OpenCode's run take the prompt options after the subcommand name, so they go there. A subcommand that rejects them, like muse resume or opencode models, runs as given, without the prompt.
```

## Models

```
description: Every tool that lets an agent be chosen also lets its model be chosen. `--model` names the model, and the tool is given it with its own option: claude --model <model>, codex -c model=<model as a TOML string>, opencode --model <provider/model>, and muse --model <model>. They go with the prompt options, so a subcommand that takes those takes the model too.
listing: When no model is given, the models of the tool are listed. On a terminal they are a menu, with Default model first, which keeps the tool's own default; cancelling the menu starts nothing. Piped, the tool runs with its default model and a note lists its models and says to choose one with `--model`. `--print-fluency` starts nothing, so it lists nothing.
listOnly: `--model` with no model lists the models of the tool, one on each line on stdout, from the same sources, starts nothing and exits with 0, like `piton agent opencode --model`. A tool that lists no models is an error, and the exit code is 1.
search: A `--model` value that is one of the tool's models is that model. Any other value is a search string: the models of the tool that contain it, ignoring case, are listed one on each line on stdout, like `piton agent opencode --model sonnet`. When no model contains it, every model is listed, and the outcome line says that nothing matched. Either way nothing starts and the exit code is 0. A tool that lists no models cannot be searched, so its value is given to the tool as the model.
sources:
  claude: Claude has no command that lists models, so the list is the aliases its --model takes for the latest model of each family, fable, opus, sonnet and haiku.
  codex: The slugs that codex debug models lists for its own model picker.
  opencode: The lines opencode models prints, each provider/model.
  muse: Muse has no command that lists its catalog, so the list is the catalog id its help names, muse-spark-1.3.
  failure: A tool that is not on PATH, or whose listing fails, lists no models, and the note says so.
exports: Belay also exports the models of each tool, the options that select a model, and a model for the launch with a first message, so other plugins that choose an agent, like the test plugin, offer models the same way. The interactive capability lets `piton agent` show the menu.
```

## Errors

A tool that is not on PATH is reported with a suggestion to print the prompt instead. The tool's own exit status is piton agent's.
