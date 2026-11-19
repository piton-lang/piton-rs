// This module is the entry of the init plugin: `piton init`, which starts a
// project by asking what it should be.
//
// It asks for the project's name, which is also its directory, then its type:
// basic, application or tether. A basic project picks its renderer. An
// application picks whether it uses Belay, and with Belay its adapters, code
// root and shape root, then whether it has tests and the crux skills. A
// tether picks whether it has tests. Every question has an argument that
// answers it instead, and without a terminal they take their defaults.
//
// The configuration is written here, from the answers. Every other file
// comes from the parts in the plugin's templates/ directory, which parts.ts
// combines. They are embedded with the plugin, so init works offline and a
// release always writes the parts it shipped with.
//
// A project that declares dependencies, like crux, has them tethered once its
// files are written, so it builds straight away; `--no-tether` leaves that
// for `piton tether`.
//
// Nothing is ever overwritten: when any file init would write already exists,
// the host says which and writes none of them.

import { definePlugin, type Context, type Json } from "@piton/plugin";
import { ADAPTERS, adaptersNamed, type Adapter } from "./adapters";
import { CONFIG, SPEC_ROOT, config, type Answers, type Belay, type Kind } from "./config";
import { files } from "./parts";

/** Thrown when the user cancels a question; init then writes nothing. */
class Cancelled extends Error {}

/** Thrown once an error is reported; init then writes nothing. */
class Stopped extends Error {}

const KINDS: { kind: Kind; description: string }[] = [
  { kind: "basic", description: "Plain Piton, compiled to JSON, YAML or Markdown" },
  { kind: "application", description: "An application's specification, with Belay to guide agents in its code" },
  { kind: "tether", description: "A package other projects install with piton tether" },
];

const RENDERERS = ["json", "yaml", "markdown"];

const DEFAULT_ADAPTER = "claude-code";
const DEFAULT_CODE_ROOT = "src";
const DEFAULT_SHAPE_ROOT = `${SPEC_ROOT}/shape`;

function init(ctx: Context, args: Record<string, Json>): number {
  try {
    return start(ctx, args);
  } catch (error) {
    if (error instanceof Cancelled) {
      ctx.report.error("init was cancelled, so nothing was written");
      return 1;
    }
    if (error instanceof Stopped) return 1;
    throw error;
  }
}

