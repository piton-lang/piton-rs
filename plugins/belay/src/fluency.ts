// This module holds the Piton fluency reference.
//
// The reference is the FLUENCY_PROMPT.md file of the project. If the project
// has no such file, Belay uses the copy that it ships with.
//
// The build writes the reference as the `piton-fluency` skill for each
// target. A tool finds that skill with its usual skill discovery. A tool
// that cannot take the full reference as an argument gets a short prompt
// that points to the skill file.

import type { Context, Fields } from "@piton/plugin";
import { ADAPTERS, type Adapter } from "./adapters";

/** The file that the GenerateFluencyPrompt skill writes, at the project root. */
export const FLUENCY_FILE = "FLUENCY_PROMPT.md";

/** The name of the skill that holds the reference. */
export const FLUENCY_SKILL = "piton-fluency";

/** The discovery description of the skill. */
export const FLUENCY_DESCRIPTION =
  "The Piton fluency reference: the Piton language, project configuration, plugins, Belay, " +
  "packages, tests and the piton CLI. Use when you read or change .pi files or " +
  "piton.config.pi, or when you run piton commands";

/** The fluency reference: the FLUENCY_PROMPT.md file of the project, or the
 * copy that Belay ships with. */
export function reference(ctx: Context): string {
  if (ctx.stat(FLUENCY_FILE)?.kind === "file") {
    const text = ctx.readFile(FLUENCY_FILE);
    if (text.trim().length > 0) return text;
  }
  return ctx.resource("fluency.md");
}

/** The SKILL.md of the `piton-fluency` skill. */
export function skillFile(ctx: Context): string {
  const fields: Fields = { name: FLUENCY_SKILL, description: FLUENCY_DESCRIPTION };
  return `${ctx.encode("frontmatter", fields)}\n${reference(ctx).trimEnd()}\n`;
}

/** The skill file that a tool reads, relative to the project root, or null
 * when no build wrote one. The skill directory of the tool comes first.
 * Then come the skill directories of the other adapters. */
export function builtSkill(ctx: Context, own: Adapter | null): string | null {
  const order = own === null ? ADAPTERS : [own, ...ADAPTERS.filter((adapter) => adapter !== own)];
  for (const adapter of order) {
    const path = `${adapter.skillRoot}/${FLUENCY_SKILL}/SKILL.md`;
    if (ctx.stat(path)?.kind === "file") return path;
  }
  return null;
}
