// This module knows the agentic tools: Claude Code, Codex, OpenCode and Muse
// Code.
//
// Each tool gets a fluency prompt. The prompt is a short primer about the
// project, then the Piton fluency reference. The reference is the
// FLUENCY_PROMPT.md file of the project. If the project has no such file, the
// plugin uses the copy that it ships with.
//
// `piton agent` starts a tool with the prompt. Other plugins, like the test
// plugin, use the tools and the prompt through the exports of this plugin.

import type { Context, Json } from "@piton/plugin";
import { ADAPTERS, adapterFor, type Adapter } from "./adapters";
import { FLUENCY_SKILL, builtSkill, reference } from "./fluency";

/** An agentic tool that Belay can start. */
export interface Tool {
  /** The name on the command line. */
  name: string;
  /** The program that runs. */
  program: string;
  /** The Belay target whose files the tool reads. */
  target: string;
  /** Subcommands that refuse the options that carry the prompt. They run as
   * given, without the prompt. */
  plainSubcommands: string[];
  /** The one subcommand that takes the prompt options after its name. */
  promptedSubcommand: string | null;
  /** How the tool lists its models: a command whose output names them, or a
   * fixed list when the tool has no such command. */
  models: { command: string[]; parse: (output: string) => string[] } | { fixed: string[] };
}

/** The slugs of the Codex model catalog that Codex shows in its own list. */
function codexModels(output: string): string[] {
  try {
    const catalog = JSON.parse(output);
    return (Array.isArray(catalog?.models) ? catalog.models : [])
      .filter((model: any) => model?.visibility === "list" && typeof model?.slug === "string")
      .map((model: any) => model.slug as string);
  } catch {
    return [];
  }
}

/** One model on each line, like `provider/model`. */
function modelLines(output: string): string[] {
  return output
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => /^[^\s]+\/[^\s]+$/.test(line));
}

export const TOOLS: Tool[] = [
  {
    name: "claude",
    program: "claude",
    target: "claude-code",
    plainSubcommands: [],
    promptedSubcommand: null,
    // Claude has no command that lists models. These are the aliases that its
    // --model takes for the latest model of each family.
    models: { fixed: ["fable", "opus", "sonnet", "haiku"] },
  },
  {
    name: "codex",
    program: "codex",
    target: "codex",
    plainSubcommands: [],
    promptedSubcommand: null,
    models: { command: ["debug", "models"], parse: codexModels },
  },
  {
    name: "opencode",
    program: "opencode",
    target: "opencode",
    plainSubcommands: [
      "upgrade", "update", "uninstall", "acp", "api", "debug", "auth", "mcp", "plugin",
      "models", "stats", "mini", "session", "service",
    ],
    promptedSubcommand: "run",
    models: { command: ["models"], parse: modelLines },
  },
  {
    name: "muse",
    program: "muse",
    target: "muse",
    plainSubcommands: [
      "resume", "config", "export", "trace", "model-profile", "skills", "voice", "plugins",
      "sandbox", "schema", "serve", "session-message", "mcp", "auth", "login", "logout",
      "init",
    ],
    promptedSubcommand: "exec",
    // Muse has no command that lists its catalog. Its help names this one.
    models: { fixed: ["muse-spark-1.3"] },
  },
];

/** The tool with this name, or null. */
export function tool(name: string): Tool | null {
  return TOOLS.find((candidate) => candidate.name === name) ?? null;
}

/** The models that a tool offers. A tool that lists its models is asked; when
 * it cannot answer, the list is empty. */
export async function models(ctx: Context, chosen: Tool): Promise<string[]> {
  if ("fixed" in chosen.models) return chosen.models.fixed;
  if (!ctx.onPath(chosen.program)) return [];
  let output = "";
  try {
    const process = ctx.spawn(chosen.program, chosen.models.command);
    for await (const { stream, line } of process) if (stream === "stdout") output += `${line}\n`;
    if ((await process.wait()) !== 0) return [];
  } catch {
    return [];
  }
  return chosen.models.parse(output);
}

/** The item of the model menu that keeps the tool's own default. */
export const DEFAULT_MODEL = "Default model";

/** Picks a model when none was given. On a terminal the models are a menu,
 * with the tool's default first. Elsewhere they are listed in a note, and the
 * tool uses its default. Gives null for the default, or undefined when the
 * user cancelled the menu. */