function start(ctx: Context, args: Record<string, Json>): number {
  const asking = ctx.terminal.stdin && ctx.terminal.stdout;
  const text = (key: string) => (typeof args[key] === "string" ? (args[key] as string) : null);
  const flag = (key: string) => args[key] === true;
  const stop = (message: string): never => {
    ctx.report.error(message);
    throw new Stopped();
  };

  const directory = text("name") ?? (asking ? askName(ctx) : ".");
  const name = ctx.directoryName(directory);

  // The type.
  let kind: Kind;
  const typed = text("type");
  if (typed !== null) {
    const found = KINDS.find((entry) => entry.kind === typed);
    if (found === undefined) stop(`\`${typed}\` is not a type of project; the types are ${quoted(KINDS.map((entry) => entry.kind))}`);
    kind = found!.kind;
  } else if (asking) {
    kind = pickKind(ctx);
  } else {
    ctx.report.error(`pick the type of project with --type: ${quoted(KINDS.map((entry) => entry.kind))}`);
    throw new Stopped();
  }

  // Arguments the type doesn't take.
  const given = (keys: string[]) => keys.filter((key) => args[key] !== null && args[key] !== undefined && args[key] !== false && !(Array.isArray(args[key]) && (args[key] as Json[]).length === 0));
  const refuse = (keys: string[], what: string) => {
    const wrong = given(keys);
    if (wrong.length > 0) stop(`--${wrong[0]} is only for ${what}`);
  };
  const belayKeys = ["belay", "no-belay", "adapter", "code-root", "shape-root", "crux", "no-crux"];
  if (kind !== "basic") refuse(["renderer"], "a basic project");
  if (kind !== "application") refuse(belayKeys, "an application");
  if (kind === "basic") refuse(["testing", "no-testing"], "an application or a tether");

  // A basic project's renderer.
  let renderer: string | null = null;
  if (kind === "basic") {
    renderer = text("renderer");
    if (renderer !== null && !RENDERERS.includes(renderer)) {
      stop(`\`${renderer}\` is not a renderer; the renderers are ${quoted(RENDERERS)}`);
    } else if (renderer === null) {
      renderer = asking ? RENDERERS[chooseIndex(ctx, "Compile to:", RENDERERS)] : RENDERERS[0];
    }
  }

  // An application's Belay.
  let belay: Belay | null = null;
  if (kind === "application") {
    const uses = flag("belay") || (!flag("no-belay") && (asking ? confirm(ctx, "Use Belay to guide agents in the code?", true) : true));
    if (uses) {
      belay = askBelay(ctx, args, asking, stop);
    } else {
      refuse(["adapter", "code-root", "shape-root", "crux", "no-crux"], "an application that uses Belay");
    }
  }

  // Tests.
  let testing = false;
  if (kind !== "basic") {
    const against = kind === "tether" ? "the specification" : "the specification and the code";
    testing = flag("testing") || (!flag("no-testing") && (!asking || confirm(ctx, `Set up piton test, judged against ${against}?`, true)));
  }

  // The crux skills.
  let crux = false;
  if (belay !== null) {
    crux = flag("crux") || (!flag("no-crux") && (asking ? confirm(ctx, "Add the crux skills?", true) : true));
  }

  const answers: Answers = { name, kind, renderer, belay, testing, crux };
  const parts = [kind, ...(belay !== null ? ["belay"] : []), ...(testing ? ["testing"] : []), ...(crux ? ["crux"] : [])];
  const written = [
    { path: CONFIG, contents: config(ctx, answers) },
    ...files(ctx, parts, {
      name,
      adapters: belay?.adapters.map((adapter) => adapter.targetId) ?? [],
      codeRoot: belay?.codeRoot ?? DEFAULT_CODE_ROOT,
      shapeRoot: belay?.shapeRoot ?? DEFAULT_SHAPE_ROOT,
      specRoot: SPEC_ROOT,
    }),
  ];

  const { existing } = ctx.scaffold(directory, written);
  if (existing.length > 0) {
    ctx.report.error(
      `\`${directory}\` already has ${existing.length === 1 ? "a file" : "files"} init would write, so nothing was written`,
    );
    for (const path of existing) ctx.report.note(display(directory, path));
    return 1;
  }

  // A project that depends on packages has them tethered, so it builds
  // without another step.
  let untethered = false;
  const tethered = ctx.tether(directory, { check: flag("no-tether") });
  if (tethered === "pending") {
    untethered = true;
  } else if (tethered === "failed") {
    ctx.report.help(`the project was written; run \`piton tether\` in ${directory} to install its dependencies`);
    return 1;
  }

  const count = ctx.paint("dim", `(${written.length} ${written.length === 1 ? "file" : "files"})`);
  ctx.report.done(`Created ${kind === "basic" ? "basic project" : kind} ${ctx.paint("name", name)}${described(answers)} ${count}`);
  ctx.stderr(`\n${ctx.paint("heading", "Next")}\n`);
  if (directory !== ".") ctx.stderr(`  ${ctx.paint("name", `cd ${directory}`)}\n`);
  if (untethered) ctx.stderr(`  ${ctx.paint("name", "piton tether")}\n`);
  ctx.stderr(`  ${ctx.paint("name", "piton build")}\n`);
  return 0;
}

/** What the project was given, for the line saying it was created. */
function described(answers: Answers): string {
  if (answers.renderer !== null) return `, compiled to ${answers.renderer}`;
  const extras: string[] = [];
  if (answers.belay !== null) extras.push(`Belay for ${answers.belay.adapters.map((adapter) => adapter.targetId).join(", ")}`);
  if (answers.testing) extras.push("piton test");
  if (answers.crux) extras.push("the crux skills");
  if (extras.length === 0) return "";
  const last = extras.pop()!;
  return ` with ${extras.length > 0 ? `${extras.join(", ")} and ${last}` : last}`;
}

