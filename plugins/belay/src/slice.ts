// This module adds `piton belay slice`.
//
// The command prints the slice of a target, like `piton slice`. It cites
// where the build of one adapter puts each declaration, and not the source.
// Belay finds the places. The host finds the slice and prints it.

import type { Context, Json } from "@piton/plugin";
import { Constructs } from "./constructs";
import { loadOptions } from "./options";
import { targetLayout } from "./plan";
import { Source } from "./source";

export function sliceCommand(ctx: Context, args: Record<string, Json>): number {
  const target = String(args.target ?? "");
  const adapter = String(args.adapter ?? "");
  if (ctx.settings() === null) {
    ctx.report.error(
      `\`--adapter ${adapter}\` cites what a Belay target compiles, and this project has no Belay configuration`,
    );
    return 1;
  }
  // The project compiles from its entry, as a build compiles it, because
  // only the build gives each declaration a place.
  const source = new Source(ctx);
  if (source.compilation.errors > 0) {
    source.compilation.reportDiagnostics();
    return 1;
  }
  const world = {
    source,
    constructs: new Constructs(source),
    options: loadOptions(source),
    markers: ctx.placeholders(),
  };
  const found = targetLayout(world, adapter);
  if ("error" in found) {
    ctx.report.error(found.error);
    return 1;
  }
  return source.compilation.slice(target, found.layout);
}
