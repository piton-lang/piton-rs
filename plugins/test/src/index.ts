// This module adds the `piton test` command.
//
// A test is an exported anchor that extends the Test anchor of @piton/test.
// Each test is one statement in plain English.
// This code does not decide if a statement is true.
// It gives each test to an agentic tool in headless mode.
// The tool reads the specification, the code, or both.
// At the end of its answer, the tool writes a verdict line.
// This command reads that line.

import { definePlugin, type Anchor, type Context, type Json } from "@piton/plugin";
import { fixPrompt, judge, relative, type Launcher, type Outcome, type Sides, type TestCase } from "./judge";
import * as output from "./output";
import { Progress } from "./progress";
import * as tags from "./tags";

const SCOPES = ["spec", "code", "both"];
const AGENTS = ["claude", "codex", "opencode", "muse"];

/** These are the default values for the scope, the tool, and the number of
 * parallel tests. They apply when the flags, the test and the settings give no
 * value. */
const DEFAULT_SCOPE = "both";
const DEFAULT_AGENT = "claude";
const DEFAULT_JOBS = 4;

interface Settings {
  against: string | null;
  agent: string | null;
  model: string | null;
  jobs: number | null;
}

/** Reads the settings of the project for this plugin. Makes sure that each
 * value is correct. If a value is not correct, it reports the error at that
 * value and gives null. */
function readSettings(ctx: Context): Settings | null {
  const settings: Settings = { against: null, agent: null, model: null, jobs: null };
  const anchor = ctx.settings();
  if (anchor === null) return settings;
  let ok = true;
  const choice = (name: "against" | "agent", choices: string[], code: string): string | null => {
    const value = anchor.properties[name];
    if (value === null || value === undefined) return null;
    if (typeof value === "string" && choices.includes(value)) return value;
    ctx.report.error(`\`${String(value)}\` is not one of ${choices.join(", ")}`, {
      code,
      location: anchor.location(name),
      help: `${name} is one of ${choices.join(", ")}`,
    });
    ok = false;
    return null;
  };
  settings.against = choice("against", SCOPES, "invalid-test-scope");
  settings.agent = choice("agent", AGENTS, "unknown-test-agent");
  const model = anchor.properties.model;
  if (model !== null && model !== undefined) {
    if (typeof model === "string" && model.trim().length > 0) {
      settings.model = model.trim();
    } else {
      ctx.report.error("`model` has to name a model", {
        code: "invalid-test-model",
        location: anchor.location("model"),
        help: "leave `model` out to judge with the agent's default model",
      });
      ok = false;
    }
  }
  const jobs = anchor.properties.jobs;
  if (jobs !== null && jobs !== undefined) {
    if (typeof jobs === "number" && Number.isInteger(jobs) && jobs >= 1) {
      settings.jobs = jobs;
    } else {
      ctx.report.error("`jobs` has to be a whole number of at least 1", {
        code: "invalid-jobs",
        location: anchor.location("jobs"),
        help: "use `jobs: 1` to run the tests one after another",
      });
      ok = false;
    }
  }
  return ok ? settings : null;
}

/** Gives the plain text of a property, without spaces at the two ends. If
 * the text is empty, it gives null. */