/** Belay's adapters and roots, from the arguments or asked for. */
function askBelay(ctx: Context, args: Record<string, Json>, asking: boolean, stop: (message: string) => never): Belay {
  let adapters: Adapter[];
  const requested = (Array.isArray(args.adapter) ? args.adapter : [])
    .flatMap((value) => String(value).split(","))
    .filter((name) => name.trim().length > 0);
  const defaults = ADAPTERS.filter((adapter) => adapter.targetId === DEFAULT_ADAPTER);
  if (requested.length > 0) {
    const named = adaptersNamed(requested);
    if ("unknown" in named) stop(`\`${named.unknown}\` is not an adapter; the adapters are ${quoted(ADAPTERS.map((adapter) => adapter.targetId))}`);
    adapters = (named as { chosen: Adapter[] }).chosen;
  } else if (asking) {
    adapters = pickAdapters(ctx, defaults);
  } else {
    adapters = defaults;
  }

  const codeRoot = root(ctx, args["code-root"], asking, {
    message: "Code root:",
    fallback: DEFAULT_CODE_ROOT,
    problem: () => null,
    stop,
  });
  const shapeRoot = root(ctx, args["shape-root"], asking, {
    message: "Shape root:",
    fallback: DEFAULT_SHAPE_ROOT,
    problem: (path) => (path.startsWith(`${SPEC_ROOT}/`) ? null : `the shape root has to be beneath ./${SPEC_ROOT}`),
    stop,
  });
  return { adapters, codeRoot, shapeRoot };
}

/** A root from its argument, or asked for until it is one, or its default. */
function root(
  ctx: Context,
  given: Json | undefined,
  asking: boolean,
  how: { message: string; fallback: string; problem: (path: string) => string | null; stop: (message: string) => never },
): string {
  const check = (typed: string): { path: string } | { problem: string } => {
    const path = normalized(typed);
    if (path === null) return { problem: `\`${typed}\` has to be a path inside the project` };
    const problem = how.problem(path);
    return problem === null ? { path } : { problem };
  };
  if (typeof given === "string") {
    const result = check(given);
    if ("problem" in result) how.stop(result.problem);
    return (result as { path: string }).path;
  }
  if (!asking) return how.fallback;
  for (;;) {
    const answer = ctx.ask(how.message, { help: `relative to the project; enter for ./${how.fallback}` });
    if (answer === null) throw new Cancelled();
    if (answer.trim() === "") return how.fallback;
    const result = check(answer.trim());
    if ("path" in result) return result.path;
    ctx.stderr(`${ctx.paint("error", result.problem)}\n`);
  }
}

/** A path inside the project without `./` or a trailing slash, `.` for the
 * project itself; null when it isn't inside the project. */
function normalized(path: string): string | null {
  if (path.startsWith("/") || /^[A-Za-z]:/.test(path)) return null;
  const parts = path.split(/[/\\]+/).filter((part) => part !== "" && part !== ".");
  if (parts.includes("..")) return null;
  return parts.length === 0 ? "." : parts.join("/");
}

/** Asks for the project's name, which is the directory it is created in. An
 * empty answer creates it in the current directory. */
function askName(ctx: Context): string {
  const answer = ctx.ask("Project name:", {
    help: "also the directory to create it in; leave empty for the current one",
  });
  if (answer === null) throw new Cancelled();
  return answer.trim() === "" ? "." : answer.trim();
}

/** Asks which type of project to start. */
function pickKind(ctx: Context): Kind {
  const width = Math.max(...KINDS.map((entry) => entry.kind.length));
  const index = chooseIndex(
    ctx,
    "Type of project:",
    KINDS.map((entry) => `${entry.kind.padEnd(width)}  ${entry.description}`),
  );
  return KINDS[index].kind;
}

function chooseIndex(ctx: Context, message: string, options: string[]): number {
  const index = ctx.choose(message, options);
  if (index === null) throw new Cancelled();
  return index;
}

function confirm(ctx: Context, message: string, fallback: boolean): boolean {
  const answer = ctx.confirm(message, { default: fallback });
  if (answer === null) throw new Cancelled();
  return answer;
}

/** Asks which adapters to build for, with `defaults` already checked. At
 * least one has to be chosen. */
function pickAdapters(ctx: Context, defaults: Adapter[]): Adapter[] {
  const width = Math.max(...ADAPTERS.map((adapter) => adapter.platform.length));
  const chosen = ctx.chooseMany(
    "Adapters:",
    ADAPTERS.map((adapter) => `${adapter.platform.padEnd(width)}  ${adapter.targetId}`),
    {
      defaults: ADAPTERS.flatMap((adapter, index) => (defaults.includes(adapter) ? [index] : [])),
      minimum: 1,
    },
  );
  if (chosen === null) throw new Cancelled();
  return ADAPTERS.filter((_, index) => chosen.includes(index));
}

function quoted(names: string[]): string {
  return names.map((name) => `\`${name}\``).join(", ");
}

/** A written path as the person running init would type it. */
function display(directory: string, path: string): string {
  return directory === "." ? path : `${directory.replace(/\/+$/, "")}/${path}`;
}

export default definePlugin({ commands: { init } });
