// This module is the entry of the init plugin: `piton init`, which starts a
// project from one of the templates in the plugin's templates/ directory.
//
// The templates come in groups, like `piton` and `belay`, and a template is
// named with its group: `belay/application`. They are embedded with the
// plugin, so init works offline and a release always writes the templates it
// shipped with.
//
// A template whose config lists Belay adapters also asks which adapters to
// build for. The adapters it lists are the default, and the chosen ones
// replace them in the written config. Files a template keeps under
// `.adapters/<target id>/` are written only when that adapter is chosen.
//
// A template whose config lists `dependencies` has them tethered once its
// files are written, so the project builds straight away; `--no-tether`
// leaves that for `piton tether`.
//
// Nothing is ever overwritten: when any file a template would write already
// exists, the host says which and writes none of them.

import { definePlugin, type Context, type Json } from "@piton/plugin";
import { ADAPTER_FILES, ADAPTERS, CONFIG, adaptersNamed, listedAdapters, withAdapters, type Adapter } from "./adapters";
import { groups, type Group, type Template } from "./templates";

/** Thrown when the user cancels a question; init then writes nothing. */
class Cancelled extends Error {}

function init(ctx: Context, args: Record<string, Json>): number {
  const all = groups(ctx);
  if (args.list === true) {
    list(ctx, all);
    return 0;
  }
  try {
    return start(ctx, all, args);
  } catch (error) {
    if (error instanceof Cancelled) {
      ctx.report.error("init was cancelled, so nothing was written");
      return 1;
    }
    throw error;
  }
}

function list(ctx: Context, all: Group[]) {
  const width = Math.max(0, ...all.flatMap((group) => group.templates.map((template) => template.name.length)));
  for (const group of all) {
    ctx.stdout(`${ctx.paint("heading", group.name)}  ${ctx.paint("dim", group.description)}${recommended(ctx, group.preferred)}\n`);
    for (const template of group.templates) {
      ctx.stdout(`  ${ctx.paint("name", template.name.padEnd(width))}  ${template.description}\n`);
    }
  }
}

function start(ctx: Context, all: Group[], args: Record<string, Json>): number {
  const asking = ctx.terminal.stdin;
  const templates = all.flatMap((group) => group.templates);

  let directory = typeof args.directory === "string" ? args.directory : null;
  if (directory === null) directory = asking ? askName(ctx) : ".";

  let template: Template;
  const named = typeof args.template === "string" ? args.template : null;
  if (named !== null) {
    const group = all.find((group) => group.name === named);
    const found = templates.find((template) => template.name === named);
    if (found !== undefined) {
      template = found;
    } else if (group !== undefined && asking) {
      template = pick(ctx, group);
    } else if (group !== undefined) {
      ctx.report.error(`\`${named}\` is a group of templates; pick one with --template: ${names(group.templates)}`);
      return 1;
    } else {
      ctx.report.error(`\`${named}\` is not a template; the templates are ${names(templates)}`);
      return 1;
    }
  } else if (asking) {
    template = pick(ctx, pickGroup(ctx, all));
  } else {
    ctx.report.error(`pick a template with --template: ${names(templates)}`);
    ctx.report.help("`piton init --list` describes each one");
    return 1;
  }

  const config = template.files.find((file) => file.path === CONFIG)?.contents ?? null;
  const listed = config === null ? [] : listedAdapters(config);
  const requested = (Array.isArray(args.adapter) ? args.adapter : [])
    .flatMap((value) => String(value).split(","))
    .filter((name) => name.trim().length > 0);
  let chosen: Adapter[];
  if (listed.length === 0) {
    if (requested.length > 0) {
      ctx.report.error(`the \`${template.name}\` template doesn't use Belay, so it has no adapters to choose`);
      return 1;
    }
    chosen = [];
  } else if (requested.length > 0) {
    const named = adaptersNamed(requested);
    if ("unknown" in named) {
      const known = ADAPTERS.map((adapter) => `\`${adapter.targetId}\``).join(", ");
      ctx.report.error(`\`${named.unknown}\` is not an adapter; the adapters are ${known}`);
      return 1;
    }
    chosen = named.chosen;
  } else if (asking) {
    chosen = pickAdapters(ctx, listed);
  } else {
    chosen = listed;
  }

  const files = template.files.flatMap(({ path, contents }) => {
    if (path.startsWith(ADAPTER_FILES)) {
      // `.adapters/<target id>/<path>` is written at `<path>`, and only when
      // that adapter is chosen.
      const rest = path.slice(ADAPTER_FILES.length);
      const slash = rest.indexOf("/");
      if (slash < 0) return [];
      const target = rest.slice(0, slash);
      if (!chosen.some((adapter) => adapter.targetId === target)) return [];
      path = rest.slice(slash + 1);
    }
    if (path === CONFIG && config !== null && chosen.length > 0) {
      contents = withAdapters(ctx, config, chosen);
    }
    return [{ path, contents }];
  });

  const { existing } = ctx.scaffold(directory, files);
  if (existing.length > 0) {
    ctx.report.error(
      `\`${directory}\` already has ${existing.length === 1 ? "a file" : "files"} the \`${template.name}\` template would write, so nothing was written`,
    );
    for (const path of existing) ctx.report.note(display(directory, path));
    return 1;
  }

  // A template that depends on packages has them tethered, so the project it
  // starts builds without another step.
  let untethered = false;
  if (config !== null) {
    const tethered = ctx.tether(directory, { check: args["no-tether"] === true });
    if (tethered === "pending") {
      untethered = true;
    } else if (tethered === "failed") {
      ctx.report.help(`the project was written; run \`piton tether\` in ${directory} to install its dependencies`);
      return 1;
    }
  }

  let summary = `Created ${ctx.paint("name", template.name)} in ${ctx.paint("name", directory)}`;
  if (chosen.length > 0) summary += ` for ${chosen.map((adapter) => adapter.targetId).join(", ")}`;
  ctx.report.done(`${summary} ${ctx.paint("dim", `(${files.length} ${files.length === 1 ? "file" : "files"})`)}`);
  ctx.stderr(`\n${ctx.paint("heading", "Next")}\n`);
  if (directory !== ".") ctx.stderr(`  ${ctx.paint("name", `cd ${directory}`)}\n`);
  if (untethered) ctx.stderr(`  ${ctx.paint("name", "piton tether")}\n`);
  ctx.stderr(`  ${ctx.paint("name", "piton build")}\n`);
  return 0;
}

