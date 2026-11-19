// The piton.config.pi `piton init` writes, from the answers.

import type { Context } from "@piton/plugin";
import type { Adapter } from "./adapters";

/** The file that holds a project's configuration. */
export const CONFIG = "piton.config.pi";

/** Where every project keeps its specification, relative to the project. */
export const SPEC_ROOT = "spec";

/** The package the crux skills come from. */
export const CRUX = "https://github.com/piton-lang/crux.git";

export type Kind = "basic" | "application" | "tether";

export interface Belay {
  adapters: Adapter[];
  /** Relative to the project without `./`, like `src`; `.` for the project
   * root. */
  codeRoot: string;
  shapeRoot: string;
}

/** What the project is to be. */
export interface Answers {
  /** The project's name. */
  name: string;
  kind: Kind;
  /** What a basic project compiles to. */
  renderer: string | null;
  belay: Belay | null;
  testing: boolean;
  crux: boolean;
}

/** The project's configuration, canonically formatted. */
export function config(ctx: Context, answers: Answers): string {
  const { kind, belay, testing, crux } = answers;
  const out: string[] = [];

  out.push("use @piton/config");
  if (belay !== null) out.push("use @piton/belay");
  if (kind === "tether") out.push("use @piton/packaging");
  if (testing) out.push("use @piton/test");
  out.push("");
  if (belay !== null) {
    out.push(`from @piton/belay import ${belay.adapters.map((adapter) => adapter.exportName).join(", ")}`, "");
  }

  out.push("export piton-config Project:");
  out.push(`    root: ./${SPEC_ROOT}`);
  out.push(`    entry: ./${SPEC_ROOT}/index.pi`);
  if (answers.renderer !== null) out.push(`    renderer: ${answers.renderer}`);
  const plugins = [...(belay !== null ? ["Belay"] : []), ...(testing ? ["Tests"] : [])];
  list(out, "plugins", plugins.map((name) => `{${name}}`));
  if (kind === "tether") list(out, "packages", ["{Library}"]);
  if (crux) list(out, "dependencies", [CRUX]);

  if (belay !== null) {
    out.push("", "belay-config Belay:");
    out.push(`    codeRoot: ${relative(belay.codeRoot)}`);
    out.push(`    shapeRoot: ${relative(belay.shapeRoot)}`);
    list(out, "adapters", belay.adapters.map((adapter) => `{${adapter.exportName}}`));
  }

  if (testing) {
    out.push("");
    if (kind === "tether") {
      out.push("// A package is a specification, so its tests judge only that.");
      out.push("test-config Tests:", "    against: spec");
    } else {
      out.push("// The tests judge both the specification and the code.");
      out.push("test-config Tests:", "    against: both");
    }
  }

  if (kind === "tether") {
    out.push("", "piton-package Library:");
    out.push(`    name: ${answers.name}`);
    out.push(`    root: ./${SPEC_ROOT}`);
  }

  return ctx.format(out.join("\n") + "\n", CONFIG);
}

/** Adds a list property, when it has items. */
function list(out: string[], property: string, items: string[]) {
  if (items.length === 0) return;
  out.push("", `    ${property}:`);
  for (const item of items) out.push(`        - ${item}`);
}

/** A root as the configuration writes it, starting with `./`. */
function relative(root: string): string {
  return root === "." ? "." : `./${root}`;
}