export async function pickModel(ctx: Context, chosen: Tool): Promise<string | null | undefined> {
  const offered = await models(ctx, chosen);
  if (ctx.terminal.stdin && ctx.terminal.stdout && offered.length > 0) {
    const picked = ctx.choose(`Which model should ${chosen.name} use?`, [DEFAULT_MODEL, ...offered]);
    if (picked === null) return undefined;
    return picked === 0 ? null : offered[picked - 1];
  }
  noteModels(ctx, chosen, offered);
  return null;
}

/** Tells the user that the tool uses its default model, and lists the models. */
export function noteModels(ctx: Context, chosen: Tool, offered: string[]): void {
  ctx.report.note(
    offered.length > 0
      ? `${chosen.name} uses its default model; its models are ${offered.join(", ")}; choose one with --model`
      : `${chosen.name} uses its default model; it listed no models; choose one with --model`,
  );
}

/** The models of `offered` that contain `search`, ignoring case. */
export function searchModels(offered: string[], search: string): string[] {
  const needle = search.toLowerCase();
  return offered.filter((model) => model.toLowerCase().includes(needle));
}

/** Reads a `--model` value. No value gives no model. A value that names one
 * of the tool's models, or any value when the tool lists no models, is the
 * model. Otherwise the value is a search: the models that contain it are
 * printed, one on each line, or every model when none contain it, and
 * nothing starts. An empty value prints every model. A tool that lists no
 * models when a list was asked for is an error. */
export async function modelFlag(ctx: Context, chosen: Tool, value: Json | undefined): Promise<{ model: string | null } | { exit: number }> {
  if (typeof value !== "string") return { model: null };
  const search = value.trim();
  const offered = await models(ctx, chosen);
  if (search.length > 0 && (offered.length === 0 || offered.includes(search))) return { model: search };
  if (offered.length === 0) {
    ctx.report.error(`${chosen.name} listed no models`, {
      help: `check that \`${chosen.program}\` is on PATH and can list its models`,
    });
    return { exit: 1 };
  }
  const found = search.length > 0 ? searchModels(offered, search) : offered;
  const shown = found.length > 0 ? found : offered;
  for (const model of shown) ctx.stdout(`${model}\n`);
  const count = (n: number) => `${n} ${n === 1 ? "model" : "models"}`;
  if (search.length === 0) ctx.report.done(`${chosen.name} offers ${count(offered.length)}`);
  else if (found.length > 0) ctx.report.done(`${count(found.length)} of ${chosen.name} ${found.length === 1 ? "matches" : "match"} \`${search}\`; choose one with --model`);
  else ctx.report.done(`no model of ${chosen.name} matches \`${search}\`, so all ${count(offered.length)} are listed`);
  return { exit: 0 };
}

/** What the agent is told about the project before it starts. */
function primer(sourceRoot: string, referenceRoot: string, instructionFile: string): string {
  return (
    "This project is written in Piton, a declarative language for agent guidance.\n" +
    "\n" +
    `- Source of truth: the \`.pi\` files under \`${sourceRoot}\`. Compiled Markdown is derived output.\n` +
    "- Make lasting changes in the Piton source, then run `piton build`; never hand-edit " +
    "generated artifacts.\n" +
    `- Compiled reference documents live under \`${referenceRoot}\`. Links between them are ` +
    "lazy: follow one when the work touches what it describes.\n" +
    `- Scoped guidance is written to \`${instructionFile}\` files placed next to the code ` +
    "they describe.\n" +
    "- `piton check` validates the specbase; `piton reach` shows what the entrypoints can see; " +
    "`piton slice file.pi#Anchor.property` prints just the spec one thing depends on."
  );
}

/** The adapters that the project configures, in their order, by target id. */
function configured(ctx: Context): Adapter[] {
  const settings = ctx.settings();
  if (settings === null) return [];
  const listed = settings.properties.adapters;
  const out: Adapter[] = [];
  for (const item of Array.isArray(listed) ? listed : []) {
    const name = item !== null && typeof item === "object" && !Array.isArray(item) ? item.$anchor : null;
    if (typeof name !== "string") continue;
    const id: Json | undefined = settings.anchors[name]?.targetId;
    const adapter = typeof id === "string" ? adapterFor(id.trim()) : null;
    if (adapter !== null) out.push(adapter);
  }
  return out;
}

/** Where a tool finds the compiled guidance of the project: the reference
 * root and the scoped instruction file.
 *
 * When the project builds for the tool, the instruction file is the file of
 * the tool. The reference root is the root of the tool when a build wrote it.
 * In a shared project only the first adapter writes a reference tree, and
 * all tools link into that tree. */