/** Asks for the project's name, which is the directory it is created in. An
 * empty answer creates it in the current directory. */
function askName(ctx: Context): string {
  const answer = ctx.ask("Project name:", {
    help: "the directory to create it in; leave empty for the current one",
  });
  if (answer === null) throw new Cancelled();
  return answer.trim() === "" ? "." : answer.trim();
}

/** Offers `choices`, each a name and a description lined up, and returns the
 * one picked. A single choice is taken without asking. */
function choose<T>(ctx: Context, message: string, choices: { value: T; name: string; description: string; preferred: boolean }[]): T {
  if (choices.length === 1) return choices[0].value;
  const width = Math.max(0, ...choices.map((choice) => choice.name.length));
  const index = ctx.choose(
    message,
    choices.map((choice) => `${choice.name.padEnd(width)}  ${choice.description}${recommended(ctx, choice.preferred)}`),
  );
  if (index === null) throw new Cancelled();
  return choices[index].value;
}

/** Asks which group of templates to start from. */
function pickGroup(ctx: Context, all: Group[]): Group {
  return choose(
    ctx,
    "Kind of project:",
    all.map((group) => ({ value: group, name: group.name, description: group.description, preferred: group.preferred })),
  );
}

/** Asks which template in `group` to use. */
function pick(ctx: Context, group: Group): Template {
  return choose(
    ctx,
    "Template:",
    group.templates.map((template) => ({
      value: template,
      name: template.name.slice(template.name.lastIndexOf("/") + 1),
      description: template.description,
      preferred: false,
    })),
  );
}

/** Asks which adapters to build for, with the ones the template lists
 * already checked. At least one has to be chosen. */
function pickAdapters(ctx: Context, listed: Adapter[]): Adapter[] {
  const width = Math.max(...ADAPTERS.map((adapter) => adapter.platform.length));
  const chosen = ctx.chooseMany(
    "Adapters:",
    ADAPTERS.map((adapter) => `${adapter.platform.padEnd(width)}  ${adapter.targetId}`),
    {
      defaults: ADAPTERS.flatMap((adapter, index) => (listed.includes(adapter) ? [index] : [])),
      minimum: 1,
    },
  );
  if (chosen === null) throw new Cancelled();
  return ADAPTERS.filter((_, index) => chosen.includes(index));
}

/** The mark beside the preferred group. */
function recommended(ctx: Context, preferred: boolean): string {
  return preferred ? `  ${ctx.paint("success", "(recommended)")}` : "";
}

function names(templates: Template[]): string {
  return templates.map((template) => `\`${template.name}\``).join(", ");
}

/** A written path as the person running init would type it. */
function display(directory: string, path: string): string {
  return directory === "." ? path : `${directory.replace(/\/+$/, "")}/${path}`;
}

export default definePlugin({ commands: { init } });
