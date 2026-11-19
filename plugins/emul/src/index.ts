// This module is the entry of the Emul plugin.
//
// The module hook of the plugin gives the Piton module of each Emul file that
// a project imports. The module holds an emul-design anchor for each design
// that the file exports. The plugin also adds `piton emul`. The command lists
// the designs and tells if the `emul` compiler is on PATH.

import { definePlugin, type Context, type ModuleFile } from "@piton/plugin";
import { synthesize } from "./designs";

/** Gives the Piton module of an Emul file. */
function module(ctx: Context, file: ModuleFile): string {
  const isFile = (path: string): boolean => {
    try {
      return ctx.stat(path)?.kind === "file";
    } catch {
      return false;
    }
  };
  return synthesize(file.text, file.path, isFile);
}

function emul(ctx: Context): number {
  const compiled = ctx.compile({ workspace: true });
  const designs = compiled.anchors({
    extends: { module: "@piton/emul", name: "EmulDesign" },
    concrete: true,
  });
  // One Emul file can come into many modules. Show each design one time.
  const seen = new Set<string>();
  let count = 0;
  for (const design of designs) {
    const { name, kind, file } = design.properties;
    const key = `${String(file)}#${String(name)}`;
    if (seen.has(key)) continue;
    seen.add(key);
    count += 1;
    ctx.stdout(`${ctx.paint("name", String(name))}  ${String(kind)}  ${ctx.paint("dim", String(file))}\n`);
  }
  if (!ctx.onPath("emul")) {
    ctx.report.warning("the `emul` compiler is not on PATH", {
      help: "install emul; agents expect it to be installed when a spec references an Emul design",
    });
  }
  if (count === 0) {
    ctx.report.done("the project imports no Emul designs");
  } else {
    ctx.report.done(`${count} Emul ${count === 1 ? "design" : "designs"}`);
  }
  return 0;
}

export default definePlugin({ module, commands: { emul } });