export function layout(ctx: Context, chosen: Tool): { referenceRoot: string; instructionFile: string } {
  const own = adapterFor(chosen.target) ?? ADAPTERS[0];
  const adapters = configured(ctx);
  const first = adapters[0] ?? null;
  const enabled = adapters.some((adapter) => adapter.targetId === chosen.target);
  const instructionFile = first !== null && !enabled ? first.instructionFile : own.instructionFile;
  let referenceRoot: string;
  if (first === null) {
    referenceRoot = own.referenceRoot;
  } else if (enabled && ctx.stat(own.referenceRoot)?.kind === "directory") {
    referenceRoot = own.referenceRoot;
  } else {
    referenceRoot = first.referenceRoot;
  }
  return { referenceRoot, instructionFile };
}

/** The primer of the project for a tool. */
function projectPrimer(ctx: Context, chosen: Tool): string {
  const { referenceRoot, instructionFile } = layout(ctx, chosen);
  return primer(ctx.project().sourceRoot, referenceRoot, instructionFile);
}

/** The full fluency prompt for a tool: the primer, then the reference. */
export function fluency(ctx: Context, chosen: Tool): string {
  return `${projectPrimer(ctx, chosen)}\n\n${reference(ctx).trimEnd()}`;
}

/** The short prompt for a tool that cannot take the full prompt as an
 * argument: the primer, then a pointer to the reference.
 *
 * The pointer names the `piton-fluency` skill that the build wrote. When no
 * build wrote the skill, the pointer names a temporary file that holds the
 * reference. */
export function pointer(ctx: Context, chosen: Tool): string {
  const skill = builtSkill(ctx, adapterFor(chosen.target));
  const path = skill ?? ctx.temporaryFile(reference(ctx), ".md");
  const what = skill === null ? "the file" : `the \`${FLUENCY_SKILL}\` skill, at`;
  return (
    `${projectPrimer(ctx, chosen)}\n\n` +
    `Before any work, read the Piton fluency reference in full: ${what} \`${path}\`. ` +
    "It tells how Piton, Belay and the piton CLI work."
  );
}

/** The text that a tool gets as its prompt, and the file that holds it when
 * the tool reads its prompt from a file.
 *
 * Claude and OpenCode read the full prompt from a temporary file. Codex and
 * Muse take the prompt as an argument, and the operating system limits the
 * size of one argument. Thus they get the short prompt. */
export function prompt(ctx: Context, chosen: Tool): { text: string; file: string | null } {
  switch (chosen.name) {
    case "claude":
    case "opencode": {
      const text = fluency(ctx, chosen);
      return { text, file: ctx.temporaryFile(text, ".md") };
    }
    default:
      return { text: pointer(ctx, chosen), file: null };
  }
}

/** The arguments and environment that start a tool with the prompt in its
 * instructions, and the given arguments after it.
 *
 * Each tool adds the prompt to its own instructions and does not replace
 * them. Claude appends the prompt file to the system prompt. Codex and Muse
 * take the short prompt as developer instructions. OpenCode reads the prompt
 * file as an instructions file. It runs a private server, because its server
 * reads the configuration from the environment. */
export function launch(
  ctx: Context,
  chosen: Tool,
  prompt: string,
  promptFile: string | null,
  args: string[],
  model: string | null = null,
): { args: string[]; env: Record<string, string> } {
  const env: Record<string, string> = {};
  let options: string[];
  switch (chosen.name) {
    case "claude":
      options = ["--append-system-prompt-file", promptFile ?? ""];
      break;
    case "codex":
      options = ["-c", `developer_instructions=${ctx.encode("tomlString", prompt)}`];
      break;
    case "opencode":
      env.OPENCODE_CONFIG_CONTENT = JSON.stringify({ instructions: [promptFile] });
      options = ["--standalone"];
      break;
    case "muse":
      options = ["-c", `run.developer_prompt=${JSON.stringify(prompt)}`];
      break;
    default:
      throw new Error(`no launcher for \`${chosen.name}\``);
  }
  if (model !== null) options = [...options, ...modelOptions(ctx, chosen, model)];
  const first = args[0];
  if (first !== undefined && chosen.plainSubcommands.includes(first)) return { args, env };
  if (first !== undefined && first === chosen.promptedSubcommand) {
    return { args: [first, ...options, ...args.slice(1)], env };
  }
  return { args: [...options, ...args], env };
}