function text(anchor: Anchor, key: string): string | null {
  const value = anchor.properties[key];
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

/** Finds each exported, concrete test in the source of the project. The
 * tests are in the order of their files, then their names. If a test is not
 * correct, it gives null. `flagScope` is the value of the `--against` flag.
 * This value has priority over the value in a test. `fallback` is the scope
 * for a test that does not give a scope. */
function collect(ctx: Context, anchors: Anchor[], flagScope: string | null, fallback: string): TestCase[] | null {
  const root = ctx.project().root;
  const tests: TestCase[] = [];
  let valid = true;
  for (const anchor of anchors) {
    const expects = text(anchor, "expects");
    if (expects === null) {
      ctx.report.error(`test \`${anchor.name}\` has no statement to judge`);
      ctx.report.note(`declared in ${relative(anchor.file, root)}`);
      valid = false;
      continue;
    }
    const own = text(anchor, "against");
    if (own !== null && !SCOPES.includes(own)) {
      ctx.report.error(
        `test \`${anchor.name}\` is judged against \`${own}\`, which is not one of ${SCOPES.join(", ")}`,
      );
      ctx.report.note(`declared in ${relative(anchor.file, root)}`);
      valid = false;
      continue;
    }
    const tagValues = anchor.properties.tags;
    tests.push({
      name: anchor.name,
      file: anchor.file,
      expects,
      description: text(anchor, "description"),
      against: flagScope ?? own ?? fallback,
      tags: Array.isArray(tagValues)
        ? tagValues
            .filter((tag): tag is string => typeof tag === "string")
            .map((tag) => tag.trim())
            .filter((tag) => tag.length > 0)
        : [],
    });
  }
  if (!valid) {
    ctx.report.stopped("some tests are not well-formed, so none were run");
    return null;
  }
  return tests;
}

/** Removes the `.` and `..` parts from `path`. */
function normalize(path: string): string {
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return "/" + parts.join("/");
}

/** Finds the tests that the targets identify. A target is the name of a
 * test, or a file or a directory with tests. If a target identifies no test,
 * it gives null. */
function select(ctx: Context, tests: TestCase[], targets: string[]): TestCase[] | null {
  const root = ctx.project().root;
  const keep = new Set<number>();
  let unknown = false;
  for (const target of targets) {
    let matched = tests.flatMap((test, index) => (test.name === target ? [index] : []));
    if (matched.length === 0) {
      const path = normalize(target.startsWith("/") ? target : `${root}/${target}`);
      matched = tests.flatMap((test, index) =>
        test.file === path || test.file.startsWith(path + "/") ? [index] : [],
      );
    }
    if (matched.length === 0) {
      ctx.report.error(`\`${target}\` matches no test by name, file or directory`);
      unknown = true;
    }
    for (const index of matched) keep.add(index);
  }
  if (unknown) {
    ctx.report.stopped("no tests were run");
    return null;
  }
  return tests.filter((_, index) => keep.has(index));
}

function plural(count: number, one: string, many: string): string {
  return count === 1 ? one : many;
}

async function test(ctx: Context, args: Record<string, Json>): Promise<number> {
  const settings = readSettings(ctx);
  if (settings === null) {
    ctx.report.stopped("the test settings have errors");
    return 1;
  }
  const agent = (args.agent as string | null) ?? settings.agent ?? DEFAULT_AGENT;
  // Belay knows the agentic tools and the fluency prompt of each.
  const agents = ctx.require("belay").agents as Agents;
  const tool = agents.list().find((candidate) => candidate.name === agent);
  if (tool === undefined) {
    ctx.report.error(`unknown agent \`${agent}\``, { help: `the agents are ${AGENTS.join(", ")}` });
    return 1;
  }
  // `--model` without a model, or with a value that is not one of the agent's
  // models, lists them as `piton agent` does, and nothing runs.
  const flag = await agents.modelFlag(tool.name, args.model);
  if ("exit" in flag) return flag.exit;
  const jobs = args.serial === true ? 1 : Math.max((args.jobs as number | null) ?? settings.jobs ?? DEFAULT_JOBS, 1);

  // Parse the expression before the compilation starts.
  // Then a mistake in the expression shows before the slow steps.
  const expression = args.tags as string | null;
  let filter: tags.Expr | null = null;
  if (expression !== null) {
    try {
      filter = tags.parse(expression);
    } catch (error) {
      ctx.report.error(`\`--tags ${expression}\`: ${(error as Error).message}`, {
        help: "combine tags with and, or, not and parentheses, like `(a and b) not c`",
      });
      return 1;
    }
  }

  // Compile all the source files under the root.
  // Do not compile only the files that the entry reaches.
  // A test runs because it is exported. The build does not need to write it.
  const compilation = ctx.compile({ workspace: true });
  if (compilation.errors > 0) {
    compilation.reportDiagnostics();
    ctx.report.stopped("the project has errors, so no tests were run");
    return 1;
  }
  compilation.reportDiagnostics();

  // Belay knows the code root, when the project configures Belay.
  const roots = ctx.require("belay").roots as { codeRoot: string | null };
  const project = { ...ctx.project(), codeRoot: roots.codeRoot };
  const flagScope = args.against as string | null;
  const fallback = flagScope ?? settings.against ?? DEFAULT_SCOPE;
  const anchors = compilation.anchors({
    extends: { module: "@piton/test", name: "Test" },
    exported: true,
    concrete: true,
    withinSourceRoot: true,
  });
  let tests = collect(ctx, anchors, flagScope, fallback);
  if (tests === null) return 1;
  const targets = (args.targets as string[] | null) ?? [];
  const selecting = targets.length > 0 || filter !== null;
  if (targets.length > 0) {
    const selected = select(ctx, tests, targets);
    if (selected === null) return 1;
    tests = selected;
  }
  if (filter !== null) {
    const expr = filter;
    tests = tests.filter((test) => tags.matches(expr, test.tags));
  }
  if (selecting && tests.length === 0) {
    ctx.report.stopped("no tests match");
    return 1;
  }
  if (tests.length === 0) {
    ctx.report.done("no tests found");
    ctx.report.note("a test is an exported `test` anchor, from `use @piton/test`");
    return 0;
  }

  const shown = (file: string) => relative(file, project.root);
  const width = output.width(ctx);

  if (args.list === true) {
    ctx.stdout(
      output.list(
        ctx,
        tests.map((test) => ({ ...test, file: shown(test.file) })),
        width,
      ),
    );
    const files = new Set(tests.map((test) => test.file)).size;
    ctx.report.done(
      `${tests.length} ${plural(tests.length, "test", "tests")} in ${files} ${plural(files, "file", "files")}`,
    );
    return 0;
  }

  if (!ctx.onPath(tool.program)) {
    ctx.report.error(`\`${tool.program}\` is not on PATH`, {
      help: `install it, or choose another agent with --agent (${AGENTS.join(", ")})`,
    });
    return 1;
  }

  const model = flag.model ?? settings.model;
  if (model === null) {
    // No model was chosen: the agent judges with its default, and the run
    // lists the models it could use instead.
    const offered = await agents.models(tool.name);
    ctx.report.note(
      offered.length > 0
        ? `${tool.name} judges with its default model; its models are ${offered.join(", ")}; choose one with --model`
        : `${tool.name} judges with its default model; it listed no models; choose one with --model`,
    );
  }

  const fluency = agents.fluency(tool.name);
  const launcher: Launcher = {
    agent: tool.name,
    program: tool.program,
    modelOptions: model === null ? [] : agents.modelOptions(tool.name, model),
    fluency,
    promptFile: tool.name === "opencode" ? ctx.temporaryFile(fluency, ".md") : null,
  };

  const progress = new Progress(ctx, tests.length);
  const started = Date.now();
  const notPassing: [output.Status, string, string, Sides][] = [];
  const fixes: [number, string, string][] = [];
  let passed = 0;
  let failed = 0;
  let errored = 0;
  let printed = 0;
  let next = 0;
  let running = true;

  const finish = (index: number, outcome: Outcome) => {
    const elapsed = progress.finish(index);
    const test = tests![index];
    if (outcome.status === "pass") passed += 1;
    else if (outcome.status === "fail") failed += 1;
    else errored += 1;
    if (outcome.status !== "pass") {
      notPassing.push([outcome.status, test.name, shown(test.file), outcome.status === "fail" ? outcome.sides : null]);
    }
    if (outcome.status === "fail") fixes.push([index, test.name, fixPrompt(project, test, outcome)]);
    if (printed > 0) ctx.stdout("\n");
    printed += 1;
    ctx.stdout(
      output.result(
        ctx,
        outcome.status,
        test.name,
        output.details(shown(test.file), test.tags),
        output.clock(elapsed),
        outcome.reason,
        outcome.status === "fail" ? outcome.explanation : null,
        outcome.status === "error" ? null : outcome.sides,
        width,
      ),
    );
    progress.draw();
  };

  // Up to `jobs` workers take the next test until no tests remain.
  // JavaScript does only one thing at a time. Thus, `next` does not need a lock.
  // The tools operate in parallel, and each worker waits for its tool.
  const worker = async () => {
    for (;;) {
      const index = next;
      next += 1;
      if (index >= tests!.length) return;
      progress.start(index, tests![index].name);
      const outcome = await judge(ctx, project, launcher, tests![index], (activity) =>
        progress.activity(index, activity),
      );
      finish(index, outcome);
    }
  };

  // The clocks and the spinner continue to move when all the tools are quiet.
  const ticker = (async () => {
    while (running) {
      await ctx.sleep(120);
      if (running) progress.tick();
    }
  })();

  await Promise.all(Array.from({ length: Math.min(jobs, tests.length) }, worker));
  running = false;
  await ticker;
  progress.clear();

  const total = tests.length;
  const took = output.clock(Date.now() - started);
  const recap = output.recap(ctx, notPassing);
  if (recap.length > 0) ctx.stdout(`\n${recap}`);
  // Show the fix prompts in the order of the tests, not in the order that the
  // tests finished.
  fixes.sort((a, b) => a[0] - b[0]);
  const prompts = output.fixPrompts(
    ctx,
    fixes.map(([, name, prompt]) => [name, prompt]),
    width,
  );
  if (prompts.length > 0) ctx.stdout(`\n${prompts}`);
  const testsWord = plural(total, "test", "tests");
  if (failed === 0 && errored === 0) {
    ctx.report.done(`${total} ${testsWord} passed in ${took}`);
    return 0;
  }
  const parts = [`${passed} passed`];
  if (failed > 0) parts.push(`${failed} failed`);
  if (errored > 0) parts.push(`${errored} could not be judged`);
  ctx.report.stopped(`${total} ${testsWord}: ${parts.join(", ")} in ${took}`);
  await handOff(ctx, agents, fixes.map(([, , prompt]) => prompt), tool.name, model);
  return 1;
}

/** The agents that Belay exports. */
interface Agents {
  list(): { name: string; program: string; target: string }[];
  fluency(name: string): string;
  models(name: string): Promise<string[]>;
  modelFlag(name: string, value: Json | undefined): Promise<{ model: string | null } | { exit: number }>;
  modelOptions(name: string, model: string): string[];
  launch(
    name: string,
    firstMessage: string | null,
    model: string | null,
  ): { program: string; args: string[]; env: Record<string, string> };
}

/** The item of the model menu that keeps the agent's own default. */
const DEFAULT_MODEL = "Default model";

/** The item of the menu that starts no agent. */
const NO_AGENT = "No agent";

/** Offers to give the fix prompts to an agent. The menu shows on a terminal
 * only, after the outcome, when one or more tests failed. A second menu then
 * lists the models of the chosen agent, unless it is the agent that judged
 * and a model was given for it. The chosen agent starts in the project root
 * with the terminal, and with all the fix prompts as its first message. The
 * exit code of the run does not change. */
async function handOff(
  ctx: Context,
  agents: Agents,
  prompts: string[],
  judged: string,
  judgedModel: string | null,
): Promise<void> {
  if (prompts.length === 0 || !ctx.terminal.stdin || !ctx.terminal.stdout) return;
  const names = agents.list().map((agent) => agent.name);
  const chosen = ctx.choose(`Give the fix ${plural(prompts.length, "prompt", "prompts")} to an agent?`, [...names, NO_AGENT]);
  if (chosen === null || chosen >= names.length) return;
  const name = names[chosen];
  let model: string | null = name === judged ? judgedModel : null;
  if (model === null) {
    const offered = await agents.models(name);
    if (offered.length > 0) {
      const picked = ctx.choose(`Which model should ${name} use?`, [DEFAULT_MODEL, ...offered]);
      if (picked === null) return;
      model = picked === 0 ? null : offered[picked - 1];
    } else {
      ctx.report.note(`${name} listed no models, so it uses its default model`);
    }
  }
  const message =
    prompts.length === 1
      ? prompts[0]
      : `Fix each of these ${prompts.length} failed Piton tests, one after another.\n\n` +
        prompts.map((prompt, index) => `## Failed test ${index + 1}\n\n${prompt}`).join("\n\n");
  const launch = agents.launch(name, message, model);
  if (!ctx.onPath(launch.program)) {
    ctx.report.error(`\`${launch.program}\` is not on PATH, so the fix prompts were not given to it`);
    return;
  }
  ctx.exec(launch.program, launch.args, { env: launch.env });
}

/** The tests are reached by the test runner, though nothing in the
 * specbase leads to them: each exported test is a root of `piton reach` and
 * of the editor's unused check. These are the tests that `piton test` finds. */
function reach(ctx: Context) {
  const compilation = ctx.compile({ workspace: true });
  const anchors = compilation.anchors({
    extends: { module: "@piton/test", name: "Test" },
    exported: true,
    concrete: true,
    withinSourceRoot: true,
  });
  return { anchors: anchors.map((anchor) => ({ file: anchor.file, name: anchor.name })) };
}

export default definePlugin({ commands: { test }, reach });
