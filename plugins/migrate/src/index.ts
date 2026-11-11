// This module is the entry of the migrate plugin.
//
// The plugin finds projects that are written for an older Piton. Before each
// command, the host shows what it finds and asks to apply the edits. The
// `piton migrate` command applies them, or with `--check` it only reports
// them.

import { definePlugin, type Context, type Json } from "@piton/plugin";
import { findings } from "./migrations";

function migrate(ctx: Context, args: Record<string, Json>): number {
  const found = findings(ctx);
  if (found.length === 0) {
    ctx.report.done("the project is up to date");
    return 0;
  }
  if (args.check === true) {
    for (const finding of found) ctx.report.error(finding.message, { help: "run `piton migrate` to update it" });
    ctx.report.stopped(`the project needs ${found.length === 1 ? "a migration" : `${found.length} migrations`}`);
    return 1;
  }
  let changed = 0;
  for (const finding of found) {
    if (finding.edits.length === 0) {
      ctx.report.warning(finding.message, { help: "this one has to be changed by hand" });
      continue;
    }
    for (const path of ctx.applyEdits(finding.edits)) {
      ctx.stdout(`${path}\n`);
      changed += 1;
    }
  }
  ctx.report.done(`updated ${changed} ${changed === 1 ? "file" : "files"}`);
  return 0;
}

export default definePlugin({
  inspect: (ctx) => ({ findings: findings(ctx) }),
  commands: { migrate },
});