/** The options that select a model. Codex takes it as a configuration value,
 * which every Codex subcommand reads; the others take --model. */
export function modelOptions(ctx: Context, chosen: Tool, model: string): string[] {
  return chosen.name === "codex" ? ["-c", `model=${ctx.encode("tomlString", model)}`] : ["--model", model];
}

/** `piton agent`: starts a tool with Piton fluency. */
export async function agentCommand(ctx: Context, args: Record<string, Json>): Promise<number> {
  const name = typeof args.agent === "string" ? args.agent : "claude";
  const chosen = tool(name);
  if (chosen === null) {
    ctx.report.error(`unknown agent \`${name}\``);
    return 1;
  }
  // `--model` without a model, or with a value that is not one of the tool's
  // models, lists them, and nothing starts.
  const flag = await modelFlag(ctx, chosen, args.model);
  if ("exit" in flag) return flag.exit;
  // To print the prompt only reads: nothing starts. The full prompt is
  // printed, also for a tool that gets the short prompt.
  if (args["print-fluency"] === true) {
    ctx.stdout(`${fluency(ctx, chosen)}\n`);
    return 0;
  }
  if (!ctx.onPath(chosen.program)) {
    ctx.report.error(
      `\`${chosen.program}\` is not on PATH; install it or print the prompt with \`piton agent ${name} --print-fluency\``,
    );
    return 1;
  }
  let model = flag.model;
  if (model === null) {
    const picked = await pickModel(ctx, chosen);
    if (picked === undefined) {
      ctx.report.stopped("no model was chosen, so nothing started");
      return 1;
    }
    model = picked;
  }
  const given = prompt(ctx, chosen);
  const passed = Array.isArray(args.args) ? args.args.filter((arg): arg is string => typeof arg === "string") : [];
  const command = launch(ctx, chosen, given.text, given.file, passed, model);
  const code = ctx.exec(chosen.program, command.args, { env: command.env });
  return code === null ? 1 : Math.max(0, Math.min(255, code));
}

/** What other plugins use: the tools, and the fluency prompt of each. Each
 * function runs with the capabilities of this plugin. */
export function agentsExport(ctx: Context) {
  return {
    list: () => TOOLS.map(({ name, program, target }) => ({ name, program, target })),
    fluency: (name: string): string => {
      const chosen = tool(name);
      if (chosen === null) throw new Error(`\`${name}\` is not an agent Piton knows`);
      return fluency(ctx, chosen);
    },
    /** The prompt of a tool: the text, and the file that holds it when the
     * tool reads its prompt from a file. Codex and Muse get the short prompt
     * that points to the reference. */
    prompt: (name: string): { text: string; file: string | null } => {
      const chosen = tool(name);
      if (chosen === null) throw new Error(`\`${name}\` is not an agent Piton knows`);
      return prompt(ctx, chosen);
    },
    /** The models that a tool offers, as its own list gives them. */
    models: (name: string): Promise<string[]> => {
      const chosen = tool(name);
      if (chosen === null) throw new Error(`\`${name}\` is not an agent Piton knows`);
      return models(ctx, chosen);
    },
    /** Reads a `--model` value as `piton agent` does: the model to use, or
     * the exit code after the models were listed. */
    modelFlag: (name: string, value: Json | undefined): Promise<{ model: string | null } | { exit: number }> => {
      const chosen = tool(name);
      if (chosen === null) throw new Error(`\`${name}\` is not an agent Piton knows`);
      return modelFlag(ctx, chosen, value);
    },
    /** The options that make a tool use a model. */
    modelOptions: (name: string, model: string): string[] => {
      const chosen = tool(name);
      if (chosen === null) throw new Error(`\`${name}\` is not an agent Piton knows`);
      return modelOptions(ctx, chosen, model);
    },
    /** How to start a tool with the terminal and the fluency prompt, with
     * `firstMessage` as the first thing the user says to it, when given. The
     * caller runs the program, with its own capabilities. */
    launch: (name: string, firstMessage: string | null = null, model: string | null = null): { program: string; args: string[]; env: Record<string, string> } => {
      const chosen = tool(name);
      if (chosen === null) throw new Error(`\`${name}\` is not an agent Piton knows`);
      const given = prompt(ctx, chosen);
      const message = firstMessage === null ? [] : chosen.name === "opencode" ? ["--prompt", firstMessage] : [firstMessage];
      const command = launch(ctx, chosen, given.text, given.file, message, model);
      return { program: chosen.program, args: command.args, env: command.env };
    },
  };
}
